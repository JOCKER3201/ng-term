//! Modal settings window (centered). Main view: CLOSE + THEMES. The THEMES
//! view is a submenu with LOOK (complete themes), STYLES (color styles from
//! themes/style) and LAYAUTS (layouts from themes/layauts). Selections are
//! written to ng-term.conf (Look= / Style= / Layaut=) and applied live.

use super::{Ctx, Rect};
use crate::config::{self, ThemeInfo};
use crate::font::FONT_UI;
use std::time::Instant;

#[derive(Clone, Copy, PartialEq)]
enum View {
    Menu,
    Themes,
    Look,
    Styles,
    Layauts,
    Font,
    Grid,
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
    OpenFont,
    OpenGrid,
    ToggleSnap,
    GridCols(i32),
    GridRows(i32),
    PadTrack,
    EditGrid,
    SizeTrack(Sect),
    FamilyBtn(Sect),
    WeightBtn(Sect),
    FamilyPick(Sect, usize),
    WeightPick(Sect, usize),
}

/// Font section: terminal or the rest of the interface.
#[derive(Clone, Copy, PartialEq)]
enum Sect {
    Term,
    Ui,
}

#[derive(Clone, Copy, PartialEq)]
enum Dropdown {
    Family(Sect),
    Weight(Sect),
}

