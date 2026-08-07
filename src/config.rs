//! User configuration: ~/.config/ng-term
//!
//! Structure:
//!   ~/.config/ng-term/ng-term.conf         — main configuration file (Key=Value)
//!   ~/.config/ng-term/themes/style/        — shared style files (*.css)
//!   ~/.config/ng-term/themes/layauts/      — shared layout files (*.layaut)
//!   ~/.config/ng-term/themes/look/<theme>/ — complete themes, each containing:
//!       meta        — metafile with a Name= field (name used in ng-term.conf)
//!       *.css       — symlink into themes/style/
//!       *.layaut    — symlink into themes/layauts/
//!
//! Complete themes hold only symlinks, so styles and layouts shared by
//! several themes exist on disk once (no duplicates).
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

pub fn load() -> Config {
    init_tree(&config_dir());
    resolve()
}

/// Resolves the effective configuration from ng-term.conf:
/// - a valid Look= always wins; Style= and Layaut= are then cleared
///   in the file so only the look remains,
/// - otherwise Style=/Layaut= are used; a missing component falls back
///   to "default" (default.css has the tron colors),
/// - nothing set -> the built-in default.
pub fn resolve() -> Config {
    // A valid look wins and clears the component options.
    if let Some(name) = current_theme_name() {
        match load_theme(&look_dir(), &name) {
            Some(cfg) => {
                if current_style_name().is_some() || current_layaut_name().is_some() {
                    clear_component_options();
                }
                return cfg;
            }
            None => eprintln!(
                "ng-term: theme '{name}' not found in {}",
                look_dir().display()
            ),
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
            if let Some(cfg) = load_theme(&look_dir(), &look_name) {
                return cfg;
            }
        }
        let sp = style_dir().join(format!("{sname}.css"));
        let lp = layauts_dir().join(format!("{lname}.layaut"));
        let theme = match std::fs::read_to_string(&sp) {
            Ok(s) => parse_css(&s),
            Err(_) => {
                eprintln!("ng-term: style '{sname}' not found in {}", style_dir().display());
                Theme::load()
            }
        };
        let layout = match std::fs::read_to_string(&lp) {
            Ok(s) => parse_layaut(&s),
            Err(_) => {
                eprintln!(
                    "ng-term: layaut '{lname}' not found in {}",
                    layauts_dir().display()
                );
                LayoutSpec::default()
            }
        };
        return Config { theme, layout };
    }

    // Default theme (hardcoded; Theme::load also honors NGTERM_THEME).
    Config {
        theme: Theme::load(),
        layout: LayoutSpec::default(),
    }
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
    (style, layaut)
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

/// Directory with shared layouts: ~/.config/ng-term/themes/layauts
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

/// Layout names available in themes/layauts (no extensions).
pub fn list_layauts() -> Vec<String> {
    list_stems(&layauts_dir(), "layaut")
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
             # Empty values or missing options = defaults built into the program.\n\
             Look=\n\
             Style=\n\
             Layaut=\n",
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
        let _ = std::fs::write(
            layauts.join("default.layaut"),
            "# ng-term panel layout: panel = x y width height\n\
             # Units: vw (percent of window width), vh (percent of height).\n\
             left_col   = 0.6vw  2.5vh  16.4vw 59.5vh\n\
             shell      = 17.5vw 2.5vh  65.0vw 60.3vh\n\
             right_col  = 83.0vw 2.5vh  16.4vw 59.5vh\n\
             filesystem = 83.0vw 17.4vh 16.4vw 79.6vh\n\
             keyboard   = 17.5vw 64.5vh 65.0vw 32.5vh\n\
             control    = 0.6vw  64.5vh 16.4vw 32.5vh\n",
        );
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
            let dir = look.join(name);
            if std::fs::create_dir_all(&dir).is_ok() {
                let _ = std::fs::write(
                    dir.join("meta"),
                    format!(
                        "Name={name}\nDescription=eDEX-UI '{name}' theme (original colors)\n"
                    ),
                );
                // A complete theme holds only symlinks to the shared files.
                let _ = std::os::unix::fs::symlink(
                    format!("../../style/{style_file}.css"),
                    dir.join(format!("{name}.css")),
                );
                let _ = std::os::unix::fs::symlink(
                    "../../layauts/default.layaut",
                    dir.join(format!("{name}.layaut")),
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
fn load_theme(themes_dir: &Path, name: &str) -> Option<Config> {
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
        let layout = find_file(&dir, "layaut")
            .and_then(|p| std::fs::read_to_string(p).ok())
            .map(|l| parse_layaut(&l))
            .unwrap_or_default();
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
