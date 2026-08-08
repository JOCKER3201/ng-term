//! System information widget: date, uptime and power state.

use super::{fit_end, Ctx, Rect};
use crate::font::FONT_UI;
use crate::system::{fmt_uptime, Snapshot};

pub fn draw(ctx: &mut Ctx, r: Rect, snap: &Snapshot) {
    let px = ctx.font_px(0.95);
    let label = ctx.theme.base.alpha(0.5);
    let now = chrono::Local::now();
    let date = now.format("%a %b %d").to_string().to_uppercase();
    let power = match snap.battery {
        Some((pct, true)) => format!("CHARGE {pct}%"),
        Some((pct, false)) => format!("BATTERY {pct}%"),
        None => "ON".to_string(),
    };
    let cols = [
        ("DATE", date),
        ("UPTIME", fmt_uptime(snap.uptime)),
        ("POWER", power),
    ];
    let cw = r.w / cols.len() as f32;
    for (i, (name, val)) in cols.iter().enumerate() {
        let cx = r.x + cw * i as f32 + cw / 2.0;
        ctx.dl
            .text_center(ctx.fonts, FONT_UI, px, cx, r.y, name, label, px * 0.1);
        let val = fit_end(ctx, px * 1.1, val, cw - px * 0.6);
        ctx.dl.text_center(
            ctx.fonts,
            FONT_UI,
            px * 1.1,
            cx,
            r.y + px * 1.6,
            &val,
            ctx.theme.base,
            px * 0.05,
        );
    }
}
