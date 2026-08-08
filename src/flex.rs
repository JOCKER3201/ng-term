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
//! bottom. On portrait windows the visible panels restack vertically
//! (large columns split into two). Every widget is an individual panel:
//! clock, sysinfo, hardware, cpu, memory, processes, shell, network,
//! filesystem, keyboard, control. The built-in default layout and custom
//! flexbox .layaut files share this engine; legacy .layaut files (fixed
//! x/y/w/h at the 16:9 reference) are re-adapted with an edge-anchored
//! transform on landscape and the flex restack on portrait.

use crate::widgets::{
    FlexColumn, FlexLayaut, Layout, LayoutMode, LayoutSpec, Panel, Rect, PANEL_COUNT,
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
            col(16.4, SIDE_MIN, SIDE_MAX, 0.0, 2, 1.0, &[
                (Panel::Clock, 7.0),
                (Panel::Sysinfo, 4.5),
                (Panel::Hardware, 5.5),
                (Panel::Cpu, 15.5),
                (Panel::Memory, 10.5),
                (Panel::Processes, 11.5),
                (Panel::Control, 31.0),
            ]),
            col(65.0, CENTER_MIN, f32::INFINITY, 1.0, 0, 1.7, &[
                (Panel::Shell, 60.3),
                (Panel::Keyboard, 32.5),
            ]),
            col(16.4, SIDE_MIN, SIDE_MAX, 0.0, 1, 2.5, &[
                (Panel::Network, 12.4),
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
            if h > w {
                // Portrait: restack the panels VISIBLE in the base using
                // the flex engine (the default structure filtered down).
                portrait_flex(&filtered_default(base), w, h)
            } else {
                Layout::compute(w, h, &edge_adapt(base, w / h))
            }
        }
    }
}

fn engine(fl: &FlexLayaut, w: f32, h: f32) -> Layout {
    if h > w {
        portrait_flex(fl, w, h)
    } else {
        landscape(fl, w, h)
    }
}

fn has_panel(fl: &FlexLayaut, p: Panel) -> bool {
    fl.columns.iter().any(|c| c.panels.iter().any(|(k, _)| *k == p))
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
        && !vis
            .iter()
            .any(|c| c.panels.iter().any(|(p, _)| *p == Panel::Control));
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

    let mut out = Layout::empty(w, h);
    for (c, node) in vis.iter().zip(&nodes) {
        let tl = tf.layout(*node).unwrap();
        let (cx, cw) = (tl.location.x, tl.size.width);
        let n = c.panels.len() as f32;
        let total: f32 =
            c.panels.iter().map(|(_, wt)| *wt).sum::<f32>() + c.gap * (n - 1.0).max(0.0);
        let mut y = top;
        for (p, wt) in &c.panels {
            let ph = wt / total.max(0.001) * hi;
            out.set(*p, Rect::new(cx, y, cw, ph));
            y += ph + c.gap / total.max(0.001) * hi;
        }
    }
    if control_dropped {
        out.set(
            Panel::Control,
            Rect::new(pad_x, content_bottom + h * 0.015, inner, bar_h),
        );
    }
    out
}

