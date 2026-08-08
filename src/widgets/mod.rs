//! Interface layout — a faithful replica of the eDEX-UI panel arrangement:
//! left column (17%), central terminal (65% x 60%), right column (17%),
//! bottom strip: file browser + on-screen keyboard.

pub mod boot;
pub mod clock;
pub mod control;
pub mod cpu;
pub mod editor;
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

/// Individually placeable widgets (panels) of the interface.
#[derive(Clone, Copy, PartialEq, Eq)]
#[repr(usize)]
pub enum Panel {
    Clock = 0,
    Sysinfo,
    Hardware,
    Cpu,
    Memory,
    Processes,
    Shell,
    Network,
    Filesystem,
    Keyboard,
    Control,
}

pub const PANEL_COUNT: usize = 11;

impl Panel {
    pub const ALL: [Panel; PANEL_COUNT] = [
        Panel::Clock,
        Panel::Sysinfo,
        Panel::Hardware,
        Panel::Cpu,
        Panel::Memory,
        Panel::Processes,
        Panel::Shell,
        Panel::Network,
        Panel::Filesystem,
        Panel::Keyboard,
        Panel::Control,
    ];

    pub fn idx(self) -> usize {
        self as usize
    }

    /// Name used in .layaut files.
    pub fn name(self) -> &'static str {
        match self {
            Panel::Clock => "clock",
            Panel::Sysinfo => "sysinfo",
            Panel::Hardware => "hardware",
            Panel::Cpu => "cpu",
            Panel::Memory => "memory",
            Panel::Processes => "processes",
            Panel::Shell => "shell",
            Panel::Network => "network",
            Panel::Filesystem => "filesystem",
            Panel::Keyboard => "keyboard",
            Panel::Control => "control",
        }
    }

    /// Label shown in the layout editor.
    pub fn label(self) -> &'static str {
        match self {
            Panel::Clock => "CLOCK",
            Panel::Sysinfo => "SYSTEM INFO",
            Panel::Hardware => "HARDWARE",
            Panel::Cpu => "CPU",
            Panel::Memory => "MEMORY",
            Panel::Processes => "PROCESSES",
            Panel::Shell => "SHELL",
            Panel::Network => "NETWORK",
            Panel::Filesystem => "FILESYSTEM",
            Panel::Keyboard => "KEYBOARD",
            Panel::Control => "CONTROL",
        }
    }

    pub fn from_name(name: &str) -> Option<Panel> {
        Panel::ALL.into_iter().find(|p| p.name() == name)
    }
}

/// A panel placed far outside the window = hidden.
pub const OFF_SPEC: PanelSpec = PanelSpec { x: 200.0, y: 0.0, w: 20.0, h: 25.0 };

/// Panel layout — positions of all panels loaded from a legacy .layaut
/// file (percent of the window at the 16:9 reference). Panels missing
/// from the file stay hidden.
#[derive(Clone)]
pub struct LayoutSpec {
    pub panels: [PanelSpec; PANEL_COUNT],
}

impl LayoutSpec {
    pub fn p(&self, p: Panel) -> &PanelSpec {
        &self.panels[p.idx()]
    }
    pub fn set(&mut self, p: Panel, s: PanelSpec) {
        self.panels[p.idx()] = s;
    }
}

impl Default for LayoutSpec {
    fn default() -> Self {
        LayoutSpec { panels: [OFF_SPEC; PANEL_COUNT] }
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
    pub panels: [Rect; PANEL_COUNT],
}

impl Layout {
    /// All panels off-screen (starting point for layout engines).
    pub fn empty(w: f32, h: f32) -> Layout {
        Layout { panels: [Rect::new(w * 2.0, 0.0, w * 0.16, h * 0.6); PANEL_COUNT] }
    }

    /// The rectangle of a panel.
    pub fn p(&self, p: Panel) -> Rect {
        self.panels[p.idx()]
    }

    pub fn set(&mut self, p: Panel, r: Rect) {
        self.panels[p.idx()] = r;
    }

    /// Insets every panel by the UI padding: the space between the
    /// content and the outer (resize) edge of the panel. Applied to
    /// drawing and hit-testing; the outer rectangles stay authoritative
    /// for layout files and the grid editor.
    pub fn padded(&self, pad: f32) -> Layout {
        let ins = |r: &Rect| {
            let p = pad.min(r.w / 4.0).min(r.h / 4.0).max(0.0);
            Rect::new(r.x + p, r.y + p, r.w - 2.0 * p, r.h - 2.0 * p)
        };
        Layout { panels: std::array::from_fn(|i| ins(&self.panels[i])) }
    }

    pub fn compute(w: f32, h: f32, spec: &LayoutSpec) -> Self {
        let vw = w / 100.0;
        let vh = h / 100.0;
        Layout {
            panels: std::array::from_fn(|i| {
                let p = &spec.panels[i];
                Rect::new(p.x * vw, p.y * vh, p.w * vw, p.h * vh)
            }),
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
