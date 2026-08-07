//! Right column: network status.

use super::{Ctx, Rect};
use crate::font::FONT_UI;
use crate::system::Snapshot;

/// Trims text (with a trailing ellipsis) so it fits the given width.
fn fit_end(ctx: &mut Ctx, px: f32, text: &str, max_w: f32) -> String {
    if ctx.fonts.measure(FONT_UI, px, text, px * 0.06) <= max_w {
        return text.to_string();
    }
    let chars: Vec<char> = text.chars().collect();
    let mut n = chars.len().saturating_sub(1);
    while n > 1 {
        let cand: String = chars[..n].iter().collect::<String>() + "\u{2026}";
        if ctx.fonts.measure(FONT_UI, px, &cand, px * 0.06) <= max_w {
            return cand;
        }
        n -= 1;
    }
    "\u{2026}".to_string()
}

pub fn draw(ctx: &mut Ctx, col: Rect, snap: &Snapshot) {
    let title_px = ctx.font_px(1.02);
    ctx.dl.module_title(
        ctx.fonts,
        col.x,
        col.y - ctx.vh(1.8),
        col.w,
        title_px,
        "NETWORK",
        &snap.iface.to_uppercase(),
        ctx.theme.base,
    );

    // Gap from the column header equal to the gap between other modules.
    let top = col.y + ctx.vh(1.8);
    let h = col.h * 0.20;
    netstat(ctx, Rect::new(col.x, top, col.w, h), snap);
}

fn netstat(ctx: &mut Ctx, r: Rect, snap: &Snapshot) {
    let title_px = ctx.font_px(1.02);
    ctx.dl.module_title(
        ctx.fonts,
        r.x,
        r.y,
        r.w,
        title_px,
        "NETWORK STATUS",
        "",
        ctx.theme.base,
    );
    let px = ctx.font_px(0.95);
    let label = ctx.theme.base.alpha(0.5);
    let rows = [
        (
            "STATE",
            if snap.online { "ONLINE".to_string() } else { "OFFLINE".to_string() },
        ),
        ("IPV4", snap.ipv4.clone().unwrap_or("UNKNOWN".into())),
        (
            "PING",
            snap.ping_ms
                .map(|ms| format!("{ms}ms"))
                .unwrap_or("--".into()),
        ),
    ];
    let top = r.y + title_px * 2.6;
    let rh = (r.h - (top - r.y)) / rows.len() as f32;
    for (i, (name, val)) in rows.iter().enumerate() {
        let y = top + rh * i as f32;
        ctx.dl.text(ctx.fonts, FONT_UI, px, r.x, y, name, label, px * 0.1);
        // Value trimmed by measured width so it never overlaps the label.
        let label_w = ctx.fonts.measure(FONT_UI, px, name, px * 0.1);
        let avail = (r.w - label_w - px).max(px * 3.0);
        let val = fit_end(ctx, px, val, avail);
        ctx.dl
            .text_right(ctx.fonts, FONT_UI, px, r.right(), y, &val, ctx.theme.base, px * 0.05);
    }
}
