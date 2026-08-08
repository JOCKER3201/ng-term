//! Interface layout — a faithful replica of the eDEX-UI panel arrangement:
//! left column (17%), central terminal (65% x 60%), right column (17%),
//! bottom strip: file browser + on-screen keyboard.

pub mod boot;
pub mod clock;
pub mod control;
pub mod cpu;
pub mod filesystem;
pub mod hardware;
pub mod keyboard;
pub mod memory;
pub mod network;
pub mod popup;
pub mod processes;
pub mod settings;
pub mod shell;
pub mod sysinfo;
pub mod telemetry;

use crate::draw::DrawList;
use crate::font::{FontSystem, FONT_UI};
use crate::theme::Theme;

#[derive(Clone, Copy, Debug)]
pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

impl Rect {
    pub fn new(x: f32, y: f32, w: f32, h: f32) -> Self {
        Rect { x, y, w, h }
    }
    pub fn contains(&self, px: f32, py: f32) -> bool {
        px >= self.x && px < self.x + self.w && py >= self.y && py < self.y + self.h
    }
    pub fn right(&self) -> f32 {
        self.x + self.w
    }
    pub fn bottom(&self) -> f32 {
        self.y + self.h
    }
    pub fn cx(&self) -> f32 {
        self.x + self.w / 2.0
    }
}

/// Panel position and size in vw/vh units (percent of the window).
#[derive(Clone, Copy)]
pub struct PanelSpec {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

/// Panel layout — default or loaded from the theme's .layaut file.
#[derive(Clone)]
pub struct LayoutSpec {
    pub left_col: PanelSpec,
    pub shell: PanelSpec,
    pub right_col: PanelSpec,
    pub filesystem: PanelSpec,
    pub keyboard: PanelSpec,
    pub control: PanelSpec,
}

impl Default for LayoutSpec {
    fn default() -> Self {
        LayoutSpec {
            left_col: PanelSpec { x: 0.6, y: 2.5, w: 16.4, h: 59.5 },
            shell: PanelSpec { x: 17.5, y: 2.5, w: 65.0, h: 60.3 },
            right_col: PanelSpec { x: 83.0, y: 2.5, w: 16.4, h: 59.5 },
            // Files in the right column under NETWORK STATUS, down to the bottom.
            filesystem: PanelSpec { x: 83.0, y: 17.4, w: 16.4, h: 79.6 },
            // Keyboard directly under the terminal, matching its width.
            keyboard: PanelSpec { x: 17.5, y: 64.5, w: 65.0, h: 32.5 },
            // Program control panel in the bottom-left corner.
            control: PanelSpec { x: 0.6, y: 64.5, w: 16.4, h: 32.5 },
        }
    }
}

/// Panels a layout can place.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Panel {
    LeftCol,
    Shell,
    RightCol,
    Filesystem,
    Keyboard,
    Control,
}

impl Panel {
    pub fn from_name(name: &str) -> Option<Panel> {
        Some(match name {
            "left_col" => Panel::LeftCol,
            "shell" => Panel::Shell,
            "right_col" => Panel::RightCol,
            "filesystem" => Panel::Filesystem,
            "keyboard" => Panel::Keyboard,
            "control" => Panel::Control,
            _ => return None,
        })
    }
}

/// One flexbox column: CSS-like width constraints plus panels stacked
/// top to bottom with height weights.
#[derive(Clone)]
pub struct FlexColumn {
    /// Preferred width as a percentage of the row (flex-basis).
    pub basis: f32,
    /// Minimum width in px (min-width).
    pub min: f32,
    /// Maximum width in px (max-width); INFINITY = unlimited.
    pub max: f32,
    /// Share of the leftover space (flex-grow).
    pub grow: f32,
    /// Collapse priority when space runs out: 1 disappears first,
    /// then 2, ...; 0 = never hidden.
    pub collapse: u32,
    /// Vertical gap between the panels, in height weight units.
    pub gap: f32,
    /// Panels top to bottom with their height weights.
    pub panels: Vec<(Panel, f32)>,
}

