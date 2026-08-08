//! Layout editor: an Android-style snap grid over the live interface.
//! Entered from SETTINGS -> GRID -> EDIT GRID. The grid becomes visible;
//! panels can be moved by dragging, resized by dragging their edges or
//! corners, removed with the X in their top-right corner and added back
//! via the ADD WIDGET button (hold an entry for 5 seconds — the list
//! hides and the widget follows the cursor until you drop it on the
//! grid). With SNAP TO GRID enabled every panel edge is aligned to the
//! grid cells — including an automatic fit of all panels when the editor
//! opens. The editor works on the OUTER panel rectangles; the widget
//! padding (SETTINGS -> GRID) insets the content inside them.
//! Bottom-right buttons: ADD WIDGET, SAVE (overwrites the currently
//! selected layout), SAVE AS (asks for a name) and CANCEL (exits
//! without saving).

use super::{Ctx, Layout, LayoutSpec, Panel, PanelSpec, Rect, OFF_SPEC, PANEL_COUNT};
use crate::font::FONT_UI;
use std::time::Instant;

/// Edge-grab margin in px (resize handles).
const EDGE: f32 = 8.0;
/// Minimum CONTENT size of a panel in px — the outer rectangle can
/// never shrink below the padding plus this much content.
const MIN_CONTENT: f32 = 30.0;
/// Hold time on an ADD WIDGET entry before placement starts.
const HOLD_SECS: f32 = 5.0;

/// What a mouse press on the editor resolved to.
pub enum EditorHit {
    /// Handled internally (drag started, widget list, empty space).
    Handled,
    /// SETTINGS — show/hide the settings window over the editor.
    Settings,
    /// SAVE — overwrite the currently selected layout.
    Save,
    /// SAVE AS — open the name prompt.
    SaveAs,
    /// EXIT — leave the editor without saving.
    Exit,
}

/// Cursor shape the editor wants at a given position.
#[derive(Clone, Copy, PartialEq)]
pub enum CursorKind {
    Normal,
    Move,
    Ew,
    Ns,
    Nwse,
    Nesw,
}

/// Active drag: moving the panel or resizing by its edges.
enum Mode {
    Move { dx: f32, dy: f32 },
    Resize { l: bool, r: bool, t: bool, b: bool },
}

pub struct Editor {
    pub active: bool,
    pub snap: bool,
    pub cols: u32,
    pub rows: u32,
    /// Widget padding: the outer rect is always this much larger than
    /// the inner content container on every side.
    padding: f32,
    /// Edited panel rects in percent of the window, Panel order.
    rects: [PanelSpec; PANEL_COUNT],
    /// The rects as they were when the editor opened — SAVE stores only
    /// the panels that differ from this.
    initial: [PanelSpec; PANEL_COUNT],
    drag: Option<(usize, Mode)>,
    /// SAVE AS name being typed; Some = the prompt is open.
    pub naming: Option<String>,
    /// ADD WIDGET list window.
    add_open: bool,
    /// Held list entry: (panel index, hold start).
    adding: Option<(usize, Instant)>,
    /// Pull-out animation after a completed hold: the widget grows from
    /// its miniature size to the placement size under the cursor.
    grow: Option<(usize, Instant, f32, f32)>,
    flash: Option<(usize, Instant)>,
}

fn pct(r: Rect, w: f32, h: f32) -> PanelSpec {
    PanelSpec {
        x: r.x / w * 100.0,
        y: r.y / h * 100.0,
        w: r.w / w * 100.0,
        h: r.h / h * 100.0,
    }
}

fn on_screen(p: &PanelSpec) -> bool {
    p.x < 100.0
}

impl Editor {
    pub fn new() -> Self {
        Editor {
            active: false,
            snap: false,
            cols: 12,
            rows: 8,
            padding: 8.0,
            rects: [OFF_SPEC; PANEL_COUNT],
            initial: [OFF_SPEC; PANEL_COUNT],
            drag: None,
            naming: None,
            add_open: false,
            adding: None,
            grow: None,
            flash: None,
        }
    }

