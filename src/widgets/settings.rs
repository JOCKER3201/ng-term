//! Modal settings window (centered). Main view: CLOSE + THEMES. The THEMES
//! view is a submenu with LOOK (complete themes), STYLES (color styles from
//! themes/style) and LAYAUTS (layouts from themes/layauts). Selections are
//! written to ng-term.conf (Look= / Style= / Layaut=) and applied live.

use super::{Ctx, Rect};
use crate::config::{self, ThemeInfo};
use crate::font::FONT_UI;
use crate::theme::Color;
use std::time::Instant;

#[derive(Clone, Copy, PartialEq)]
enum View {
    Menu,
    Themes,
    Look,
    Styles,
    Layauts,
}

#[derive(Clone, Copy, PartialEq)]
enum Act {
    Close,
    Back,
    OpenThemes,
    OpenLook,
    OpenStyles,
    OpenLayauts,
    Look(usize),
    Style(usize),
    Layaut(usize),
}

pub struct Settings {
    pub open: bool,
    view: View,
    looks: Vec<ThemeInfo>,
    styles: Vec<String>,
    layauts: Vec<String>,
    /// Current selections from ng-term.conf (highlighted in the lists).
    current_look: Option<String>,
    current_style: Option<String>,
    current_layaut: Option<String>,
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
            looks: Vec::new(),
            styles: Vec::new(),
            layauts: Vec::new(),
            current_look: None,
            current_style: None,
            current_layaut: None,
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

    /// Click handling. Returns true when the configuration changed
    /// (the caller should re-resolve and apply it).
    pub fn click(&mut self, x: f32, y: f32, w: f32, h: f32) -> bool {
        if !self.open {
            return false;
        }
        if !modal_rect(w, h).contains(x, y) {
            // Clicks outside the window are swallowed; closing is done
            // with the CLOSE button (or ESC), not by clicking outside.
            return false;
        }
        let act = self.hits.iter().find(|(r, _)| r.contains(x, y)).map(|&(_, a)| a);
        let Some(act) = act else { return false };
        self.flash = Some((act, Instant::now()));
        match act {
            Act::Close => self.open = false,
            Act::Back => {
                self.view = match self.view {
                    View::Look | View::Styles | View::Layauts => View::Themes,
                    _ => View::Menu,
                }
            }
            Act::OpenThemes => self.view = View::Themes,
            Act::OpenLook => {
                // Scanned when the view is opened.
                self.looks = config::list_themes();
                self.refresh_current();
                self.view = View::Look;
            }
            Act::OpenStyles => {
                self.styles = config::list_styles();
                self.refresh_current();
                self.view = View::Styles;
            }
            Act::OpenLayauts => {
                self.layauts = config::list_layauts();
                self.refresh_current();
                self.view = View::Layauts;
            }
            Act::Look(i) => {
                // A look replaces everything: Style= and Layaut= are cleared.
                if let Some(info) = self.looks.get(i) {
                    config::set_theme_option(&info.name);
                    config::clear_component_options();
                    self.refresh_current();
                    return true;
                }
            }
            Act::Style(i) => {
                // A component clears Look=; the missing other component
                // is automatically set to "default".
                if let Some(name) = self.styles.get(i).cloned() {
                    config::set_style_option(&name);
                    if config::current_layaut_name().is_none() {
                        config::set_layaut_option("default");
                    }
                    config::clear_look_option();
                    config::canonicalize_components();
                    self.refresh_current();
                    return true;
                }
            }
            Act::Layaut(i) => {
                if let Some(name) = self.layauts.get(i).cloned() {
                    config::set_layaut_option(&name);
                    if config::current_style_name().is_none() {
                        config::set_style_option("default");
                    }
                    config::clear_look_option();
                    config::canonicalize_components();
                    self.refresh_current();
                    return true;
                }
            }
        }
        false
    }

