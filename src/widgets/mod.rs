//! Interface layout — a faithful replica of the eDEX-UI panel arrangement:
//! left column (17%), central terminal (65% x 60%), right column (17%),
//! bottom strip: file browser + on-screen keyboard.

// Widgets come from the external lib-ng-widgets crate; the modules kept
// here are the app-side ones (settings/config UI, layout editor, popup,
// boot animation).
pub mod boot;
pub mod editor;
pub mod popup;
pub mod settings;

pub use ng_widgets::{
    clock, control, cpu, filesystem, hardware, keyboard, memory, network,
    processes, shell, sysinfo,
};

// The core framework (Rect, Ctx, the panel/layout model, fit_end) comes
// from the lib-ng-widgets crate.
pub use ng_base::base::*;
