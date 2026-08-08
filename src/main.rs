//! ng-term — an independent sci-fi terminal inspired by eDEX-UI, in Rust + Vulkan.
//! Left column with telemetry, central terminal, right column with network
//! and files, on-screen keyboard and control panel at the bottom.

mod config;
mod gfx;
mod pty;
mod shaders;
mod system;
mod widgets;

// The platform-independent base (drawing, fonts, themes, layout engine,
// terminal emulation) lives in the external lib-ng-base crate; re-export
// its modules under crate:: so the rest of the code links against them
// like before. This tree keeps only the Linux-specific parts.
pub use ng_base::{draw, flex, font, term, theme};

use std::path::{Path, PathBuf};
use std::sync::mpsc::Receiver;
use std::time::Instant;

use winit::event::{ElementState, Event, MouseButton, MouseScrollDelta, WindowEvent};
use winit::event_loop::{ControlFlow, EventLoopBuilder};
use winit::keyboard::{Key, ModifiersState, NamedKey};
use winit::platform::wayland::EventLoopBuilderExtWayland;
use winit::platform::x11::EventLoopBuilderExtX11;
use winit::window::{CursorIcon, Fullscreen, WindowBuilder};

use crate::pty::PtyEvent;
use crate::widgets::shell::TAB_COUNT;

/// One terminal session (tab): PTY + emulation + parser.
struct Session {
    term: term::Term,
    pty: pty::Pty,
    rx: Receiver<PtyEvent>,
    parser: vte::Parser,
}

impl Session {
    fn spawn(cols: usize, rows: usize, cwd: &Path) -> std::io::Result<Session> {
        let (pty, rx) = pty::Pty::spawn(cols as u16, rows as u16, Some(cwd))?;
        Ok(Session {
            term: term::Term::new(cols, rows),
            pty,
            rx,
            parser: vte::Parser::new(),
        })
    }

    /// Processes PTY data; returns true if the shell has exited.
    fn pump(&mut self) -> bool {
        let mut exited = false;
        for ev in self.rx.try_iter() {
            match ev {
                PtyEvent::Data(data) => {
                    let mut performer = term::Performer { term: &mut self.term };
                    for byte in data {
                        self.parser.advance(&mut performer, byte);
                    }
                }
                PtyEvent::Exited => exited = true,
            }
        }
        if !self.term.responses.is_empty() {
            let resp = std::mem::take(&mut self.term.responses);
            self.pty.write(&resp);
        }
        exited
    }
}

