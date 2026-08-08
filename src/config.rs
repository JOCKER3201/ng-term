//! User configuration and theme data.
//!
//! Configuration (XDG_CONFIG_HOME):
//!   ~/.config/ng-term/ng-term.conf        — main configuration (Key=Value)
//!   ~/.config/ng-term/shellrc             — generated bash startup file
//!
//! Everything a theme is made of is DATA, not configuration, so it lives
//! under XDG_DATA_HOME:
//!   ~/.local/share/ng-term/style/         — shared style files (*.css)
//!   ~/.local/share/ng-term/layauts/       — custom layout files (*.layaut)
//!   ~/.local/share/ng-term/sounds/<set>/  — sound themes, one directory
//!       each; the DIRECTORY NAME is the theme name. Inside, a "meta"
//!       file maps every interface event to the sound file that plays
//!       for it, next to the audio files themselves.
//!   ~/.local/share/ng-term/look/<theme>/  — complete themes:
//!       meta        — metafile with a Name= field (name used in ng-term.conf)
//!       *.css       — symlink into ../../style/
//!       *.layaut    — optional; without it the adaptive default is used
//!       *.sounds    — optional directory symlink into ../../sounds/;
//!                     without it the "default" set is used
//!
//! Configurations written before this split keep a themes/ directory in
//! the config directory; it is moved into the data directory on startup.
//!
//! EVERY layout is computed from the ACTUAL window size every frame (see
//! src/flex.rs), so resizing or moving the window reflows the interface
//! live. Layout files come in two formats:
//!
//! Flexbox (recommended) — CSS-like columns, same engine as the built-in
//! default (min/max widths, collapse priorities, portrait restack):
//!   [column]            # columns are laid out left to right
//!   basis = 16.4        # preferred width, % of the row (flex-basis)
//!   min = 168           # min-width in px
//!   max = 340           # max-width in px (omit = unlimited)
//!   grow = 0            # share of leftover space (flex-grow)
//!   collapse = 2        # 1 disappears first when space runs out; 0 = never
//!   gap = 2.5           # vertical gap between panels (weight units)
//!   panel = left_col 59.5   # panels top->bottom with height weights
//!   panel = control 32.5
//! Panels: clock, sysinfo, hardware, cpu, memory, processes, shell,
//! network, filesystem, keyboard, control.
//!
//! Legacy — "<panel> = x y w h" percentages at the 16:9 reference,
//! re-adapted to the window continuously (edge-anchored transform on
//! landscape, a vertical restack on portrait).
//!
//! In ng-term.conf the Look=<name> option picks a complete theme by the
//! metafile's Name= field. The Style=, Layaut= and Sounds= options name
//! components: files from style/ and layauts/ (without extensions) and a
//! directory from sounds/. Empty values or missing options = defaults
//! built into the code.

use crate::theme::{Color, Theme};
use crate::widgets::{FlexColumn, FlexLayaut, LayoutMode, LayoutSpec, Panel, PanelSpec, WidgetDef};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// A per-resolution override: panel rectangles saved ONLY for a given
/// screen resolution and diagonal ("[1920x1080@27]" sections of a
/// .layaut file). Contains just the panels that were changed.
#[derive(Clone)]
pub struct ResOverride {
    pub w: u32,
    pub h: u32,
    pub diag: u32,
    pub panels: Vec<(Panel, PanelSpec)>,
}

/// A complete layout definition: the base (built-in flex, a custom flex
/// description or a legacy fixed spec) plus the per-resolution override
/// sections from the same .layaut file.
#[derive(Clone, Default)]
pub struct LayoutDef {
    pub base: LayoutMode,
    pub overrides: Vec<ResOverride>,
}

impl LayoutDef {
    fn from_base(base: LayoutMode) -> Self {
        LayoutDef { base, overrides: Vec::new() }
    }

    /// The override matching the given screen (resolution + diagonal).
    pub fn pick(&self, key: (u32, u32, u32)) -> Option<&ResOverride> {
        self.overrides
            .iter()
            .find(|o| (o.w, o.h, o.diag) == key)
    }
}

pub struct Config {
    pub theme: Theme,
    pub layout: LayoutDef,
}

/// Theme found in the data directory (name from the metafile).
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


/// Directory with complete themes: ~/.local/share/ng-term/look
fn look_dir() -> PathBuf {
    data_dir().join("look")
}

/// Scans the look/ directory; returns entries with a valid metafile (Name=).
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
    // The registry must exist before anything parses a layout: panels
    // are resolved by name against it.
    ng::base::set_registry(load_widget_registry());
    resolve()
}

/// Layout by name: a custom file from layauts/ — flexbox format
/// ([column] sections) or the legacy fixed format — or, for "default"
/// (when no such file exists), the built-in flexbox layout (src/flex.rs).
fn layaut_by_name(name: &str) -> Option<LayoutDef> {
    if let Ok(text) = std::fs::read_to_string(layauts_dir().join(format!("{name}.layaut"))) {
        return Some(parse_layaut_file(&text, name));
    }
    if name == "default" {
        return Some(LayoutDef::from_base(LayoutMode::Flex));
    }
    None
}

/// Header of a per-resolution override section: "[1920x1080@27]".
fn parse_res_header(line: &str) -> Option<(u32, u32, u32)> {
    let inner = line.strip_prefix('[')?.strip_suffix(']')?;
    let (res, diag) = inner.split_once('@')?;
    let (w, h) = res.split_once('x')?;
    Some((
        w.trim().parse().ok()?,
        h.trim().parse().ok()?,
        diag.trim().parse().ok()?,
    ))
}

/// Splits a .layaut file into its base text and the per-resolution
/// override sections (everything after the first "[WxH@D]" header).
fn split_layaut_sections(text: &str) -> (String, Vec<ResOverride>) {
    let mut base = String::new();
    let mut sections: Vec<ResOverride> = Vec::new();
    let mut current: Option<ResOverride> = None;
    for line in text.lines() {
        let trimmed = line.split('#').next().unwrap_or("").trim();
        if let Some((w, h, diag)) = parse_res_header(trimmed) {
            if let Some(sec) = current.take() {
                sections.push(sec);
            }
            current = Some(ResOverride { w, h, diag, panels: Vec::new() });
            continue;
        }
        match current.as_mut() {
            None => {
                base.push_str(line);
                base.push('\n');
            }
            Some(sec) => {
                let Some((k, v)) = trimmed.split_once('=') else { continue };
                let nums: Vec<f32> = v
                    .split_whitespace()
                    .filter_map(|t| t.parse::<f32>().ok().filter(|x| x.is_finite()))
                    .collect();
                if nums.len() != 4 {
                    continue;
                }
                if let Some(panel) = Panel::from_name(k.trim()) {
                    sec.panels.retain(|(p, _)| *p != panel);
                    sec.panels.push((
                        panel,
                        PanelSpec { x: nums[0], y: nums[1], w: nums[2], h: nums[3] },
                    ));
                }
            }
        }
    }
    if let Some(sec) = current.take() {
        sections.push(sec);
    }
    (base, sections)
}

