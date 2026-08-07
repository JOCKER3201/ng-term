//! Window-driven responsive layout — "like a website".
//!
//! Every frame the panel layout is computed from the ACTUAL window size,
//! so resizing or moving the window reflows the interface live. The
//! built-in default layout is a flexbox row solved by the `taffy` crate —
//! the same layout algorithm web pages use: side columns have real
//! min/max pixel widths and shrink before the terminal does, the terminal
//! column absorbs all remaining space. When a column can no longer fit its
//! minimum width it collapses, in priority order (right column first, then
//! the left one), and on portrait windows the interface restacks into a
//! vertical column. Custom .layaut files (authored at the 16:9 reference)
//! go through the same window-driven adaptation.

use crate::widgets::{Layout, LayoutMode, LayoutSpec, PanelSpec, Rect};
use taffy::prelude::{auto, length, percent};
use taffy::style::{AvailableSpace, FlexDirection};
use taffy::{Size, Style, TaffyTree};

/// CSS-like pixel constraints of the columns (min-width / max-width).
const SIDE_MIN: f32 = 168.0;
const SIDE_MAX: f32 = 340.0;
const CENTER_MIN: f32 = 430.0;

/// Layout for the current window size, recomputed every frame.
pub fn compute(w: f32, h: f32, mode: &LayoutMode) -> Layout {
    match mode {
        LayoutMode::Flex => {
            if h > w {
                portrait(w, h)
            } else {
                landscape(w, h)
            }
        }
        LayoutMode::Fixed(base) => {
            let spec = if h > w {
                reflow_base(base, h)
            } else {
                edge_adapt(base, w / h)
            };
            Layout::compute(w, h, &spec)
        }
    }
}

/// Rectangle far outside the window — a hidden (collapsed) panel.
fn off(w: f32, h: f32) -> Rect {
    Rect::new(w * 2.0, 0.0, w * 0.16, h * 0.6)
}

/// Landscape flexbox layout: [left column | terminal | right column].
fn landscape(w: f32, h: f32) -> Layout {
    let pad_x = (w * 0.006).max(4.0);
    let gap = (w * 0.005).max(4.0);
    let inner = w - 2.0 * pad_x;

    // Collapse breakpoints derived from the content minimums, not from
    // fixed device classes: drop the right column first, then the left.
    // Side columns also need enough height for their stacked modules.
    let tall = h >= 520.0;
    let has_right = tall && inner >= CENTER_MIN + 2.0 * SIDE_MIN + 2.0 * gap;
    let has_left = tall && inner >= CENTER_MIN + SIDE_MIN + gap;
    if !has_left {
        return compact(w, h);
    }

    let mut tf: TaffyTree<()> = TaffyTree::new();
    let side = Style {
        flex_basis: percent(0.164),
        flex_grow: 0.0,
        flex_shrink: 1.0,
        min_size: Size { width: length(SIDE_MIN), height: auto() },
        max_size: Size { width: length(SIDE_MAX), height: auto() },
        ..Default::default()
    };
    let center = Style {
        flex_basis: percent(0.65),
        flex_grow: 1.0,
        flex_shrink: 1.0,
        min_size: Size { width: length(CENTER_MIN), height: auto() },
        ..Default::default()
    };
    let left_n = tf.new_leaf(side.clone()).unwrap();
    let center_n = tf.new_leaf(center).unwrap();
    let mut children = vec![left_n, center_n];
    let right_n = if has_right {
        let n = tf.new_leaf(side).unwrap();
        children.push(n);
        Some(n)
    } else {
        None
    };
    let root = tf
        .new_with_children(
            Style {
                flex_direction: FlexDirection::Row,
                size: Size { width: length(w), height: length(h) },
                padding: taffy::Rect {
                    left: length(pad_x),
                    right: length(pad_x),
                    top: length(0.0),
                    bottom: length(0.0),
                },
                gap: Size { width: length(gap), height: length(0.0) },
                ..Default::default()
            },
            &children,
        )
        .unwrap();
    tf.compute_layout(
        root,
        Size { width: AvailableSpace::Definite(w), height: AvailableSpace::Definite(h) },
    )
    .unwrap();
    let col = |n: taffy::NodeId| {
        let l = tf.layout(n).unwrap();
        (l.location.x, l.size.width)
    };
    let (lx, lw) = col(left_n);
    let (cx, cw) = col(center_n);

    // Vertical placement inside the columns — the classic proportions
    // (a 94.5vh span from 2.5vh to 97vh at the 16:9 reference).
    let top = h * 0.025;
    let hi = h - top - h * 0.03;
    let f = |v: f32| v / 94.5 * hi;

    Layout {
        left_col: Rect::new(lx, top, lw, f(59.5)),
        control: Rect::new(lx, top + f(62.0), lw, f(32.5)),
        shell: Rect::new(cx, top, cw, f(60.3)),
        keyboard: Rect::new(cx, top + f(62.0), cw, f(32.5)),
        right_col: match right_n {
            Some(n) => {
                let (rx, rw) = col(n);
                Rect::new(rx, top, rw, f(59.5))
            }
            None => off(w, h),
        },
        filesystem: match right_n {
            Some(n) => {
                let (rx, rw) = col(n);
                Rect::new(rx, top + f(14.9), rw, f(79.6))
            }
            None => off(w, h),
        },
    }
}