fn main() {
    // Configuration: ~/.config/ng-term (created on first start).
    let (cfg, startup_warning) = config::load();
    let mut theme = cfg.theme;
    let mut layout_spec = cfg.layout;
    let mut fonts = font::FontSystem::new();
    // Font preferences (size scales + family/weight, terminal and UI).
    let (mut font_scale, tfam, twgt) = config::term_font_prefs();
    let (mut ui_font_scale, ufam, uwgt) = config::ui_font_prefs();
    // Widget padding: content inset from the outer panel edge (GRID view).
    let mut ui_padding = config::grid_prefs().3 as f32;
    let mut last_term_key = (tfam.clone().unwrap_or_default(), twgt.clone().unwrap_or_default());
    let mut last_ui_key = (ufam.clone().unwrap_or_default(), uwgt.clone().unwrap_or_default());
    if tfam.is_some() || twgt.is_some() {
        if let Some(f) = font::load_variant_for(tfam.as_deref(), twgt.as_deref(), false) {
            fonts.set_mono(f);
        }
    }
    if ufam.is_some() || uwgt.is_some() {
        if let Some(f) = font::load_variant_for(ufam.as_deref(), uwgt.as_deref(), true) {
            fonts.set_ui(f);
        }
    }

    // Window backend selection: Wayland natively, but an X11 session or
    // gamescope (a gaming compositor exposing XWayland) forces X11.
    let wayland = std::env::var("WAYLAND_DISPLAY")
        .map(|v| !v.is_empty())
        .unwrap_or(false);
    let x11 = std::env::var("DISPLAY").map(|v| !v.is_empty()).unwrap_or(false);
    let gamescope = std::env::var("GAMESCOPE_WAYLAND_DISPLAY").is_ok()
        || std::env::var("XDG_CURRENT_DESKTOP")
            .map(|d| d.to_lowercase().contains("gamescope"))
            .unwrap_or(false)
        || std::env::var("XDG_SESSION_DESKTOP")
            .map(|d| d.to_lowercase().contains("gamescope"))
            .unwrap_or(false);

    let event_loop = {
        let mut builder = EventLoopBuilder::new();
        if gamescope && x11 {
            eprintln!("ng-term: gamescope detected — X11 backend");
            builder.with_x11();
        } else if wayland && !gamescope {
            eprintln!("ng-term: Wayland backend (native)");
            builder.with_wayland();
        } else if x11 {
            eprintln!("ng-term: X11 backend");
            builder.with_x11();
        }
        builder.build().expect("cannot create event loop")
    };

    // Monitor resolution check (orientation-agnostic: a rotated 720x1280
    // panel is fine). Below the minimum the program does NOT start — only
    // a small dialog window with the message is shown.
    let monitor_size = event_loop
        .primary_monitor()
        .or_else(|| event_loop.available_monitors().next())
        .map(|m| m.size());
    if let Some(s) = monitor_size {
        let (long, short) = (s.width.max(s.height), s.width.min(s.height));
        if long < 1280 || short < 720 {
            eprintln!(
                "ng-term: monitor resolution {}x{} is below the 1280x720 minimum",
                s.width, s.height
            );
            run_resolution_dialog(event_loop, theme, fonts, s.width, s.height);
            return;
        }
    }

    let window = WindowBuilder::new()
        .with_title("ng-term")
        .with_decorations(false)
        .with_inner_size(winit::dpi::LogicalSize::new(1600.0, 900.0))
        // Start fullscreen right away, like eDEX-UI.
        .with_fullscreen(Some(Fullscreen::Borderless(None)))
        .build(&event_loop)
        .expect("cannot create window");
    // Minimum window size in landscape orientation.
    window.set_min_inner_size(Some(winit::dpi::PhysicalSize::new(1280u32, 720u32)));

    // Per-screen layout override matching the current monitor
    // (resolution + diagonal), refreshed on resize and config changes.
    let mut active_ov = layout_spec.pick(screen_key(&window)).cloned();

    let mut gfx = gfx::Gfx::new(&window);

    // System telemetry in the background.
    let sys = system::start();

    // Home directory — default start directory for the terminal and file panel.
    let home = std::env::var("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("/"));

    // Terminal sessions (tabs). Slot 0 starts immediately.
    let mut grid = (80usize, 24usize);
    let mut sessions: Vec<Option<Session>> = (0..TAB_COUNT).map(|_| None).collect();
    match Session::spawn(grid.0, grid.1, &home) {
        Ok(s) => sessions[0] = Some(s),
        Err(e) => {
            // No PTY = the terminal cannot run; exit cleanly with a
            // message instead of a panic backtrace.
            eprintln!("ng-term: cannot start the shell (PTY): {e}");
            return;
        }
    }
    let mut active: usize = 0;

    // Panel state.
    let mut kb = widgets::keyboard::Keyboard::new();
    let mut fsp = widgets::filesystem::Filesystem::new(home.clone());
    let mut control = widgets::control::Control::new();
    let mut settings = widgets::settings::Settings::new();
    let mut editor = widgets::editor::Editor::new();
    let mut popup = widgets::popup::Popup::new();
    if let Some(w) = startup_warning {
        popup.show(w);
    }

    let mut dl = draw::DrawList::new();
    let start = Instant::now();
    let mut mods = ModifiersState::empty();
    let mut mouse = (0.0f32, 0.0f32);

    event_loop
        .run(move |event, elwt| {
            elwt.set_control_flow(ControlFlow::Poll);
            match event {
                Event::WindowEvent { event, .. } => match event {
                    WindowEvent::CloseRequested => {
                        eprintln!("ng-term: compositor requested window close");
                        elwt.exit();
                    }
                    WindowEvent::Resized(_) | WindowEvent::ScaleFactorChanged { .. } => {
                        gfx.resize();
                        active_ov = layout_spec.pick(screen_key(&window)).cloned();
                    }
                    WindowEvent::ModifiersChanged(m) => mods = m.state(),
                    WindowEvent::CursorMoved { position, .. } => {
                        mouse = (position.x as f32, position.y as f32);
                        if editor.active && !settings.open {
                            let size = window.inner_size();
                            let (fw, fh) = (size.width as f32, size.height as f32);
                            editor.mouse_move(mouse.0, mouse.1, fw, fh);
                            // Move/resize cursors over the panels.
                            use widgets::editor::CursorKind;
                            window.set_cursor_icon(
                                match editor.cursor_at(mouse.0, mouse.1, fw, fh) {
                                    CursorKind::Move => CursorIcon::Grab,
                                    CursorKind::Ew => CursorIcon::EwResize,
                                    CursorKind::Ns => CursorIcon::NsResize,
                                    CursorKind::Nwse => CursorIcon::NwseResize,
                                    CursorKind::Nesw => CursorIcon::NeswResize,
                                    CursorKind::Normal => CursorIcon::Default,
                                },
                            );
                            return;
                        }
                        if settings.open {
                            settings.drag(mouse.0);
                        }
                        // Pointer cursor over the terminal tabs.
                        let size = window.inner_size();
                        let layout = outer_layout(
                            &layout_spec,
                            active_ov.as_ref(),
                            size.width as f32,
                            size.height as f32,
                            ui_padding,
                        )
                        .padded(ui_padding);
                        let pointer = if settings.open {
                            settings.hover(mouse.0, mouse.1)
                        } else {
                            let over_tab =
                                widgets::shell::tab_rects(layout.p(widgets::Panel::Shell), size.height as f32)
                                    .iter()
                                    .any(|tr| tr.contains(mouse.0, mouse.1));
                            let over_btn =
                                widgets::control::button_rects(layout.p(widgets::Panel::Control), size.height as f32)
                                    .iter()
                                    .any(|br| br.contains(mouse.0, mouse.1));
                            over_tab || over_btn
                        };
                        window.set_cursor_icon(if pointer {
                            CursorIcon::Pointer
                        } else {
                            CursorIcon::Default
                        });
                    }
                    WindowEvent::MouseWheel { delta, .. } => {
                        if editor.active {
                            return;
                        }
                        let dy = match delta {
                            MouseScrollDelta::LineDelta(_, y) => y,
                            MouseScrollDelta::PixelDelta(p) => p.y as f32 / 20.0,
                        };
                        let size = window.inner_size();
                        let layout = outer_layout(
                            &layout_spec,
                            active_ov.as_ref(),
                            size.width as f32,
                            size.height as f32,
                            ui_padding,
                        )
                        .padded(ui_padding);
                        if layout.p(widgets::Panel::Shell).contains(mouse.0, mouse.1) {
                            if let Some(s) = sessions[active].as_mut() {
                                s.term.scroll_view((dy * 3.0) as i32);
                            }
                        } else if layout.p(widgets::Panel::Filesystem).contains(mouse.0, mouse.1) {
                            fsp.wheel(dy * 40.0);
                        }
                    }
                    WindowEvent::MouseInput {
                        state: ElementState::Released,
                        button: MouseButton::Left,
                        ..
                    } => {
                        if editor.active && !settings.open {
                            editor.mouse_up();
                            return;
                        }
                        if editor.active && settings.open {
                            settings.release();
                            let (snap, cols, rows, pad) = config::grid_prefs();
                            let size = window.inner_size();
                            editor.sync_prefs(
                                snap,
                                cols,
                                rows,
                                pad as f32,
                                size.width as f32,
                                size.height as f32,
                            );
                            ui_padding = pad as f32;
                            return;
                        }
                        if settings.open && settings.release() {
                                let (new_cfg, warn) = config::resolve();
                                theme = new_cfg.theme;
                                layout_spec = new_cfg.layout;
                                active_ov =
                                    layout_spec.pick(screen_key(&window)).cloned();
                                if let Some(w) = warn {
                                    popup.show(w);
                                }
                                let (tscale, tfam, twgt) = config::term_font_prefs();
                                let (uscale, ufam, uwgt) = config::ui_font_prefs();
                                font_scale = tscale;
                                ui_font_scale = uscale;
                                let tkey = (
                                    tfam.clone().unwrap_or_default(),
                                    twgt.clone().unwrap_or_default(),
                                );
                                if tkey != last_term_key {
                                    last_term_key = tkey;
                                    if tfam.is_none() && twgt.is_none() {
                                        fonts.set_mono(font::load_default_mono());
                                    } else if let Some(f) = font::load_variant_for(
                                        tfam.as_deref(),
                                        twgt.as_deref(),
                                        false,
                                    ) {
                                        fonts.set_mono(f);
                                    }
                                }
                                let ukey = (
                                    ufam.clone().unwrap_or_default(),
                                    uwgt.clone().unwrap_or_default(),
                                );
                                if ukey != last_ui_key {
                                    last_ui_key = ukey;
                                    if ufam.is_none() && uwgt.is_none() {
                                        fonts.set_ui(font::load_default_ui());
                                    } else if let Some(f) = font::load_variant_for(
                                        ufam.as_deref(),
                                        uwgt.as_deref(),
                                        true,
                                    ) {
                                        fonts.set_ui(f);
                                    }
                                }
                            }
                    }
                    WindowEvent::MouseInput {
                        state: ElementState::Pressed,
                        button: MouseButton::Left,
                        ..
                    } => {
                        let size = window.inner_size();
                        let layout = outer_layout(
                            &layout_spec,
                            active_ov.as_ref(),
                            size.width as f32,
                            size.height as f32,
                            ui_padding,
                        )
                        .padded(ui_padding);
                        // A click on the warning popup dismisses it.
                        if popup.click(
                            mouse.0,
                            mouse.1,
                            size.width as f32,
                            size.height as f32,
                        ) {
                            return;
                        }
                        // The layout editor captures all clicks while active
                        // (unless the settings window is open over it).
                        if editor.active && !settings.open {
                            match editor.mouse_down(
                                mouse.0,
                                mouse.1,
                                size.width as f32,
                                size.height as f32,
                            ) {
                                widgets::editor::EditorHit::Save => {
                                    // Overwrite the currently selected layout —
                                    // only the changes, for this screen.
                                    let name = config::effective_components()
                                        .1
                                        .unwrap_or_else(|| "default".to_string());
                                    editor_save(
                                        &mut editor,
                                        &name,
                                        false,
                                        &mut theme,
                                        &mut layout_spec,
                                        &mut active_ov,
                                        &mut popup,
                                        screen_key(&window),
                                    );
                                }
                                widgets::editor::EditorHit::SaveAs => {
                                    editor.naming = Some(String::new());
                                }
                                widgets::editor::EditorHit::Exit => {
                                    // Back to the settings window, GRID view.
                                    editor.stop();
                                    settings.show_grid();
                                }
                                widgets::editor::EditorHit::Settings => {
                                    settings.show_grid();
                                }
                                widgets::editor::EditorHit::Handled => {}
                            }
                            return;
                        }
                        // An open settings window captures all clicks —
                        // except the editor buttons, which share its plane.
                        if settings.open {
                            if editor.active {
                                if let Some(hit) = editor.buttons_hit(
                                    mouse.0,
                                    mouse.1,
                                    size.width as f32,
                                    size.height as f32,
                                ) {
                                    match hit {
                                        widgets::editor::EditorHit::Settings => {
                                            // Toggle: hide the window.
                                            settings.close();
                                        }
                                        widgets::editor::EditorHit::Save => {
                                            let name = config::effective_components()
                                                .1
                                                .unwrap_or_else(|| "default".to_string());
                                            editor_save(
                                                &mut editor,
                                                &name,
                                                false,
                                                &mut theme,
                                                &mut layout_spec,
                                                &mut active_ov,
                                                &mut popup,
                                                screen_key(&window),
                                            );
                                        }
                                        widgets::editor::EditorHit::SaveAs => {
                                            settings.close();
                                            editor.naming = Some(String::new());
                                        }
                                        widgets::editor::EditorHit::Exit => {
                                            editor.stop();
                                            settings.show_grid();
                                        }
                                        widgets::editor::EditorHit::Handled => {}
                                    }
                                    return;
                                }
                            }
                            if settings.click(
                                mouse.0,
                                mouse.1,
                                size.width as f32,
                                size.height as f32,
                            ) {
                                let (new_cfg, warn) = config::resolve();
                                theme = new_cfg.theme;
                                layout_spec = new_cfg.layout;
                                active_ov =
                                    layout_spec.pick(screen_key(&window)).cloned();
                                if let Some(w) = warn {
                                    popup.show(w);
                                }
                                let (tscale, tfam, twgt) = config::term_font_prefs();
                                let (uscale, ufam, uwgt) = config::ui_font_prefs();
                                font_scale = tscale;
                                ui_font_scale = uscale;
                                let tkey = (
                                    tfam.clone().unwrap_or_default(),
                                    twgt.clone().unwrap_or_default(),
                                );
                                if tkey != last_term_key {
                                    last_term_key = tkey;
                                    if tfam.is_none() && twgt.is_none() {
                                        fonts.set_mono(font::load_default_mono());
                                    } else if let Some(f) = font::load_variant_for(
                                        tfam.as_deref(),
                                        twgt.as_deref(),
                                        false,
                                    ) {
                                        fonts.set_mono(f);
                                    }
                                }
                                let ukey = (
                                    ufam.clone().unwrap_or_default(),
                                    uwgt.clone().unwrap_or_default(),
                                );
                                if ukey != last_ui_key {
                                    last_ui_key = ukey;
                                    if ufam.is_none() && uwgt.is_none() {
                                        fonts.set_ui(font::load_default_ui());
                                    } else if let Some(f) = font::load_variant_for(
                                        ufam.as_deref(),
                                        uwgt.as_deref(),
                                        true,
                                    ) {
                                        fonts.set_ui(f);
                                    }
                                }
                            }
                            // EDIT GRID: hide settings, enter the editor
                            // with the current panel rectangles.
                            if settings.edit_requested {
                                settings.edit_requested = false;
                                if !editor.active {
                                    let (snap, cols, rows, pad) = config::grid_prefs();
                                    // The editor edits the OUTER panel rects.
                                    let outer = outer_layout(
                                        &layout_spec,
                                        active_ov.as_ref(),
                                        size.width as f32,
                                        size.height as f32,
                                        ui_padding,
                                    );
                                    editor.start(
                                        &outer,
                                        size.width as f32,
                                        size.height as f32,
                                        snap,
                                        cols,
                                        rows,
                                        pad as f32,
                                    );
                                }
                                // With the editor already running the window
                                // simply hides — back to the grid.
                            }
                            if editor.active {
                                let (snap, cols, rows, pad) = config::grid_prefs();
                                editor.sync_prefs(
                                    snap,
                                    cols,
                                    rows,
                                    pad as f32,
                                    size.width as f32,
                                    size.height as f32,
                                );
                            }
                            return;
                        }
                        // Terminal tabs: switching / opening a new session.
                        let tab_hit = widgets::shell::tab_rects(layout.p(widgets::Panel::Shell), size.height as f32)
                            .iter()
                            .position(|tr| tr.contains(mouse.0, mouse.1));
                        if let Some(i) = tab_hit {
                            if sessions[i].is_some() {
                                active = i;
                            } else {
                                // A new tab starts in the file panel's directory.
                                match Session::spawn(grid.0, grid.1, &fsp.cwd) {
                                    Ok(s) => {
                                        sessions[i] = Some(s);
                                        active = i;
                                    }
                                    Err(e) => eprintln!("ng-term: cannot open PTY: {e}"),
                                }
                            }
                        } else if layout.p(widgets::Panel::Keyboard).contains(mouse.0, mouse.1) {
                            if let Some(bytes) = kb.click(mouse.0, mouse.1) {
                                if let Some(s) = sessions[active].as_mut() {
                                    s.pty.write(&bytes);
                                    s.term.view_offset = 0;
                                }
                            }
                        } else if layout.p(widgets::Panel::Filesystem).contains(mouse.0, mouse.1) {
                            match fsp.click(mouse.0, mouse.1) {
                                Some(widgets::filesystem::FsEvent::OpenDir(dir)) => {
                                    // Entering a directory = cd in the active tab
                                    // (leading space skips bash history).
                                    if let Some(s) = sessions[active].as_mut() {
                                        let quoted =
                                            dir.display().to_string().replace('\'', "'\\''");
                                        s.pty.write(format!(" cd '{quoted}'\r").as_bytes());
                                        s.term.view_offset = 0;
                                    }
                                }
                                Some(widgets::filesystem::FsEvent::OpenFile(file)) => {
                                    // Application associated with the extension in the system.
                                    let _ = std::process::Command::new("xdg-open")
                                        .arg(&file)
                                        .stdin(std::process::Stdio::null())
                                        .stdout(std::process::Stdio::null())
                                        .stderr(std::process::Stdio::null())
                                        .spawn();
                                }
                                None => {}
                            }
                        } else if let Some(btn) =
                            control.click(mouse.0, mouse.1, layout.p(widgets::Panel::Control), size.height as f32)
                        {
                            match btn {
                                widgets::control::BTN_EXIT => {
                                    eprintln!("ng-term: closed from the control panel");
                                    elwt.exit();
                                }
                                widgets::control::BTN_SETTINGS => settings.show(),
                                _ => {}
                            }
                        }
                    }
                    WindowEvent::KeyboardInput { event: key_event, .. } => {
                        if key_event.state != ElementState::Pressed {
                            return;
                        }
                        // Layout editor: the SAVE AS prompt takes typing;
                        // otherwise ESC exits without saving. Nothing
                        // reaches the terminal.
                        if editor.active && !settings.open {
                            if editor.naming.is_some() {
                                match &key_event.logical_key {
                                    Key::Named(NamedKey::Enter) => {
                                        if let Some(name) = editor.naming.clone() {
                                            if !name.is_empty() {
                                                editor_save(
                                                    &mut editor,
                                                    &name,
                                                    true,
                                                    &mut theme,
                                                    &mut layout_spec,
                                                    &mut active_ov,
                                                    &mut popup,
                                                    screen_key(&window),
                                                );
                                            }
                                        }
                                    }
                                    Key::Named(NamedKey::Escape) => editor.naming = None,
                                    Key::Named(NamedKey::Backspace) => editor.backspace(),
                                    Key::Character(s) => editor.type_char(s),
                                    _ => {}
                                }
                            } else if let Key::Named(NamedKey::Escape) =
                                key_event.logical_key
                            {
                                editor.stop();
                            }
                            return;
                        }
                        // Open settings window: ESC closes, other keys
                        // do not reach the terminal.
                        if settings.open {
                            if let Key::Named(NamedKey::Escape) = key_event.logical_key {
                                settings.close();
                            }
                            return;
                        }
                        // Application shortcuts.
                        if let Key::Named(NamedKey::F11) = key_event.logical_key {
                            let fs = window.fullscreen();
                            window.set_fullscreen(if fs.is_some() {
                                None
                            } else {
                                Some(Fullscreen::Borderless(None))
                            });
                            return;
                        }
                        if mods.control_key() && mods.shift_key() {
                            if let Key::Character(s) = &key_event.logical_key {
                                if s.eq_ignore_ascii_case("q") {
                                    elwt.exit();
                                    return;
                                }
                            }
                        }
                        let app_cursor = sessions[active]
                            .as_ref()
                            .map(|s| s.term.app_cursor)
                            .unwrap_or(false);
                        if let Some(bytes) =
                            key_to_bytes(&key_event.logical_key, mods, app_cursor)
                        {
                            // Highlight the key on the on-screen keyboard.
                            match &key_event.logical_key {
                                Key::Character(s) => {
                                    if let Some(c) = s.chars().next() {
                                        kb.flash_char(c);
                                    }
                                }
                                Key::Named(NamedKey::Enter) => kb.flash_label("ENTER"),
                                Key::Named(NamedKey::Backspace) => kb.flash_label("BACK"),
                                Key::Named(NamedKey::Space) => kb.flash_label("SPACE"),
                                Key::Named(NamedKey::Tab) => kb.flash_label("TAB"),
                                Key::Named(NamedKey::Escape) => kb.flash_label("ESC"),
                                _ => {}
                            }
                            if let Some(s) = sessions[active].as_mut() {
                                s.pty.write(&bytes);
                                s.term.view_offset = 0;
                            }
                        }
                    }
                    WindowEvent::RedrawRequested => {
                        // Live preview of the size sliders while dragging.
                        if let Some((tscale, uscale)) = settings.live_scales() {
                            font_scale = tscale;
                            ui_font_scale = uscale;
                        }
                        // Live widget padding while the GRID view is open.
                        if let Some(p) = settings.live_padding() {
                            ui_padding = p as f32;
                        }
                        // The layout is recomputed from the window size every
                        // frame (src/flex.rs), so moving the window to another
                        // monitor or resizing it reflows the interface live.
                        // 1. PTY data for all sessions; exited sessions free their slot.
                        for slot in sessions.iter_mut() {
                            let exited = slot.as_mut().map(|s| s.pump()).unwrap_or(false);
                            if exited {
                                *slot = None;
                            }
                        }
                        if sessions[active].is_none() {
                            // Active session died — switch to the first live one.
                            match sessions.iter().position(|s| s.is_some()) {
                                Some(i) => active = i,
                                None => {
                                    eprintln!("ng-term: all shells have exited");
                                    elwt.exit();
                                    return;
                                }
                            }
                        }

                        // 2. The file panel follows the active shell.
                        let cwd = sessions[active].as_ref().and_then(|s| s.pty.child_cwd());
                        fsp.follow(cwd);

                        // 3. Build the draw list.
                        let size = window.inner_size();
                        let (w, h) = (size.width as f32, size.height as f32);
                        if w < 8.0 || h < 8.0 {
                            return;
                        }
                        // Perform any deferred glyph-atlas reset at the frame
                        // boundary, never mid-frame (see font.rs).
                        fonts.begin_frame();
                        dl.clear();
                        let snap = sys.lock().unwrap().clone();
                        let mut ctx = widgets::Ctx {
                            dl: &mut dl,
                            fonts: &mut fonts,
                            theme: &theme,
                            w,
                            h,
                            t: start.elapsed().as_secs_f64(),
                            mouse,
                            term_font_scale: font_scale,
                            ui_font_scale,
                            panel_scale: 1.0,
                        };

                        let booting = widgets::boot::draw(&mut ctx);
                        if !booting {
                            // The editor shows its edited rectangles (WYSIWYG).
                            // Widgets draw inside the padded (content) rects;
                            // the editor overlay shows the outer edges.
                            let layout = if editor.active {
                                editor.layout(w, h)
                            } else {
                                outer_layout(
                                    &layout_spec,
                                    active_ov.as_ref(),
                                    w,
                                    h,
                                    ui_padding,
                                )
                            }
                            .padded(ui_padding);
                            // Telemetry widgets — each an individual panel;
                            // their text scales with the panel width.
                            use widgets::Panel as P;
                            {
                                let tele: [(P, fn(&mut widgets::Ctx, widgets::Rect, &system::Snapshot)); 5] = [
                                    (P::Clock, widgets::clock::draw),
                                    (P::Sysinfo, widgets::sysinfo::draw),
                                    (P::Hardware, widgets::hardware::draw),
                                    (P::Cpu, widgets::cpu::draw),
                                    (P::Memory, widgets::memory::draw),
                                ];
                                for (panel, f) in tele {
                                    let r = layout.p(panel);
                                    ctx.panel_scale = ctx.panel_font_scale(&r, panel);
                                    f(&mut ctx, r, &snap);
                                    ctx.panel_scale = 1.0;
                                }
                                let r = layout.p(P::Processes);
                                ctx.panel_scale = ctx.panel_font_scale(&r, P::Processes);
                                widgets::processes::draw(&mut ctx, r, &snap);
                                ctx.panel_scale = 1.0;
                            }
                            widgets::network::draw(&mut ctx, layout.p(P::Network), &snap);

                            let occupied: [bool; TAB_COUNT] =
                                std::array::from_fn(|i| sessions[i].is_some());
                            let active_term = &sessions[active].as_ref().unwrap().term;
                            let (cols, rows) = widgets::shell::draw(
                                &mut ctx,
                                layout.p(P::Shell),
                                active_term,
                                &occupied,
                                active,
                            );
                            fsp.draw(&mut ctx, layout.p(P::Filesystem));
                            kb.draw(&mut ctx, layout.p(P::Keyboard));
                            control.draw(&mut ctx, layout.p(P::Control));
                            // Settings window drawn on top.
                            // Grid overlay + editor controls on top of the
                            // live panels. The closure draws live widget
                            // miniatures inside the ADD WIDGET window.
                            if editor.active {
                                editor.draw(&mut ctx, |ctx, panel, r| {
                                    ctx.panel_scale =
                                        ctx.panel_font_scale(&r, widgets::Panel::ALL[panel]);
                                    match widgets::Panel::ALL[panel] {
                                        P::Clock => widgets::clock::draw(ctx, r, &snap),
                                        P::Sysinfo => widgets::sysinfo::draw(ctx, r, &snap),
                                        P::Hardware => {
                                            widgets::hardware::draw(ctx, r, &snap)
                                        }
                                        P::Cpu => widgets::cpu::draw(ctx, r, &snap),
                                        P::Memory => widgets::memory::draw(ctx, r, &snap),
                                        P::Processes => {
                                            widgets::processes::draw(ctx, r, &snap)
                                        }
                                        P::Shell => {
                                            let _ = widgets::shell::draw(
                                                ctx,
                                                r,
                                                active_term,
                                                &occupied,
                                                active,
                                            );
                                        }
                                        P::Network => widgets::network::draw(ctx, r, &snap),
                                        P::Filesystem => fsp.draw(ctx, r),
                                        P::Keyboard => kb.draw(ctx, r),
                                        P::Control => control.draw(ctx, r),
                                    }
                                    ctx.panel_scale = 1.0;
                                });
                            }
                            settings.draw(&mut ctx);
                            // With the settings window open over the editor
                            // its buttons share the window's plane.
                            if editor.active && settings.open {
                                editor.draw_buttons(&mut ctx);
                            }
                            // Warning popup on the very top.
                            popup.draw(&mut ctx);

                            // Fit all session grids to the panel size.
                            if (cols, rows) != grid {
                                grid = (cols, rows);
                                for s in sessions.iter_mut().flatten() {
                                    s.term.resize(cols, rows);
                                    s.pty.resize(cols as u16, rows as u16);
                                }
                            }
                        }

                        // 4. Render.
                        let atlas = if fonts.atlas_dirty {
                            fonts.atlas_dirty = false;
                            Some(fonts.atlas.clone())
                        } else {
                            None
                        };
                        gfx.render(
                            &window,
                            &dl.verts,
                            atlas.as_deref(),
                            [theme.bg.r, theme.bg.g, theme.bg.b, 1.0],
                        );
                    }
                    _ => {}
                },
                Event::AboutToWait => {
                    window.request_redraw();
                }
                _ => {}
            }
        })
        .expect("event loop ended with an error");
}

/// The current screen key: monitor resolution + diagonal in inches.
fn screen_key(window: &winit::window::Window) -> (u32, u32, u32) {
    match window.current_monitor().or_else(|| window.primary_monitor()) {
        Some(m) => {
            let s = m.size();
            let diag = m
                .name()
                .map(|n| config::monitor_diag_inches(&n))
                .unwrap_or(0);
            (s.width, s.height, diag)
        }
        None => (0, 0, 0),
    }
}

/// Outer layout for the current frame: the flex engine result plus the
/// per-screen override panels (before padding).
fn outer_layout(
    def: &config::LayoutDef,
    active: Option<&config::ResOverride>,
    w: f32,
    h: f32,
    pad: f32,
) -> widgets::Layout {
    let mut l = flex::compute(w, h, &def.base, pad);
    if let Some(ov) = active {
        for (p, ps) in &ov.panels {
            l.set(
                *p,
                widgets::Rect::new(
                    ps.x / 100.0 * w,
                    ps.y / 100.0 * h,
                    ps.w / 100.0 * w,
                    ps.h / 100.0 * h,
                ),
            );
        }
    }
    l
}

/// Saves the layout edited in the grid editor and applies it live.
/// `select` = also make it the selected layout (SAVE AS); a plain SAVE
/// keeps the current selection. Only the CHANGED panels are written,
/// into the section of the current screen (resolution + diagonal).
#[allow(clippy::too_many_arguments)]
fn editor_save(
    editor: &mut widgets::editor::Editor,
    name: &str,
    select: bool,
    theme: &mut theme::Theme,
    layout_spec: &mut config::LayoutDef,
    active_ov: &mut Option<config::ResOverride>,
    popup: &mut widgets::popup::Popup,
    key: (u32, u32, u32),
) {
    if name.is_empty() {
        return;
    }
    // SAVE AS writes ALL panels as the base of the (new) file; SAVE
    // rewrites the base on its own screen or stores only the changes in
    // the section of the current screen.
    let result = if select {
        config::save_layaut_full(name, &editor.spec(), key)
    } else {
        config::save_layaut_overrides(
            name,
            key,
            &editor.changes_since_start(),
            &editor.spec(),
        )
    };
    if let Err(e) = result {
        popup.show(format!("Cannot save layout '{name}': {e}"));
        return;
    }
    if select
        || (config::current_theme_name().is_none()
            && config::current_layaut_name().is_none())
    {
        config::select_layaut(name);
    }
    let (new_cfg, warn) = config::resolve();
    *theme = new_cfg.theme;
    *layout_spec = new_cfg.layout;
    *active_ov = layout_spec.pick(key).cloned();
    if let Some(wmsg) = warn {
        popup.show(wmsg);
    }
    editor.stop();
}

/// A small dialog window shown INSTEAD of the program when the monitor
/// resolution is below the 1280x720 minimum. OK, Enter/Escape or closing
/// the window quits.
fn run_resolution_dialog(
    event_loop: winit::event_loop::EventLoop<()>,
    theme: theme::Theme,
    mut fonts: font::FontSystem,
    mw: u32,
    mh: u32,
) {
    let window = WindowBuilder::new()
        .with_title("ng-term")
        .with_inner_size(winit::dpi::LogicalSize::new(640.0, 200.0))
        .with_resizable(false)
        .build(&event_loop)
        .expect("cannot create window");
    let mut gfx = gfx::Gfx::new(&window);
    let mut dl = draw::DrawList::new();
    let mut mouse = (0.0f32, 0.0f32);

    event_loop
        .run(move |event, elwt| {
            elwt.set_control_flow(ControlFlow::Wait);
            match event {
                Event::WindowEvent { event, .. } => match event {
                    WindowEvent::CloseRequested => elwt.exit(),
                    WindowEvent::Resized(_) | WindowEvent::ScaleFactorChanged { .. } => {
                        gfx.resize();
                    }
                    WindowEvent::CursorMoved { position, .. } => {
                        mouse = (position.x as f32, position.y as f32);
                        window.request_redraw();
                    }
                    WindowEvent::MouseInput {
                        state: ElementState::Pressed,
                        button: MouseButton::Left,
                        ..
                    } => {
                        let size = window.inner_size();
                        let ok = widgets::popup::resolution_dialog_ok_rect(
                            size.width as f32,
                            size.height as f32,
                        );
                        if ok.contains(mouse.0, mouse.1) {
                            elwt.exit();
                        }
                    }
                    WindowEvent::KeyboardInput { event: key_event, .. } => {
                        if key_event.state == ElementState::Pressed
                            && matches!(
                                key_event.logical_key,
                                Key::Named(NamedKey::Escape) | Key::Named(NamedKey::Enter)
                            )
                        {
                            elwt.exit();
                        }
                    }
                    WindowEvent::RedrawRequested => {
                        let size = window.inner_size();
                        let (w, h) = (size.width as f32, size.height as f32);
                        fonts.begin_frame();
                        dl.clear();
                        let mut ctx = widgets::Ctx {
                            dl: &mut dl,
                            fonts: &mut fonts,
                            theme: &theme,
                            w,
                            h,
                            t: 0.0,
                            mouse,
                            term_font_scale: 1.0,
                            ui_font_scale: 1.0,
                            panel_scale: 1.0,
                        };
                        widgets::popup::draw_resolution_dialog(&mut ctx, mw, mh);
                        let atlas = if fonts.atlas_dirty {
                            fonts.atlas_dirty = false;
                            Some(fonts.atlas.clone())
                        } else {
                            None
                        };
                        gfx.render(
                            &window,
                            &dl.verts,
                            atlas.as_deref(),
                            [theme.bg.r, theme.bg.g, theme.bg.b, 1.0],
                        );
                    }
                    _ => {}
                },
                Event::AboutToWait => {
                    window.request_redraw();
                }
                _ => {}
            }
        })
        .expect("event loop ended with an error");
}

/// Mapping of physical keys to terminal sequences.
fn key_to_bytes(key: &Key, mods: ModifiersState, app_cursor: bool) -> Option<Vec<u8>> {
    let esc: u8 = 0x1b;
    match key {
        Key::Character(s) => {
            let text = s.as_str();
            if mods.control_key() {
                if let Some(c) = text.chars().next() {
                    let lc = c.to_ascii_lowercase();
                    if lc.is_ascii_alphabetic() || "[\\]^_@".contains(lc) {
                        let mut out = Vec::new();
                        if mods.alt_key() {
                            out.push(esc);
                        }
                        out.push((lc as u8) & 0x1f);
                        return Some(out);
                    }
                }
                return None;
            }
            let mut out = Vec::new();
            if mods.alt_key() {
                out.push(esc);
            }
            out.extend_from_slice(text.as_bytes());
            Some(out)
        }
        Key::Named(n) => {
            let arrows = |ch: u8| -> Vec<u8> {
                if app_cursor {
                    vec![esc, b'O', ch]
                } else {
                    vec![esc, b'[', ch]
                }
            };
            let seq: Vec<u8> = match n {
                NamedKey::Enter => vec![b'\r'],
                NamedKey::Backspace => vec![0x7f],
                NamedKey::Tab => vec![b'\t'],
                NamedKey::Escape => vec![esc],
                NamedKey::Space => vec![b' '],
                NamedKey::ArrowUp => arrows(b'A'),
                NamedKey::ArrowDown => arrows(b'B'),
                NamedKey::ArrowRight => arrows(b'C'),
                NamedKey::ArrowLeft => arrows(b'D'),
                NamedKey::Home => arrows(b'H'),
                NamedKey::End => arrows(b'F'),
                NamedKey::PageUp => b"\x1b[5~".to_vec(),
                NamedKey::PageDown => b"\x1b[6~".to_vec(),
                NamedKey::Insert => b"\x1b[2~".to_vec(),
                NamedKey::Delete => b"\x1b[3~".to_vec(),
                NamedKey::F1 => b"\x1bOP".to_vec(),
                NamedKey::F2 => b"\x1bOQ".to_vec(),
                NamedKey::F3 => b"\x1bOR".to_vec(),
                NamedKey::F4 => b"\x1bOS".to_vec(),
                NamedKey::F5 => b"\x1b[15~".to_vec(),
                NamedKey::F6 => b"\x1b[17~".to_vec(),
                NamedKey::F7 => b"\x1b[18~".to_vec(),
                NamedKey::F8 => b"\x1b[19~".to_vec(),
                NamedKey::F9 => b"\x1b[20~".to_vec(),
                NamedKey::F10 => b"\x1b[21~".to_vec(),
                NamedKey::F11 => return None,
                NamedKey::F12 => b"\x1b[24~".to_vec(),
                _ => return None,
            };
            let mut out = Vec::new();
            if mods.alt_key() && *n != NamedKey::Escape {
                out.push(esc);
            }
            out.extend_from_slice(&seq);
            Some(out)
        }
        _ => None,
    }
}
