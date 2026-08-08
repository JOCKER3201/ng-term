//! Hardware inspector widget: manufacturer, model and chassis.

use super::{fit_end, Ctx, Rect};
use crate::font::FONT_UI;
use crate::system::Snapshot;

pub fn draw(ctx: &mut Ctx, r: Rect, snap: &Snapshot) {
    let px = ctx.font_px(0.95);
    let label = ctx.theme.base.alpha(0.5);
    let rows = [
        ("MANUFACTURER", snap.manufacturer.to_uppercase()),
        ("MODEL", snap.model.to_uppercase()),
        ("CHASSIS", snap.chassis.clone()),
    ];
    let rh = r.h / rows.len() as f32;
    for (i, (name, val)) in rows.iter().enumerate() {
        let y = r.y + rh * i as f32 + (rh - px * 1.3) / 2.0;
        ctx.dl.text(ctx.fonts, FONT_UI, px, r.x, y, name, label, px * 0.1);
        // Value trimmed by measured width, not by a character count.
        let label_w = ctx.fonts.measure(FONT_UI, px, name, px * 0.1);
        let avail = (r.w - label_w - px).max(px * 3.0);
        let val = fit_end(ctx, px, val, avail);
        ctx.dl
            .text_right(ctx.fonts, FONT_UI, px, r.right(), y, &val, ctx.theme.base, px * 0.05);
    }
}
