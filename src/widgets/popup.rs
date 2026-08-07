//! On-screen warning popup (e.g. an element cannot be loaded for the
//! current screen size). Auto-hides after a few seconds; any click on it
//! dismisses it immediately.

use super::Ctx;
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
