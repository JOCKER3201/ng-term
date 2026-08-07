//! Font loading and the glyph atlas (single-channel, R8).
//!
//! eDEX-UI uses "United Sans" (UI) and "Fira Mono" (terminal). The .woff2
//! files from the eDEX repository can be converted to .ttf and dropped into
//! ./fonts — they are picked up automatically. Otherwise we look for
//! similar system fonts.

use fontdue::{Font, FontSettings};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

pub const ATLAS_W: usize = 1024;
pub const ATLAS_H: usize = 1024;

pub const FONT_UI: u8 = 0;
pub const FONT_MONO: u8 = 1;

#[derive(Clone, Copy)]
pub struct Glyph {
    pub u0: f32,
    pub v0: f32,
    pub u1: f32,
    pub v1: f32,
    pub w: f32,
    pub h: f32,
    /// Offset of the bitmap's left edge relative to the pen.
    pub xmin: f32,
    /// Offset of the bitmap's bottom edge relative to the baseline (Y axis up).
    pub ymin: f32,
    pub advance: f32,
}

pub struct FontSystem {
    fonts: [Font; 2],
    pub atlas: Vec<u8>,
    pub atlas_dirty: bool,
    cache: HashMap<(u8, u32, char), Option<Glyph>>,
    // simple shelf packer
    cur_x: usize,
    cur_y: usize,
    row_h: usize,
}

impl FontSystem {
    pub fn new() -> Self {
        let (ui, mono) = load_fonts();
        let mut fs = FontSystem {
            fonts: [ui, mono],
            atlas: vec![0u8; ATLAS_W * ATLAS_H],
            atlas_dirty: true,
            cache: HashMap::new(),
            cur_x: 2,
            cur_y: 2,
            row_h: 0,
        };
        // White pixel (0,0..2x2) for solid fills.
        for y in 0..2 {
            for x in 0..2 {
                fs.atlas[y * ATLAS_W + x] = 255;
            }
        }
        fs
    }

    /// Replaces the terminal font (settings change); resets the atlas.
    pub fn set_mono(&mut self, font: Font) {
        self.fonts[FONT_MONO as usize] = font;
        self.reset_atlas();
    }

    /// Replaces the interface font (settings change); resets the atlas.
    pub fn set_ui(&mut self, font: Font) {
        self.fonts[FONT_UI as usize] = font;
        self.reset_atlas();
    }

    /// UV of the white pixel — used by solid shapes.
    pub fn white_uv() -> (f32, f32) {
        (0.5 / ATLAS_W as f32, 0.5 / ATLAS_H as f32)
    }

    /// Clears the atlas and cache (e.g. when full after many resizes).
    fn reset_atlas(&mut self) {
        self.atlas.iter_mut().for_each(|p| *p = 0);
        for y in 0..2 {
            for x in 0..2 {
                self.atlas[y * ATLAS_W + x] = 255;
            }
        }
        self.cache.clear();
        self.cur_x = 2;
        self.cur_y = 2;
        self.row_h = 0;
        self.atlas_dirty = true;
    }

    pub fn glyph(&mut self, font: u8, px: f32, ch: char) -> Option<Glyph> {
        let key = (font, (px * 4.0).round() as u32, ch);
        if let Some(g) = self.cache.get(&key) {
            return *g;
        }
        let f = &self.fonts[font as usize];
        let (metrics, bitmap) = f.rasterize(ch, px);
        if metrics.width == 0 || metrics.height == 0 {
            let g = Some(Glyph {
                u0: 0.0,
                v0: 0.0,
                u1: 0.0,
                v1: 0.0,
                w: 0.0,
                h: 0.0,
                xmin: 0.0,
                ymin: 0.0,
                advance: metrics.advance_width,
            });
            self.cache.insert(key, g);
            return g;
        }
        let (w, h) = (metrics.width, metrics.height);
        if self.cur_x + w + 2 > ATLAS_W {
            self.cur_x = 2;
            self.cur_y += self.row_h + 2;
            self.row_h = 0;
        }
        if self.cur_y + h + 2 > ATLAS_H {
            // Atlas full — start over (rare, after many resizes).
            self.reset_atlas();
            if self.cur_y + h + 2 > ATLAS_H {
                return None;
            }
        }
        let (ax, ay) = (self.cur_x, self.cur_y);
        for row in 0..h {
            let dst = (ay + row) * ATLAS_W + ax;
            self.atlas[dst..dst + w].copy_from_slice(&bitmap[row * w..row * w + w]);
        }
        self.cur_x += w + 2;
        self.row_h = self.row_h.max(h);
        self.atlas_dirty = true;

        let g = Some(Glyph {
            u0: ax as f32 / ATLAS_W as f32,
            v0: ay as f32 / ATLAS_H as f32,
            u1: (ax + w) as f32 / ATLAS_W as f32,
            v1: (ay + h) as f32 / ATLAS_H as f32,
            w: w as f32,
            h: h as f32,
            xmin: metrics.xmin as f32,
            ymin: metrics.ymin as f32,
            advance: metrics.advance_width,
        });
        self.cache.insert(key, g);
        g
    }

