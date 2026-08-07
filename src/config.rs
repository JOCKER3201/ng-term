//! User configuration: ~/.config/ng-term
//!
//! Structure:
//!   ~/.config/ng-term/ng-term.conf        — main configuration (Key=Value)
//!   ~/.config/ng-term/themes/style/       — shared style files (*.css)
//!   ~/.config/ng-term/themes/layauts/     — custom layout files (*.layaut)
//!   ~/.config/ng-term/themes/look/<theme>/ — complete themes:
//!       meta        — metafile with a Name= field (name used in ng-term.conf)
//!       *.css       — symlink into themes/style/
//!       *.layaut    — optional; without it the adaptive default is used
//!
//! EVERY layout (the built-in default and any .layaut file, authored at
//! the 16:9 reference) is adapted CONTINUOUSLY to the screen: on landscape
//! an edge-anchored transform keeps side columns at a constant absolute
//! width for any aspect ratio; on portrait the visible panels are reflowed
//! into a vertical stack (panels hidden in the base stay hidden). The
//! screen is detected from EDID/mode data and re-checked at runtime when
//! the window moves to another monitor. NGTERM_SCREEN= and NGTERM_ASPECT=
//! override the detection.
//!
//! In ng-term.conf the Look=<name> option picks a complete theme by the
//! metafile's Name= field. The Style= and Layaut= options name files from
//! themes/style and themes/layauts (without extensions). Empty values or
//! missing options = defaults built into the code.

use crate::theme::{Color, Theme};
use crate::widgets::{LayoutSpec, PanelSpec};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

pub struct Config {
    pub theme: Theme,
    pub layout: LayoutSpec,
}

/// Theme found in ~/.config/ng-term/themes (name from the metafile).
#[derive(Clone)]
pub struct ThemeInfo {
    pub name: String,
    #[allow(dead_code)]
    pub dir: PathBuf,
}

/// Built-in theme palettes — colors of all default eDEX-UI themes.
/// (name, r, g, b, background, grey, terminal fg, terminal bg, cursor)
const BUILTIN_THEMES: [(&str, u8, u8, u8, &str, &str, &str, &str, &str); 10] = [
    ("apollo", 235, 235, 235, "#191919", "#262827", "#ebebeb", "#191919", "#ebebeb"),
    ("blade", 204, 94, 55, "#090B0A", "#262827", "#cc5e37", "#090B0A", "#cc5e37"),
    ("chalkboard", 239, 240, 235, "#222430", "#222430", "#eff0eb", "#282a36", "#97979b"),
    ("cyborg", 95, 215, 215, "#0a3333", "#034747", "#a3c2c2", "#0a3333", "#5cffff"),
    ("interstellar", 3, 169, 244, "#dedede", "#bfbfbf", "#03A9F4", "#dedede", "#03A9F4"),
    ("matrix", 0, 143, 17, "#090B0A", "#262827", "#00ff41", "#0D0208", "#00ff41"),
    ("navy", 20, 119, 205, "#222430", "#222430", "#87b7cc", "#222430", "#87b7cc"),
    ("nord", 216, 222, 233, "#2E3440", "#4c566a", "#D8DEE9", "#2E3440", "#D8DEE9"),
    ("red", 204, 0, 34, "#090B0A", "#0f2e3d", "#cc0022", "#090B0A", "#cc0022"),
    ("tron", 170, 207, 209, "#05080d", "#262828", "#aacfd1", "#05080d", "#aacfd1"),
];

/// Screen size categories (subdirectories of themes/look and themes/layauts).
const CATEGORIES: [&str; 4] = ["normal", "big-screen", "small-screen", "ultra-small-screen"];

/// Known aspect ratios in use today (directory name, ratio) — landscape,
/// their portrait counterparts, and phone ratios (Android / mobile Linux).
const ASPECTS: [(&str, f32); 22] = [
    // Landscape.
    ("16x9", 16.0 / 9.0),
    ("16x10", 16.0 / 10.0),
    ("21x9", 21.0 / 9.0),
    ("32x9", 32.0 / 9.0),
    ("4x3", 4.0 / 3.0),
    ("3x2", 3.0 / 2.0),
    ("5x4", 5.0 / 4.0),
    // Phones in landscape.
    ("18x9", 18.0 / 9.0),
    ("19x9", 19.0 / 9.0),
    ("19.5x9", 19.5 / 9.0),
    ("20x9", 20.0 / 9.0),
    // Portrait counterparts.
    ("9x16", 9.0 / 16.0),
    ("10x16", 10.0 / 16.0),
    ("9x21", 9.0 / 21.0),
    ("9x32", 9.0 / 32.0),
    ("3x4", 3.0 / 4.0),
    ("2x3", 2.0 / 3.0),
    ("4x5", 4.0 / 5.0),
    ("9x18", 9.0 / 18.0),
    ("9x19", 9.0 / 19.0),
    ("9x19.5", 9.0 / 19.5),
    ("9x20", 9.0 / 20.0),
];

/// Nearest known aspect ratio for a width/height ratio.
fn aspect_for_ratio(r: f32) -> &'static str {
    ASPECTS
        .iter()
        .min_by(|a, b| {
            (a.1 - r)
                .abs()
                .partial_cmp(&(b.1 - r).abs())
                .unwrap_or(std::cmp::Ordering::Equal)
        })
        .map(|(name, _)| *name)
        .unwrap_or("16x9")
}

fn spec(x: f32, y: f32, w: f32, h: f32) -> PanelSpec {
    PanelSpec { x, y, w, h }
}

