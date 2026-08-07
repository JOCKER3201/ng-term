//! Modal settings window (centered). The main view contains the THEMES
//! entry; clicking it shows a back button in the top left and, next to it
//! and below, the list of themes scanned from ~/.config/ng-term/themes.

use super::{Ctx, Rect};
use crate::config::{self, ThemeInfo};
use crate::font::FONT_UI;
use crate::theme::Color;
use std::time::Instant;

#[derive(Clone, Copy, PartialEq)]
enum View {
    Menu,
    Themes,
}

#[derive(Clone, Copy, PartialEq)]
enum Act {
    Close,
    Back,
    OpenThemes,
    Theme(usize),
}

pub struct Settings {
    pub open: bool,
    view: View,
    themes: Vec<ThemeInfo>,
    /// Name of the currently set theme (highlighted in the list).
    current: Option<String>,
    hits: Vec<(Rect, Act)>,
    flash: Option<(Act, Instant)>,
}

/// Modal window rectangle.
fn modal_rect(w: f32, h: f32) -> Rect {
    let mw = (w * 0.40).max(320.0);
    let mh = (h * 0.52).max(260.0);
    Rect::new((w - mw) / 2.0, (h - mh) / 2.0, mw, mh)
}

impl Settings {
    pub fn new() -> Self {
        Settings {
            open: false,
            view: View::Menu,
            themes: Vec::new(),
            current: None,
            hits: Vec::new(),
            flash: None,
        }
    }

    pub fn show(&mut self) {
        self.open = true;
        self.view = View::Menu;
    }

    pub fn close(&mut self) {
        self.open = false;
    }

    /// Whether the cursor is over an interactive element of the window.
    pub fn hover(&self, x: f32, y: f32) -> bool {
        self.hits.iter().any(|(r, _)| r.contains(x, y))
    }

    /// Click handling. Returns the theme name if one was selected.
    pub fn click(&mut self, x: f32, y: f32, w: f32, h: f32) -> Option<String> {
        if !self.open {
            return None;
        }
        if !modal_rect(w, h).contains(x, y) {
            // Clicks outside the window are swallowed; closing is done
            // with the CLOSE button (or ESC), not by clicking outside.
            return None;
        }
        let act = self.hits.iter().find(|(r, _)| r.contains(x, y)).map(|&(_, a)| a);
        if let Some(act) = act {
            self.flash = Some((act, Instant::now()));
            match act {
                Act::Close => self.open = false,
                Act::OpenThemes => {
                    // Themes are scanned when the THEMES view is opened.
                    self.themes = config::list_themes();
                    self.current = config::current_theme_name();
                    self.view = View::Themes;
                }
                Act::Back => self.view = View::Menu,
                Act::Theme(i) => {
                    if let Some(info) = self.themes.get(i) {
                        self.current = Some(info.name.clone());
                        return Some(info.name.clone());
                    }
                }
            }
        }
        None
    }

