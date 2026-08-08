//! Window-driven responsive layout — "like a website".
//!
//! Every frame the panel layout is computed from the ACTUAL window size,
//! so resizing or moving the window reflows the interface live. Layouts
//! are flexbox column descriptions (FlexLayaut) solved by the `taffy`
//! crate — the same layout algorithm web pages use: columns have real
//! min/max pixel widths (side columns shrink before the terminal does)
//! and collapse priorities (when a column can no longer fit its minimum
//! width it disappears — collapse=1 first, then 2, ...). If the control
//! panel loses its column, it comes back as a full-width bar at the
//! bottom. On portrait windows the visible panels restack vertically.
//! The built-in default layout and custom flexbox .layaut files share
//! this engine; legacy .layaut files (fixed x/y/w/h at the 16:9
//! reference) are re-adapted to the window with the older transform.

use crate::widgets::{
    FlexColumn, FlexLayaut, Layout, LayoutMode, LayoutSpec, Panel, PanelSpec, Rect,
};
use taffy::prelude::{auto, length, percent};
use taffy::style::{AvailableSpace, FlexDirection};
use taffy::{Size, Style, TaffyTree};

/// CSS-like pixel constraints of the built-in default columns.
const SIDE_MIN: f32 = 168.0;
const SIDE_MAX: f32 = 340.0;
const CENTER_MIN: f32 = 430.0;

/// The built-in default layout as a flexbox description — the same
/// structure a theme author writes in a flexbox .layaut file.
fn default_flex() -> FlexLayaut {
    let col = |basis, min, max, grow, collapse, gap, panels: &[(Panel, f32)]| FlexColumn {
        basis,
        min,
        max,
        grow,
        collapse,
        gap,
        panels: panels.to_vec(),
    };
    FlexLayaut {
        columns: vec![
            col(16.4, SIDE_MIN, SIDE_MAX, 0.0, 2, 2.5, &[
                (Panel::LeftCol, 59.5),
                (Panel::Control, 32.5),
            ]),
            col(65.0, CENTER_MIN, f32::INFINITY, 1.0, 0, 1.7, &[
                (Panel::Shell, 60.3),
                (Panel::Keyboard, 32.5),
            ]),
            col(16.4, SIDE_MIN, SIDE_MAX, 0.0, 1, 2.5, &[
                (Panel::RightCol, 12.4),
                (Panel::Filesystem, 79.6),
            ]),
        ],
    }
}