/// Weight options offered in the WEIGHT dropdown.
const WEIGHTS: [&str; 5] = ["Light", "Regular", "Medium", "SemiBold", "Bold"];

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
    /// Font view state, indexed by section (0 = Term, 1 = Ui).
    families: [Vec<String>; 2],
    cur_family: [Option<String>; 2],
    cur_weight: [Option<String>; 2],
    /// Font sizes in percent (50-200).
    cur_size: [u32; 2],
    dragging_size: Option<Sect>,
    slider_rect: [Rect; 2],
    dropdown: Option<Dropdown>,
    /// When the dropdown was opened — drives the accordion animation.
    dropdown_since: Option<Instant>,
    /// Grid editor preferences (GRID view).
    grid_snap: bool,
    grid_cols: u32,
    grid_rows: u32,
    /// Widget padding in px (0-40) + its slider state.
    grid_pad: u32,
    dragging_pad: bool,
    pad_rect: Rect,
    /// Set by EDIT GRID — main enters the layout editor and clears it.
    pub edit_requested: bool,
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
            families: [Vec::new(), Vec::new()],
            cur_family: [None, None],
            cur_weight: [None, None],
            cur_size: [100, 100],
            dragging_size: None,
            slider_rect: [Rect::new(0.0, 0.0, 0.0, 0.0); 2],
            dropdown: None,
            dropdown_since: None,
            grid_snap: false,
            grid_cols: 12,
            grid_rows: 8,
            grid_pad: 8,
            dragging_pad: false,
            pad_rect: Rect::new(0.0, 0.0, 0.0, 0.0),
            edit_requested: false,
            hits: Vec::new(),
            flash: None,
        }
    }

    fn sect_idx(sect: Sect) -> usize {
        match sect {
            Sect::Term => 0,
            Sect::Ui => 1,
        }
    }

    /// Slider range per section: terminal 50-200%, interface 75-125%.
    fn size_range(sect: Sect) -> (f32, f32) {
        match sect {
            Sect::Term => (50.0, 200.0),
            Sect::Ui => (75.0, 125.0),
        }
    }

    fn set_size_from_x(&mut self, sect: Sect, x: f32) {
        let i = Self::sect_idx(sect);
        let (min, max) = Self::size_range(sect);
        let track = self.slider_rect[i];
        let t = ((x - track.x) / track.w.max(1.0)).clamp(0.0, 1.0);
        self.cur_size[i] = (min + t * (max - min)).round() as u32;
    }

    fn set_pad_from_x(&mut self, x: f32) {
        let track = self.pad_rect;
        let t = ((x - track.x) / track.w.max(1.0)).clamp(0.0, 1.0);
        self.grid_pad = (t * 40.0).round() as u32;
    }

    /// Mouse move while dragging a size slider.
    pub fn drag(&mut self, x: f32) {
        if let Some(sect) = self.dragging_size {
            self.set_size_from_x(sect, x);
        }
        if self.dragging_pad {
            self.set_pad_from_x(x);
        }
    }

    /// Live widget padding while the GRID view is open — applied every
    /// frame so dragging the PADDING slider works immediately.
    pub fn live_padding(&self) -> Option<u32> {
        if self.open && self.view == View::Grid {
            Some(self.grid_pad)
        } else {
            None
        }
    }

    /// Live font scales for the sliders in the FONT view — applied every
    /// frame so dragging changes the size smoothly, not on release.
    pub fn live_scales(&self) -> Option<(f32, f32)> {
        if self.open && self.view == View::Font {
            Some((
                self.cur_size[0] as f32 / 100.0,
                self.cur_size[1] as f32 / 100.0,
            ))
        } else {
            None
        }
    }

    /// Mouse button released; returns true when the configuration changed.
    pub fn release(&mut self) -> bool {
        if self.dragging_pad {
            self.dragging_pad = false;
            config::set_grid_padding(self.grid_pad);
        }
        if let Some(sect) = self.dragging_size.take() {
            let i = Self::sect_idx(sect);
            match sect {
                Sect::Term => config::set_term_font_size(self.cur_size[i]),
                Sect::Ui => config::set_ui_font_size(self.cur_size[i]),
            }
            return true;
        }
        false
    }

    pub fn show(&mut self) {
        self.open = true;
        self.view = View::Menu;
    }

    /// Opens the settings window straight at the GRID view — used by the
    /// layout editor's SETTINGS button and its CANCEL return path.
    pub fn show_grid(&mut self) {
        self.open = true;
        let (snap, cols, rows, pad) = config::grid_prefs();
        self.grid_snap = snap;
        self.grid_cols = cols;
        self.grid_rows = rows;
        self.grid_pad = pad;
        self.view = View::Grid;
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
        // Topmost element wins (dropdown items are drawn last). Elements
        // are checked BEFORE the window bounds, so dropdown items that
        // extend past the window edge remain clickable.
        let act = self
            .hits
            .iter()
            .rev()
            .find(|(r, _)| r.contains(x, y))
            .map(|&(_, a)| a);
        let Some(act) = act else {
            // No element hit: swallow the click; a click inside the
            // window closes an open dropdown.
            if modal_rect(w, h).contains(x, y) {
                self.dropdown = None;
            }
            return false;
        };
        self.flash = Some((act, Instant::now()));
        match act {
            Act::Close => self.open = false,
            Act::Back => {
                self.dropdown = None;
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
            Act::OpenGrid => {
                let (snap, cols, rows, pad) = config::grid_prefs();
                self.grid_snap = snap;
                self.grid_cols = cols;
                self.grid_rows = rows;
                self.grid_pad = pad;
                self.view = View::Grid;
            }
            Act::ToggleSnap => {
                self.grid_snap = !self.grid_snap;
                config::set_grid_snap(self.grid_snap);
            }
            Act::GridCols(d) => {
                self.grid_cols = (self.grid_cols as i32 + d).clamp(2, 32) as u32;
                config::set_grid_cols(self.grid_cols);
            }
            Act::GridRows(d) => {
                self.grid_rows = (self.grid_rows as i32 + d).clamp(2, 32) as u32;
                config::set_grid_rows(self.grid_rows);
            }
            Act::PadTrack => {
                self.dragging_pad = true;
                self.set_pad_from_x(x);
            }
            Act::EditGrid => {
                self.edit_requested = true;
                self.open = false;
            }
            Act::OpenFont => {
                self.families = [
                    crate::font::available_mono_families(),
                    crate::font::available_ui_families(),
                ];
                let (tscale, tfam, twgt) = config::term_font_prefs();
                let (uscale, ufam, uwgt) = config::ui_font_prefs();
                self.cur_size = [
                    (tscale * 100.0).round() as u32,
                    (uscale * 100.0).round() as u32,
                ];
                self.cur_family = [tfam, ufam];
                self.cur_weight = [twgt, uwgt];
                self.dropdown = None;
                self.view = View::Font;
            }
            Act::SizeTrack(sect) => {
                self.dragging_size = Some(sect);
                self.set_size_from_x(sect, x);
            }
            Act::FamilyBtn(sect) => {
                self.dropdown = if self.dropdown == Some(Dropdown::Family(sect)) {
                    None
                } else {
                    self.dropdown_since = Some(Instant::now());
                    Some(Dropdown::Family(sect))
                };
            }
            Act::WeightBtn(sect) => {
                self.dropdown = if self.dropdown == Some(Dropdown::Weight(sect)) {
                    None
                } else {
                    self.dropdown_since = Some(Instant::now());
                    Some(Dropdown::Weight(sect))
                };
            }
            Act::FamilyPick(sect, i) => {
                self.dropdown = None;
                let si = Self::sect_idx(sect);
                let value = if i == 0 {
                    // First entry: DEFAULT (auto-detected font).
                    None
                } else {
                    self.families[si].get(i - 1).cloned()
                };
                match sect {
                    Sect::Term => {
                        config::set_term_font_family(value.as_deref().unwrap_or(""))
                    }
                    Sect::Ui => config::set_ui_font_family(value.as_deref().unwrap_or("")),
                }
                self.cur_family[si] = value;
                return true;
            }
            Act::WeightPick(sect, i) => {
                self.dropdown = None;
                if let Some(w) = WEIGHTS.get(i) {
                    let si = Self::sect_idx(sect);
                    match sect {
                        Sect::Term => config::set_term_font_weight(w),
                        Sect::Ui => config::set_ui_font_weight(w),
                    }
                    self.cur_weight[si] = Some(w.to_string());
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

        // Dim the background and draw the window frame (ng_object).
        ng_object::window::backdrop(ctx, 0.55);
        let m = modal_rect(ctx.w, ctx.h);
        ng_object::window::frame(ctx, m);

        let pad = ctx.vh(1.4);
        let title_px = ctx.font_px(1.02);
        let title = match self.view {
            View::Menu => "SETTINGS",
            View::Themes => "SETTINGS \u{2014} THEMES",
            View::Look => "SETTINGS \u{2014} LOOK",
            View::Styles => "SETTINGS \u{2014} STYLES",
            View::Layauts => "SETTINGS \u{2014} LAYAUTS",
            View::Font => "SETTINGS \u{2014} FONT",
            View::Grid => "SETTINGS \u{2014} GRID",
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
                // Menu entries: THEMES, FONT and GRID.
                let bw = content.w * 0.6;
                let bx = content.x + (content.w - bw) / 2.0;
                let entries = [
                    ("THEMES", Act::OpenThemes),
                    ("FONT", Act::OpenFont),
                    ("GRID", Act::OpenGrid),
                ];
                for (i, (label, act)) in entries.into_iter().enumerate() {
                    let y = content.y + (btn_h + gap) * (i as f32 + 1.0);
                    self.button(ctx, Rect::new(bx, y, bw, btn_h), label, act);
                }
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
            View::Font => self.draw_font_view(ctx, content, btn_h, gap, corner_w),
            View::Grid => self.draw_grid_view(ctx, content, btn_h, gap, corner_w),
        }
    }

    /// GRID view: snap checkbox, column/row counts and the EDIT GRID
    /// button that enters the layout editor.
    fn draw_grid_view(
        &mut self,
        ctx: &mut Ctx,
        content: Rect,
        btn_h: f32,
        gap: f32,
        corner_w: f32,
    ) {
        let base = ctx.theme.base;
        self.button(
            ctx,
            Rect::new(content.x, content.y, corner_w, btn_h),
            "BACK",
            Act::Back,
        );

        let px = ctx.font_px(1.0);
        let mut y = content.y + btn_h + gap * 2.0;

        // SNAP TO GRID checkbox (ng_object; the whole row toggles).
        let row = Rect::new(content.x, y, content.w, btn_h);
        let hover = row.contains(ctx.mouse.0, ctx.mouse.1);
        ng_object::checkbox::draw(ctx, row, "SNAP TO GRID", self.grid_snap, hover);
        self.hits.push((row, Act::ToggleSnap));
        y += btn_h + gap;

        // COLUMNS / ROWS spinners: label left, [-] value [+] right.
        for (label, value, minus, plus) in [
            ("COLUMNS", self.grid_cols, Act::GridCols(-1), Act::GridCols(1)),
            ("ROWS", self.grid_rows, Act::GridRows(-1), Act::GridRows(1)),
        ] {
            ctx.dl.text(
                ctx.fonts,
                FONT_UI,
                px,
                content.x,
                y + (btn_h - px * 1.3) / 2.0,
                label,
                base.alpha(0.75),
                px * 0.1,
            );
            let bw = btn_h * 1.15;
            let val_w = px * 3.2;
            let plus_r = Rect::new(content.right() - bw, y, bw, btn_h);
            let minus_r = Rect::new(plus_r.x - val_w - bw, y, bw, btn_h);
            self.button(ctx, minus_r, "-", minus);
            self.button(ctx, plus_r, "+", plus);
            ctx.dl.text_center(
                ctx.fonts,
                FONT_UI,
                px,
                minus_r.right() + val_w / 2.0,
                y + (btn_h - px * 1.3) / 2.0,
                &value.to_string(),
                base,
                px * 0.1,
            );
            y += btn_h + gap;
        }

        // PADDING slider — same form as the font SIZE sliders.
        ctx.dl.text(
            ctx.fonts,
            FONT_UI,
            px,
            content.x,
            y + (btn_h - px * 1.3) / 2.0,
            "PADDING",
            base.alpha(0.75),
            px * 0.1,
        );
        let label_w = ctx.fonts.measure(FONT_UI, px, "PADDING", px * 0.1) + px * 2.0;
        let value_w = ctx.fonts.measure(FONT_UI, px, "40 PX", px * 0.05) + px;
        let track = Rect::new(content.x + label_w, y, content.w - label_w - value_w, btn_h);
        self.pad_rect = track;
        let t = (self.grid_pad as f32 / 40.0).clamp(0.0, 1.0);
        ng_object::slider::track(ctx, track, t);
        ctx.dl.text_right(
            ctx.fonts,
            FONT_UI,
            px,
            content.right(),
            y + (btn_h - px * 1.3) / 2.0,
            &format!("{} PX", self.grid_pad),
            base,
            px * 0.05,
        );
        self.hits.push((track, Act::PadTrack));
        y += btn_h + gap;

        // EDIT GRID: hides this window and enters the layout editor.
        let bw = content.w * 0.6;
        let bx = content.x + (content.w - bw) / 2.0;
        self.button(
            ctx,
            Rect::new(bx, y + gap, bw, btn_h),
            "EDIT GRID",
            Act::EditGrid,
        );
    }

    /// FONT view: TERMINAL and INTERFACE sections, each with a size
    /// slider and family/weight dropdowns, separated by module headers.
    fn draw_font_view(
        &mut self,
        ctx: &mut Ctx,
        content: Rect,
        btn_h: f32,
        gap: f32,
        corner_w: f32,
    ) {
        self.button(
            ctx,
            Rect::new(content.x, content.y, corner_w, btn_h),
            "BACK",
            Act::Back,
        );

        let mut y = content.y + btn_h + gap;
        let mut anchors: Vec<(Sect, Rect, Rect)> = Vec::new();
        for (sect, header) in [(Sect::Term, "TERMINAL"), (Sect::Ui, "INTERFACE")] {
            let (fam_rect, wgt_rect, next_y) =
                self.draw_font_section(ctx, content, y, btn_h, gap, sect, header);
            anchors.push((sect, fam_rect, wgt_rect));
            y = next_y;
        }

        // Open dropdown list (drawn last = on top, reverse hit-testing).
        let item_h = btn_h * 0.8;
        for (sect, fam_rect, wgt_rect) in anchors {
            match self.dropdown {
                Some(Dropdown::Family(d)) if d == sect => {
                    let si = Self::sect_idx(sect);
                    let mut names = vec!["DEFAULT".to_string()];
                    names.extend(self.families[si].iter().map(|f| f.to_uppercase()));
                    self.draw_dropdown(ctx, fam_rect, item_h, &names, |i| {
                        Act::FamilyPick(sect, i)
                    });
                }
                Some(Dropdown::Weight(d)) if d == sect => {
                    let names: Vec<String> =
                        WEIGHTS.iter().map(|w| w.to_uppercase()).collect();
                    self.draw_dropdown(ctx, wgt_rect, item_h, &names, |i| {
                        Act::WeightPick(sect, i)
                    });
                }
                _ => {}
            }
        }
    }

    /// One font section: header separator + SIZE slider + FAMILY/WEIGHT
    /// buttons. Returns the two dropdown anchors and the next free y.
    #[allow(clippy::too_many_arguments)]
    fn draw_font_section(
        &mut self,
        ctx: &mut Ctx,
        content: Rect,
        top: f32,
        btn_h: f32,
        gap: f32,
        sect: Sect,
        header: &str,
    ) -> (Rect, Rect, f32) {
        let base = ctx.theme.base;
        let si = Self::sect_idx(sect);
        let title_px = ctx.font_px(1.02);
        // Section separator like every other module header.
        ctx.dl.module_title(
            ctx.fonts,
            content.x,
            top,
            content.w,
            title_px,
            header,
            "",
            base,
        );

        let px = ctx.font_px(1.0);
        let row_x = content.x;
        let row_w = content.w;

        // SIZE: label, slider track with a knob, percent value.
        let size_y = top + title_px * 2.4;
        ctx.dl.text(
            ctx.fonts,
            FONT_UI,
            px,
            row_x,
            size_y + (btn_h - px * 1.3) / 2.0,
            "SIZE",
            base,
            px * 0.1,
        );
        let label_w = ctx.fonts.measure(FONT_UI, px, "SIZE", px * 0.1) + px * 2.0;
        let value_w = ctx.fonts.measure(FONT_UI, px, "200%", px * 0.05) + px;
        let track = Rect::new(row_x + label_w, size_y, row_w - label_w - value_w, btn_h);
        self.slider_rect[si] = track;
        let (rmin, rmax) = Self::size_range(sect);
        let t = ((self.cur_size[si] as f32 - rmin) / (rmax - rmin)).clamp(0.0, 1.0);
        ng_object::slider::track(ctx, track, t);
        ctx.dl.text_right(
            ctx.fonts,
            FONT_UI,
            px,
            content.right(),
            size_y + (btn_h - px * 1.3) / 2.0,
            &format!("{}%", self.cur_size[si]),
            base,
            px * 0.05,
        );
        self.hits.push((track, Act::SizeTrack(sect)));

        // FAMILY and WEIGHT dropdown buttons.
        let fam_y = size_y + btn_h + gap;
        let fam_label = format!(
            "FAMILY: {}",
            self.cur_family[si].as_deref().unwrap_or("DEFAULT").to_uppercase()
        );
        let fam_rect = Rect::new(row_x, fam_y, row_w, btn_h);
        self.button(ctx, fam_rect, &fam_label, Act::FamilyBtn(sect));

        let wgt_y = fam_y + btn_h + gap;
        let wgt_label = format!(
            "WEIGHT: {}",
            self.cur_weight[si].as_deref().unwrap_or("REGULAR").to_uppercase()
        );
        let wgt_rect = Rect::new(row_x, wgt_y, row_w, btn_h);
        self.button(ctx, wgt_rect, &wgt_label, Act::WeightBtn(sect));

        (fam_rect, wgt_rect, wgt_y + btn_h + gap)
    }

    /// Dropdown list under an anchor button.
    fn draw_dropdown<F: Fn(usize) -> Act>(
        &mut self,
        ctx: &mut Ctx,
        anchor: Rect,
        item_h: f32,
        names: &[String],
        make_act: F,
    ) {
        // Accordion animation: the list unfolds from the anchor's edge.
        let t = self
            .dropdown_since
            .map(|s| (s.elapsed().as_secs_f32() / 0.15).clamp(0.0, 1.0))
            .unwrap_or(1.0);
        let p = 1.0 - (1.0 - t) * (1.0 - t); // ease-out
        for (i, (r, _full)) in ng_object::dropdown::accordion(ctx, anchor, item_h, names, p)
            .into_iter()
            .enumerate()
        {
            self.hits.push((r, make_act(i)));
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
        // With an open dropdown only its items react to the mouse.
        let hover = self.dropdown.is_none() && r.contains(ctx.mouse.0, ctx.mouse.1);
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
        let st = ng_object::button::ButtonState { hover, flash, selected: is_current };
        if act == Act::Back {
            // The base button (ng_object) plus a left arrow and a label
            // shifted to make room for it.
            ng_object::button::draw(ctx, r, "", st);
            let px = ctx.font_px(1.0);
            let color = if hover || flash || is_current { base } else { base.alpha(0.7) };
            let skew = r.h * 0.7;
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
            ng_object::button::draw(ctx, r, label, st);
        }
        self.hits.push((r, act));
    }
}