/// Portrait restack: terminal, keyboard, then the remaining panels in a
/// row of columns (a source column with many panels splits in two; the
/// control panel is anchored at the bottom of its column). Phone-sized
/// windows show just the terminal, keyboard and a control bar.
fn portrait_flex(fl: &FlexLayaut, w: f32, h: f32) -> Layout {
    let small = h < 900.0;
    let pad = (w * 0.008).max(4.0);
    let gap = (h * 0.012).max(4.0);
    let iw = w - 2.0 * pad;
    let mut out = Layout::empty(w, h);

    // Row columns: each source column contributes its panels (minus the
    // full-width shell/keyboard); more than 4 body panels split in two.
    let mut chunks: Vec<(Vec<(Panel, f32)>, bool)> = Vec::new();
    for c in &fl.columns {
        let body: Vec<(Panel, f32)> = c
            .panels
            .iter()
            .filter(|(p, _)| {
                !matches!(p, Panel::Shell | Panel::Keyboard | Panel::Control)
            })
            .cloned()
            .collect();
        let ctl = c.panels.iter().any(|(p, _)| *p == Panel::Control);
        if body.len() > 4 {
            let split = 4;
            chunks.push((body[..split].to_vec(), false));
            chunks.push((body[split..].to_vec(), ctl));
        } else if !body.is_empty() || ctl {
            chunks.push((body, ctl));
        }
    }

    let has_shell = has_panel(fl, Panel::Shell);
    let has_kb = has_panel(fl, Panel::Keyboard);
    let has_ctl = has_panel(fl, Panel::Control);
    let has_row = !chunks.is_empty() && !small;

    let kb_h = if has_kb {
        if small { h * 0.30 } else { h * 0.185 }
    } else {
        0.0
    };
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

    let mut y = gap;
    if has_shell {
        out.set(Panel::Shell, Rect::new(pad, y, iw, shell_h));
        y += shell_h + gap;
    }
    if kb_h > 0.0 {
        out.set(Panel::Keyboard, Rect::new(pad, y, iw, kb_h));
        y += kb_h + gap;
    }
    if !has_shell && !has_kb && has_row {
        // No full-width panels: the row takes the whole height.
        y = gap;
    }
    let row_h = if has_row && !has_shell && !has_kb {
        h - 2.0 * gap
    } else {
        row_h
    };

    if has_row {
        // Column headers (e.g. NETWORK) draw above their rect — start
        // the columns slightly lower to leave room for them.
        let d = h * 0.025;
        let cgap = (w * 0.01).max(4.0);
        let units: f32 = chunks
            .iter()
            .map(|(body, _)| if body.len() >= 4 { 1.2 } else { 1.0 })
            .sum();
        let ncols = chunks.len() as f32;
        let cw = (iw - cgap * (ncols - 1.0).max(0.0)) / units.max(0.5);
        let mut x = pad;
        for (body, ctl) in &chunks {
            let this_w = cw * if body.len() >= 4 { 1.2 } else { 1.0 };
            let ctl_h = if *ctl { h * 0.135 } else { 0.0 };
            let stack_h = row_h - d - ctl_h - if *ctl { gap } else { 0.0 };
            // Stack the body panels by their weights, with a minimum
            // height so short panels (e.g. NETWORK) keep their rows
            // readable instead of spilling over the next panel.
            let gap_w = 1.0f32;
            let total: f32 = body.iter().map(|(_, wt)| *wt).sum::<f32>()
                + gap_w * (body.len() as f32 - 1.0).max(0.0);
            let gap_px = gap_w / total.max(0.001) * stack_h;
            let mut hs: Vec<f32> = body
                .iter()
                .map(|(_, wt)| wt / total.max(0.001) * stack_h)
                .collect();
            let min_ph = (h * 0.085).min(stack_h / body.len().max(1) as f32);
            let mut excess = 0.0;
            for ph in hs.iter_mut() {
                if *ph < min_ph {
                    excess += min_ph - *ph;
                    *ph = min_ph;
                }
            }
            if excess > 0.0 {
                if let Some(imax) = (0..hs.len())
                    .max_by(|&a, &b| hs[a].partial_cmp(&hs[b]).unwrap())
                {
                    hs[imax] = (hs[imax] - excess).max(min_ph);
                }
            }
            let mut py = y + d;
            for ((p, _), ph) in body.iter().zip(&hs) {
                out.set(*p, Rect::new(x, py, this_w, *ph));
                py += ph + gap_px;
            }
            if *ctl {
                out.set(
                    Panel::Control,
                    Rect::new(x, y + row_h - ctl_h, this_w, ctl_h),
                );
            }
            x += this_w + cgap;
        }
    } else if ctl_bar_h > 0.0 {
        out.set(Panel::Control, Rect::new(pad, y, iw, ctl_bar_h));
    }
    out
}

/// The default flex structure filtered down to the panels visible in a
/// legacy fixed layout — used for its portrait restack.
fn filtered_default(base: &LayoutSpec) -> FlexLayaut {
    let mut fl = default_flex();
    for c in fl.columns.iter_mut() {
        c.panels.retain(|(p, _)| base.p(*p).x < 100.0);
    }
    fl.columns.retain(|c| !c.panels.is_empty());
    fl
}

/// Landscape adaptation of legacy fixed .layaut files (authored at the
/// 16:9 reference): an edge-anchored horizontal transform — panels keep
/// their distance to the nearer window edge, so side columns keep a sane
/// width on any aspect ratio.
fn edge_adapt(base: &LayoutSpec, ratio: f32) -> LayoutSpec {
    let f = ((16.0 / 9.0) / ratio).clamp(0.5, 1.4);
    if (f - 1.0).abs() < 0.001 {
        return base.clone();
    }
    let mut out = base.clone();
    for i in 0..PANEL_COUNT {
        let p = &base.panels[i];
        if p.x >= 100.0 {
            continue;
        }
        let a = p.x;
        let b = p.x + p.w;
        let na = if a <= 50.0 { a * f } else { 100.0 - (100.0 - a) * f };
        let nb = if b <= 50.0 { b * f } else { 100.0 - (100.0 - b) * f };
        out.panels[i] = crate::widgets::PanelSpec {
            x: na,
            y: p.y,
            w: (nb - na).max(1.0),
            h: p.h,
        };
    }
    out
}