/// Numeric ratio of the current (snapped) aspect.
fn current_ratio() -> f32 {
    ASPECTS
        .iter()
        .find(|(a, _)| *a == screen_aspect())
        .map(|(_, r)| *r)
        .unwrap_or(16.0 / 9.0)
}

/// Built-in main layout at the 16:9 reference, per screen size category.
fn builtin_base(cat: &str) -> LayoutSpec {
    match cat {
        // Big screens: slimmer side columns, wider terminal.
        "big-screen" => LayoutSpec {
            left_col: spec(0.5, 2.5, 14.5, 59.5),
            shell: spec(15.5, 2.5, 69.0, 60.3),
            right_col: spec(85.0, 2.5, 14.5, 59.5),
            filesystem: spec(85.0, 17.4, 14.5, 79.6),
            keyboard: spec(15.5, 64.5, 69.0, 32.5),
            control: spec(0.5, 64.5, 14.5, 32.5),
        },
        // Small screens: wider side columns, taller keyboard.
        "small-screen" => LayoutSpec {
            left_col: spec(0.5, 2.0, 19.0, 58.0),
            shell: spec(20.0, 2.0, 60.0, 56.0),
            right_col: spec(80.5, 2.0, 19.0, 58.0),
            filesystem: spec(80.5, 16.9, 19.0, 80.1),
            keyboard: spec(20.0, 60.0, 60.0, 37.0),
            control: spec(0.5, 60.0, 19.0, 37.0),
        },
        // Ultra small screens: terminal + keyboard + control only.
        "ultra-small-screen" => LayoutSpec {
            left_col: spec(200.0, 0.0, 16.0, 60.0),
            shell: spec(0.5, 1.5, 99.0, 53.5),
            right_col: spec(200.0, 0.0, 16.0, 60.0),
            filesystem: spec(200.0, 0.0, 16.0, 80.0),
            keyboard: spec(0.5, 56.5, 99.0, 26.0),
            control: spec(0.5, 84.0, 99.0, 13.5),
        },
        // Normal: the classic eDEX-style layout.
        _ => LayoutSpec {
            left_col: spec(0.6, 2.5, 16.4, 59.5),
            shell: spec(17.5, 2.5, 65.0, 60.3),
            right_col: spec(83.0, 2.5, 16.4, 59.5),
            filesystem: spec(83.0, 17.4, 16.4, 79.6),
            keyboard: spec(17.5, 64.5, 65.0, 32.5),
            control: spec(0.6, 64.5, 16.4, 32.5),
        },
    }
}

fn on_screen(p: &PanelSpec) -> bool {
    p.x < 100.0
}

/// Portrait reflow: the panels VISIBLE in the base layout are stacked
/// vertically — terminal, keyboard, a row of side panels, control.
/// Panels placed off-screen in the base stay hidden, so a minimal base
/// automatically yields the phone arrangement.
fn portrait_reflow(base: &LayoutSpec) -> LayoutSpec {
    let off = spec(200.0, 0.0, 16.0, 60.0);
    let small = matches!(
        screen_category(),
        "small-screen" | "ultra-small-screen"
    );
    let has_kb = on_screen(&base.keyboard);
    let has_left = on_screen(&base.left_col);
    let has_right = on_screen(&base.right_col);
    let has_fs = on_screen(&base.filesystem);
    let has_ctl = on_screen(&base.control);
    let has_row = has_left || has_right || has_fs;

    let kb_h = if has_kb {
        if small { 31.0 } else { 18.5 }
    } else {
        0.0
    };
    let row_h = if has_row { 33.5 } else { 0.0 };
    let ctl_bar_h = if has_ctl && !has_row { 13.5 } else { 0.0 };

    let gap = 1.5f32;
    let mut used = 0.0f32;
    for h in [kb_h, row_h, ctl_bar_h] {
        if h > 0.0 {
            used += h + gap;
        }
    }
    let shell_h = (97.0 - gap - used).max(20.0);

    let mut out = LayoutSpec {
        shell: spec(0.5, gap, 99.0, shell_h),
        keyboard: off,
        left_col: off,
        right_col: off,
        filesystem: off,
        control: off,
    };
    let mut y = gap + shell_h + gap;
    if has_kb {
        out.keyboard = spec(0.5, y, 99.0, kb_h);
        y += kb_h + gap;
    }
    if has_row {
        // Side panels as columns of one row; control joins as a column.
        let mut cols = 0;
        if has_left {
            cols += 1;
        }
        if has_right || has_fs {
            cols += 1;
        }
        if has_ctl {
            cols += 1;
        }
        let cw = (99.0 - (cols as f32 - 1.0)) / cols as f32;
        let mut x = 0.5;
        if has_left {
            // The telemetry column gets a wider slot when possible.
            let w = if cols == 3 { cw * 1.2 } else { cw };
            out.left_col = spec(x, y, w, row_h);
            x += w + 1.0;
        }
        if has_right || has_fs {
            let w = cw;
            if has_right {
                out.right_col = spec(x, y, w, row_h);
            }
            if has_fs {
                let fs_y = if has_right { y + 9.0 } else { y };
                out.filesystem = spec(x, fs_y, w, row_h - (fs_y - y));
            }
            x += w + 1.0;
        }
        if has_ctl {
            let w = (99.5 - x).max(10.0);
            out.control = spec(x, y, w, row_h);
        }
    } else if has_ctl {
        out.control = spec(0.5, y, 99.0, ctl_bar_h);
    }
    out
}