    /// Refreshes the selection highlights: the look from Look= and the
    /// effective style/layaut components (a selected look also marks the
    /// components it is composed of).
    fn refresh_current(&mut self) {
        self.current_look = config::current_theme_name();
        let (style, layaut) = config::effective_components();
        self.current_style = style;
        self.current_layaut = layaut;
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
            View::Look => "SETTINGS \u{2014} LOOK",
            View::Styles => "SETTINGS \u{2014} STYLES",
            View::Layauts => "SETTINGS \u{2014} LAYAUTS",
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
        let corner_w = (content.w * 0.22).max(70.0);

        match self.view {
            View::Menu => {
                // Close button in the top left of the main view.
                self.button(
                    ctx,
                    Rect::new(content.x, content.y, corner_w, btn_h),
                    "CLOSE",
                    Act::Close,
                );
                // Menu entry: THEMES.
                let bw = content.w * 0.6;
                self.button(
                    ctx,
                    Rect::new(
                        content.x + (content.w - bw) / 2.0,
                        content.y + btn_h + gap,
                        bw,
                        btn_h,
                    ),
                    "THEMES",
                    Act::OpenThemes,
                );
            }
            View::Themes => {
                // Submenu: LOOK / STYLES / LAYAUTS.
                self.button(
                    ctx,
                    Rect::new(content.x, content.y, corner_w, btn_h),
                    "BACK",
                    Act::Back,
                );
                let bw = content.w * 0.6;
                let bx = content.x + (content.w - bw) / 2.0;
                let entries = [
                    ("LOOK", Act::OpenLook),
                    ("STYLES", Act::OpenStyles),
                    ("LAYAUTS", Act::OpenLayauts),
                ];
                for (i, (label, act)) in entries.into_iter().enumerate() {
                    let y = content.y + (btn_h + gap) * (i as f32 + 1.0);
                    self.button(ctx, Rect::new(bx, y, bw, btn_h), label, act);
                }
            }
            View::Look => {
                let names: Vec<String> =
                    self.looks.iter().map(|t| t.name.clone()).collect();
                self.item_grid(ctx, content, btn_h, gap, corner_w, &names, Act::Look);
                self.empty_note(ctx, content, btn_h, gap, &names, "NO LOOKS FOUND");
            }
            View::Styles => {
                let names = self.styles.clone();
                self.item_grid(ctx, content, btn_h, gap, corner_w, &names, Act::Style);
                self.empty_note(ctx, content, btn_h, gap, &names, "NO STYLES FOUND");
            }
            View::Layauts => {
                let names = self.layauts.clone();
                self.item_grid(ctx, content, btn_h, gap, corner_w, &names, Act::Layaut);
                self.empty_note(ctx, content, btn_h, gap, &names, "NO LAYAUTS FOUND");
            }
        }
    }

    /// BACK button + items next to it and below, in rows.
    #[allow(clippy::too_many_arguments)]
    fn item_grid(
        &mut self,
        ctx: &mut Ctx,
        content: Rect,
        btn_h: f32,
        gap: f32,
        corner_w: f32,
        names: &[String],
        make_act: fn(usize) -> Act,
    ) {
        let _ = corner_w;
        self.button(
            ctx,
            Rect::new(content.x, content.y, (content.w * 0.22).max(70.0), btn_h),
            "BACK",
            Act::Back,
        );
        let cols = 3usize;
        let bw = (content.w - gap * (cols as f32 - 1.0)) / cols as f32;
        let mut col = 1usize; // the first row starts next to BACK
        let mut y = content.y;
        for (i, name) in names.iter().enumerate() {
            if col >= cols {
                col = 0;
                y += btn_h + gap;
            }
            if y + btn_h > content.bottom() {
                break;
            }
            let br = Rect::new(content.x + col as f32 * (bw + gap), y, bw, btn_h);
            let label = name.to_uppercase();
            self.button(ctx, br, &label, make_act(i));
            col += 1;
        }
    }

    fn empty_note(
        &mut self,
        ctx: &mut Ctx,
        content: Rect,
        btn_h: f32,
        gap: f32,
        names: &[String],
        note: &str,
    ) {
        if !names.is_empty() {
            return;
        }
        let px = ctx.font_px(1.0);
        ctx.dl.text_center(
            ctx.fonts,
            FONT_UI,
            px,
            content.cx(),
            content.y + btn_h + gap,
            note,
            ctx.theme.base.alpha(0.5),
            px * 0.1,
        );
    }

    /// Button in the terminal-tab style (slant, hover, flash on click).
    fn button(&mut self, ctx: &mut Ctx, r: Rect, label: &str, act: Act) {
        let base = ctx.theme.base;
        let hover = r.contains(ctx.mouse.0, ctx.mouse.1);
        let flash = self
            .flash
            .map(|(a, t)| a == act && t.elapsed().as_secs_f32() < 0.15)
            .unwrap_or(false);
        // The currently selected item is highlighted like an active tab.
        let is_current = match act {
            Act::Look(i) => {
                self.looks.get(i).map(|t| Some(&t.name) == self.current_look.as_ref())
                    == Some(true)
            }
            Act::Style(i) => {
                self.styles.get(i).map(|s| Some(s) == self.current_style.as_ref())
                    == Some(true)
            }
            Act::Layaut(i) => {
                self.layauts.get(i).map(|s| Some(s) == self.current_layaut.as_ref())
                    == Some(true)
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