    pub fn draw(&mut self, ctx: &mut Ctx) {
        if !self.open {
            return;
        }
        self.hits.clear();
        let base = ctx.theme.base;

        // Dim the background behind the window.
        ctx.dl
            .rect(0.0, 0.0, ctx.w, ctx.h, Color::rgb8(0, 0, 0).alpha(0.55));

        let m = modal_rect(ctx.w, ctx.h);
        ctx.dl.rect(m.x, m.y, m.w, m.h, ctx.theme.bg);
        ctx.dl
            .chamfer_frame(m.x, m.y, m.w, m.h, ctx.vh(1.1), ctx.vh(0.18).max(1.5), base.alpha(0.7));

        let pad = ctx.vh(1.4);
        let title_px = ctx.font_px(1.02);
        let title = match self.view {
            View::Menu => "SETTINGS",
            View::Themes => "SETTINGS \u{2014} THEMES",
        };
        ctx.dl.module_title(
            ctx.fonts,
            m.x + pad,
            m.y + pad,
            m.w - 2.0 * pad,
            title_px,
            title,
            "",
            base,
        );

        let content = Rect::new(
            m.x + pad,
            m.y + pad + title_px * 2.8,
            m.w - 2.0 * pad,
            m.h - 2.0 * pad - title_px * 2.8,
        );
        let btn_h = ctx.vh(4.2);
        let gap = ctx.vh(1.2);

        match self.view {
            View::Menu => {
                // Close button in the top left of the main view.
                let close_w = (content.w * 0.22).max(70.0);
                self.button(
                    ctx,
                    Rect::new(content.x, content.y, close_w, btn_h),
                    "CLOSE",
                    Act::Close,
                );
                // Menu entry: THEMES.
                let bw = content.w * 0.6;
                let br = Rect::new(
                    content.x + (content.w - bw) / 2.0,
                    content.y + btn_h + gap,
                    bw,
                    btn_h,
                );
                self.button(ctx, br, "THEMES", Act::OpenThemes);
            }
            View::Themes => {
                // Back button in the top left.
                let back_w = (content.w * 0.22).max(70.0);
                let back_r = Rect::new(content.x, content.y, back_w, btn_h);
                self.button(ctx, back_r, "BACK", Act::Back);

                // Themes: next to the back button and below, in rows.
                let cols = 3usize;
                let bw = (content.w - gap * (cols as f32 - 1.0)) / cols as f32;
                let mut col = 1usize; // the first row starts next to BACK
                let mut y = content.y;
                let names: Vec<String> =
                    self.themes.iter().map(|t| t.name.to_uppercase()).collect();
                for (i, name) in names.iter().enumerate() {
                    if col >= cols {
                        col = 0;
                        y += btn_h + gap;
                    }
                    if y + btn_h > content.bottom() {
                        break;
                    }
                    let br = Rect::new(content.x + col as f32 * (bw + gap), y, bw, btn_h);
                    self.button(ctx, br, name, Act::Theme(i));
                    col += 1;
                }
                if self.themes.is_empty() {
                    let px = ctx.font_px(1.0);
                    ctx.dl.text_center(
                        ctx.fonts,
                        FONT_UI,
                        px,
                        content.cx(),
                        content.y + btn_h + gap,
                        "NO THEMES FOUND",
                        base.alpha(0.5),
                        px * 0.1,
                    );
                }
            }
        }
    }

    /// Button in the terminal-tab style (slant, hover, flash on click).
    fn button(&mut self, ctx: &mut Ctx, r: Rect, label: &str, act: Act) {
        let base = ctx.theme.base;
        let hover = r.contains(ctx.mouse.0, ctx.mouse.1);
        let flash = self
            .flash
            .map(|(a, t)| a == act && t.elapsed().as_secs_f32() < 0.15)
            .unwrap_or(false);
        // The currently set theme is highlighted like an active tab.
        let is_current = match act {
            Act::Theme(i) => {
                self.themes.get(i).map(|t| Some(&t.name) == self.current.as_ref()) == Some(true)
            }
            _ => false,
        };
        let fill = if flash {
            base.alpha(0.35)
        } else if hover {
            base.alpha(0.22)
        } else if is_current {
            base.alpha(0.12)
        } else {
            ctx.theme.bg
        };
        let skew = r.h * 0.7;
        let pts = [
            [r.x + skew, r.y],
            [r.right(), r.y],
            [r.right() - skew, r.bottom()],
            [r.x, r.bottom()],
        ];
        ctx.dl.quad(pts, fill);
        ctx.dl
            .polyline(&pts, 1.0, base.alpha(if hover || flash { 0.8 } else { 0.4 }), true);

        let px = ctx.font_px(1.0);
        let color = if hover || flash || is_current { base } else { base.alpha(0.7) };
        // Left arrow on the back button.
        if act == Act::Back {
            let s = (r.h * 0.14).max(4.0);
            let ax = r.x + skew * 0.8 + s;
            let cy = r.y + r.h / 2.0;
            ctx.dl
                .quad([[ax - s, cy], [ax + s, cy - s], [ax + s, cy + s], [ax + s, cy + s]], color);
            ctx.dl.text_center(
                ctx.fonts,
                FONT_UI,
                px,
                r.cx() + s,
                r.y + (r.h - px * 1.3) / 2.0,
                label,
                color,
                px * 0.1,
            );
        } else {
            ctx.dl.text_center(
                ctx.fonts,
                FONT_UI,
                px,
                r.cx(),
                r.y + (r.h - px * 1.3) / 2.0,
                label,
                color,
                px * 0.1,
            );
        }
        self.hits.push((r, act));
    }
}