/// Continuous adaptation of ANY layout (authored at the 16:9 reference)
/// to the current screen: on landscape screens an edge-anchored horizontal
/// transform keeps side columns at a constant absolute width; on portrait
/// screens the layout is reflowed into a vertical stack.
fn adapt_spec(base: LayoutSpec) -> LayoutSpec {
    let ratio = current_ratio();
    if ratio < 1.0 {
        return portrait_reflow(&base);
    }
    let f = ((16.0 / 9.0) / ratio).clamp(0.5, 1.4);
    if (f - 1.0).abs() < 0.001 {
        return base;
    }
    let tr = |p: &PanelSpec| -> PanelSpec {
        if !on_screen(p) {
            return *p;
        }
        let a = p.x;
        let b = p.x + p.w;
        let na = if a <= 50.0 { a * f } else { 100.0 - (100.0 - a) * f };
        let nb = if b <= 50.0 { b * f } else { 100.0 - (100.0 - b) * f };
        spec(na, p.y, (nb - na).max(1.0), p.h)
    };
    LayoutSpec {
        left_col: tr(&base.left_col),
        shell: tr(&base.shell),
        right_col: tr(&base.right_col),
        filesystem: tr(&base.filesystem),
        keyboard: tr(&base.keyboard),
        control: tr(&base.control),
    }
}

/// Adaptive default layout: the built-in main layout adapted to the screen.
pub fn adaptive_layout() -> LayoutSpec {
    adapt_spec(builtin_base(screen_category()))
}

/// EDID diagonal in inches of a /sys/class/drm connector directory.
fn edid_inches(dir: &Path) -> Option<f32> {
    let edid = std::fs::read(dir.join("edid")).ok()?;
    // EDID bytes 21/22: physical width/height in cm.
    if edid.len() >= 23 {
        let w = edid[21] as f32;
        let h = edid[22] as f32;
        if w > 0.0 && h > 0.0 {
            return Some((w * w + h * h).sqrt() / 2.54);
        }
    }
    None
}

/// Width/height ratio of a connector's preferred mode (first modes line).
fn mode_ratio(dir: &Path) -> Option<f32> {
    let modes = std::fs::read_to_string(dir.join("modes")).ok()?;
    let first = modes.lines().next()?;
    let (w, h) = first.split_once('x')?;
    let w: f32 = w.trim().parse().ok()?;
    let h: f32 = h.trim().parse().ok()?;
    if h > 0.0 {
        Some(w / h)
    } else {
        None
    }
}

/// Connected display connectors: (sysfs dir, diagonal in inches).
fn connected_displays() -> Vec<(PathBuf, f32)> {
    let mut out = Vec::new();
    if let Ok(rd) = std::fs::read_dir("/sys/class/drm") {
        for entry in rd.flatten() {
            let p = entry.path();
            let connected = std::fs::read_to_string(p.join("status"))
                .map(|s| s.trim() == "connected")
                .unwrap_or(false);
            if connected {
                if let Some(d) = edid_inches(&p) {
                    out.push((p, d));
                }
            }
        }
    }
    out
}

/// The biggest connected display (sysfs dir, inches).
fn best_display() -> Option<(PathBuf, f32)> {
    connected_displays()
        .into_iter()
        .max_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal))
}

/// Screen diagonal in inches; with several displays the largest one wins.
fn detect_screen_inches() -> Option<f32> {
    best_display().map(|(_, d)| d)
}

/// Category for a given diagonal in inches.
fn category_for_inches(d: f32) -> &'static str {
    if d <= 10.0 {
        "ultra-small-screen"
    } else if d <= 20.0 {
        "small-screen"
    } else if d < 32.0 {
        "normal"
    } else {
        "big-screen"
    }
}

fn env_category() -> Option<&'static str> {
    let v = std::env::var("NGTERM_SCREEN").ok()?;
    CATEGORIES.iter().find(|c| **c == v).copied()
}

/// Active category index in CATEGORIES; usize::MAX = not initialized yet.
static ACTIVE_CATEGORY: std::sync::atomic::AtomicUsize =
    std::sync::atomic::AtomicUsize::new(usize::MAX);

fn set_category(cat: &'static str) {
    if let Some(idx) = CATEGORIES.iter().position(|c| *c == cat) {
        ACTIVE_CATEGORY.store(idx, std::sync::atomic::Ordering::Relaxed);
    }
}

/// Current screen size category; detected on first use, overridable with
/// NGTERM_SCREEN=<category>, updated at runtime by update_category_for_monitor.
pub fn screen_category() -> &'static str {
    let idx = ACTIVE_CATEGORY.load(std::sync::atomic::Ordering::Relaxed);
    if idx < CATEGORIES.len() {
        return CATEGORIES[idx];
    }
    let cat = if let Some(c) = env_category() {
        eprintln!("ng-term: screen category forced by NGTERM_SCREEN -> {c}");
        c
    } else {
        let inches = detect_screen_inches();
        let c = inches.map(category_for_inches).unwrap_or("normal");
        match inches {
            Some(d) => eprintln!("ng-term: screen {d:.1}\" -> {c}"),
            None => eprintln!("ng-term: screen size unknown -> {c}"),
        }
        c
    };
    set_category(cat);
    cat
}

/// Sysfs directory of a specific connector (e.g. "DP-2").
fn connector_dir(connector: &str) -> Option<PathBuf> {
    let suffix = format!("-{connector}");
    std::fs::read_dir("/sys/class/drm")
        .ok()?
        .flatten()
        .map(|e| e.path())
        .find(|p| {
            p.file_name()
                .map(|n| n.to_string_lossy().ends_with(&suffix))
                .unwrap_or(false)
        })
}