/// Compact landscape (very small windows): terminal, keyboard, control bar.
fn compact(w: f32, h: f32) -> Layout {
    let pad = (w * 0.005).max(4.0);
    let gap = (h * 0.015).max(4.0);
    let iw = w - 2.0 * pad;
    let ctl_h = (h * 0.135).max(52.0).min(h * 0.25);
    let kb_h = h * 0.26;
    let shell_h = (h - 2.0 * gap - kb_h - ctl_h - 2.0 * gap).max(60.0);
    let mut y = gap;
    let shell = Rect::new(pad, y, iw, shell_h);
    y += shell_h + gap;
    let keyboard = Rect::new(pad, y, iw, kb_h);
    y += kb_h + gap;
    let control = Rect::new(pad, y, iw, ctl_h);
    Layout {
        shell,
        keyboard,
        control,
        left_col: off(w, h),
        right_col: off(w, h),
        filesystem: off(w, h),
    }
}

/// Portrait stack: terminal, keyboard, then (on windows tall enough) a row
/// of side panels + control; on phone-sized windows just a control bar.
fn portrait(w: f32, h: f32) -> Layout {
    let small = h < 900.0;
    let pad = (w * 0.008).max(4.0);
    let gap = (h * 0.012).max(4.0);
    let iw = w - 2.0 * pad;

    let kb_h = if small { h * 0.30 } else { h * 0.185 };
    let row_h = if small { 0.0 } else { h * 0.325 };
    // The control bar needs ~13vh: title (3.4) + two buttons (2 x 4.2 + 1.2).
    let ctl_h = if small { h * 0.135 } else { 0.0 };
    let mut used = kb_h + gap;
    if row_h > 0.0 {
        used += row_h + gap;
    }
    if ctl_h > 0.0 {
        used += ctl_h + gap;
    }
    let shell_h = (h - 2.0 * gap - used).max(h * 0.2);

    let mut y = gap;
    let shell = Rect::new(pad, y, iw, shell_h);
    y += shell_h + gap;
    let keyboard = Rect::new(pad, y, iw, kb_h);
    y += kb_h + gap;

    if row_h > 0.0 {
        // Row: telemetry | network + files | control.
        let cgap = (w * 0.01).max(4.0);
        let cw = (iw - 2.0 * cgap) / 3.2;
        let left_w = cw * 1.2;
        // Left/middle columns start slightly lower, so their headers
        // (drawn above the column) line up with the MEMORY title of the
        // control column and all three columns span the same height.
        let d = h * 0.03;
        let mut x = pad;
        let left_col = Rect::new(x, y + d, left_w, row_h - d);
        x += left_w + cgap;
        let right_col = Rect::new(x, y + d, cw, row_h - d);
        let fs_y = y + d + h * 0.09;
        let filesystem = Rect::new(x, fs_y, cw, y + row_h - fs_y);
        x += cw + cgap;
        // The control panel sits at the very bottom of its column; the
        // space above it takes MEMORY + TOP PROCESSES (drawn in main).
        let ctl_h = h * 0.135;
        let control =
            Rect::new(x, y + row_h - ctl_h, (w - pad - x).max(60.0), ctl_h);
        Layout { shell, keyboard, left_col, right_col, filesystem, control }
    } else {
        let control = Rect::new(pad, y, iw, ctl_h);
        Layout {
            shell,
            keyboard,
            control,
            left_col: off(w, h),
            right_col: off(w, h),
            filesystem: off(w, h),
        }
    }
}