/// Parses a complete .layaut file: the base (flex or legacy format; an
/// empty base means the built-in default) plus the resolution sections.
fn parse_layaut_file(text: &str, name: &str) -> LayoutDef {
    let (base_text, overrides) = split_layaut_sections(text);
    let has_panel_lines = base_text.lines().any(|l| {
        let t = l.split('#').next().unwrap_or("").trim();
        t.split_once('=')
            .and_then(|(k, _)| Panel::from_name(k.trim()))
            .is_some()
    });
    let base = if base_text.contains("[column]") {
        match parse_flex_layaut(&base_text) {
            Some(fl) => LayoutMode::Custom(fl),
            None => {
                eprintln!(
                    "ng-term: no valid columns in '{name}.layaut' — using the default layout"
                );
                LayoutMode::Flex
            }
        }
    } else if has_panel_lines {
        LayoutMode::Fixed(parse_layaut(&base_text))
    } else {
        // Overrides only: the built-in default is the base.
        LayoutMode::Flex
    };
    LayoutDef { base, overrides }
}

/// Parses the flexbox .layaut format (see the module header).
fn parse_flex_layaut(src: &str) -> Option<FlexLayaut> {
    let mut columns: Vec<FlexColumn> = Vec::new();
    let mut cur: Option<FlexColumn> = None;
    for line in src.lines() {
        let line = line.split('#').next().unwrap_or("").trim();
        if line.is_empty() {
            continue;
        }
        if line.eq_ignore_ascii_case("[column]") {
            if let Some(c) = cur.take() {
                columns.push(c);
            }
            cur = Some(FlexColumn {
                basis: 20.0,
                min: 60.0,
                max: f32::INFINITY,
                grow: 0.0,
                collapse: 0,
                gap: 2.5,
                panels: Vec::new(),
            });
            continue;
        }
        let Some(c) = cur.as_mut() else { continue };
        let Some((k, v)) = line.split_once('=') else { continue };
        let (k, v) = (k.trim(), v.trim());
        // Reject non-finite ("nan"/"inf") so they never reach the layout
        // engine, where NaN would produce off-screen/garbled geometry.
        let num = |v: &str| {
            v.trim_end_matches(['%', 'p', 'x'])
                .trim()
                .parse::<f32>()
                .ok()
                .filter(|x| x.is_finite())
        };
        match k {
            "basis" => c.basis = num(v).unwrap_or(c.basis),
            "min" => c.min = num(v).unwrap_or(c.min),
            "max" => c.max = num(v).unwrap_or(c.max),
            "grow" => c.grow = num(v).unwrap_or(c.grow),
            "collapse" => c.collapse = num(v).unwrap_or(0.0) as u32,
            "gap" => c.gap = num(v).unwrap_or(c.gap),
            "panel" => {
                let mut it = v.split_whitespace();
                let name = it.next().unwrap_or("");
                let weight = it
                    .next()
                    .and_then(|t| t.parse::<f32>().ok())
                    .filter(|x| x.is_finite())
                    .unwrap_or(50.0);
                match Panel::from_name(name) {
                    Some(p) => c.panels.push((p, weight.max(1.0))),
                    None => eprintln!("ng-term: unknown panel in .layaut: {name}"),
                }
            }
            other => eprintln!("ng-term: unknown option in .layaut: {other}"),
        }
    }
    if let Some(c) = cur.take() {
        columns.push(c);
    }
    columns.retain(|c| !c.panels.is_empty());
    if columns.is_empty() {
        None
    } else {
        Some(FlexLayaut { columns })
    }
}

/// The complete default theme: the default style plus the built-in
/// responsive default layout, which adapts itself to any window — so
/// theme authors never have to provide size variants.
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

/// Clears the component options (a complete look was selected).
pub fn clear_component_options() {
    set_conf_kv("Style", "");
    set_conf_kv("Layaut", "");
    set_conf_kv("Sounds", "");
}

/// The component names a look is composed of, read from its symlinks:
/// (style, layaut, sounds).
fn look_components(dir: &Path) -> (Option<String>, Option<String>, Option<String>) {
    let mut style = None;
    let mut layaut = None;
    let mut sounds = None;
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
                // The sounds component is a whole directory, so the
                // symlink target's final component is the theme name.
                Some("sounds") => sounds = stem,
                _ => {}
            }
        }
    }
    // A look without a layout or sounds symlink uses the defaults.
    (
        style,
        layaut.or_else(|| Some("default".to_string())),
        sounds.or_else(|| Some("default".to_string())),
    )
}

/// Finds a look whose components are exactly (style, layaut, sounds).
fn find_matching_look(style: &str, layaut: &str, sounds: &str) -> Option<String> {
    for info in list_themes() {
        let (s, l, snd) = look_components(&info.dir);
        if s.as_deref() == Some(style)
            && l.as_deref() == Some(layaut)
            && snd.as_deref() == Some(sounds)
        {
            return Some(info.name);
        }
    }
    None
}

/// Effective component names (style, layaut, sounds) implied by the
/// current configuration — for a selected look these are its symlink
/// targets, in component mode the Style=/Layaut=/Sounds= values (a
/// missing one falls back to "default"), and with nothing set all three
/// are "default".
pub fn effective_components() -> (Option<String>, Option<String>, Option<String>) {
    if let Some(name) = current_theme_name() {
        if let Some(info) = list_themes().into_iter().find(|t| t.name == name) {
            return look_components(&info.dir);
        }
        return (None, None, None);
    }
    let s = current_style_name();
    let l = current_layaut_name();
    let snd = current_sounds_name();
    if s.is_none() && l.is_none() && snd.is_none() {
        return (
            Some("default".into()),
            Some("default".into()),
            Some("default".into()),
        );
    }
    (
        Some(s.unwrap_or_else(|| "default".into())),
        Some(l.unwrap_or_else(|| "default".into())),
        Some(snd.unwrap_or_else(|| "default".into())),
    )
}