    /// Enters edit mode with the CURRENT panel rectangles (WYSIWYG).
    /// With snapping enabled all panels are fitted to the grid at once.
    pub fn start(
        &mut self,
        layout: &Layout,
        w: f32,
        h: f32,
        snap: bool,
        cols: u32,
        rows: u32,
        padding: f32,
    ) {
        self.active = true;
        self.snap = snap;
        self.cols = cols.max(2);
        self.rows = rows.max(2);
        self.padding = padding.max(0.0);
        self.naming = None;
        self.drag = None;
        self.add_open = false;
        self.adding = None;
        self.grow = None;
        self.rects = std::array::from_fn(|i| pct(layout.panels[i], w, h));
        if self.snap {
            self.snap_all(w, h);
        }
        self.initial = self.rects;
    }

    pub fn stop(&mut self) {
        self.active = false;
        self.naming = None;
        self.drag = None;
        self.add_open = false;
        self.adding = None;
        self.grow = None;
    }

    /// Fits every visible panel to the grid: each edge lands on the
    /// nearest cell boundary.
    fn snap_all(&mut self, w: f32, h: f32) {
        let cw = w / self.cols as f32;
        let ch = h / self.rows as f32;
        for i in 0..self.rects.len() {
            if !on_screen(&self.rects[i]) {
                continue;
            }
            let r = self.px_rect(i, w, h);
            let c0 = (r.x / cw).round().clamp(0.0, self.cols as f32 - 1.0);
            let c1 = ((r.right()) / cw).round().clamp(c0 + 1.0, self.cols as f32);
            let r0 = (r.y / ch).round().clamp(0.0, self.rows as f32 - 1.0);
            let r1 = ((r.bottom()) / ch).round().clamp(r0 + 1.0, self.rows as f32);
            let snapped =
                Rect::new(c0 * cw, r0 * ch, (c1 - c0) * cw, (r1 - r0) * ch);
            self.rects[i] = pct(snapped, w, h);
        }
    }

    fn px_rect(&self, i: usize, w: f32, h: f32) -> Rect {
        let p = &self.rects[i];
        Rect::new(p.x / 100.0 * w, p.y / 100.0 * h, p.w / 100.0 * w, p.h / 100.0 * h)
    }

    /// The edited layout in window pixels (drawn instead of the normal one).
    pub fn layout(&self, w: f32, h: f32) -> Layout {
        Layout { panels: std::array::from_fn(|i| self.px_rect(i, w, h)) }
    }

    /// The edited layout as a percent spec for saving.
    pub fn spec(&self) -> LayoutSpec {
        LayoutSpec { panels: self.rects }
    }

    /// Panels whose rectangles differ from the given reference spec
    /// (with a small tolerance) — the "only the changes" save payload.
    pub fn changes_vs(&self, reference: &LayoutSpec) -> Vec<(Panel, PanelSpec)> {
        let mut out = Vec::new();
        for panel in Panel::ALL {
            let a = &self.rects[panel.idx()];
            let b = reference.p(panel);
            let both_hidden = a.x >= 100.0 && b.x >= 100.0;
            let same = (a.x - b.x).abs() < 0.05
                && (a.y - b.y).abs() < 0.05
                && (a.w - b.w).abs() < 0.05
                && (a.h - b.h).abs() < 0.05;
            if !both_hidden && !same {
                out.push((panel, *a));
            }
        }
        out
    }

    /// Panels changed since the editor was opened.
    pub fn changes_since_start(&self) -> Vec<(Panel, PanelSpec)> {
        self.changes_vs(&LayoutSpec { panels: self.initial })
    }

    fn save_buttons(w: f32, h: f32) -> [Rect; 6] {
        let bw = (w * 0.10).max(110.0);
        let bh = (h * 0.045).max(30.0);
        let gap = bh * 0.35;
        let x = w - bw - w * 0.012;
        let y1 = h - 6.0 * bh - 5.0 * gap - h * 0.02;
        std::array::from_fn(|i| Rect::new(x, y1 + i as f32 * (bh + gap), bw, bh))
    }

    /// Applies grid preferences changed in the settings window while the
    /// editor is running; enabling snap auto-fits all panels.
    pub fn sync_prefs(&mut self, snap: bool, cols: u32, rows: u32, padding: f32, w: f32, h: f32) {
        let was = self.snap;
        self.cols = cols.max(2);
        self.rows = rows.max(2);
        self.padding = padding.max(0.0);
        self.snap = snap;
        if snap && !was {
            self.snap_all(w, h);
        }
    }

