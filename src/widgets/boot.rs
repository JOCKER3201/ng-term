//! Boot screen — a quick "boot log" and logo, like the eDEX-UI startup sequence.

use super::Ctx;
use crate::font::{FONT_MONO, FONT_UI};

const BOOT_LOG: &[&str] = &[
    "ng-term kernel interface initialized",
    "vulkan: loading ICDs and enumerating physical devices",
    "vulkan: swapchain acquired, FIFO present mode",
    "gpu pipeline compiled (naga -> SPIR-V)",
    "glyph atlas online (1024x1024 R8)",
    "mounting /proc and /sys data sources",
    "reading DMI tables",
    "cpu governor: performance metrics attached",
    "memory watcher armed",
    "spawning pty master/slave pair",
    "exec user shell",
    "network probe scheduled",
    "loading world map projection",
    "keyboard matrix mapped (en-US)",
    "filesystem tracker linked to shell cwd",
    "theme loaded: tron",
    "audio subsystem: skipped (headless fx)",
    "compositor bypass: direct-to-swapchain",
    "all modules nominal",
    "initiating boot sequence...",
];

/// Draws the boot screen. Returns true while the sequence lasts.
pub fn draw(ctx: &mut Ctx) -> bool {
    let t = ctx.t;
    if t > 3.0 {
        return false;
    }
    let px = ctx.font_px(1.3);
    if t < 1.8 {
        // Scrolling log.
        let lines_shown = ((t / 1.6) * BOOT_LOG.len() as f64) as usize;
        let mut y = ctx.vh(3.0);
        for (i, line) in BOOT_LOG.iter().take(lines_shown + 1).enumerate() {
            let color = if i == lines_shown {
                ctx.theme.base
            } else {
                ctx.theme.base.alpha(0.6)
            };
            ctx.dl.text(
                ctx.fonts,
                FONT_MONO,
                px,
                ctx.vw(2.0),
                y,
                &format!("[{:>8.4}] {}", i as f64 * 0.0138 + 0.02, line),
                color,
                0.0,
            );
            y += px * 1.45;
        }
    } else {
        // Logo.
        let big = ctx.font_px(7.0);
        ctx.dl.text_center(
            ctx.fonts,
            FONT_UI,
            big,
            ctx.w / 2.0,
            ctx.h / 2.0 - big,
            "NG-TERM",
            ctx.theme.base,
            big * 0.15,
        );
        let sub = ctx.font_px(1.2);
        let blink = if ctx.t.fract() < 0.5 { 1.0 } else { 0.35 };
        ctx.dl.text_center(
            ctx.fonts,
            FONT_UI,
            sub,
            ctx.w / 2.0,
            ctx.h / 2.0 + big * 0.4,
            "INITIATING BOOT SEQUENCE",
            ctx.theme.base.alpha(blink),
            sub * 0.3,
        );
    }
    true
}