/// Active aspect index in ASPECTS; usize::MAX = not initialized yet.
static ACTIVE_ASPECT: std::sync::atomic::AtomicUsize =
    std::sync::atomic::AtomicUsize::new(usize::MAX);

fn set_aspect(aspect: &'static str) {
    if let Some(idx) = ASPECTS.iter().position(|(a, _)| *a == aspect) {
        ACTIVE_ASPECT.store(idx, std::sync::atomic::Ordering::Relaxed);
    }
}

fn env_aspect() -> Option<&'static str> {
    let v = std::env::var("NGTERM_ASPECT").ok()?;
    ASPECTS.iter().find(|(a, _)| *a == v).map(|(a, _)| *a)
}

/// Current screen aspect ratio; detected on first use, overridable with
/// NGTERM_ASPECT=<name>, updated at runtime by update_screen_for_monitor.
pub fn screen_aspect() -> &'static str {
    let idx = ACTIVE_ASPECT.load(std::sync::atomic::Ordering::Relaxed);
    if idx < ASPECTS.len() {
        return ASPECTS[idx].0;
    }
    let aspect = if let Some(a) = env_aspect() {
        eprintln!("ng-term: aspect ratio forced by NGTERM_ASPECT -> {a}");
        a
    } else {
        let ratio = best_display().and_then(|(dir, _)| mode_ratio(&dir));
        let a = ratio.map(aspect_for_ratio).unwrap_or("16x9");
        match ratio {
            Some(r) => eprintln!("ng-term: aspect ratio {r:.3} -> {a}"),
            None => eprintln!("ng-term: aspect ratio unknown -> {a}"),
        }
        a
    };
    set_aspect(aspect);
    aspect
}

/// Recomputes the size category and aspect ratio for the monitor the
/// window currently sits on (connector name from the windowing system).
/// Returns true when either changed. NGTERM_SCREEN / NGTERM_ASPECT pin
/// their respective values permanently.
pub fn update_screen_for_monitor(monitor_name: &str) -> bool {
    // Windowing systems sometimes append details after the connector name.
    let connector = monitor_name.split_whitespace().next().unwrap_or(monitor_name);
    let Some(dir) = connector_dir(connector) else { return false };
    let mut changed = false;

    if env_category().is_none() {
        if let Some(inches) = edid_inches(&dir) {
            let cat = category_for_inches(inches);
            if cat != screen_category() {
                eprintln!("ng-term: screen changed: {connector} {inches:.1}\" -> {cat}");
                set_category(cat);
                changed = true;
            }
        }
    }
    if env_aspect().is_none() {
        if let Some(ratio) = mode_ratio(&dir) {
            let aspect = aspect_for_ratio(ratio);
            if aspect != screen_aspect() {
                eprintln!(
                    "ng-term: aspect changed: {connector} {ratio:.3} -> {aspect}"
                );
                set_aspect(aspect);
                changed = true;
            }
        }
    }
    changed
}

/// Directory with complete themes: ~/.config/ng-term/themes/look
fn look_dir() -> PathBuf {
    config_dir().join("themes").join("look")
}

/// Scans the themes/look directory; returns entries with a valid metafile (Name=).
pub fn list_themes() -> Vec<ThemeInfo> {
    let dir = look_dir();
    let mut out = Vec::new();
    if let Ok(rd) = std::fs::read_dir(&dir) {
        for entry in rd.flatten() {
            let p = entry.path();
            if !p.is_dir() {
                continue;
            }
            if let Some(meta) = read_meta(&p) {
                if let Some(name) = parse_kv(&meta).get("Name") {
                    if !name.is_empty() {
                        out.push(ThemeInfo { name: name.clone(), dir: p });
                    }
                }
            }
        }
    }
    out.sort_by(|a, b| a.name.cmp(&b.name));
    out
}

pub fn load() -> (Config, Option<String>) {
    init_tree(&config_dir());
    resolve()
}

/// Layout by name: a custom file from themes/layauts, or — for "default"
/// (when no such file exists) — the generated portrait file on portrait
/// screens, then the adaptive layout computed in code.
fn layaut_by_name(name: &str) -> Option<LayoutSpec> {
    if let Ok(text) = std::fs::read_to_string(layauts_dir().join(format!("{name}.layaut"))) {
        return Some(parse_layaut(&text));
    }
    if name == "default" {
        let ratio = ASPECTS
            .iter()
            .find(|(a, _)| *a == screen_aspect())
            .map(|(_, r)| *r)
            .unwrap_or(16.0 / 9.0);
        if ratio < 1.0 {
            let file = if matches!(
                screen_category(),
                "small-screen" | "ultra-small-screen"
            ) {
                "portrait-small.layaut"
            } else {
                "portrait.layaut"
            };
            if let Ok(text) = std::fs::read_to_string(layauts_dir().join(file)) {
                return Some(parse_layaut(&text));
            }
        }
        return Some(adaptive_layout());
    }
    None
}

/// The complete default theme for the current screen: the default style
/// plus the default layout of the current aspect/size. The generator
/// guarantees a default for every variant, so theme authors do not have
/// to provide any variants themselves.
fn default_theme_config() -> Config {
    let theme = std::fs::read_to_string(style_dir().join("default.css"))
        .map(|s| parse_css(&s))
        .unwrap_or_else(|_| Theme::load());
    let layout = layaut_by_name("default").unwrap_or_default();
    Config { theme, layout }
}

