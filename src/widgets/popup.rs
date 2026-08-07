//! On-screen warning popup (e.g. an element cannot be loaded for the
//! current screen size). Auto-hides after a few seconds; any click on it
//! dismisses it immediately.

use super::{Ctx, Rect};
use crate::font::FONT_UI;
use std::time::Instant;

const SHOW_SECS: f32 = 8.0;

pub struct Popup {
    msg: Option<(String, Instant)>,
}

impl Popup {
    pub fn new() -> Self {
        Popup { msg: None }
    }

    pub fn show(&mut self, message: String) {
        self.msg = Some((message, Instant::now()));
    }

    /// Dismisses the popup if the click landed on it; returns true then.
    pub fn click(&mut self, x: f32, y: f32, w: f32, h: f32) -> bool {
        if let Some((msg, _)) = &self.msg {
            // Recompute the box like draw() does (without fonts: generous hit box).
            let bw = (w * 0.5).max(300.0);
            let bh = h * 0.08;
            let bx = (w - bw) / 2.0;
            let by = h * 0.10;
            let _ = msg;
            if x >= bx && x <= bx + bw && y >= by && y <= by + bh {
                self.msg = None;
                return true;
            }
        }
        false
    }

    pub fn draw(&mut self, ctx: &mut Ctx) {
        let Some((msg, t0)) = &self.msg else { return };
        if t0.elapsed().as_secs_f32() > SHOW_SECS {
            self.msg = None;
            return;
        }
        let msg = msg.clone();
        let base = ctx.theme.base;

        let px = ctx.font_px(1.1);
        let title_px = ctx.font_px(0.95);
        let text_w = ctx.fonts.measure(FONT_UI, px, &msg, px * 0.05);
        let bw = (text_w + ctx.vw(4.0)).max(ctx.w * 0.5).min(ctx.w * 0.9);
        let bh = ctx.h * 0.08;
        let bx = (ctx.w - bw) / 2.0;
        let by = ctx.h * 0.10;

        ctx.dl.rect(bx, by, bw, bh, ctx.theme.bg);
        ctx.dl
            .chamfer_frame(bx, by, bw, bh, ctx.vh(0.9), ctx.vh(0.18).max(1.5), base.alpha(0.8));
        ctx.dl.text_center(
            ctx.fonts,
            FONT_UI,
            title_px,
            bx + bw / 2.0,
            by + bh * 0.12,
            "WARNING",
            base.alpha(0.6),
            title_px * 0.2,
        );
        ctx.dl.text_center(
            ctx.fonts,
            FONT_UI,
            px,
            bx + bw / 2.0,
            by + bh * 0.45,
            &msg,
            base,
            px * 0.05,
        );
    }
}

/// OK button rectangle of the resolution dialog — geometry shared by
/// drawing and hit-testing in main.
pub fn resolution_dialog_ok_rect(w: f32, h: f32) -> Rect {
    let bw = (w * 0.22).max(90.0);
    let bh = h * 0.17;
    Rect::new((w - bw) / 2.0, h * 0.66, bw, bh)
}

/// Content of the standalone resolution dialog window, shown INSTEAD of
/// the program when the monitor resolution is below the minimum.
pub fn draw_resolution_dialog(ctx: &mut Ctx, mw: u32, mh: u32) {
    let base = ctx.theme.base;
    let (w, h) = (ctx.w, ctx.h);
    ctx.dl.rect(0.0, 0.0, w, h, ctx.theme.bg);
    ctx.dl.chamfer_frame(
        ctx.vw(1.2),
        ctx.vh(4.0),
        w - ctx.vw(2.4),
        h - ctx.vh(8.0),
        ctx.vh(4.0),
        ctx.vh(0.7).max(1.5),
        base.alpha(0.8),
    );

    let title_px = ctx.vh(7.5).max(10.0);
    ctx.dl.text_center(
        ctx.fonts,
        FONT_UI,
        title_px,
        w / 2.0,
        ctx.vh(13.0),
        "WARNING",
        base.alpha(0.6),
        title_px * 0.2,
    );
    let px = ctx.vh(8.5).max(12.0);
    ctx.dl.text_center(
        ctx.fonts,
        FONT_UI,
        px,
        w / 2.0,
        ctx.vh(29.0),
        &format!("Monitor resolution {mw}x{mh} is too small"),
        base,
        px * 0.05,
    );
    ctx.dl.text_center(
        ctx.fonts,
        FONT_UI,
        px,
        w / 2.0,
        ctx.vh(44.0),
        "ng-term requires a resolution of at least 1280x720",
        base,
        px * 0.05,
    );

    // OK button — a parallelogram like the control panel buttons.
    let br = resolution_dialog_ok_rect(w, h);
    let hover = br.contains(ctx.mouse.0, ctx.mouse.1);
    let skew = br.h * 0.7;
    let fill = if hover { base.alpha(0.22) } else { ctx.theme.bg };
    ctx.dl.quad(
        [
            [br.x + skew, br.y],
            [br.right(), br.y],
            [br.right() - skew, br.bottom()],
            [br.x, br.bottom()],
        ],
        fill,
    );
    ctx.dl.polyline(
        &[
            [br.x + skew, br.y],
            [br.right(), br.y],
            [br.right() - skew, br.bottom()],
            [br.x, br.bottom()],
        ],
        1.0,
        base.alpha(if hover { 0.8 } else { 0.4 }),
        true,
    );
    let bpx = ctx.vh(7.0).max(10.0);
    let color = if hover { base } else { base.alpha(0.7) };
    ctx.dl.text_center(
        ctx.fonts,
        FONT_UI,
        bpx,
        br.cx(),
        br.y + (br.h - bpx * 1.3) / 2.0,
        "OK",
        color,
        bpx * 0.1,
    );
}
