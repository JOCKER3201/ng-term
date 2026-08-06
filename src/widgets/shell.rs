//! Central terminal panel — frame with clipped corners (augmented-ui),
//! tab bar with slanted sides like eDEX, character grid.

use super::{Ctx, Rect};
use crate::font::{FONT_MONO, FONT_UI};
use crate::term::{
    Cell, CellColor, Term, FLAG_BOLD, FLAG_DIM, FLAG_INVERSE, FLAG_UNDERLINE, FLAG_WIDE_SPACER,
};
use crate::theme::{xterm_256, Color};

pub const TAB_COUNT: usize = 5;

/// Tab rectangles — geometry shared by drawing and hit-testing in main.
pub fn tab_rects(r: Rect, window_h: f32) -> [Rect; TAB_COUNT] {
    let pad = window_h / 100.0 * 0.74;
    let tab_h = window_h / 100.0 * 2.6;
    let tabs = Rect::new(r.x + pad, r.y + pad, r.w - 2.0 * pad, tab_h);
    let tw = tabs.w / TAB_COUNT as f32;
    std::array::from_fn(|i| Rect::new(tabs.x + tw * i as f32, tabs.y, tw, tabs.h))
}

/// Returns the desired grid size (columns, rows) — main resizes the PTY.
pub fn draw(
    ctx: &mut Ctx,
    r: Rect,
    term: &Term,
    occupied: &[bool; TAB_COUNT],
    active: usize,
) -> (usize, usize) {
    let base = ctx.theme.base;
    let pad = ctx.vh(0.74);

    // Frame with clipped corners.
    ctx.dl
        .chamfer_frame(r.x, r.y, r.w, r.h, ctx.vh(1.1), ctx.vh(0.18).max(1.5), base.alpha(0.5));

    // Tab bar (5 tabs, slanted sides — skewX(35deg) like eDEX).
    let rects = tab_rects(r, ctx.h);
    let tabs_r = Rect::new(
        rects[0].x,
        rects[0].y,
        rects[TAB_COUNT - 1].right() - rects[0].x,
        rects[0].h,
    );
    let tab_h = tabs_r.h;
    let skew = tab_h * 0.7;
    for (i, tr) in rects.iter().enumerate() {
        let is_active = i == active;
        let hover = tr.contains(ctx.mouse.0, ctx.mouse.1);
        let fill = if is_active {
            base.alpha(0.12)
        } else if hover {
            base.alpha(0.22)
        } else {
            ctx.theme.bg
        };
        // Parallelogram.
        ctx.dl.quad(
            [
                [tr.x + skew, tr.y],
                [tr.right(), tr.y],
                [tr.right() - skew, tr.bottom()],
                [tr.x, tr.bottom()],
            ],
            fill,
        );
        let px = ctx.font_px(1.0);
        let label = if occupied[i] {
            if i == 0 { "MAIN SHELL" } else { "SHELL" }
        } else {
            "EMPTY"
        };
        let color = if is_active {
            base
        } else if hover {
            base.alpha(0.85)
        } else if occupied[i] {
            base.alpha(0.6)
        } else {
            base.alpha(0.4)
        };
        let text = if is_active {
            format!("#{} {}", i + 1, label)
        } else if !occupied[i] && hover {
            format!("+ {label}")
        } else {
            label.to_string()
        };
        ctx.dl.text_center(
            ctx.fonts,
            FONT_UI,
            px,
            tr.cx(),
            tr.y + (tab_h - px * 1.3) / 2.0,
            &text,
            color,
            px * 0.08,
        );
    }
    // Line under the tabs.
    ctx.dl.line(
        tabs_r.x,
        tabs_r.bottom() + 1.0,
        tabs_r.right(),
        tabs_r.bottom() + 1.0,
        ctx.vh(0.18).max(1.0),
        base.alpha(0.5),
    );

    // Terminal grid area.
    let grid_r = Rect::new(
        r.x + pad,
        tabs_r.bottom() + pad * 0.8,
        r.w - 2.0 * pad,
        r.h - tab_h - 2.6 * pad,
    );

    let px = ctx.font_px(1.45);
    let cell_w = ctx.fonts.mono_advance(px).max(1.0);
    let (ascent, line_h) = ctx.fonts.line_metrics(FONT_MONO, px);
    let cell_h = line_h.max(1.0);
    let cols = (grid_r.w / cell_w).floor().max(2.0) as usize;
    let rows = (grid_r.h / cell_h).floor().max(2.0) as usize;

    // Cell drawing (only as many as the terminal actually has).
    let draw_rows = rows.min(term.rows);
    let draw_cols = cols.min(term.cols);
    for y in 0..draw_rows {
        let Some(row) = term.view_row(y) else { continue };
        let cy = grid_r.y + y as f32 * cell_h;
        for x in 0..draw_cols {
            // Scrollback rows may have a different length after a resize.
            let Some(cell) = row.get(x) else { break };
            if cell.flags & FLAG_WIDE_SPACER != 0 {
                continue;
            }
            let (fg, bg) = resolve_colors(ctx, cell);
            if let Some(bgc) = bg {
                use unicode_width::UnicodeWidthChar;
                let wide = cell.ch.width().unwrap_or(1) > 1;
                let w = if wide { cell_w * 2.0 } else { cell_w };
                let w = w.min(grid_r.right() - (grid_r.x + x as f32 * cell_w));
                ctx.dl
                    .rect(grid_r.x + x as f32 * cell_w, cy, w, cell_h, bgc);
            }
            if cell.ch != ' ' {
                draw_cell_char(ctx, cell.ch, grid_r.x + x as f32 * cell_w, cy, ascent, px, fg);
            }
            if cell.flags & FLAG_UNDERLINE != 0 {
                ctx.dl.rect(
                    grid_r.x + x as f32 * cell_w,
                    cy + cell_h - 1.5,
                    cell_w,
                    1.0,
                    fg,
                );
            }
        }
    }

    // Cursor (blinking block).
    if term.cursor_visible && term.view_offset == 0 && term.cur_y < draw_rows {
        let blink = ctx.t.fract() < 0.6;
        if blink {
            let cx = grid_r.x + term.cur_x as f32 * cell_w;
            let cy = grid_r.y + term.cur_y as f32 * cell_h;
            ctx.dl.rect(cx, cy, cell_w, cell_h, ctx.theme.cursor);
            // Character under the cursor in the background color.
            if let Some(row) = term.view_row(term.cur_y) {
                if let Some(cell) = row.get(term.cur_x) {
                    if cell.ch != ' ' {
                        draw_cell_char(ctx, cell.ch, cx, cy, ascent, px, ctx.theme.term_bg);
                    }
                }
            }
        }
    }

    // Scrollback position indicator.
    if term.view_offset > 0 {
        let px_s = ctx.font_px(0.9);
        ctx.dl.text_right(
            ctx.fonts,
            FONT_UI,
            px_s,
            grid_r.right(),
            grid_r.y,
            &format!("SCROLL +{}", term.view_offset),
            ctx.theme.base,
            px_s * 0.08,
        );
    }

    (cols, rows)
}