/// Layout for the current window size, recomputed every frame.
pub fn compute(w: f32, h: f32, mode: &LayoutMode) -> Layout {
    match mode {
        LayoutMode::Flex => engine(&default_flex(), w, h),
        LayoutMode::Custom(fl) => engine(fl, w, h),
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

fn set_panel(l: &mut Layout, p: Panel, r: Rect) {
    match p {
        Panel::LeftCol => l.left_col = r,
        Panel::Shell => l.shell = r,
        Panel::RightCol => l.right_col = r,
        Panel::Filesystem => l.filesystem = r,
        Panel::Keyboard => l.keyboard = r,
        Panel::Control => l.control = r,
    }
}

fn has_panel(fl: &FlexLayaut, p: Panel) -> bool {
    fl.columns.iter().any(|c| c.panels.iter().any(|(k, _)| *k == p))
}

fn engine(fl: &FlexLayaut, w: f32, h: f32) -> Layout {
    if h > w {
        portrait(fl, w, h)
    } else {
        landscape(fl, w, h)
    }
}

/// Landscape flexbox layout: the columns in a row, solved by taffy.
fn landscape(fl: &FlexLayaut, w: f32, h: f32) -> Layout {
    let pad_x = (w * 0.006).max(4.0);
    let gap = (w * 0.005).max(4.0);
    let inner = w - 2.0 * pad_x;

    // Collapse: drop columns (lowest collapse value first) while the
    // visible minimum widths do not fit; low windows also have no room
    // for the stacked side modules, so all collapsible columns go.
    let tall = h >= 520.0;
    let mut vis: Vec<&FlexColumn> = fl.columns.iter().collect();
    loop {
        let mins: f32 = vis.iter().map(|c| c.min.max(60.0)).sum::<f32>()
            + gap * (vis.len().saturating_sub(1)) as f32;
        let any_collapsible = vis.iter().any(|c| c.collapse > 0);
        if (mins <= inner && (tall || !any_collapsible)) || !any_collapsible {
            break;
        }
        let idx = vis
            .iter()
            .enumerate()
            .filter(|(_, c)| c.collapse > 0)
            .min_by_key(|(_, c)| c.collapse)
            .map(|(i, _)| i)
            .unwrap();
        vis.remove(idx);
    }

    // A layout that lost the control panel's column gets a full-width
    // control bar at the bottom instead.
    let control_dropped = has_panel(fl, Panel::Control)
        && !vis.iter().any(|c| c.panels.iter().any(|(p, _)| *p == Panel::Control));
    let bar_h = if control_dropped { h * 0.135 } else { 0.0 };

    // Column widths via taffy (flex-basis/grow/shrink + min/max).
    let mut tf: TaffyTree<()> = TaffyTree::new();
    let mut nodes = Vec::new();
    for c in &vis {
        let style = Style {
            flex_basis: percent(c.basis / 100.0),
            flex_grow: c.grow,
            flex_shrink: 1.0,
            min_size: Size { width: length(c.min.max(60.0)), height: auto() },
            max_size: Size {
                width: if c.max.is_finite() { length(c.max) } else { auto() },
                height: auto(),
            },
            ..Default::default()
        };
        nodes.push(tf.new_leaf(style).unwrap());
    }
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
            &nodes,
        )
        .unwrap();
    tf.compute_layout(
        root,
        Size { width: AvailableSpace::Definite(w), height: AvailableSpace::Definite(h) },
    )
    .unwrap();

    // Vertical placement: panels stacked by their height weights; gaps
    // count as weight units, so the classic proportions (a 94.5vh span
    // from 2.5vh to 97vh) come out exactly for the default layout.
    let top = h * 0.025;
    let mut content_bottom = h * 0.97;
    if control_dropped {
        content_bottom -= bar_h + h * 0.015;
    }
    let hi = (content_bottom - top).max(1.0);

    let mut out = Layout {
        left_col: off(w, h),
        shell: off(w, h),
        right_col: off(w, h),
        filesystem: off(w, h),
        keyboard: off(w, h),
        control: off(w, h),
    };
    for (c, node) in vis.iter().zip(&nodes) {
        let tl = tf.layout(*node).unwrap();
        let (cx, cw) = (tl.location.x, tl.size.width);
        let n = c.panels.len() as f32;
        let total: f32 =
            c.panels.iter().map(|(_, wt)| *wt).sum::<f32>() + c.gap * (n - 1.0).max(0.0);
        let mut y = top;
        for (p, wt) in &c.panels {
            let ph = wt / total.max(0.001) * hi;
            set_panel(&mut out, *p, Rect::new(cx, y, cw, ph));
            y += ph + c.gap / total.max(0.001) * hi;
        }
    }
    if control_dropped {
        out.control = Rect::new(pad_x, content_bottom + h * 0.015, inner, bar_h);
    }
    out
}

/// Portrait stack: terminal, keyboard, then (on windows tall enough) a
/// row of the remaining panels with the control panel bottom-anchored;
/// on phone-sized windows just a full-width control bar.
fn portrait(fl: &FlexLayaut, w: f32, h: f32) -> Layout {
    let has_kb = has_panel(fl, Panel::Keyboard);
    let has_left = has_panel(fl, Panel::LeftCol);
    let has_right = has_panel(fl, Panel::RightCol);
    let has_fs = has_panel(fl, Panel::Filesystem);
    let has_ctl = has_panel(fl, Panel::Control);

    let small = h < 900.0;
    let pad = (w * 0.008).max(4.0);
    let gap = (h * 0.012).max(4.0);
    let iw = w - 2.0 * pad;

    let kb_h = if has_kb {
        if small { h * 0.30 } else { h * 0.185 }
    } else {
        0.0
    };
    let has_row = (has_left || has_right || has_fs) && !small;
    let row_h = if has_row { h * 0.325 } else { 0.0 };
    // The control bar needs ~13vh: title (3.4) + two buttons (2 x 4.2 + 1.2).
    let ctl_bar_h = if has_ctl && !has_row { h * 0.135 } else { 0.0 };

    let mut used = 0.0;
    for ph in [kb_h, row_h, ctl_bar_h] {
        if ph > 0.0 {
            used += ph + gap;
        }
    }
    let shell_h = (h - 2.0 * gap - used).max(h * 0.2);

    let mut out = Layout {
        left_col: off(w, h),
        shell: off(w, h),
        right_col: off(w, h),
        filesystem: off(w, h),
        keyboard: off(w, h),
        control: off(w, h),
    };
    let mut y = gap;
    out.shell = Rect::new(pad, y, iw, shell_h);
    y += shell_h + gap;
    if kb_h > 0.0 {
        out.keyboard = Rect::new(pad, y, iw, kb_h);
        y += kb_h + gap;
    }
    if has_row {
        // Row: telemetry | network + files | control. Left/middle columns
        // start slightly lower, so their headers (drawn above the column)
        // line up with the MEMORY title of the control column and all
        // columns span the same height.
        let d = h * 0.03;
        let cgap = (w * 0.01).max(4.0);
        let mut units = 0.0f32;
        if has_left {
            units += 1.2;
        }
        if has_right || has_fs {
            units += 1.0;
        }
        if has_ctl {
            units += 1.0;
        }
        let ncols = (has_left as u32 + (has_right || has_fs) as u32 + has_ctl as u32) as f32;
        let cw = (iw - cgap * (ncols - 1.0).max(0.0)) / units.max(1.0);
        let mut x = pad;
        if has_left {
            out.left_col = Rect::new(x, y + d, cw * 1.2, row_h - d);
            x += cw * 1.2 + cgap;
        }
        if has_right || has_fs {
            if has_right {
                // Short panel: NETWORK STATUS only, the files below it.
                let nh = if has_fs { h * 0.078 } else { row_h - d };
                out.right_col = Rect::new(x, y + d, cw, nh);
            }
            if has_fs {
                let fs_y = if has_right { y + d + h * 0.09 } else { y + d };
                out.filesystem = Rect::new(x, fs_y, cw, y + row_h - fs_y);
            }
            x += cw + cgap;
        }
        if has_ctl {
            // The control panel sits at the very bottom of its column;
            // the space above it takes MEMORY + TOP PROCESSES (main).
            let ctl_h = h * 0.135;
            out.control =
                Rect::new(x, y + row_h - ctl_h, (w - pad - x).max(60.0), ctl_h);
        }
    } else if ctl_bar_h > 0.0 {
        out.control = Rect::new(pad, y, iw, ctl_bar_h);
    }
    out
}

// ---------------------------------------------------------------------------
// Adaptation of legacy fixed .layaut files (authored at 16:9).

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
