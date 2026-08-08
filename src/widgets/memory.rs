//! Memory widget: eDEX-style dot grid, usage line and swap bar.

use super::{fit_end, Ctx, Rect};
use crate::font::FONT_UI;
use crate::system::{fmt_bytes, Snapshot};

pub fn draw(ctx: &mut Ctx, r: Rect, snap: &Snapshot) {
    let title_px = ctx.font_px(1.02);
    ctx.dl.module_title(
        ctx.fonts,
        r.x,
        r.y,
        r.w,
        title_px,
        "MEMORY",
        "",
        ctx.theme.base,
    );
    let top = r.y + title_px * 2.4;

    // Dot grid like the eDEX ramwatcher.
    let px = ctx.font_px(0.9);
    let text_h = px * 1.5;
    let swap_h = px * 1.5;
    let grid = Rect::new(r.x, top, r.w, r.h - (top - r.y) - text_h - swap_h);
    let dot = (ctx.vh(0.55)).max(3.0);
    let gap = dot * 0.55;
    let cols = ((grid.w + gap) / (dot + gap)).floor().max(1.0) as usize;
    let rows = ((grid.h + gap) / (dot + gap)).floor().max(1.0) as usize;
    let total_dots = cols * rows;
    let frac = if snap.mem_total > 0 {
        snap.mem_used as f32 / snap.mem_total as f32
    } else {
        0.0
    };
    let active = (frac * total_dots as f32).round() as usize;
    for i in 0..total_dots {
        let x = grid.x + (i % cols) as f32 * (dot + gap);
        let y = grid.y + (i / cols) as f32 * (dot + gap);
        let c = if i < active {
            ctx.theme.base
        } else {
            ctx.theme.base.alpha(0.2)
        };
        ctx.dl.rect(x, y, dot, dot, c);
    }

    let ty = grid.bottom() + px * 0.35;
    let using = format!(
        "USING {} OUT OF {}",
        fmt_bytes(snap.mem_used),
        fmt_bytes(snap.mem_total)
    );
    let using = fit_end(ctx, px, &using, r.w);
    ctx.dl.text(ctx.fonts, FONT_UI, px, r.x, ty, &using, ctx.theme.base, px * 0.05);

    // Swap bar.
    let sy = ty + text_h;
    let label = "SWAP";
    let lw = ctx.fonts.measure(FONT_UI, px, label, px * 0.1) + px;
    ctx.dl
        .text(ctx.fonts, FONT_UI, px, r.x, sy, label, ctx.theme.base.alpha(0.5), px * 0.1);
    let bar = Rect::new(
        r.x + lw,
        sy + px * 0.25,
        (r.w - lw - px * 5.0).max(px),
        px * 0.6,
    );
    ctx.dl
        .rect_outline(bar.x, bar.y, bar.w, bar.h, 1.0, ctx.theme.base.alpha(0.3));
    let sfrac = if snap.swap_total > 0 {
        snap.swap_used as f32 / snap.swap_total as f32
    } else {
        0.0
    };
    ctx.dl
        .rect(bar.x, bar.y, bar.w * sfrac.clamp(0.0, 1.0), bar.h, ctx.theme.base);
    ctx.dl.text_right(
        ctx.fonts,
        FONT_UI,
        px,
        r.right(),
        sy,
        &fmt_bytes(snap.swap_used),
        ctx.theme.base.alpha(0.7),
        px * 0.05,
    );
}