fn draw_cell_char(ctx: &mut Ctx, ch: char, x: f32, y: f32, ascent: f32, px: f32, color: Color) {
    // Draw a single glyph with the monospace font on the cell baseline.
    let _ = ascent;
    let s = ch.to_string();
    ctx.dl.text(ctx.fonts, FONT_MONO, px, x, y, &s, color, 0.0);
}

/// Cell colors accounting for SGR (bold/dim/inverse).
fn resolve_colors(ctx: &Ctx, cell: &Cell) -> (Color, Option<Color>) {
    let theme = ctx.theme;
    let mut fg = match cell.fg {
        CellColor::Default => theme.term_fg,
        CellColor::Indexed(i) => {
            let i = if cell.flags & FLAG_BOLD != 0 && i < 8 { i + 8 } else { i };
            xterm_256(i, &theme.ansi)
        }
        CellColor::Rgb(r, g, b) => Color::rgb8(r, g, b),
    };
    let mut bg = match cell.bg {
        CellColor::Default => None,
        CellColor::Indexed(i) => Some(xterm_256(i, &theme.ansi)),
        CellColor::Rgb(r, g, b) => Some(Color::rgb8(r, g, b)),
    };
    if cell.flags & FLAG_DIM != 0 {
        fg = fg.dim(0.6);
    }
    if cell.flags & FLAG_INVERSE != 0 {
        let old_fg = fg;
        fg = bg.unwrap_or(theme.term_bg);
        bg = Some(old_fg);
    }
    (fg, bg)
}