/// If the effective Style=/Layaut=/Sounds= triple (a missing one falls
/// back to "default") matches some look, rewrites ng-term.conf so that
/// only Look= is set and returns the look name.
pub fn canonicalize_components() -> Option<String> {
    if current_theme_name().is_some() {
        return None;
    }
    let style_set = current_style_name();
    let layaut_set = current_layaut_name();
    let sounds_set = current_sounds_name();
    if style_set.is_none() && layaut_set.is_none() && sounds_set.is_none() {
        return None;
    }
    let s = style_set.unwrap_or_else(|| "default".into());
    let l = layaut_set.unwrap_or_else(|| "default".into());
    let snd = sounds_set.unwrap_or_else(|| "default".into());
    let look = find_matching_look(&s, &l, &snd)?;
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

/// Accepts a config value only if it is a single safe path component
/// (no separators, not "..", not absolute) — so Look=/Style=/Layaut=
/// values joined into data-directory paths cannot escape it.
fn safe_component(name: &str) -> Option<String> {
    let n = name.trim();
    if n.is_empty() || n == "." || n == ".." {
        return None;
    }
    if n.contains('/') || n.contains('\\') || n.contains('\0') {
        return None;
    }
    // Must be exactly one normal path component.
    let mut comps = Path::new(n).components();
    match (comps.next(), comps.next()) {
        (Some(std::path::Component::Normal(c)), None) if c == n => Some(n.to_string()),
        _ => None,
    }
}

/// Current Look= value from ng-term.conf (if a safe, non-empty name).
pub fn current_theme_name() -> Option<String> {
    let text = std::fs::read_to_string(config_dir().join("ng-term.conf")).ok()?;
    let kv = parse_kv(&text);
    kv.get("Look").and_then(|s| safe_component(s))
}

/// Saves the look choice to ng-term.conf, preserving the rest of the file.
pub fn set_theme_option(name: &str) {
    set_conf_kv("Look", name);
}

fn conf_kv() -> HashMap<String, String> {
    parse_kv(&std::fs::read_to_string(config_dir().join("ng-term.conf")).unwrap_or_default())
}

/// Directory with shared styles: ~/.local/share/ng-term/style
fn style_dir() -> PathBuf {
    data_dir().join("style")
}

/// Directory with custom layouts: ~/.local/share/ng-term/layauts
fn layauts_dir() -> PathBuf {
    data_dir().join("layauts")
}

/// Directory with widget descriptions: ~/.local/share/ng-term/widgets
fn widgets_dir() -> PathBuf {
    data_dir().join("widgets")
}

/// Builds the widget registry by scanning the widgets directory. Each
/// subdirectory holding a "widget" file is one widget, and the DIRECTORY
/// NAME is its name — so adding a widget means adding a directory, and
/// removing one removes it from the program.
///
/// A directory that yields nothing usable falls back to the built-in
/// set, so the program can never end up with no widgets at all.
pub fn load_widget_registry() -> Vec<WidgetDef> {
    let out = scan_widget_dir(&widgets_dir());
    if !out.is_empty() {
        eprintln!(
            "ng-term: {} widgets from {}",
            out.len(),
            widgets_dir().display()
        );
        return out;
    }
    if out.is_empty() {
        eprintln!(
            "ng-term: no widgets found in {} \u{2014} using the built-in set",
            widgets_dir().display()
        );
        return ng::base::builtin_widgets();
    }
    out
}

/// The script of a widget, if its directory holds one. A widget is a
/// directory with `<name>.rhai` in it, alongside whatever assets the
/// widget needs — that file IS the widget.
pub fn widget_script(name: &str) -> Option<ng::script::Script> {
    let name = safe_component(name)?;
    let path = widgets_dir().join(&name).join(format!("{name}.rhai"));
    path.is_file()
        .then(|| ng::script::Script::load(&path))
        .flatten()
}

/// The block description of a widget, if its file carries one. A file
/// with blocks describes how the widget draws; one without leaves that
/// to the compiled renderer registered under the same name.
pub fn widget_desc(name: &str) -> Option<ng::desc::Desc> {
    let name = safe_component(name)?;
    let text = std::fs::read_to_string(widgets_dir().join(name).join("widget")).ok()?;
    let desc = ng::desc::Desc::parse(&text);
    (!desc.is_empty()).then_some(desc)
}

/// The scan itself, over an explicit directory.
fn scan_widget_dir(dir: &Path) -> Vec<WidgetDef> {
    let mut out: Vec<WidgetDef> = Vec::new();
    if let Ok(rd) = std::fs::read_dir(dir) {
        let mut dirs: Vec<PathBuf> = rd
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.is_dir())
            .collect();
        // Sorted, so panel order does not depend on the filesystem.
        dirs.sort();
        for dir in dirs {
            let Some(name) = dir.file_name().and_then(|n| n.to_str()) else { continue };
            let Some(name) = safe_component(name) else { continue };
            // A directory is a widget when it holds its script, or the
            // metadata file the compiled widgets still use. The script
            // is the widget; the metadata file only carries what the
            // layout engine needs about widgets that have no script.
            let script = dir.join(format!("{name}.rhai"));
            let meta = dir.join("widget");
            if !script.is_file() && !meta.is_file() {
                continue;
            }
            let text = std::fs::read_to_string(&meta).unwrap_or_default();
            let kv = parse_kv(&text);
            let num = |key: &str, def: f32| {
                kv.get(key)
                    .and_then(|v| v.trim().parse::<f32>().ok())
                    .filter(|x| x.is_finite() && *x > 0.0)
                    .unwrap_or(def)
            };
            let label = kv
                .get("Label")
                .filter(|l| !l.is_empty())
                .cloned()
                .unwrap_or_else(|| name.to_uppercase());
            let builtin = kv
                .get("Builtin")
                .map(|b| b.trim())
                .filter(|b| !b.is_empty())
                .map(|b| b.to_string());
            out.push(WidgetDef {
                name,
                label,
                ref_h_vh: num("RefHeight", 10.0),
                min_h_vh: num("MinHeight", 6.0),
                builtin,
            });
        }
    }
    out
}

/// Writes a description file for every built-in widget that is MISSING,
/// on the same terms as the styles and sounds: presence by name only, so
/// an edited file stays the user's and a deleted one comes back.
fn restore_builtin_widgets(dir: &Path) {
    for def in ng::base::builtin_widgets() {
        let wd = dir.join(&def.name);
        if std::fs::create_dir_all(&wd).is_err() {
            continue;
        }
        let file = wd.join("widget");
        if file.exists() {
            continue;
        }
        let builtin = match &def.builtin {
            Some(b) => format!("Builtin={b}\n"),
            None => String::new(),
        };
        let _ = std::fs::write(
            &file,
            format!(
                "# ng-term widget.\n\
                 #\n\
                 # The directory name is the widget's name \u{2014} the name used in\n\
                 # .layaut files. Removing this directory removes the widget from\n\
                 # the program; copying it under a new name adds one.\n\
                 #\n\
                 # Label      shown in the layout editor\n\
                 # RefHeight  height (vh) at which it renders at 100% scale\n\
                 # MinHeight  minimum content height (vh) kept by the layout engine\n\
                 \n\
                 Label={}\n\
                 RefHeight={}\n\
                 MinHeight={}\n\
                 {}",
                def.label, def.ref_h_vh, def.min_h_vh, builtin
            ),
        );
    }
}

/// Directory with sound themes: ~/.local/share/ng-term/sounds
fn sounds_dir() -> PathBuf {
    data_dir().join("sounds")
}

/// Sound theme names — the subdirectories of sounds/ that carry a
/// metafile. Unlike looks, the DIRECTORY NAME is the theme name.
pub fn list_sound_themes() -> Vec<String> {
    let mut out = Vec::new();
    if let Ok(rd) = std::fs::read_dir(sounds_dir()) {
        for entry in rd.flatten() {
            let p = entry.path();
            if !p.is_dir() || read_meta(&p).is_none() {
                continue;
            }
            if let Some(name) = p.file_name().and_then(|s| s.to_str()) {
                out.push(name.to_string());
            }
        }
    }
    out.sort();
    out
}

