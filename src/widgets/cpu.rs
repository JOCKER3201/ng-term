//! CPU widget: per-core horizontal fill gauges in two columns,
//! with a load-average and temperature footer.

use super::{fit_end, Ctx, Rect};
use crate::font::FONT_UI;
use crate::system::Snapshot;

pub fn draw(ctx: &mut Ctx, r: Rect, snap: &Snapshot) {
    let title_px = ctx.font_px(1.02);
    let left_w = ctx.fonts.measure(FONT_UI, title_px, "CPU USAGE", title_px * 0.06);
    let avail = (r.w - left_w - title_px * 3.0).max(title_px * 4.0);
    let name = fit_end(ctx, title_px, &snap.cpu_name.to_uppercase(), avail);
    ctx.dl.module_title(
        ctx.fonts,
        r.x,
        r.y,
        r.w,
        title_px,
        "CPU USAGE",
        &name,
        ctx.theme.base,
    );
    let top = r.y + title_px * 2.4;
    let footer_h = title_px * 2.0;
    let area = Rect::new(r.x, top, r.w, r.h - (top - r.y) - footer_h);

    let n = snap.cpu_hist.len().max(1);
    // Per-core gauges in two columns like eDEX.
    let gcols = if n >= 2 { 2 } else { 1 };
    let grows = n.div_ceil(gcols);
    let gap = ctx.vh(0.5);
    let gw = (area.w - gap * (gcols as f32 - 1.0)) / gcols as f32;
    let gh = (area.h - gap * (grows as f32 - 1.0)) / grows as f32;

    for i in 0..n {
        let gx = area.x + (i % gcols) as f32 * (gw + gap);
        let gy = area.y + (i / gcols) as f32 * (gh + gap);
        ctx.dl
            .rect_outline(gx, gy, gw, gh, 1.0, ctx.theme.base.alpha(0.2));
        // Horizontal bar: frame filled from the left by current core usage.
        let v = snap.cpu_per_core.get(i).copied().unwrap_or(0.0);
        let fill_w = (v / 100.0).clamp(0.0, 1.0) * (gw - 2.0);
        if fill_w >= 1.0 {
            ctx.dl
                .rect(gx + 1.0, gy + 1.0, fill_w, gh - 2.0, ctx.theme.base.alpha(0.85));
        }
        // Current value on the right; when the bar reaches the text,
        // draw it in the background color for readability.
        if gh > ctx.font_px(0.8) * 1.4 {
            let px = ctx.font_px(0.75);
            let text = format!("{v:>3.0}%");
            let tw = ctx.fonts.measure(FONT_UI, px, &text, 0.0);
            let color = if fill_w >= gw - 2.0 - tw - 4.0 {
                ctx.theme.bg
            } else {
                ctx.theme.base.alpha(0.7)
            };
            ctx.dl.text(
                ctx.fonts,
                FONT_UI,
                px,
                gx + gw - tw - 3.0,
                gy + (gh - px * 1.3) / 2.0,
                &text,
                color,
                0.0,
            );
        }
    }

    // Footer: load average + temperature.
    let px = ctx.font_px(0.9);
    let fy = r.bottom() - px * 1.4;
    let load = format!(
        "LOAD {:.2} {:.2} {:.2}",
        snap.load_avg[0], snap.load_avg[1], snap.load_avg[2]
    );
    let temp_text = snap.temp_c.map(|t| format!("TEMP {t:.0}\u{00B0}C"));
    let temp_w = temp_text
        .as_ref()
        .map(|t| ctx.fonts.measure(FONT_UI, px, t, px * 0.05))
        .unwrap_or(0.0);
    let load = fit_end(ctx, px, &load, (r.w - temp_w - px).max(px * 3.0));
    ctx.dl
        .text(ctx.fonts, FONT_UI, px, r.x, fy, &load, ctx.theme.base.alpha(0.7), px * 0.05);
    if let Some(t) = &temp_text {
        ctx.dl.text_right(
            ctx.fonts,
            FONT_UI,
            px,
            r.right(),
            fy,
            t,
            ctx.theme.base.alpha(0.7),
            px * 0.05,
        );
    }
}