/// Resolves the effective configuration from ng-term.conf:
/// - a valid Look= always wins; Style= and Layaut= are then cleared
///   in the file so only the look remains,
/// - otherwise Style=/Layaut= are used; a missing component falls back
///   to "default" (default.css has the tron colors),
/// - nothing set -> the built-in default.
///
/// The second value is an English warning for the on-screen popup when an
/// element is unavailable for the current screen size.
pub fn resolve() -> (Config, Option<String>) {
    let mut warning: Option<String> = None;

    // A valid look wins and clears the component options.
    if let Some(name) = current_theme_name() {
        match load_theme(&look_dir(), &name, &mut warning) {
            Some(cfg) => {
                if current_style_name().is_some() || current_layaut_name().is_some() {
                    clear_component_options();
                }
                return (cfg, warning);
            }
            None => {
                eprintln!(
                    "ng-term: theme '{name}' not found in {}",
                    look_dir().display()
                );
                warning = Some(format!(
                    "Look '{name}' is not available on this screen \u{2014} using the default theme"
                ));
                return (default_theme_config(), warning);
            }
        }
    }

    // Component mode: an empty component falls back to "default".
    let style_name = current_style_name();
    let layaut_name = current_layaut_name();
    if style_name.is_some() || layaut_name.is_some() {
        let sname = style_name.unwrap_or_else(|| "default".into());
        let lname = layaut_name.unwrap_or_else(|| "default".into());

        // If the pair matches a look's components, the file is rewritten
        // to that look (components cleared) and the look is loaded.
        if let Some(look_name) = canonicalize_components() {
            if let Some(cfg) = load_theme(&look_dir(), &look_name, &mut warning) {
                return (cfg, warning);
            }
        }
        let sp = style_dir().join(format!("{sname}.css"));
        let theme = match std::fs::read_to_string(&sp) {
            Ok(s) => parse_css(&s),
            Err(_) => {
                eprintln!("ng-term: style '{sname}' not found in {}", style_dir().display());
                Theme::load()
            }
        };
        let layout = match layaut_by_name(&lname) {
            Some(l) => l,
            None => {
                eprintln!(
                    "ng-term: layaut '{lname}' not found in {}",
                    layauts_dir().display()
                );
                warning = Some(format!(
                    "Layaut '{lname}' is not available on this screen \u{2014} using the default theme"
                ));
                return (default_theme_config(), warning);
            }
        };
        return (Config { theme, layout }, warning);
    }

    // Nothing selected: the complete default theme.
    (default_theme_config(), warning)
}

/// Clears the Style= and Layaut= options (a complete look was selected).
pub fn clear_component_options() {
    set_conf_kv("Style", "");
    set_conf_kv("Layaut", "");
}

/// Style/layaut component names a look is composed of (from its symlinks).
fn look_components(dir: &Path) -> (Option<String>, Option<String>) {
    let mut style = None;
    let mut layaut = None;
    if let Ok(rd) = std::fs::read_dir(dir) {
        for entry in rd.flatten() {
            let p = entry.path();
            let ext = p
                .extension()
                .and_then(|e| e.to_str())
                .map(|e| e.to_ascii_lowercase());
            // Only symlinks count: a look composed of shared components.
            let Ok(target) = std::fs::read_link(&p) else { continue };
            let stem = target
                .file_stem()
                .and_then(|s| s.to_str())
                .map(|s| s.to_string());
            match ext.as_deref() {
                Some("css") => style = stem,
                Some("layaut") => layaut = stem,
                _ => {}
            }
        }
    }
    // A look without a layout symlink uses the adaptive default.
    (style, layaut.or_else(|| Some("default".to_string())))
}

/// Finds a look whose components are exactly (style, layaut).
fn find_matching_look(style: &str, layaut: &str) -> Option<String> {
    for info in list_themes() {
        let (s, l) = look_components(&info.dir);
        if s.as_deref() == Some(style) && l.as_deref() == Some(layaut) {
            return Some(info.name);
        }
    }
    None
}

/// Effective component names (style, layaut) implied by the current
/// configuration — for a selected look these are its symlink targets,
/// in component mode the Style=/Layaut= values (missing one falls back
/// to "default"), and with nothing set the "default"/"default" pair.
pub fn effective_components() -> (Option<String>, Option<String>) {
    if let Some(name) = current_theme_name() {
        if let Some(info) = list_themes().into_iter().find(|t| t.name == name) {
            return look_components(&info.dir);
        }
        return (None, None);
    }
    let s = current_style_name();
    let l = current_layaut_name();
    if s.is_none() && l.is_none() {
        return (Some("default".into()), Some("default".into()));
    }
    (
        Some(s.unwrap_or_else(|| "default".into())),
        Some(l.unwrap_or_else(|| "default".into())),
    )
}

/// If the effective Style=/Layaut= pair (missing one falls back to
/// "default") matches some look, rewrites ng-term.conf so that only
/// Look= is set and returns the look name.
pub fn canonicalize_components() -> Option<String> {
    if current_theme_name().is_some() {
        return None;
    }
    let style_set = current_style_name();
    let layaut_set = current_layaut_name();
    if style_set.is_none() && layaut_set.is_none() {
        return None;
    }
    let s = style_set.unwrap_or_else(|| "default".into());
    let l = layaut_set.unwrap_or_else(|| "default".into());
    let look = find_matching_look(&s, &l)?;
    set_theme_option(&look);
    clear_component_options();
    Some(look)
}

/// Clears the Look= option (a component was selected).
pub fn clear_look_option() {
    set_conf_kv("Look", "");
}

