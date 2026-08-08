//! Telemetry column composer: arranges the individual widgets (clock,
//! sysinfo, hardware, cpu, memory, processes) into the SYSTEM column and
//! its portrait variants. Each widget lives in its own module and draws
//! standalone into any rectangle.

use super::{clock, cpu, hardware, memory, processes, sysinfo, Ctx, Rect};
use crate::system::Snapshot;

/// Landscape SYSTEM column: all six widgets. Content-driven heights —
/// the fixed sections (clock, sysinfo, hardware) take their natural text
/// height; CPU, MEMORY and TOP PROCESSES share all the remaining space.
pub fn draw(ctx: &mut Ctx, col: Rect, snap: &Snapshot) {
    ctx.panel_scale = ctx.panel_font_scale(&col);
    header(ctx, col, snap);
    let (clock_h, sys_h, hw_h) = natural_heights(ctx);
    let gap = ctx.vh(1.2);
    let flex = (col.h - clock_h - sys_h - hw_h - gap * 5.0).max(ctx.vh(8.0));
    let (wc, wm, wt) = (26.0, 19.5, 21.0);
    let tot = wc + wm + wt;
    draw_sections(
        ctx,
        col,
        snap,
        &[
            (clock_h, clock::draw),
            (sys_h, sysinfo::draw),
            (hw_h, hardware::draw),
            (flex * wc / tot, cpu::draw),
            (flex * wm / tot, memory::draw),
            (flex * wt / tot, processes::draw),
        ],
    );
    ctx.panel_scale = 1.0;
}

/// Portrait variant of the column: without MEMORY and TOP PROCESSES —
/// those move under the control panel (draw_mem_procs).
pub fn draw_top(ctx: &mut Ctx, col: Rect, snap: &Snapshot) {
    ctx.panel_scale = ctx.panel_font_scale(&col);
    header(ctx, col, snap);
    let (clock_h, sys_h, hw_h) = natural_heights(ctx);
    let gap = ctx.vh(1.2);
    let cpu_h = (col.h - clock_h - sys_h - hw_h - gap * 3.0).max(ctx.vh(6.0));
    draw_sections(
        ctx,
        col,
        snap,
        &[
            (clock_h, clock::draw),
            (sys_h, sysinfo::draw),
            (hw_h, hardware::draw),
            (cpu_h, cpu::draw),
        ],
    );
    ctx.panel_scale = 1.0;
}

/// Portrait: MEMORY (with swap) + TOP PROCESSES, drawn in the free space
/// above the control panel buttons.
pub fn draw_mem_procs(ctx: &mut Ctx, col: Rect, snap: &Snapshot) {
    ctx.panel_scale = ctx.panel_font_scale(&col);
    // MEMORY takes its natural block (title, a few dot rows, the USING
    // and SWAP lines); the process list takes all the rest.
    let gap = ctx.vh(1.2);
    let title_px = ctx.font_px(1.02);
    let p = ctx.font_px(0.9);
    let dot_row = ctx.vh(0.55).max(3.0) * 1.55;
    let mem_h = (title_px * 2.4 + dot_row * 4.0 + p * 3.2).min(col.h * 0.55);
    let top_h = (col.h - mem_h - gap).max(ctx.vh(6.0));
    draw_sections(
        ctx,
        col,
        snap,
        &[(mem_h, memory::draw), (top_h, processes::draw)],
    );
    ctx.panel_scale = 1.0;
}

/// Column header like eDEX ("SYSTEM" + hostname in the column title).
fn header(ctx: &mut Ctx, col: Rect, snap: &Snapshot) {
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
}

/// Natural pixel heights of the fixed sections (clock, sysinfo, hardware),
/// derived from the current font size — so they scale with the panel.
fn natural_heights(ctx: &Ctx) -> (f32, f32, f32) {
    let p = ctx.font_px(0.95);
    let clock_h = ctx.font_px(3.2) * 2.1;
    let sys_h = p * 4.0;
    let hw_h = p * 1.65 * 3.0;
    (clock_h, sys_h, hw_h)
}

fn draw_sections(
    ctx: &mut Ctx,
    col: Rect,
    snap: &Snapshot,
    slots: &[(f32, fn(&mut Ctx, Rect, &Snapshot))],
) {
    let gap = ctx.vh(1.2);
    let mut y = col.y;
    for (h, f) in slots {
        f(ctx, Rect::new(col.x, y, col.w, *h), snap);
        y += h + gap;
    }
}
