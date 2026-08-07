//! Interface layout — a faithful replica of the eDEX-UI panel arrangement:
//! left column (17%), central terminal (65% x 60%), right column (17%),
//! bottom strip: file browser + on-screen keyboard.

pub mod boot;
pub mod control;
pub mod filesystem;
pub mod keyboard;
pub mod left;
pub mod popup;
pub mod right;
pub mod settings;
pub mod shell;

use crate::draw::DrawList;
use crate::font::FontSystem;
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

/// How the panel layout is produced (see src/flex.rs).
#[derive(Clone)]
pub enum LayoutMode {
    /// Built-in responsive default: a flexbox tree computed from the
    /// actual window size every frame.
    Flex,
    /// A custom .layaut file: a fixed 16:9 base, re-adapted to the
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
}

impl<'a> Ctx<'a> {
    pub fn vh(&self, v: f32) -> f32 {
        self.h / 100.0 * v
    }
    pub fn vw(&self, v: f32) -> f32 {
        self.w / 100.0 * v
    }
    /// Interface font size: scaled by UIFontSize= (text only), min 8 px.
    pub fn font_px(&self, v: f32) -> f32 {
        (self.vh(v) * self.ui_font_scale).max(8.0)
    }
}