    /// Hidden panels offered by the ADD WIDGET window.
    fn hidden_panels(&self) -> Vec<usize> {
        (0..self.rects.len()).filter(|&i| !on_screen(&self.rects[i])).collect()
    }

    /// ADD WIDGET window rect and its item rects (widget miniatures).
    fn add_list_rects(&self, w: f32, h: f32) -> (Rect, Vec<Rect>) {
        let items = self.hidden_panels().len().max(1);
        let bw = (w * 0.30).max(300.0);
        let pad = (h * 0.012).max(6.0);
        let title_h = (h * 0.05).max(30.0);
        // Miniature height: 16:9-ish, shrunk so everything fits on screen.
        let ih = (h * 0.14)
            .max(64.0)
            .min((h * 0.86 - title_h - pad) / items as f32 - pad);
        let bh = title_h + items as f32 * (ih + pad) + pad * 2.0;
        let bx = (w - bw) / 2.0;
        let by = (h - bh) / 2.0;
        let list = (0..items)
            .map(|i| {
                Rect::new(
                    bx + pad,
                    by + title_h + pad + i as f32 * (ih + pad),
                    bw - 2.0 * pad,
                    ih,
                )
            })
            .collect();
        (Rect::new(bx, by, bw, bh), list)
    }

    /// The X (remove) button rect of a panel.
    fn x_rect(r: &Rect) -> Rect {
        let s = 18.0f32.min(r.w * 0.2).min(r.h * 0.4);
        Rect::new(r.right() - s - 4.0, r.y + 4.0, s, s)
    }

    /// Topmost panel whose body or edge area contains the point,
    /// with the edge flags: (index, left, right, top, bottom).
    fn panel_at(&self, x: f32, y: f32, w: f32, h: f32) -> Option<(usize, bool, bool, bool, bool)> {
        for i in (0..self.rects.len()).rev() {
            if !on_screen(&self.rects[i]) {
                continue;
            }
            let r = self.px_rect(i, w, h);
            let outer = Rect::new(
                r.x - EDGE,
                r.y - EDGE,
                r.w + 2.0 * EDGE,
                r.h + 2.0 * EDGE,
            );
            if !outer.contains(x, y) {
                continue;
            }
            let l = (x - r.x).abs() <= EDGE;
            let rr = (x - r.right()).abs() <= EDGE;
            let t = (y - r.y).abs() <= EDGE;
            let b = (y - r.bottom()).abs() <= EDGE;
            if l || rr || t || b || r.contains(x, y) {
                return Some((i, l, rr, t, b));
            }
        }
        None
    }

    /// True when the point is over the editor's own controls (buttons,
    /// the ADD WIDGET window or the name prompt) — nothing underneath
    /// may react or highlight then.
    fn over_ui(&self, x: f32, y: f32, w: f32, h: f32) -> bool {
        if self.naming.is_some() || self.add_open {
            return true;
        }
        Self::save_buttons(w, h).iter().any(|b| b.contains(x, y))
    }

    /// Cursor shape for the given position (resize arrows on the edges).
    pub fn cursor_at(&self, x: f32, y: f32, w: f32, h: f32) -> CursorKind {
        if let Some((i, mode)) = &self.drag {
            if self.rects[*i].x >= 100.0 {
                return CursorKind::Normal;
            }
            return match mode {
                Mode::Move { .. } => CursorKind::Move,
                Mode::Resize { l, r, t, b } => edge_cursor(*l, *r, *t, *b),
            };
        }
        if self.over_ui(x, y, w, h) {
            return CursorKind::Normal;
        }
        match self.panel_at(x, y, w, h) {
            Some((i, l, r, t, b)) => {
                let pr = self.px_rect(i, w, h);
                if Self::x_rect(&pr).contains(x, y) {
                    CursorKind::Normal
                } else if l || r || t || b {
                    edge_cursor(l, r, t, b)
                } else {
                    CursorKind::Move
                }
            }
            None => CursorKind::Normal,
        }
    }

