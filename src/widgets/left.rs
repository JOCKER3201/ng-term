//! Left column: clock, system information, hardware inspector,
//! CPU gauges, memory (dot grid), process list.

use super::{Ctx, Rect};
use crate::font::FONT_UI;
use crate::system::{fmt_bytes, fmt_uptime, Snapshot};

pub fn draw(ctx: &mut Ctx, col: Rect, snap: &Snapshot) {
    // Column header like eDEX ("PANEL" + hostname in the column title).
    let title_px = ctx.font_px(1.02);
    ctx.dl.module_title(
        ctx.fonts,
        col.x,
        col.y - ctx.vh(1.8),
        col.w,
        title_px,
        "SYSTEM",
        &snap.hostname.to_uppercase(),
        ctx.theme.base,
    );

    // Column split into modules (fractions of the column height).
    draw_slots(
        ctx,
        col,
        snap,
        &[
            (0.12, clock),
            (0.08, sysinfo),
            (0.135, hardware),
            (0.26, cpu),
            (0.195, memory),
            (0.21, toplist),
        ],
    );
}

/// Portrait variant of the column: without MEMORY and TOP PROCESSES —
/// those move under the control panel (draw_mem_procs).
pub fn draw_top(ctx: &mut Ctx, col: Rect, snap: &Snapshot) {
    let title_px = ctx.font_px(1.02);
    ctx.dl.module_title(
        ctx.fonts,
        col.x,
        col.y - ctx.vh(1.8),
        col.w,
        title_px,
        "SYSTEM",
        &snap.hostname.to_uppercase(),
        ctx.theme.base,
    );
    draw_slots(
        ctx,
        col,
        snap,
        &[(0.202, clock), (0.134, sysinfo), (0.227, hardware), (0.437, cpu)],
    );
}

/// Portrait: MEMORY (with swap) + TOP PROCESSES, drawn in the free space
/// under the control panel buttons.
pub fn draw_mem_procs(ctx: &mut Ctx, col: Rect, snap: &Snapshot) {
    draw_slots(ctx, col, snap, &[(0.48, memory), (0.52, toplist)]);
}

fn draw_slots(
    ctx: &mut Ctx,
    col: Rect,
    snap: &Snapshot,
    slots: &[(f32, fn(&mut Ctx, Rect, &Snapshot))],
) {
    let gap = ctx.vh(1.2);
    let usable = col.h - gap * (slots.len() as f32 - 1.0);
    let mut y = col.y;
    for (frac, f) in slots {
        let h = usable * frac;
        f(ctx, Rect::new(col.x, y, col.w, h), snap);
        y += h + gap;
    }
}

fn clock(ctx: &mut Ctx, r: Rect, _snap: &Snapshot) {
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

fn sysinfo(ctx: &mut Ctx, r: Rect, snap: &Snapshot) {
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

fn hardware(ctx: &mut Ctx, r: Rect, snap: &Snapshot) {
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

fn cpu(ctx: &mut Ctx, r: Rect, snap: &Snapshot) {
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

fn memory(ctx: &mut Ctx, r: Rect, snap: &Snapshot) {
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

fn toplist(ctx: &mut Ctx, r: Rect, snap: &Snapshot) {
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

/// Trims text (with a trailing ellipsis) so it fits the given width.
fn fit_end(ctx: &mut Ctx, px: f32, text: &str, max_w: f32) -> String {
    if ctx.fonts.measure(FONT_UI, px, text, px * 0.06) <= max_w {
        return text.to_string();
    }
    let chars: Vec<char> = text.chars().collect();
    let mut n = chars.len().saturating_sub(1);
    while n > 1 {
        let cand: String =
            chars[..n].iter().collect::<String>() + "\u{2026}";
        if ctx.fonts.measure(FONT_UI, px, &cand, px * 0.06) <= max_w {
            return cand;
        }
        n -= 1;
    }
    "\u{2026}".to_string()
}

