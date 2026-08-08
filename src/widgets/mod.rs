//! Interface layout — a faithful replica of the eDEX-UI panel arrangement:
//! left column (17%), central terminal (65% x 60%), right column (17%),
//! bottom strip: file browser + on-screen keyboard.

pub mod boot;
pub mod clock;
pub mod control;
pub mod cpu;
pub mod editor;
pub mod filesystem;
pub mod hardware;
pub mod keyboard;
pub mod memory;
pub mod network;
pub mod popup;
pub mod processes;
pub mod settings;
pub mod shell;
pub mod sysinfo;

// The core framework (Rect, Ctx, the panel/layout model, fit_end) comes
// from the lib-ng-widgets crate.
pub use ng_widgets::base::*;