    /// Hit-test of the editor buttons only — also used while the
    /// settings window is open over the editor (the buttons share its
    /// plane and stay clickable).
    pub fn buttons_hit(&mut self, x: f32, y: f32, w: f32, h: f32) -> Option<EditorHit> {
        let btns = Self::save_buttons(w, h);
        if btns[0].contains(x, y) {
            // SETTINGS — show/hide the window over the editor.
            self.flash = Some((0, Instant::now()));
            return Some(EditorHit::Settings);
        }
        if btns[1].contains(x, y) {
            // ADD WIDGET — toggle the list window (handled internally).
            self.flash = Some((1, Instant::now()));
            self.add_open = true;
            return Some(EditorHit::Handled);
        }
        if btns[2].contains(x, y) {
            self.flash = Some((2, Instant::now()));
            return Some(EditorHit::Save);
        }
        if btns[3].contains(x, y) {
            self.flash = Some((3, Instant::now()));
            return Some(EditorHit::SaveAs);
        }
        if btns[4].contains(x, y) {
            // CANCEL — revert the unsaved changes, stay in the editor.
            self.flash = Some((4, Instant::now()));
            self.rects = self.initial;
            self.drag = None;
            self.grow = None;
            return Some(EditorHit::Handled);
        }
        if btns[5].contains(x, y) {
            self.flash = Some((5, Instant::now()));
            return Some(EditorHit::Exit);
        }
        None
    }

    /// Mouse press. Only meaningful while active.
    pub fn mouse_down(&mut self, x: f32, y: f32, w: f32, h: f32) -> EditorHit {
        if self.naming.is_some() {
            // The name prompt is keyboard-driven; clicks fall through.
            return EditorHit::Handled;
        }
        if self.add_open {
            // Hold an entry to start placing it; any other click closes.
            let (_, items) = self.add_list_rects(w, h);
            let hidden = self.hidden_panels();
            for (slot, ir) in items.iter().enumerate() {
                if ir.contains(x, y) {
                    if let Some(&panel) = hidden.get(slot) {
                        self.adding = Some((panel, Instant::now()));
                    }
                    return EditorHit::Handled;
                }
            }
            self.add_open = false;
            return EditorHit::Handled;
        }
        if let Some(hit) = self.buttons_hit(x, y, w, h) {
            return hit;
        }
        if let Some((i, l, rr, t, b)) = self.panel_at(x, y, w, h) {
            let r = self.px_rect(i, w, h);
            // X in the top-right corner removes the widget from the grid.
            if Self::x_rect(&r).contains(x, y) {
                self.rects[i] = OFF_SPEC;
                self.drag = None;
                ng_base::sound::emit(ng_base::sound::Event::Drop);
                return EditorHit::Handled;
            }
            if l || rr || t || b {
                self.drag = Some((i, Mode::Resize { l, r: rr, t, b }));
            } else {
                self.drag = Some((i, Mode::Move { dx: x - r.x, dy: y - r.y }));
            }
            ng_base::sound::emit(ng_base::sound::Event::Grab);
        }
        EditorHit::Handled
    }

