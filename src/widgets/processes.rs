//! Process list widget: top processes with PID/NAME/CPU/MEM columns.

use super::{fit_end, Ctx, Rect};
use crate::font::FONT_UI;
use crate::system::Snapshot;

pub fn draw(ctx: &mut Ctx, r: Rect, snap: &Snapshot) {
    let title_px = ctx.font_px(1.02);
    let px = ctx.font_px(0.92);

    // Column geometry shared by the header and rows (mono font).
    let mono1 = ctx.fonts.measure(FONT_UI, px, "0", 0.0);
    let pid_right = r.x + 6.0 * mono1;
    let name_x = r.x + px * 4.5;
    // Right block: "{:>5.1}%" (CPU, 6 ch) + space + "{:>4.1}%" (MEM, 5 ch).
    let cpu_right = r.right() - 6.0 * mono1;

    // Column headers where the old title used to be — values land
    // exactly under the column names.
    let hc = ctx.theme.base;
    let ls = title_px * 0.06;
    ctx.dl
        .text_right(ctx.fonts, FONT_UI, title_px, pid_right, r.y, "PID", hc, ls);
    ctx.dl.text(ctx.fonts, FONT_UI, title_px, name_x, r.y, "NAME", hc, ls);
    ctx.dl
        .text_right(ctx.fonts, FONT_UI, title_px, cpu_right, r.y, "CPU", hc, ls);
    ctx.dl
        .text_right(ctx.fonts, FONT_UI, title_px, r.right(), r.y, "MEM", hc, ls);

    // Underline with "whiskers" like the other modules.
    let line_c = ctx.theme.base.alpha(0.3);
    let lh = title_px * 1.75;
    ctx.dl.line(r.x, r.y + lh, r.right(), r.y + lh, 1.0, line_c);
    ctx.dl
        .line(r.x, r.y + lh - title_px * 0.45, r.x, r.y + lh, 1.0, line_c);
    ctx.dl
        .line(r.right(), r.y + lh - title_px * 0.45, r.right(), r.y + lh, 1.0, line_c);

    let row_h = px * 1.55;
    let mut y = r.y + title_px * 2.4;
    for p in &snap.top {
        if y + row_h > r.bottom() {
            break;
        }
        ctx.dl.text_right(
            ctx.fonts,
            FONT_UI,
            px,
            pid_right,
            y,
            &format!("{}", p.pid),
            ctx.theme.base.alpha(0.6),
            0.0,
        );
        let cpu_txt = format!("{:.1}%", p.cpu);
        let cpu_w = ctx.fonts.measure(FONT_UI, px, &cpu_txt, 0.0);
        let avail = (cpu_right - cpu_w - name_x - px).max(px * 2.0);
        let name = fit_end(ctx, px, &p.name, avail);
        ctx.dl.text(ctx.fonts, FONT_UI, px, name_x, y, &name, ctx.theme.base, 0.0);
        ctx.dl.text_right(
            ctx.fonts,
            FONT_UI,
            px,
            cpu_right,
            y,
            &cpu_txt,
            ctx.theme.base.alpha(0.8),
            0.0,
        );
        ctx.dl.text_right(
            ctx.fonts,
            FONT_UI,
            px,
            r.right(),
            y,
            &format!("{:.1}%", p.mem_pct),
            ctx.theme.base.alpha(0.8),
            0.0,
        );
        y += row_h;
    }
}