    /// Line metrics: (ascent, line height).
    pub fn line_metrics(&self, font: u8, px: f32) -> (f32, f32) {
        if let Some(m) = self.fonts[font as usize].horizontal_line_metrics(px) {
            (m.ascent, m.ascent - m.descent + m.line_gap)
        } else {
            (px * 0.8, px * 1.2)
        }
    }

    /// Cell width for the monospace font.
    pub fn mono_advance(&mut self, px: f32) -> f32 {
        self.glyph(FONT_MONO, px, 'M').map(|g| g.advance).unwrap_or(px * 0.6)
    }

    pub fn measure(&mut self, font: u8, px: f32, text: &str, letter_spacing: f32) -> f32 {
        let mut w = 0.0;
        for ch in text.chars() {
            if let Some(g) = self.glyph(font, px, ch) {
                w += g.advance + letter_spacing;
            }
        }
        w
    }
}

fn try_load(path: &Path) -> Option<Font> {
    let data = std::fs::read(path).ok()?;
    Font::from_bytes(data, FontSettings::default()).ok()
}

/// Recursive search for a font file whose name (case-insensitive,
/// separators stripped) contains one of the patterns.
fn find_font(dirs: &[PathBuf], patterns: &[&str]) -> Option<PathBuf> {
    fn walk(dir: &Path, patterns: &[&str], depth: u32, out: &mut Option<PathBuf>) {
        if depth > 4 || out.is_some() {
            return;
        }
        let Ok(rd) = std::fs::read_dir(dir) else { return };
        for entry in rd.flatten() {
            if out.is_some() {
                return;
            }
            let p = entry.path();
            if p.is_dir() {
                walk(&p, patterns, depth + 1, out);
            } else {
                let name: String = p
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("")
                    .to_lowercase()
                    .chars()
                    .filter(|c| c.is_ascii_alphanumeric())
                    .collect();
                if !(name.ends_with("ttf") || name.ends_with("otf")) {
                    continue;
                }
                // Avoid italic variants; bold only when explicitly requested.
                if name.contains("italic") || name.contains("oblique") {
                    continue;
                }
                for pat in patterns {
                    if name.contains(pat) {
                        if name.contains("bold") && !pat.contains("bold") {
                            continue;
                        }
                        *out = Some(p.clone());
                        break;
                    }
                }
            }
        }
    }
    for &pat in patterns {
        let mut found = None;
        for d in dirs {
            walk(d, &[pat], 0, &mut found);
            if found.is_some() {
                return found;
            }
        }
    }
    None
}

fn font_dirs() -> Vec<PathBuf> {
    let mut dirs = vec![PathBuf::from("fonts")];
    if let Ok(home) = std::env::var("HOME") {
        dirs.push(PathBuf::from(format!("{home}/.local/share/fonts")));
        dirs.push(PathBuf::from(format!("{home}/.fonts")));
    }
    dirs.push(PathBuf::from("/usr/share/fonts"));
    dirs.push(PathBuf::from("/usr/local/share/fonts"));
    dirs
}

/// Curated monospace families for the settings dropdown
/// (display name, normalized filename pattern).
const MONO_FAMILIES: [(&str, &str); 12] = [
    ("Fira Mono", "firamono"),
    ("Fira Code", "firacode"),
    ("JetBrains Mono", "jetbrainsmono"),
    ("DejaVu Sans Mono", "dejavusansmono"),
    ("Liberation Mono", "liberationmono"),
    ("Noto Sans Mono", "notosansmono"),
    ("Ubuntu Mono", "ubuntumono"),
    ("Source Code Pro", "sourcecodepro"),
    ("Hack", "hack"),
    ("IBM Plex Mono", "ibmplexmono"),
    ("Cascadia Code", "cascadiacode"),
    ("Inconsolata", "inconsolata"),
];

/// Curated interface (UI) families (display name, filename pattern).
const UI_FAMILIES: [(&str, &str); 7] = [
    ("United Sans", "unitedsans"),
    ("Oxanium", "oxanium"),
    ("Rajdhani", "rajdhani"),
    ("Exo 2", "exo2"),
    ("Orbitron", "orbitron"),
    ("Saira Condensed", "sairacondensed"),
    ("Saira", "saira"),
];