    /// Mouse move while a panel is being dragged or resized.
    pub fn mouse_move(&mut self, x: f32, y: f32, w: f32, h: f32) {
        // Wandering far away from the held ADD WIDGET entry cancels the
        // hold (a generous margin — small drift while holding is fine).
        if let Some((panel, _)) = self.adding {
            let (_, items) = self.add_list_rects(w, h);
            let hidden = self.hidden_panels();
            let still = hidden
                .iter()
                .position(|&p| p == panel)
                .and_then(|slot| items.get(slot))
                .map(|ir| {
                    let m = 30.0;
                    Rect::new(ir.x - m, ir.y - m, ir.w + 2.0 * m, ir.h + 2.0 * m)
                        .contains(x, y)
                })
                .unwrap_or(false);
            if !still {
                self.adding = None;
            }
        }
        let Some((i, mode)) = &self.drag else { return };
        let i = *i;
        let cw = w / self.cols as f32;
        let ch = h / self.rows as f32;
        let r = self.px_rect(i, w, h);
        match mode {
            Mode::Move { dx, dy } => {
                let mut nx = (x - dx).clamp(0.0, (w - r.w).max(0.0));
                let mut ny = (y - dy).clamp(0.0, (h - r.h).max(0.0));
                if self.snap {
                    // The panel's corner sticks to the nearest cell boundary.
                    nx = (nx / cw).round() * cw;
                    ny = (ny / ch).round() * ch;
                    nx = nx.clamp(0.0, (w - r.w).max(0.0));
                    ny = ny.clamp(0.0, (h - r.h).max(0.0));
                }
                self.rects[i].x = nx / w * 100.0;
                self.rects[i].y = ny / h * 100.0;
            }
            Mode::Resize { l, r: rr, t, b } => {
                let (l, rr, t, b) = (*l, *rr, *t, *b);
                let (mut x0, mut x1) = (r.x, r.right());
                let (mut y0, mut y1) = (r.y, r.bottom());
                let m = self.min_outer();
                let min_w = if self.snap { cw.max(m) } else { m };
                let min_h = if self.snap { ch.max(m) } else { m };
                // In a tiny window (or a dense grid) the minimum size can
                // exceed the space available on the opposite side, which
                // would make the clamp bounds cross (lo > hi) and panic;
                // oclamp orders them so it never does.
                let oclamp = |v: f32, lo: f32, hi: f32| {
                    if hi < lo { lo } else { v.clamp(lo, hi) }
                };
                if l {
                    x0 = oclamp(x, 0.0, x1 - min_w);
                    if self.snap {
                        x0 = oclamp((x0 / cw).round() * cw, 0.0, x1 - min_w);
                    }
                }
                if rr {
                    x1 = oclamp(x, x0 + min_w, w);
                    if self.snap {
                        x1 = oclamp((x1 / cw).round() * cw, x0 + min_w, w);
                    }
                }
                if t {
                    y0 = oclamp(y, 0.0, y1 - min_h);
                    if self.snap {
                        y0 = oclamp((y0 / ch).round() * ch, 0.0, y1 - min_h);
                    }
                }
                if b {
                    y1 = oclamp(y, y0 + min_h, h);
                    if self.snap {
                        y1 = oclamp((y1 / ch).round() * ch, y0 + min_h, h);
                    }
                }
                self.rects[i] = pct(Rect::new(x0, y0, (x1 - x0).max(1.0), (y1 - y0).max(1.0)), w, h);
            }
        }
    }

    pub fn mouse_up(&mut self) {
        if self.drag.is_some() {
            // Snapping makes the release land on the grid, so it gets
            // the sharper confirmation of the two.
            ng_base::sound::emit(if self.snap {
                ng_base::sound::Event::Snap
            } else {
                ng_base::sound::Event::Drop
            });
        }
        self.drag = None;
        self.adding = None;
        // Releasing mid-animation finishes the growth instantly.
        self.grow = None;
    }

    /// Feeds a typed character into the SAVE AS prompt.
    pub fn type_char(&mut self, text: &str) {
        if let Some(name) = self.naming.as_mut() {
            for ch in text.chars() {
                let ch = ch.to_ascii_lowercase();
                if (ch.is_ascii_alphanumeric() || ch == '-' || ch == '_') && name.len() < 40 {
                    name.push(ch);
                }
            }
        }
    }

    pub fn backspace(&mut self) {
        if let Some(name) = self.naming.as_mut() {
            name.pop();
        }
    }

    /// Opaque parallelogram button (ng_object).
    fn draw_button(ctx: &mut Ctx, br: &Rect, label: &str, hover: bool, flash: bool) {
        ng_object::button::draw(
            ctx,
            *br,
            label,
            ng_object::button::ButtonState { hover, flash, selected: false },
        );
    }

    /// The smallest allowed OUTER panel size: padding on both sides
    /// plus the minimum content.
    fn min_outer(&self) -> f32 {
        2.0 * self.padding + MIN_CONTENT
    }

    /// Placement size of a freshly added widget.
    fn spawn_size(&self, w: f32, h: f32) -> (f32, f32) {
        let m = self.min_outer();
        if self.snap {
            ((w / self.cols as f32 * 3.0).max(m), (h / self.rows as f32 * 2.0).max(m))
        } else {
            ((w * 0.20).max(m), (h * 0.25).max(m))
        }
    }