/// Path of the bash startup file generated by ng-term.
pub fn shellrc_path() -> PathBuf {
    config_dir().join("shellrc")
}

/// Current Look= value from ng-term.conf (if non-empty).
pub fn current_theme_name() -> Option<String> {
    let text = std::fs::read_to_string(config_dir().join("ng-term.conf")).ok()?;
    let kv = parse_kv(&text);
    kv.get("Look")
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

/// Saves the look choice to ng-term.conf, preserving the rest of the file.
pub fn set_theme_option(name: &str) {
    set_conf_kv("Look", name);
}

fn conf_kv() -> HashMap<String, String> {
    parse_kv(&std::fs::read_to_string(config_dir().join("ng-term.conf")).unwrap_or_default())
}

/// Directory with shared styles: ~/.config/ng-term/themes/style
fn style_dir() -> PathBuf {
    config_dir().join("themes").join("style")
}

/// Directory with custom layouts: ~/.config/ng-term/themes/layauts
fn layauts_dir() -> PathBuf {
    config_dir().join("themes").join("layauts")
}

/// File stems (no extension) of files with the given extension in a directory.
fn list_stems(dir: &Path, ext: &str) -> Vec<String> {
    let mut out = Vec::new();
    if let Ok(rd) = std::fs::read_dir(dir) {
        for entry in rd.flatten() {
            let p = entry.path();
            let matches = p.is_file()
                && p.extension()
                    .and_then(|e| e.to_str())
                    .map(|e| e.eq_ignore_ascii_case(ext))
                    .unwrap_or(false);
            if matches {
                if let Some(stem) = p.file_stem().and_then(|s| s.to_str()) {
                    out.push(stem.to_string());
                }
            }
        }
    }
    out.sort();
    out
}

/// Style names available in themes/style (no extensions).
pub fn list_styles() -> Vec<String> {
    list_stems(&style_dir(), "css")
}

/// Layout names: the adaptive "default" plus custom files in themes/layauts.
pub fn list_layauts() -> Vec<String> {
    let mut out = vec!["default".to_string()];
    for stem in list_stems(&layauts_dir(), "layaut") {
        if stem != "default" {
            out.push(stem);
        }
    }
    out
}

/// Current Style= value from ng-term.conf (if non-empty).
pub fn current_style_name() -> Option<String> {
    conf_kv()
        .get("Style")
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

/// Current Layaut= value from ng-term.conf (if non-empty).
pub fn current_layaut_name() -> Option<String> {
    conf_kv()
        .get("Layaut")
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

pub fn set_style_option(name: &str) {
    set_conf_kv("Style", name);
}

fn font_prefs_for(prefix: &str, min: f32, max: f32) -> (f32, Option<String>, Option<String>) {
    let kv = conf_kv();
    let scale = kv
        .get(&format!("{prefix}FontSize"))
        .and_then(|v| v.trim().parse::<f32>().ok())
        .map(|p| (p / 100.0).clamp(min, max))
        .unwrap_or(1.0);
    let get = |key: String| {
        kv.get(&key)
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
    };
    (
        scale,
        get(format!("{prefix}FontFamily")),
        get(format!("{prefix}FontWeight")),
    )
}

/// Terminal font preferences: (size scale, family, weight).
pub fn term_font_prefs() -> (f32, Option<String>, Option<String>) {
    font_prefs_for("Term", 0.5, 2.0)
}

/// Interface font preferences: (size scale, family, weight).
pub fn ui_font_prefs() -> (f32, Option<String>, Option<String>) {
    font_prefs_for("UI", 0.75, 1.25)
}

pub fn set_term_font_size(percent: u32) {
    set_conf_kv("TermFontSize", &percent.to_string());
}

pub fn set_term_font_family(name: &str) {
    set_conf_kv("TermFontFamily", name);
}

pub fn set_term_font_weight(name: &str) {
    set_conf_kv("TermFontWeight", name);
}

pub fn set_ui_font_size(percent: u32) {
    set_conf_kv("UIFontSize", &percent.to_string());
}

pub fn set_ui_font_family(name: &str) {
    set_conf_kv("UIFontFamily", name);
}

pub fn set_ui_font_weight(name: &str) {
    set_conf_kv("UIFontWeight", name);
}

pub fn set_layaut_option(name: &str) {
    set_conf_kv("Layaut", name);
}

/// Sets Key=Value in ng-term.conf, preserving the rest of the file.
fn set_conf_kv(key: &str, value: &str) {
    let path = config_dir().join("ng-term.conf");
    let text = std::fs::read_to_string(&path).unwrap_or_default();
    let mut lines: Vec<String> = text.lines().map(String::from).collect();
    let mut replaced = false;
    for line in lines.iter_mut() {
        let t = line.trim_start();
        if t.starts_with(&format!("{key}=")) {
            *line = format!("{key}={value}");
            replaced = true;
            break;
        }
    }
    if !replaced {
        lines.push(format!("{key}={value}"));
    }
    let mut out = lines.join("\n");
    out.push('\n');
    if let Err(e) = std::fs::write(&path, out) {
        eprintln!("ng-term: cannot write {}: {e}", path.display());
    }
}

fn config_dir() -> PathBuf {
    if let Ok(xdg) = std::env::var("XDG_CONFIG_HOME") {
        if !xdg.is_empty() {
            return PathBuf::from(xdg).join("ng-term");
        }
    }
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".into());
    PathBuf::from(home).join(".config").join("ng-term")
}

/// Creates the config directory, the ng-term.conf file, the themes tree
/// (look/, style/, layauts/) and (on first run) the sample "tron" theme.
fn init_tree(dir: &Path) {
    let themes = dir.join("themes");
    let themes_existed = themes.is_dir();
    let look = themes.join("look");
    let style = themes.join("style");
    let layauts = themes.join("layauts");
    for d in [&look, &style, &layauts] {
        if let Err(e) = std::fs::create_dir_all(d) {
            eprintln!("ng-term: cannot create {}: {e}", d.display());
            return;
        }
    }

    let conf = dir.join("ng-term.conf");
    if !conf.exists() {
        let _ = std::fs::write(
            &conf,
            "# ng-term configuration\n\
             #\n\
             # Look=<name>    — picks a complete theme by the Name= field of the\n\
             #                  metafile ~/.config/ng-term/themes/look/<dir>/meta\n\
             # Style=<name>   — style file from themes/style (no extension)\n\
             # Layaut=<name>  — layout file from themes/layauts (no extension)\n\
             # TermFont*/UIFont*  — terminal / interface font settings:\n\
             #   *FontSize=<percent> — Term: 50-200, UI: 75-125\n\
             #   *FontFamily=<name>  — family from the settings list\n\
             #   *FontWeight=<name>  — Light/Regular/Medium/SemiBold/Bold\n\
             # Empty values or missing options = defaults built into the program.\n\
             Look=\n\
             Style=\n\
             Layaut=\n\
             TermFontSize=\n\
             TermFontFamily=\n\
             TermFontWeight=\n\
             UIFontSize=\n\
             UIFontFamily=\n\
             UIFontWeight=\n",
        );
    }

    // Shell startup file: source ~/.bashrc + opening files with the
    // associated application when just a file name is typed.
    let rc = dir.join("shellrc");
    if !rc.exists() {
        let _ = std::fs::write(
            &rc,
            "# ng-term: bash startup file (generated once; feel free to edit)\n\
             [ -f \"$HOME/.bashrc\" ] && source \"$HOME/.bashrc\"\n\
             \n\
             # Typing the name of an existing file opens it with the\n\
             # application associated with its extension (xdg-open).\n\
             command_not_found_handle() {\n\
             \x20   if [ -e \"$1\" ] && [ ! -d \"$1\" ]; then\n\
             \x20       (xdg-open \"$1\" >/dev/null 2>&1 &)\n\
             \x20       return 0\n\
             \x20   fi\n\
             \x20   printf 'bash: %s: command not found\\n' \"$1\" >&2\n\
             \x20   return 127\n\
             }\n",
        );
    }

    // Built-in themes are created only when themes/ is first created.
    if !themes_existed {
        // Shared styles (one copy; layouts are adaptive, no files needed).
        for (name, r, g, b, bg, grey, fg, tbg, cur) in BUILTIN_THEMES {
            // The default style is tron's palette under the name "default".
            let style_file = if name == "tron" { "default" } else { name };
            let _ = std::fs::write(
                style.join(format!("{style_file}.css")),
                format!(
                    "/* Colors from the original eDEX-UI theme: {name} */\n\
                     :root {{\n\
                     \x20   --color-r: {r};\n\
                     \x20   --color-g: {g};\n\
                     \x20   --color-b: {b};\n\
                     \x20   --background: {bg};\n\
                     \x20   --grey: {grey};\n\
                     }}\n\
                     terminal {{\n\
                     \x20   foreground: {fg};\n\
                     \x20   background: {tbg};\n\
                     \x20   cursor: {cur};\n\
                     }}\n"
                ),
            );
            // A complete look: metafile + style symlink; the layout is the
            // adaptive default (no layout symlink needed).
            let dir = look.join(name);
            if std::fs::create_dir_all(&dir).is_ok() {
                let _ = std::fs::write(
                    dir.join("meta"),
                    format!(
                        "Name={name}\nDescription=eDEX-UI '{name}' theme (original colors)\n"
                    ),
                );
                let _ = std::os::unix::fs::symlink(
                    format!("../../style/{style_file}.css"),
                    dir.join(format!("{name}.css")),
                );
            }
        }
    }
}

/// Parser for Key=Value files (# and ; comments).
fn parse_kv(text: &str) -> HashMap<String, String> {
    let mut map = HashMap::new();
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') || line.starts_with(';') {
            continue;
        }
        if let Some((k, v)) = line.split_once('=') {
            map.insert(k.trim().to_string(), v.trim().to_string());
        }
    }
    map
}

/// Searches themes/ for a directory whose metafile has Name=<name>
/// and loads its style (.css) and layout (.layaut).
fn load_theme(
    themes_dir: &Path,
    name: &str,
    warning: &mut Option<String>,
) -> Option<Config> {
    let rd = std::fs::read_dir(themes_dir).ok()?;
    for entry in rd.flatten() {
        let dir = entry.path();
        if !dir.is_dir() {
            continue;
        }
        let Some(meta_text) = read_meta(&dir) else { continue };
        let meta = parse_kv(&meta_text);
        if meta.get("Name").map(|n| n.as_str()) != Some(name) {
            continue;
        }
        // Theme found: style + layout (missing parts fall back to defaults).
        let theme = find_file(&dir, "css")
            .and_then(|p| std::fs::read_to_string(p).ok())
            .map(|css| parse_css(&css))
            .unwrap_or_else(Theme::tron);
        // A symlinked layout is resolved BY NAME in the layouts directory
        // of the current aspect ratio and screen size; a regular file is
        // used directly. A missing variant switches to the default theme.
        let layout = match find_file(&dir, "layaut") {
            Some(p) => match std::fs::read_link(&p) {
                Ok(target) => {
                    let stem = target
                        .file_stem()
                        .and_then(|s| s.to_str())
                        .unwrap_or("default")
                        .to_string();
                    match layaut_by_name(&stem) {
                        Some(l) => l,
                        None => {
                            *warning = Some(format!(
                                "Layaut '{stem}' is not available on this screen \u{2014} using the default theme"
                            ));
                            return Some(default_theme_config());
                        }
                    }
                }
                Err(_) => std::fs::read_to_string(&p)
                    .ok()
                    .map(|l| adapt_spec(parse_layaut(&l)))
                    .unwrap_or_default(),
            },
            None => layaut_by_name("default").unwrap_or_default(),
        };
        return Some(Config { theme, layout });
    }
    None
}

/// Metafile: a file named "meta" or with the ".meta" extension.
fn read_meta(dir: &Path) -> Option<String> {
    let exact = dir.join("meta");
    if exact.is_file() {
        return std::fs::read_to_string(exact).ok();
    }
    find_file(dir, "meta").and_then(|p| std::fs::read_to_string(p).ok())
}

fn find_file(dir: &Path, ext: &str) -> Option<PathBuf> {
    std::fs::read_dir(dir)
        .ok()?
        .flatten()
        .map(|e| e.path())
        .find(|p| {
            p.is_file()
                && p.extension()
                    .and_then(|e| e.to_str())
                    .map(|e| e.eq_ignore_ascii_case(ext))
                    .unwrap_or(false)
        })
}

/// Simplified CSS parser: `selector { key: value; }` blocks.
fn parse_css(src: &str) -> Theme {
    let src = strip_css_comments(src);
    let mut blocks: HashMap<String, HashMap<String, String>> = HashMap::new();
    let mut rest = src.as_str();
    while let Some(ob) = rest.find('{') {
        let sel = rest[..ob].trim().to_lowercase();
        let Some(cb_rel) = rest[ob + 1..].find('}') else { break };
        let body = &rest[ob + 1..ob + 1 + cb_rel];
        let mut props = HashMap::new();
        for decl in body.split(';') {
            if let Some((k, v)) = decl.split_once(':') {
                props.insert(k.trim().to_lowercase(), v.trim().to_string());
            }
        }
        blocks.insert(sel, props);
        rest = &rest[ob + 1 + cb_rel + 1..];
    }

    let mut theme = Theme::tron();
    if let Some(root) = blocks.get(":root").or_else(|| blocks.get("colors")) {
        let num = |key: &str| -> Option<u8> { root.get(key)?.parse().ok() };
        if let (Some(r), Some(g), Some(b)) =
            (num("--color-r"), num("--color-g"), num("--color-b"))
        {
            let base = Color::rgb8(r, g, b);
            theme.base = base;
            theme.term_fg = base;
            theme.cursor = base;
        }
        if let Some(c) = root
            .get("--background")
            .or_else(|| root.get("--light-black"))
            .and_then(|v| Color::from_hex(v))
        {
            theme.bg = c;
            theme.term_bg = c;
        }
        if let Some(c) = root.get("--grey").and_then(|v| Color::from_hex(v)) {
            theme.grey = c;
        }
    }
    if let Some(term) = blocks.get("terminal") {
        if let Some(c) = term.get("foreground").and_then(|v| Color::from_hex(v)) {
            theme.term_fg = c;
        }
        if let Some(c) = term.get("background").and_then(|v| Color::from_hex(v)) {
            theme.term_bg = c;
        }
        if let Some(c) = term.get("cursor").and_then(|v| Color::from_hex(v)) {
            theme.cursor = c;
        }
    }
    // Optional full ANSI palette: `palette { color0..color15 }` block.
    if let Some(pal) = blocks.get("palette") {
        for i in 0..16 {
            if let Some(c) = pal
                .get(&format!("color{i}"))
                .and_then(|v| Color::from_hex(v))
            {
                theme.ansi[i] = c;
            }
        }
    }
    theme
}

fn strip_css_comments(src: &str) -> String {
    let mut out = String::with_capacity(src.len());
    let mut rest = src;
    while let Some(start) = rest.find("/*") {
        out.push_str(&rest[..start]);
        match rest[start..].find("*/") {
            Some(end) => rest = &rest[start + end + 2..],
            None => return out,
        }
    }
    out.push_str(rest);
    out
}

/// Parser for .layaut files: `panel = x y width height` (vw/vh units).
fn parse_layaut(src: &str) -> LayoutSpec {
    let mut spec = LayoutSpec::default();
    for line in src.lines() {
        let line = line.split('#').next().unwrap_or("").trim();
        if line.is_empty() {
            continue;
        }
        let Some((k, v)) = line.split_once('=') else { continue };
        let nums: Vec<f32> = v
            .split_whitespace()
            .filter_map(|t| {
                t.trim_end_matches("vw")
                    .trim_end_matches("vh")
                    .parse::<f32>()
                    .ok()
            })
            .collect();
        if nums.len() != 4 {
            continue;
        }
        let p = PanelSpec {
            x: nums[0],
            y: nums[1],
            w: nums[2],
            h: nums[3],
        };
        match k.trim() {
            "left_col" => spec.left_col = p,
            "shell" => spec.shell = p,
            "right_col" => spec.right_col = p,
            "filesystem" => spec.filesystem = p,
            "keyboard" => spec.keyboard = p,
            "control" => spec.control = p,
            other => eprintln!("ng-term: unknown panel in .layaut: {other}"),
        }
    }
    spec
}