// ---------------------------------------------------------------------------
// Adaptation of fixed .layaut files (authored at the 16:9 reference).

fn spec(x: f32, y: f32, w: f32, h: f32) -> PanelSpec {
    PanelSpec { x, y, w, h }
}

fn on_screen(p: &PanelSpec) -> bool {
    p.x < 100.0
}

/// Landscape: edge-anchored horizontal transform — panels keep their
/// distance to the nearer window edge, so side columns keep a sane width
/// on any aspect ratio.
fn edge_adapt(base: &LayoutSpec, ratio: f32) -> LayoutSpec {
    let f = ((16.0 / 9.0) / ratio).clamp(0.5, 1.4);
    if (f - 1.0).abs() < 0.001 {
        return base.clone();
    }
    let tr = |p: &PanelSpec| -> PanelSpec {
        if !on_screen(p) {
            return *p;
        }
        let a = p.x;
        let b = p.x + p.w;
        let na = if a <= 50.0 { a * f } else { 100.0 - (100.0 - a) * f };
        let nb = if b <= 50.0 { b * f } else { 100.0 - (100.0 - b) * f };
        spec(na, p.y, (nb - na).max(1.0), p.h)
    };
    LayoutSpec {
        left_col: tr(&base.left_col),
        shell: tr(&base.shell),
        right_col: tr(&base.right_col),
        filesystem: tr(&base.filesystem),
        keyboard: tr(&base.keyboard),
        control: tr(&base.control),
    }
}

/// Portrait reflow of a fixed base: the panels VISIBLE in the base are
/// stacked vertically — terminal, keyboard, a row of side panels, control.
/// Panels placed off-screen in the base stay hidden, so a minimal base
/// automatically yields the phone arrangement.
fn reflow_base(base: &LayoutSpec, win_h: f32) -> LayoutSpec {
    let off = spec(200.0, 0.0, 16.0, 60.0);
    let small = win_h < 900.0;
    let has_kb = on_screen(&base.keyboard);
    let has_left = on_screen(&base.left_col);
    let has_right = on_screen(&base.right_col);
    let has_fs = on_screen(&base.filesystem);
    let has_ctl = on_screen(&base.control);
    let has_row = has_left || has_right || has_fs;

    let kb_h = if has_kb {
        if small { 31.0 } else { 18.5 }
    } else {
        0.0
    };
    let row_h = if has_row { 33.5 } else { 0.0 };
    let ctl_bar_h = if has_ctl && !has_row { 13.5 } else { 0.0 };

    let gap = 1.5f32;
    let mut used = 0.0f32;
    for h in [kb_h, row_h, ctl_bar_h] {
        if h > 0.0 {
            used += h + gap;
        }
    }
    let shell_h = (97.0 - gap - used).max(20.0);

    let mut out = LayoutSpec {
        shell: spec(0.5, gap, 99.0, shell_h),
        keyboard: off,
        left_col: off,
        right_col: off,
        filesystem: off,
        control: off,
    };
    let mut y = gap + shell_h + gap;
    if has_kb {
        out.keyboard = spec(0.5, y, 99.0, kb_h);
        y += kb_h + gap;
    }
    if has_row {
        let mut cols = 0;
        if has_left {
            cols += 1;
        }
        if has_right || has_fs {
            cols += 1;
        }
        if has_ctl {
            cols += 1;
        }
        let cw = (99.0 - (cols as f32 - 1.0)) / cols as f32;
        let mut x = 0.5;
        if has_left {
            let w = if cols == 3 { cw * 1.2 } else { cw };
            out.left_col = spec(x, y, w, row_h);
            x += w + 1.0;
        }
        if has_right || has_fs {
            let w = cw;
            if has_right {
                out.right_col = spec(x, y, w, row_h);
            }
            if has_fs {
                let fs_y = if has_right { y + 9.0 } else { y };
                out.filesystem = spec(x, fs_y, w, row_h - (fs_y - y));
            }
            x += w + 1.0;
        }
        if has_ctl {
            let w = (99.5 - x).max(10.0);
            out.control = spec(x, y, w, row_h);
        }
    } else if has_ctl {
        out.control = spec(0.5, y, 99.0, ctl_bar_h);
    }
    out
}