fn pattern_for(display: &str) -> Option<&'static str> {
    MONO_FAMILIES
        .iter()
        .chain(UI_FAMILIES.iter())
        .find(|(name, _)| *name == display)
        .map(|(_, pat)| *pat)
}

fn available_from(table: &[(&str, &str)]) -> Vec<String> {
    let dirs = font_dirs();
    table
        .iter()
        .filter(|(_, pat)| find_font(&dirs, &[pat]).is_some())
        .map(|(name, _)| name.to_string())
        .collect()
}

/// Monospace families actually available on this system (terminal font).
pub fn available_mono_families() -> Vec<String> {
    available_from(&MONO_FAMILIES)
}

/// Interface families available on this system (UI list first, then mono).
pub fn available_ui_families() -> Vec<String> {
    let mut out = available_from(&UI_FAMILIES);
    out.extend(available_from(&MONO_FAMILIES));
    out
}

/// Default search patterns used when no family is selected.
const DEFAULT_MONO_PATTERNS: [&str; 6] = [
    "firamono", "firacode", "jetbrainsmono", "dejavusansmono",
    "liberationmono", "notosansmono",
];
const DEFAULT_UI_PATTERNS: [&str; 8] = [
    "unitedsansmedium", "unitedsans", "oxanium", "rajdhani",
    "exo2", "orbitron", "sairacondensed", "saira",
];

/// Loads a font by family display name and weight
/// (Light/Regular/Medium/SemiBold/Bold). With no family selected the
/// weight is searched across the default families of the given kind.
pub fn load_variant_for(
    family: Option<&str>,
    weight: Option<&str>,
    ui: bool,
) -> Option<Font> {
    let dirs = font_dirs();
    let w = weight.unwrap_or("Regular").to_lowercase().replace(' ', "");
    let base: Vec<&str> = match family.and_then(pattern_for) {
        Some(p) => vec![p],
        None => {
            if ui {
                DEFAULT_UI_PATTERNS.to_vec()
            } else {
                DEFAULT_MONO_PATTERNS.to_vec()
            }
        }
    };
    // The requested weight first, across all candidate families. For the
    // default UI font the weighted search also covers the mono families,
    // because United Sans ships in a single weight only.
    let mut weighted = base.clone();
    if ui && family.is_none() {
        weighted.extend(DEFAULT_MONO_PATTERNS);
    }
    if w != "regular" {
        for pat in &weighted {
            let c = format!("{pat}{w}");
            if let Some(p) = find_font(&dirs, &[c.as_str()]) {
                if let Some(f) = try_load(&p) {
                    return Some(f);
                }
            }
        }
    }
    // ...then the regular variants.
    for pat in &base {
        for c in [format!("{pat}regular"), pat.to_string()] {
            if let Some(p) = find_font(&dirs, &[c.as_str()]) {
                if let Some(f) = try_load(&p) {
                    return Some(f);
                }
            }
        }
    }
    None
}

/// Loads the default terminal font (Fira Mono like eDEX, then fallbacks).
pub fn load_default_mono() -> Font {
    let dirs = font_dirs();
    let mono_path = std::env::var("NGTERM_FONT_MONO").ok().map(PathBuf::from).or_else(|| {
        find_font(
            &dirs,
            &[
                "firamonoregular", "firamono", "firacoderegular", "firacode",
                "jetbrainsmonoregular", "jetbrainsmono", "dejavusansmono",
                "liberationmonoregular", "liberationmono", "notosansmono",
            ],
        )
    });
    mono_path.as_deref().and_then(try_load).unwrap_or_else(|| {
        panic!(
            "ng-term: no monospace font (.ttf/.otf) found.\n\
             Point NGTERM_FONT_MONO at one or drop it into ./fonts"
        )
    })
}

/// Loads the default interface font (United Sans like eDEX, then similar
/// "technical" typefaces; falls back to the monospace font).
pub fn load_default_ui() -> Font {
    let dirs = font_dirs();
    let ui_path = std::env::var("NGTERM_FONT_UI").ok().map(PathBuf::from).or_else(|| {
        find_font(
            &dirs,
            &[
                "unitedsansmedium", "unitedsans", "oxanium", "rajdhani",
                "exo2", "orbitron", "sairacondensed", "saira",
            ],
        )
    });
    ui_path.as_deref().and_then(try_load).unwrap_or_else(|| {
        eprintln!("ng-term: no UI font (United Sans) — using the monospace font");
        load_default_mono()
    })
}

fn load_fonts() -> (Font, Font) {
    (load_default_ui(), load_default_mono())
}
