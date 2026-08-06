//! Program control panel (bottom-left corner): EXIT and SETTINGS.
//! Buttons look and behave like the terminal tabs:
//! slanted sides, hover highlight, flash on click.

use super::{Ctx, Rect};
use crate::font::FONT_UI;
use std::time::Instant;

pub const BTN_EXIT: usize = 0;
pub const BTN_SETTINGS: usize = 1;
const LABELS: [&str; 2] = ["EXIT", "SETTINGS"];

/// Button rectangles — geometry shared by drawing and hit-testing.
pub fn button_rects(r: Rect, window_h: f32) -> [Rect; 2] {
    let vh = window_h / 100.0;
    let btn_h = 4.2 * vh;
    let gap = 1.2 * vh;
    let top = r.y + 3.4 * vh; // room for the module title
    std::array::from_fn(|i| Rect::new(r.x, top + (btn_h + gap) * i as f32, r.w, btn_h))
}

pub struct Control {
    pressed: [Option<Instant>; 2],
}

impl Control {
    pub fn new() -> Self {
        Control { pressed: [None, None] }
    }

    /// Click; returns the index of the hit button (BTN_EXIT / BTN_SETTINGS).
    pub fn click(&mut self, x: f32, y: f32, r: Rect, window_h: f32) -> Option<usize> {
        let idx = button_rects(r, window_h)
            .iter()
            .position(|b| b.contains(x, y))?;
        self.pressed[idx] = Some(Instant::now());
        Some(idx)
    }

    pub fn draw(&mut self, ctx: &mut Ctx, r: Rect) {
        let base = ctx.theme.base;
        let title_px = ctx.font_px(1.02);
        ctx.dl.module_title(
            ctx.fonts,
            r.x,
            r.y,
            r.w,
            title_px,
            "CONTROL PANEL",
            "",
            base,
        );

        let now = Instant::now();
        let rects = button_rects(r, ctx.h);
        for (i, br) in rects.iter().enumerate() {
            let hover = br.contains(ctx.mouse.0, ctx.mouse.1);
            let flash = self.pressed[i]
                .map(|t| now.duration_since(t).as_secs_f32() < 0.15)
                .unwrap_or(false);
            let fill = if flash {
                base.alpha(0.35)
            } else if hover {
                base.alpha(0.22)
            } else {
                ctx.theme.bg
            };
            // Parallelogram like the terminal tabs (skewX).
            let skew = br.h * 0.7;
            ctx.dl.quad(
                [
                    [br.x + skew, br.y],
                    [br.right(), br.y],
                    [br.right() - skew, br.bottom()],
                    [br.x, br.bottom()],
                ],
                fill,
            );
            // Outline with the same slant.
            ctx.dl.polyline(
                &[
                    [br.x + skew, br.y],
                    [br.right(), br.y],
                    [br.right() - skew, br.bottom()],
                    [br.x, br.bottom()],
                ],
                1.0,
                base.alpha(if hover || flash { 0.8 } else { 0.4 }),
                true,
            );
            let px = ctx.font_px(1.1);
            let color = if hover || flash { base } else { base.alpha(0.7) };
            ctx.dl.text_center(
                ctx.fonts,
                FONT_UI,
                px,
                br.cx(),
                br.y + (br.h - px * 1.3) / 2.0,
                LABELS[i],
                color,
                px * 0.1,
            );
        }
    }
}
