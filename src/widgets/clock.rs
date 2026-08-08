//! Clock widget: big HH:MM:SS with eDEX-style blinking colons.

use super::{Ctx, Rect};
use crate::font::FONT_UI;
use crate::system::Snapshot;

pub fn draw(ctx: &mut Ctx, r: Rect, _snap: &Snapshot) {
    use chrono::Timelike;
    let now = chrono::Local::now();
    let mut px = ctx.font_px(3.4);
    let text = format!("{:02}:{:02}:{:02}", now.hour(), now.minute(), now.second());
    // Shrink to fit narrow columns.
    let total = ctx.fonts.measure(FONT_UI, px, &text, px * 0.08);
    if total > r.w {
        px *= (r.w / total) * 0.97;
    }
    // Segments drawn separately, colons blink like in eDEX.
    let blink = (ctx.t.fract() < 0.5) as i32 as f32 * 0.75 + 0.25;
    let total_w = ctx.fonts.measure(FONT_UI, px, &text, px * 0.08);
    let mut x = r.cx() - total_w / 2.0;
    let y = r.y + (r.h - px * 1.2) / 2.0;
    for ch in text.chars() {
        let color = if ch == ':' {
            ctx.theme.base.alpha(blink)
        } else {
            ctx.theme.base
        };
        let s = ch.to_string();
        let w = ctx.dl.text(ctx.fonts, FONT_UI, px, x, y, &s, color, 0.0);
        x += w + px * 0.08;
    }
}