/// Directory of the sound theme the current configuration selects —
/// from Sounds=, or from the chosen look's *.sounds symlink. None when
/// it names a set that is not installed.
pub fn active_sounds_dir() -> Option<PathBuf> {
    let (_, _, sounds) = effective_components();
    let name = safe_component(&sounds?)?;
    let dir = sounds_dir().join(name);
    dir.is_dir().then_some(dir)
}

/// Current Sounds= value from ng-term.conf (if a safe, non-empty name).
pub fn current_sounds_name() -> Option<String> {
    conf_kv().get("Sounds").and_then(|s| safe_component(s))
}

pub fn set_sounds_option(name: &str) {
    set_conf_kv("Sounds", name);
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

/// Style names available in style/ (no extensions).
pub fn list_styles() -> Vec<String> {
    list_stems(&style_dir(), "css")
}

/// Layout names: the adaptive "default" plus custom files in layauts/.
pub fn list_layauts() -> Vec<String> {
    let mut out = vec!["default".to_string()];
    for stem in list_stems(&layauts_dir(), "layaut") {
        if stem != "default" {
            out.push(stem);
        }
    }
    out
}

/// Current Style= value from ng-term.conf (if a safe, non-empty name).
pub fn current_style_name() -> Option<String> {
    conf_kv().get("Style").and_then(|s| safe_component(s))
}

/// Current Layaut= value from ng-term.conf (if a safe, non-empty name).
pub fn current_layaut_name() -> Option<String> {
    conf_kv().get("Layaut").and_then(|s| safe_component(s))
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

/// Sound preferences: (master volume 0-100, typing sounds, ambient bed).
/// Everything on by default — a fresh install should be heard.
pub fn sound_prefs() -> (u32, bool, bool) {
    let kv = conf_kv();
    let volume = kv
        .get("SoundVolume")
        .and_then(|v| v.trim().parse::<u32>().ok())
        .map(|v| v.min(100))
        .unwrap_or(100);
    let flag = |key: &str| {
        kv.get(key)
            .map(|v| v.trim() != "0")
            .unwrap_or(true)
    };
    (volume, flag("SoundTyping"), flag("SoundAmbient"))
}

pub fn set_sound_volume(percent: u32) {
    set_conf_kv("SoundVolume", &percent.min(100).to_string());
}

pub fn set_sound_typing(on: bool) {
    set_conf_kv("SoundTyping", if on { "1" } else { "0" });
}

pub fn set_sound_ambient(on: bool) {
    set_conf_kv("SoundAmbient", if on { "1" } else { "0" });
}

/// Grid editor preferences: (snap to grid, columns, rows, widget padding px).
pub fn grid_prefs() -> (bool, u32, u32, u32) {
    let kv = conf_kv();
    let num = |key: &str, def: u32| {
        kv.get(key)
            .and_then(|v| v.trim().parse::<u32>().ok())
            .unwrap_or(def)
            .clamp(2, 64)
    };
    // Snap is opt-in (off by default).
    let snap = kv
        .get("GridSnap")
        .map(|v| v.trim() == "1" || v.trim().eq_ignore_ascii_case("true"))
        .unwrap_or(false);
    let pad = kv
        .get("GridPadding")
        .and_then(|v| v.trim().parse::<u32>().ok())
        .unwrap_or(8)
        .min(40);
    (snap, num("GridCols", 12), num("GridRows", 8), pad)
}

pub fn set_grid_snap(on: bool) {
    set_conf_kv("GridSnap", if on { "1" } else { "0" });
}

pub fn set_grid_cols(n: u32) {
    set_conf_kv("GridCols", &n.to_string());
}

pub fn set_grid_rows(n: u32) {
    set_conf_kv("GridRows", &n.to_string());
}

pub fn set_grid_padding(n: u32) {
    set_conf_kv("GridPadding", &n.to_string());
}

/// Selects a layout by name, with the standard component rules (the
/// missing style falls back to "default", Look= is cleared and the pair
/// is canonicalized back to a look when it matches one).
pub fn select_layaut(name: &str) {
    set_layaut_option(name);
    if current_style_name().is_none() {
        set_style_option("default");
    }
    clear_look_option();
    canonicalize_components();
}

/// The screen key recorded in a file's base ("screen = 1920x1080@27").
fn base_screen_of(base_text: &str) -> Option<(u32, u32, u32)> {
    for line in base_text.lines() {
        let t = line.split('#').next().unwrap_or("").trim();
        if let Some((k, v)) = t.split_once('=') {
            if k.trim() == "screen" {
                return parse_res_header(&format!("[{}]", v.trim()));
            }
        }
    }
    None
}

/// Serializes a FULL layout as the base of a .layaut file, recording
/// the screen it was created on.
fn serialize_base(spec: &LayoutSpec, key: (u32, u32, u32)) -> String {
    let mut out = String::from(
        "# ng-term layout saved from the grid editor.\n\
         # Format: <panel> = x y w h (percent of the window).\n",
    );
    out.push_str(&format!("screen = {}x{}@{}\n", key.0, key.1, key.2));
    for panel in Panel::all() {
        let ps = spec.p(panel);
        out.push_str(&format!(
            "{} = {:.2} {:.2} {:.2} {:.2}\n",
            panel.name(),
            ps.x,
            ps.y,
            ps.w,
            ps.h
        ));
    }
    out
}

fn serialize_sections(out: &mut String, sections: &[ResOverride]) {
    for sec in sections {
        out.push('\n');
        out.push_str(&format!("[{}x{}@{}]\n", sec.w, sec.h, sec.diag));
        for (panel, ps) in &sec.panels {
            out.push_str(&format!(
                "{} = {:.2} {:.2} {:.2} {:.2}\n",
                panel.name(),
                ps.x,
                ps.y,
                ps.w,
                ps.h
            ));
        }
    }
}

/// SAVE AS: writes ALL panels into the MAIN section (the base) of a new
/// .layaut file, recording the screen it was created on. Any previous
/// content of the file is replaced.
pub fn save_layaut_full(
    name: &str,
    spec: &LayoutSpec,
    key: (u32, u32, u32),
) -> std::io::Result<()> {
    let dir = layauts_dir();
    std::fs::create_dir_all(&dir)?;
    std::fs::write(dir.join(format!("{name}.layaut")), serialize_base(spec, key))
}

/// SAVE: on the screen the base was created on, the base itself is
/// rewritten with the full layout; on ANY OTHER screen only the changed
/// panels are written into that screen's "[WxH@D]" section. The rest of
/// the file always stays untouched.
pub fn save_layaut_overrides(
    name: &str,
    key: (u32, u32, u32),
    changes: &[(Panel, PanelSpec)],
    full: &LayoutSpec,
) -> std::io::Result<()> {
    let dir = layauts_dir();
    std::fs::create_dir_all(&dir)?;
    let path = dir.join(format!("{name}.layaut"));
    let text = std::fs::read_to_string(&path).unwrap_or_default();
    let (base, mut sections) = split_layaut_sections(&text);

    if base_screen_of(&base) == Some(key) {
        // Editing on the base's own screen: rewrite the base in full.
        let mut out = serialize_base(full, key);
        serialize_sections(&mut out, &sections);
        return std::fs::write(path, out);
    }

    // Another screen: merge the changes into its section.
    let sec = match sections
        .iter_mut()
        .find(|o| (o.w, o.h, o.diag) == key)
    {
        Some(s) => s,
        None => {
            sections.push(ResOverride {
                w: key.0,
                h: key.1,
                diag: key.2,
                panels: Vec::new(),
            });
            sections.last_mut().unwrap()
        }
    };
    for (panel, spec) in changes {
        sec.panels.retain(|(p, _)| p != panel);
        sec.panels.push((*panel, *spec));
    }

    let mut out = String::new();
    let base_trim = base.trim_end();
    if !base_trim.is_empty() {
        out.push_str(base_trim);
        out.push('\n');
    } else {
        out.push_str(
            "# ng-term layout: per-screen overrides on top of the default layout.\n",
        );
    }
    serialize_sections(&mut out, &sections);
    std::fs::write(path, out)
}

/// Screen diagonal in inches of the monitor with the given connector
/// name (EDID bytes 21/22, physical size in cm); 0 = unknown.
pub fn monitor_diag_inches(monitor_name: &str) -> u32 {
    let connector = monitor_name
        .split_whitespace()
        .next()
        .unwrap_or(monitor_name);
    let suffix = format!("-{connector}");
    let Some(dir) = std::fs::read_dir("/sys/class/drm")
        .ok()
        .and_then(|rd| {
            rd.flatten().map(|e| e.path()).find(|p| {
                p.file_name()
                    .map(|n| n.to_string_lossy().ends_with(&suffix))
                    .unwrap_or(false)
            })
        })
    else {
        return 0;
    };
    let Ok(edid) = std::fs::read(dir.join("edid")) else { return 0 };
    if edid.len() >= 23 {
        let w = edid[21] as f32;
        let h = edid[22] as f32;
        if w > 0.0 && h > 0.0 {
            return ((w * w + h * h).sqrt() / 2.54).round() as u32;
        }
    }
    0
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

/// Data directory: ~/.local/share/ng-term. Holds everything a theme is
/// made of (look/, style/, layauts/, sounds/) — those are data, not
/// configuration, so they belong under XDG_DATA_HOME while ng-term.conf
/// stays in the config directory.
fn data_dir() -> PathBuf {
    if let Ok(xdg) = std::env::var("XDG_DATA_HOME") {
        if !xdg.is_empty() {
            return PathBuf::from(xdg).join("ng-term");
        }
    }
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".into());
    PathBuf::from(home).join(".local").join("share").join("ng-term")
}

/// Creates the config directory and ng-term.conf, moves a pre-split
/// themes/ tree into the data directory, creates the data tree (look/,
/// style/, layauts/, sounds/) and, on first run, the built-in looks.
/// Sound themes are seeded on every run, so a configuration made before
/// they existed gains them.
fn init_tree(dir: &Path) {
    let data = data_dir();
    migrate_themes_dir(&dir.join("themes"), &data);

    let look = data.join("look");
    let style = data.join("style");
    let layauts = data.join("layauts");
    let sounds = data.join("sounds");
    let widgets = data.join("widgets");
    for d in [&look, &style, &layauts, &sounds, &widgets] {
        if let Err(e) = std::fs::create_dir_all(d) {
            eprintln!("ng-term: cannot create {}: {e}", d.display());
            return;
        }
    }
    if let Err(e) = std::fs::create_dir_all(dir) {
        eprintln!("ng-term: cannot create {}: {e}", dir.display());
        return;
    }

    let conf = dir.join("ng-term.conf");
    if !conf.exists() {
        let _ = std::fs::write(
            &conf,
            "# ng-term configuration\n\
             #\n\
             # Look=<name>    — picks a complete theme by the Name= field of the\n\
             #                  metafile ~/.local/share/ng-term/look/<dir>/meta\n\
             # Style=<name>   — .css from ~/.local/share/ng-term/style (no extension)\n\
             # Layaut=<name>  — .layaut from ~/.local/share/ng-term/layauts\n\
             # Sounds=<name>  — sound theme directory from ~/.local/share/ng-term/sounds\n\
             # TermFont*/UIFont*  — terminal / interface font settings:\n\
             #   *FontSize=<percent> — Term: 50-200, UI: 75-125\n\
             #   *FontFamily=<name>  — family from the settings list\n\
             #   *FontWeight=<name>  — Light/Regular/Medium/SemiBold/Bold\n\
             # Empty values or missing options = defaults built into the program.\n\
             Look=\n\
             Style=\n\
             Layaut=\n\
             Sounds=\n\
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

    restore_builtin_themes(&look, &style);
    restore_builtin_widgets(&widgets);
    seed_sound_themes(&sounds);
    link_default_sounds(&look, &sounds);
}

/// Writes every built-in style and look file that is MISSING, on every
/// start. The test is presence by name only — content is never compared,
/// so an edited file stays the user's, while a deleted one is treated as
/// a gap to fill. The point is that no sequence of deletions can leave
/// the program with no themes at all.
///
/// These are generated rather than copied from the installed share/
/// directory on purpose: a style is a dozen lines of text, so producing
/// it from the palette table keeps the guarantee even when nothing at
/// all was installed alongside the binary.
fn restore_builtin_themes(look: &Path, style: &Path) {
    for (name, r, g, b, bg, grey, fg, tbg, cur) in BUILTIN_THEMES {
        // The default style is tron's palette under the name "default".
        let style_file = if name == "tron" { "default" } else { name };
        let css = style.join(format!("{style_file}.css"));
        if !css.exists() {
            let _ = std::fs::write(
                &css,
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
        }
        // A complete look: metafile + style symlink; the layout is the
        // adaptive default (no layout symlink needed).
        let dir = look.join(name);
        if std::fs::create_dir_all(&dir).is_err() {
            continue;
        }
        let meta = dir.join("meta");
        if !meta.exists() {
            let _ = std::fs::write(
                &meta,
                format!("Name={name}\nDescription=eDEX-UI '{name}' theme (original colors)\n"),
            );
        }
        // symlink_metadata(), so a link whose target is momentarily
        // missing counts as present and is not replaced — the css write
        // above is what repairs it.
        let link = dir.join(format!("{name}.css"));
        if link.symlink_metadata().is_err() {
            let _ = std::os::unix::fs::symlink(
                format!("../../style/{style_file}.css"),
                &link,
            );
        }
    }
}

/// Moves a pre-split ~/.config/ng-term/themes tree into the data
/// directory and removes it. Entries that already exist on the data side
/// are left where they are rather than overwritten, and anything that
/// could not be moved keeps the old directory alive so nothing is ever
/// silently lost.
fn migrate_themes_dir(old: &Path, data: &Path) {
    if !old.is_dir() {
        return;
    }
    if let Err(e) = std::fs::create_dir_all(data) {
        eprintln!("ng-term: cannot create {}: {e}", data.display());
        return;
    }
    let Ok(rd) = std::fs::read_dir(old) else { return };
    for entry in rd.flatten() {
        let from = entry.path();
        let to = data.join(entry.file_name());
        if !move_merge(&from, &to) {
            eprintln!("ng-term: cannot move {} to {}", from.display(), to.display());
        }
    }
    // Only succeeds once the directory is empty — exactly the condition
    // under which dropping it is safe.
    if std::fs::remove_dir(old).is_ok() {
        eprintln!(
            "ng-term: themes moved from {} to {}",
            old.display(),
            data.display()
        );
    }
}

/// Moves `from` onto `to`, merging directory into directory so that a
/// name already taken on the destination side costs only that one entry
/// instead of its whole subtree. Returns whether `from` was consumed.
fn move_merge(from: &Path, to: &Path) -> bool {
    let dest = std::fs::symlink_metadata(to);
    if dest.is_err() {
        // Free name: a rename keeps symlinks (and their relative
        // targets) intact; the copy fallback is only needed for a
        // cross-filesystem move.
        if std::fs::rename(from, to).is_ok() {
            return true;
        }
        if copy_tree(from, to).is_ok() {
            let _ = std::fs::remove_dir_all(from);
            return true;
        }
        return false;
    }
    let src_is_dir = std::fs::symlink_metadata(from)
        .map(|m| m.is_dir())
        .unwrap_or(false);
    if !(src_is_dir && dest.map(|m| m.is_dir()).unwrap_or(false)) {
        // The destination exists and is not a directory to merge into —
        // whatever is already there wins.
        return false;
    }
    let Ok(rd) = std::fs::read_dir(from) else { return false };
    let mut all = true;
    for entry in rd.flatten() {
        all &= move_merge(&entry.path(), &to.join(entry.file_name()));
    }
    all && std::fs::remove_dir(from).is_ok()
}

/// Recursive copy that reproduces symlinks as symlinks instead of
/// following them — a look's *.css/*.layaut/*.sounds entries are
/// relative links and must stay that way.
fn copy_tree(from: &Path, to: &Path) -> std::io::Result<()> {
    let meta = std::fs::symlink_metadata(from)?;
    if meta.file_type().is_symlink() {
        let target = std::fs::read_link(from)?;
        return std::os::unix::fs::symlink(target, to);
    }
    if meta.is_dir() {
        std::fs::create_dir_all(to)?;
        for entry in std::fs::read_dir(from)?.flatten() {
            copy_tree(&entry.path(), &to.join(entry.file_name()))?;
        }
        return Ok(());
    }
    std::fs::copy(from, to).map(|_| ())
}

/// Copies the shipped sound themes into sounds/, restoring anything
/// MISSING file by file. A file that is already there is never touched
/// regardless of its contents, so replacing a sound with your own keeps
/// it; deleting one brings the original back on the next start.
fn seed_sound_themes(sounds: &Path) {
    let Some(src) = installed_sounds_dir() else { return };
    // A ~/.local install puts the shipped sets in the data directory
    // itself, so source and destination are one and the same and there
    // is nothing to do. Comparing the Options directly would be wrong:
    // two failed canonicalizations are not a match.
    let same = match (std::fs::canonicalize(&src), std::fs::canonicalize(sounds)) {
        (Ok(a), Ok(b)) => a == b,
        _ => src == *sounds,
    };
    if same {
        return;
    }
    restore_missing_sounds(&src, sounds);
}

/// Per-file restore of every shipped sound theme found in `src`.
fn restore_missing_sounds(src: &Path, sounds: &Path) {
    let Ok(rd) = std::fs::read_dir(src) else { return };
    for entry in rd.flatten() {
        let from = entry.path();
        if !from.is_dir() {
            continue;
        }
        let Some(name) = from.file_name() else { continue };
        let to = sounds.join(name);
        if std::fs::create_dir_all(&to).is_err() {
            continue;
        }
        let Ok(files) = std::fs::read_dir(&from) else { continue };
        for f in files.flatten() {
            let p = f.path();
            if !p.is_file() {
                continue;
            }
            let Some(fname) = p.file_name() else { continue };
            let dest = to.join(fname);
            // Presence by name only: whatever is already there is the
            // user's and stays untouched, whatever its contents.
            if !dest.exists() {
                let _ = std::fs::copy(&p, &dest);
            }
        }
    }
}

/// Gives every look that has no sounds symlink the "default" set, so an
/// existing look/ tree gains sounds without being recreated.
fn link_default_sounds(look: &Path, sounds: &Path) {
    if !sounds.join("default").is_dir() {
        return;
    }
    let Ok(rd) = std::fs::read_dir(look) else { return };
    for entry in rd.flatten() {
        let dir = entry.path();
        if !dir.is_dir() {
            continue;
        }
        // look_components() reports a missing symlink as "default", so
        // the presence of a *.sounds entry is what has to be checked.
        let has_sounds = std::fs::read_dir(&dir)
            .map(|rd| {
                rd.flatten().any(|e| {
                    e.path()
                        .extension()
                        .and_then(|x| x.to_str())
                        .map(|x| x.eq_ignore_ascii_case("sounds"))
                        .unwrap_or(false)
                })
            })
            .unwrap_or(false);
        if has_sounds {
            continue;
        }
        let Some(name) = dir.file_name().and_then(|s| s.to_str()) else { continue };
        let _ = std::os::unix::fs::symlink(
            "../../sounds/default",
            dir.join(format!("{name}.sounds")),
        );
    }
}

/// Directory holding the sound themes shipped with the program:
/// $PREFIX/share/ng-term/sounds, or ./assets/sounds when running from a
/// source checkout.
///
/// Installed into ~/.local this is the data directory itself, so there
/// is nothing to copy and seeding stops at the identity check in
/// seed_sound_themes(). A system install puts it under /usr, from where
/// the sets are copied into the user's data directory on first run.
fn installed_sounds_dir() -> Option<PathBuf> {
    let mut candidates: Vec<PathBuf> = Vec::new();
    // Next to the binary: <prefix>/bin/ng-term -> <prefix>/share/ng-term.
    if let Ok(exe) = std::env::current_exe() {
        if let Some(prefix) = exe.parent().and_then(|p| p.parent()) {
            candidates.push(prefix.join("share").join("ng-term").join("sounds"));
        }
    }
    candidates.push(PathBuf::from("/usr/local/share/ng-term/sounds"));
    candidates.push(PathBuf::from("/usr/share/ng-term/sounds"));
    // Source checkout, for `cargo run`. Debug builds only: this one is
    // relative to the working directory, and a release binary must never
    // seed itself from whatever happens to sit in the directory it was
    // started from.
    #[cfg(debug_assertions)]
    candidates.push(PathBuf::from("assets/sounds"));
    candidates.into_iter().find(|p| p.is_dir())
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

/// Searches look/ for a directory whose metafile has Name=<name>
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
        // A symlinked layout is resolved BY NAME in the layouts directory;
        // a regular file is used directly. A missing layout switches to
        // the default theme.
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
                    .map(|l| parse_layaut_file(&l, name))
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
                    .filter(|x| x.is_finite())
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
        match Panel::from_name(k.trim()) {
            Some(panel) => spec.set(panel, p),
            None => {
                // "screen" is base metadata (the screen the base was
                // created on), not a panel.
                if k.trim() != "screen" {
                    eprintln!("ng-term: unknown panel in .layaut: {}", k.trim());
                }
            }
        }
    }
    spec
}

#[cfg(test)]
mod tests {
    use super::*;

    /// SAVE AS writes the full base recording its screen; SAVE on the
    /// base's screen rewrites the base, SAVE on other screens stores
    /// only the changes in their sections; everything else is preserved.
    #[test]
    fn safe_component_blocks_traversal() {
        assert!(safe_component("tron").is_some());
        assert!(safe_component("my-layaut_2").is_some());
        assert!(safe_component("../../etc/passwd").is_none());
        assert!(safe_component("..").is_none());
        assert!(safe_component("a/b").is_none());
        assert!(safe_component("/abs").is_none());
        assert!(safe_component("").is_none());
        assert!(safe_component("x\\y").is_none());
    }

    /// The pre-split themes/ tree moves into the data directory with its
    /// relative symlinks intact, anything already on the data side wins,
    /// and the old directory disappears once it is empty.
    #[test]
    fn themes_dir_migrates_to_data_dir() {
        let root = std::env::temp_dir().join("ng-term-migrate-test");
        let _ = std::fs::remove_dir_all(&root);
        let old = root.join("config").join("themes");
        let data = root.join("data");

        std::fs::create_dir_all(old.join("style")).unwrap();
        std::fs::create_dir_all(old.join("look").join("tron")).unwrap();
        std::fs::create_dir_all(old.join("layauts")).unwrap();
        std::fs::write(old.join("style").join("default.css"), ":root {}").unwrap();
        std::fs::write(old.join("layauts").join("mine.layaut"), "[column]").unwrap();
        std::fs::write(old.join("look").join("tron").join("meta"), "Name=tron\n").unwrap();
        std::os::unix::fs::symlink(
            "../../style/default.css",
            old.join("look").join("tron").join("tron.css"),
        )
        .unwrap();
        // Already present on the data side: must NOT be overwritten.
        std::fs::create_dir_all(data.join("layauts")).unwrap();
        std::fs::write(data.join("layauts").join("keep.layaut"), "kept").unwrap();

        migrate_themes_dir(&old, &data);

        assert!(!old.exists(), "the old themes/ directory must be gone");
        assert!(data.join("style").join("default.css").is_file());
        assert!(data.join("look").join("tron").join("meta").is_file());
        // The symlink survived as a symlink, and its relative target
        // still resolves now that the themes/ level is gone.
        let link = data.join("look").join("tron").join("tron.css");
        assert!(std::fs::symlink_metadata(&link).unwrap().file_type().is_symlink());
        assert_eq!(std::fs::read_to_string(&link).unwrap(), ":root {}");
        // The pre-existing layauts/ was kept whole, not replaced.
        assert_eq!(
            std::fs::read_to_string(data.join("layauts").join("keep.layaut")).unwrap(),
            "kept"
        );

        // Running again on a config that has no themes/ is a no-op.
        migrate_themes_dir(&old, &data);
        assert!(data.join("style").join("default.css").is_file());
        let _ = std::fs::remove_dir_all(&root);
    }

    /// Seeding restores missing sound files without touching the ones
    /// that are there, and gives every look that lacks one a symlink to
    /// "default" — and running twice must not disturb anything.
    #[test]
    fn sound_themes_seed_and_link() {
        let root = std::env::temp_dir().join("ng-term-sound-seed-test");
        let _ = std::fs::remove_dir_all(&root);
        let src = root.join("share");
        let sounds = root.join("data").join("sounds");
        let look = root.join("data").join("look");

        // A shipped set, as the Makefile would install it.
        std::fs::create_dir_all(src.join("default")).unwrap();
        std::fs::write(src.join("default").join("meta"), "Click=click.wav\n").unwrap();
        std::fs::write(src.join("default").join("click.wav"), b"SHIPPED").unwrap();
        std::fs::write(src.join("default").join("error.wav"), b"SHIPPED").unwrap();
        // Two looks: one plain, one that already picks its own set.
        std::fs::create_dir_all(look.join("tron")).unwrap();
        std::fs::create_dir_all(look.join("matrix")).unwrap();
        std::os::unix::fs::symlink(
            "../../sounds/retro",
            look.join("matrix").join("matrix.sounds"),
        )
        .unwrap();
        // The user replaced one sound and deleted the other.
        std::fs::create_dir_all(sounds.join("default")).unwrap();
        std::fs::write(sounds.join("default").join("click.wav"), b"MINE").unwrap();

        restore_missing_sounds(&src, &sounds);

        // Replaced file untouched, deleted file restored.
        assert_eq!(
            std::fs::read(sounds.join("default").join("click.wav")).unwrap(),
            b"MINE"
        );
        assert_eq!(
            std::fs::read(sounds.join("default").join("error.wav")).unwrap(),
            b"SHIPPED"
        );

        link_default_sounds(&look, &sounds);

        assert!(sounds.join("default").join("click.wav").is_file());
        // The plain look gained the default link...
        let link = look.join("tron").join("tron.sounds");
        assert_eq!(
            std::fs::read_link(&link).unwrap(),
            PathBuf::from("../../sounds/default")
        );
        // ...and the one with its own choice was left alone.
        assert_eq!(
            std::fs::read_link(look.join("matrix").join("matrix.sounds")).unwrap(),
            PathBuf::from("../../sounds/retro")
        );
        assert_eq!(
            look_components(&look.join("tron")).2.as_deref(),
            Some("default")
        );
        assert_eq!(
            look_components(&look.join("matrix")).2.as_deref(),
            Some("retro")
        );

        // Idempotent: a second pass changes nothing.
        link_default_sounds(&look, &sounds);
        assert_eq!(
            std::fs::read_link(&link).unwrap(),
            PathBuf::from("../../sounds/default")
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    /// No amount of deleting can leave the program without themes: every
    /// missing built-in file comes back, while edited ones are kept.
    #[test]
    fn builtin_themes_restore_only_what_is_missing() {
        let root = std::env::temp_dir().join("ng-term-restore-test");
        let _ = std::fs::remove_dir_all(&root);
        let look = root.join("look");
        let style = root.join("style");
        std::fs::create_dir_all(&look).unwrap();
        std::fs::create_dir_all(&style).unwrap();

        // Empty tree: everything is written.
        restore_builtin_themes(&look, &style);
        for (name, ..) in BUILTIN_THEMES {
            assert!(look.join(name).join("meta").is_file(), "{name} meta");
            assert!(
                look.join(name).join(format!("{name}.css")).is_symlink(),
                "{name} css link"
            );
        }
        assert!(style.join("default.css").is_file());
        assert!(style.join("matrix.css").is_file());
        // The symlinks resolve through the restored styles.
        assert!(std::fs::read_to_string(look.join("tron").join("tron.css"))
            .unwrap()
            .contains("--color-r"));

        // The user edits one style and wipes a whole look.
        std::fs::write(style.join("matrix.css"), "/* mine */").unwrap();
        std::fs::remove_dir_all(look.join("nord")).unwrap();
        std::fs::remove_file(style.join("red.css")).unwrap();

        restore_builtin_themes(&look, &style);

        // The edit survived...
        assert_eq!(
            std::fs::read_to_string(style.join("matrix.css")).unwrap(),
            "/* mine */"
        );
        // ...the deletions were repaired.
        assert!(look.join("nord").join("meta").is_file());
        assert!(look.join("nord").join("nord.css").is_symlink());
        assert!(std::fs::read_to_string(style.join("red.css"))
            .unwrap()
            .contains("--color-r"));
        let _ = std::fs::remove_dir_all(&root);
    }

    /// Test widgets, resolved by name against the registry the same way
    /// the rest of the program does.
    fn wp(name: &str) -> Panel {
        Panel::from_name(name).expect("built-in widget must be registered")
    }

    /// The registry is built from the directory: a description file
    /// defines a widget, its directory name IS its name, and an empty
    /// directory falls back to the built-in set rather than to nothing.
    #[test]
    fn widget_registry_reads_the_directory() {
        let root = std::env::temp_dir().join("ng-term-widget-registry-test");
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("mywidget")).unwrap();
        std::fs::write(
            root.join("mywidget").join("widget"),
            "Label=MY WIDGET\nRefHeight=9.5\nMinHeight=4\n",
        )
        .unwrap();
        // A directory without a description file is not a widget.
        std::fs::create_dir_all(root.join("notawidget")).unwrap();
        // A built-in, to check Builtin= is carried through.
        std::fs::create_dir_all(root.join("shell")).unwrap();
        std::fs::write(root.join("shell").join("widget"), "Builtin=shell\n").unwrap();

        let defs = scan_widget_dir(&root);
        assert_eq!(defs.len(), 2, "only directories with a description count");
        // Sorted, so panel order never depends on the filesystem.
        assert_eq!(defs[0].name, "mywidget");
        assert_eq!(defs[0].label, "MY WIDGET");
        assert_eq!(defs[0].ref_h_vh, 9.5);
        assert!(defs[0].builtin.is_none());
        assert_eq!(defs[1].name, "shell");
        assert_eq!(defs[1].builtin.as_deref(), Some("shell"));
        // A label-less widget falls back to its own name.
        assert_eq!(defs[1].label, "SHELL");

        assert!(scan_widget_dir(&root.join("nope")).is_empty());
        let _ = std::fs::remove_dir_all(&root);
    }

    /// Every built-in widget must have a directory to be found in.
    /// Sizes are deliberately NOT compared: they belong to the layout,
    /// not to the widget, so a widget directory no longer carries them.
    #[test]
    fn every_builtin_widget_has_a_directory() {
        let dir = Path::new("assets/widgets");
        let files = scan_widget_dir(dir);
        assert!(!files.is_empty(), "assets/widgets must ship widgets");
        for c in ng::base::builtin_widgets() {
            assert!(
                files.iter().any(|f| f.name == c.name),
                "no widget directory for '{}'",
                c.name
            );
        }
    }

    #[test]
    fn overrides_roundtrip() {
        let name = "unittest-roundtrip";
        let path = layauts_dir().join(format!("{name}.layaut"));
        std::fs::create_dir_all(layauts_dir()).unwrap();

        // SAVE AS on a 2560x1440 32" screen: the full base.
        let mut full = LayoutSpec::default();
        full.set(wp("clock"), PanelSpec { x: 1.0, y: 2.0, w: 10.0, h: 10.0 });
        full.set(wp("shell"), PanelSpec { x: 20.0, y: 2.0, w: 60.0, h: 60.0 });
        save_layaut_full(name, &full, (2560, 1440, 32)).unwrap();
        let text = std::fs::read_to_string(&path).unwrap();
        assert!(text.contains("screen = 2560x1440@32"));
        assert!(text.contains("clock = 1.00 2.00 10.00 10.00"));

        // SAVE on the SAME screen: the base itself is rewritten in full.
        let mut full2 = full.clone();
        full2.set(wp("clock"), PanelSpec { x: 3.0, y: 4.0, w: 11.0, h: 11.0 });
        save_layaut_overrides(name, (2560, 1440, 32), &[], &full2).unwrap();
        let text = std::fs::read_to_string(&path).unwrap();
        assert!(text.contains("clock = 3.00 4.00 11.00 11.00"));
        assert!(!text.contains("[2560x1440@32]"));

        // First save on a DIFFERENT screen: one changed panel -> section.
        let fs_spec = PanelSpec { x: 30.0, y: 10.0, w: 20.0, h: 40.0 };
        save_layaut_overrides(
            name,
            (1920, 1080, 27),
            &[(wp("filesystem"), fs_spec)],
            &full2,
        )
        .unwrap();
        // Another screen: another panel.
        let kb_spec = PanelSpec { x: 5.0, y: 60.0, w: 90.0, h: 30.0 };
        save_layaut_overrides(
            name,
            (1280, 720, 7),
            &[(wp("keyboard"), kb_spec)],
            &full2,
        )
        .unwrap();
        // First screen again: update the same panel.
        let fs_spec2 = PanelSpec { x: 40.0, y: 12.0, w: 22.0, h: 44.0 };
        save_layaut_overrides(
            name,
            (1920, 1080, 27),
            &[(wp("filesystem"), fs_spec2)],
            &full2,
        )
        .unwrap();

        let text = std::fs::read_to_string(&path).unwrap();
        let def = parse_layaut_file(&text, name);
        // Base preserved (rewritten clock position from the same-screen SAVE).
        assert!(text.contains("clock = 3.00 4.00 11.00 11.00"));
        assert!(matches!(def.base, LayoutMode::Fixed(_)));
        // Two sections, exact matches only.
        assert_eq!(def.overrides.len(), 2);
        assert!(def.pick((2560, 1440, 27)).is_none());
        let big = def.pick((1920, 1080, 27)).unwrap();
        assert_eq!(big.panels.len(), 1);
        let (p, ps) = &big.panels[0];
        assert_eq!(*p, wp("filesystem"));
        assert!((ps.x - 40.0).abs() < 0.01 && (ps.h - 44.0).abs() < 0.01);
        let small = def.pick((1280, 720, 7)).unwrap();
        assert_eq!(small.panels.len(), 1);
        assert_eq!(small.panels[0].0, wp("keyboard"));

        std::fs::remove_file(&path).unwrap();
    }
}