    /// Draws just the editor's button stack — called from draw() and
    /// again ON TOP of the settings window when it is open over the
    /// editor, so the buttons share the window's plane.
    pub fn draw_buttons(&mut self, ctx: &mut Ctx) {
        let (w, h) = (ctx.w, ctx.h);
        let (mx, my) = ctx.mouse;
        let now = Instant::now();
        let btns = Self::save_buttons(w, h);
        let labels = ["SETTINGS", "ADD WIDGET", "SAVE", "SAVE AS", "CANCEL", "EXIT"];
        for (i, br) in btns.iter().enumerate() {
            let hover = !self.add_open && self.naming.is_none() && br.contains(mx, my);
            let flash = self
                .flash
                .map(|(fi, t)| fi == i && now.duration_since(t).as_secs_f32() < 0.15)
                .unwrap_or(false);
            Self::draw_button(ctx, br, labels[i], hover, flash);
        }
    }

    /// Draws the visible grid, panel outlines and the editor controls on
    /// top of the live interface. The `mini` callback draws a live
    /// miniature of the given panel into a rectangle (used by the ADD
    /// WIDGET window). Also advances the ADD WIDGET hold — after 5
    /// seconds the widget pulls out of the window, grows and follows
    /// the cursor.
    pub fn draw<F: FnMut(&mut Ctx, usize, Rect)>(&mut self, ctx: &mut Ctx, mut mini: F) {
        let base = ctx.theme.base;
        let (w, h) = (ctx.w, ctx.h);
        let (mx, my) = ctx.mouse;

        // ADD WIDGET hold finished -> the widget pulls out of the window
        // (it starts at its miniature size and grows under the cursor).
        if let Some((panel, t0)) = self.adding {
            if t0.elapsed().as_secs_f32() >= HOLD_SECS {
                let (_, items) = self.add_list_rects(w, h);
                let slot = self.hidden_panels().iter().position(|&p| p == panel);
                let (mw, mh) = slot
                    .and_then(|s| items.get(s))
                    .map(|ir| (ir.w, ir.h))
                    .unwrap_or((w * 0.1, h * 0.1));
                self.adding = None;
                self.add_open = false;
                let r = Rect::new(
                    (mx - mw / 2.0).clamp(0.0, (w - mw).max(0.0)),
                    (my - mh / 2.0).clamp(0.0, (h - mh).max(0.0)),
                    mw,
                    mh,
                );
                self.rects[panel] = pct(r, w, h);
                self.drag = Some((panel, Mode::Move { dx: mw / 2.0, dy: mh / 2.0 }));
                self.grow = Some((panel, Instant::now(), mw, mh));
            }
        }

        // Growth animation: miniature -> placement size, centred on the
        // cursor while it is being dragged.
        if let Some((panel, t0, mw, mh)) = self.grow {
            let t = (t0.elapsed().as_secs_f32() / 0.25).min(1.0);
            let e = 1.0 - (1.0 - t) * (1.0 - t);
            let (tw, th) = self.spawn_size(w, h);
            let (cw_, ch_) = (mw + (tw - mw) * e, mh + (th - mh) * e);
            let r = self.px_rect(panel, w, h);
            let (cx, cy) = (r.x + r.w / 2.0, r.y + r.h / 2.0);
            let nr = Rect::new(
                (cx - cw_ / 2.0).clamp(0.0, (w - cw_).max(0.0)),
                (cy - ch_ / 2.0).clamp(0.0, (h - ch_).max(0.0)),
                cw_,
                ch_,
            );
            self.rects[panel] = pct(nr, w, h);
            if let Some((di, mode)) = self.drag.as_mut() {
                if *di == panel {
                    if let Mode::Move { dx, dy } = mode {
                        *dx = cw_ / 2.0;
                        *dy = ch_ / 2.0;
                    }
                }
            }
            if t >= 1.0 {
                self.grow = None;
                if self.snap && self.drag.is_none() {
                    self.snap_all(w, h);
                }
            }
        }

        // The visible grid.
        let grid_c = base.alpha(0.16);
        for i in 0..=self.cols {
            let x = i as f32 / self.cols as f32 * w;
            ctx.dl.line(x, 0.0, x, h, 1.0, grid_c);
        }
        for i in 0..=self.rows {
            let y = i as f32 / self.rows as f32 * h;
            ctx.dl.line(0.0, y, w, y, 1.0, grid_c);
        }

        // Nothing under the editor's own controls reacts to the cursor.
        let ui_hover = self.over_ui(mx, my, w, h);

        // Panel outlines, name tags and remove buttons.
        let dragged = self.drag.as_ref().map(|(i, _)| *i);
        for i in 0..self.rects.len() {
            if !on_screen(&self.rects[i]) {
                continue;
            }
            let r = self.px_rect(i, w, h);
            let hot = dragged == Some(i)
                || (dragged.is_none() && !ui_hover && {
                    let outer = Rect::new(
                        r.x - EDGE,
                        r.y - EDGE,
                        r.w + 2.0 * EDGE,
                        r.h + 2.0 * EDGE,
                    );
                    outer.contains(mx, my)
                });
            if hot {
                ctx.dl.rect(r.x, r.y, r.w, r.h, base.alpha(0.08));
            }
            ctx.dl.rect_outline(
                r.x,
                r.y,
                r.w,
                r.h,
                if hot { 2.0 } else { 1.0 },
                base.alpha(if hot { 0.9 } else { 0.45 }),
            );
            // Corner resize handles on the hot panel.
            if hot {
                let s = 6.0;
                for (cx, cy) in [
                    (r.x, r.y),
                    (r.right(), r.y),
                    (r.x, r.bottom()),
                    (r.right(), r.bottom()),
                ] {
                    ctx.dl.rect(cx - s / 2.0, cy - s / 2.0, s, s, base);
                }
            }
            let px = ctx.font_px(0.95);
            let label = Panel::ALL[i].label();
            let tw = ctx.fonts.measure(FONT_UI, px, label, px * 0.1);
            ctx.dl
                .rect(r.x, r.y, tw + px * 1.2, px * 1.7, ctx.theme.bg.alpha(0.9));
            ctx.dl.text(
                ctx.fonts,
                FONT_UI,
                px,
                r.x + px * 0.6,
                r.y + px * 0.25,
                label,
                if hot { base } else { base.alpha(0.75) },
                px * 0.1,
            );
            // X in the top-right corner removes the widget from the grid.
            let xr = Self::x_rect(&r);
            let x_hot = !ui_hover && xr.contains(mx, my);
            ctx.dl.rect(xr.x, xr.y, xr.w, xr.h, ctx.theme.bg);
            ctx.dl.rect_outline(
                xr.x,
                xr.y,
                xr.w,
                xr.h,
                1.0,
                base.alpha(if x_hot { 0.9 } else { 0.45 }),
            );
            let m = xr.w * 0.28;
            let c = if x_hot { base } else { base.alpha(0.6) };
            ctx.dl
                .line(xr.x + m, xr.y + m, xr.right() - m, xr.bottom() - m, 1.5, c);
            ctx.dl
                .line(xr.right() - m, xr.y + m, xr.x + m, xr.bottom() - m, 1.5, c);
        }

        // The editor buttons in the bottom-right corner.
        self.draw_buttons(ctx);

        // Hint line in the bottom-left corner.
        let hint_px = ctx.font_px(0.9);
        ctx.dl.text(
            ctx.fonts,
            FONT_UI,
            hint_px,
            w * 0.012,
            h - hint_px * 2.0,
            "DRAG TO MOVE \u{2014} DRAG EDGES TO RESIZE \u{2014} X REMOVES \u{2014} ESC EXITS WITHOUT SAVING",
            base.alpha(0.6),
            hint_px * 0.08,
        );

        // ADD WIDGET list window (opaque).
        if self.add_open {
            let (win, items) = self.add_list_rects(w, h);
            ng_object::window::backdrop(ctx, 0.4);
            ng_object::window::frame(ctx, win);
            let tpx = ctx.font_px(0.95);
            ctx.dl.text_center(
                ctx.fonts,
                FONT_UI,
                tpx,
                win.cx(),
                win.y + tpx * 0.6,
                "ADD WIDGET \u{2014} HOLD 5S TO PLACE",
                base.alpha(0.7),
                tpx * 0.12,
            );
            let hidden = self.hidden_panels();
            if hidden.is_empty() {
                let px = ctx.font_px(0.95);
                ctx.dl.text_center(
                    ctx.fonts,
                    FONT_UI,
                    px,
                    win.cx(),
                    win.y + win.h / 2.0,
                    "ALL WIDGETS ARE PLACED",
                    base.alpha(0.6),
                    px * 0.1,
                );
            }
            for (slot, ir) in items.iter().enumerate() {
                let Some(&panel) = hidden.get(slot) else { break };
                let held = self.adding.map(|(p, _)| p == panel).unwrap_or(false);
                let hover = ir.contains(mx, my);
                ctx.dl.rect(ir.x, ir.y, ir.w, ir.h, ctx.theme.bg);
                // Live miniature of the widget (headers drawn above the
                // rect by some widgets get a little headroom).
                let head = ctx.vh(2.0);
                let m = 6.0;
                mini(
                    ctx,
                    panel,
                    Rect::new(
                        ir.x + m,
                        ir.y + m + head,
                        ir.w - 2.0 * m,
                        (ir.h - 2.0 * m - head).max(10.0),
                    ),
                );
                // Hold progress fills the entry from the left.
                if held {
                    if let Some((_, t0)) = self.adding {
                        let t = (t0.elapsed().as_secs_f32() / HOLD_SECS).clamp(0.0, 1.0);
                        ctx.dl.rect(ir.x, ir.y, ir.w * t, ir.h, base.alpha(0.25));
                    }
                } else if hover {
                    ctx.dl.rect(ir.x, ir.y, ir.w, ir.h, base.alpha(0.08));
                }
                ctx.dl.rect_outline(
                    ir.x,
                    ir.y,
                    ir.w,
                    ir.h,
                    1.0,
                    base.alpha(if hover || held { 0.8 } else { 0.4 }),
                );
                // Small name tag like on the panels.
                let px = ctx.font_px(0.85);
                let tw = ctx.fonts.measure(FONT_UI, px, Panel::ALL[panel].label(), px * 0.1);
                ctx.dl
                    .rect(ir.x, ir.y, tw + px * 1.2, px * 1.7, ctx.theme.bg.alpha(0.9));
                ctx.dl.text(
                    ctx.fonts,
                    FONT_UI,
                    px,
                    ir.x + px * 0.6,
                    ir.y + px * 0.25,
                    Panel::ALL[panel].label(),
                    if hover || held { base } else { base.alpha(0.75) },
                    px * 0.1,
                );
            }
        }

        // SAVE AS name prompt.
        if let Some(name) = self.naming.clone() {
            ng_object::window::backdrop(ctx, 0.55);
            let bw = (w * 0.4).max(320.0);
            let bh = (h * 0.16).max(110.0);
            let bx = (w - bw) / 2.0;
            let by = (h - bh) / 2.0;
            ng_object::window::frame(ctx, Rect::new(bx, by, bw, bh));
            let tpx = ctx.font_px(1.0);
            ctx.dl.text_center(
                ctx.fonts,
                FONT_UI,
                tpx,
                bx + bw / 2.0,
                by + bh * 0.14,
                "SAVE AS \u{2014} TYPE A NAME",
                base.alpha(0.7),
                tpx * 0.15,
            );
            // Name field with a blinking cursor.
            let npx = ctx.font_px(1.25);
            let shown = format!("{name}{}", if ctx.t.fract() < 0.6 { "_" } else { " " });
            ctx.dl.text_center(
                ctx.fonts,
                FONT_UI,
                npx,
                bx + bw / 2.0,
                by + bh * 0.42,
                &shown,
                base,
                npx * 0.1,
            );
            let hpx = ctx.font_px(0.85);
            ctx.dl.text_center(
                ctx.fonts,
                FONT_UI,
                hpx,
                bx + bw / 2.0,
                by + bh * 0.72,
                "ENTER SAVES \u{2014} ESC CANCELS",
                base.alpha(0.5),
                hpx * 0.1,
            );
        }
    }
}

/// Resize cursor for the given edge combination.
fn edge_cursor(l: bool, r: bool, t: bool, b: bool) -> CursorKind {
    if (l && t) || (r && b) {
        CursorKind::Nwse
    } else if (r && t) || (l && b) {
        CursorKind::Nesw
    } else if l || r {
        CursorKind::Ew
    } else {
        CursorKind::Ns
    }
}
