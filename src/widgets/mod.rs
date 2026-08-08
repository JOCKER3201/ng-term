//! Interface layout — a faithful replica of the eDEX-UI panel arrangement:
//! left column (17%), central terminal (65% x 60%), right column (17%),
//! bottom strip: file browser + on-screen keyboard.

// The widgets themselves come from the external `widgets` crate; the
// framework they are written against is lib-ng-widgets. The modules kept
// here are the app-side interface: the settings window, the layout
// editor, the popup and the boot animation.
pub mod boot;
pub mod editor;
pub mod popup;
pub mod settings;

pub use ng_builtins::{
    clock, control, cpu, filesystem, hardware, keyboard, memory, network,
    processes, shell, sysinfo,
};

// Geometry, the panel/layout model and text fitting come from the base.
pub use ng_base::base::*;