/// A flexbox layout: columns laid out left to right.
#[derive(Clone)]
pub struct FlexLayaut {
    pub columns: Vec<FlexColumn>,
}

/// How the panel layout is produced (see src/flex.rs).
#[derive(Clone)]
pub enum LayoutMode {
    /// Built-in responsive default: a flexbox tree computed from the
    /// actual window size every frame.
    Flex,
    /// A custom flexbox .layaut file — same engine as the default.
    Custom(FlexLayaut),
    /// A legacy .layaut file: a fixed 16:9 base, re-adapted to the
    /// window every frame.
    Fixed(LayoutSpec),
}

impl Default for LayoutMode {
    fn default() -> Self {
        LayoutMode::Flex
    }
}

/// Computed panel rectangles (in physical pixels).
pub struct Layout {
    pub left_col: Rect,
    pub shell: Rect,
    pub right_col: Rect,
    pub filesystem: Rect,
    pub keyboard: Rect,
    pub control: Rect,
}

impl Layout {
    pub fn compute(w: f32, h: f32, spec: &LayoutSpec) -> Self {
        let vw = w / 100.0;
        let vh = h / 100.0;
        let r = |p: &PanelSpec| Rect::new(p.x * vw, p.y * vh, p.w * vw, p.h * vh);
        Layout {
            left_col: r(&spec.left_col),
            shell: r(&spec.shell),
            right_col: r(&spec.right_col),
            filesystem: r(&spec.filesystem),
            keyboard: r(&spec.keyboard),
            control: r(&spec.control),
        }
    }
}

/// Drawing context passed to the panels.
pub struct Ctx<'a> {
    pub dl: &'a mut DrawList,
    pub fonts: &'a mut FontSystem,
    pub theme: &'a Theme,
    /// Window width/height in px.
    pub w: f32,
    pub h: f32,
    /// Time since application start, in seconds.
    pub t: f64,
    /// Mouse cursor position.
    pub mouse: (f32, f32),
    /// Terminal font size multiplier (TermFontSize= in ng-term.conf).
    pub term_font_scale: f32,
    /// Interface font size multiplier (UIFontSize= in ng-term.conf).
    pub ui_font_scale: f32,
    /// Font scale of the panel being drawn (container-query style):
    /// narrow columns shrink their text. Panels set it on entry and
    /// reset it to 1.0 when done; full-width panels leave it at 1.0.
    pub panel_scale: f32,
}

impl<'a> Ctx<'a> {
    pub fn vh(&self, v: f32) -> f32 {
        self.h / 100.0 * v
    }
    pub fn vw(&self, v: f32) -> f32 {
        self.w / 100.0 * v
    }
    /// Interface font size: scaled by UIFontSize= (text only) and by the
    /// width of the panel being drawn, min 8 px.
    pub fn font_px(&self, v: f32) -> f32 {
        (self.vh(v) * self.ui_font_scale * self.panel_scale).max(8.0)
    }
    /// Panel-relative font scale (like a CSS container query): full size
    /// at the reference column width (30% of the window height, i.e. a
    /// classic side column), shrinking down to 62% in narrow columns.
    pub fn panel_font_scale(&self, r: &Rect) -> f32 {
        (r.w / (self.h * 0.30)).clamp(0.62, 1.0)
    }
}

/// Trims text (with a trailing ellipsis) so it fits the given width —
/// shared by the telemetry widgets.
pub(crate) fn fit_end(ctx: &mut Ctx, px: f32, text: &str, max_w: f32) -> String {
    if ctx.fonts.measure(FONT_UI, px, text, px * 0.06) <= max_w {
        return text.to_string();
    }
    let chars: Vec<char> = text.chars().collect();
    let mut n = chars.len().saturating_sub(1);
    while n > 1 {
        let cand: String = chars[..n].iter().collect::<String>() + "\u{2026}";
        if ctx.fonts.measure(FONT_UI, px, &cand, px * 0.06) <= max_w {
            return cand;
        }
        n -= 1;
    }
    "\u{2026}".to_string()
}
