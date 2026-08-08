//! ng-term — an independent sci-fi terminal inspired by eDEX-UI, in Rust + Vulkan.
//! Left column with telemetry, central terminal, right column with network
//! and files, on-screen keyboard and control panel at the bottom.

mod audio;
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
pub use ng::{draw, flex, font, term, theme};

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

    // Sound. Optional by design: without a device the program simply
    // runs silent. The theme's meta file is what maps events to files.
    let mut audio = audio::Audio::new();
    if audio.is_none() {
        eprintln!("ng-term: no audio output available — running silent");
    }
    if let (Some(a), Some(dir)) = (audio.as_mut(), config::active_sounds_dir()) {
        a.load_theme(&dir);
        let (vol, typing, ambient) = config::sound_prefs();
        a.set_volume(vol as f32 / 100.0);
        a.set_typing_enabled(typing);
        a.set_ambient_enabled(ambient);
        eprintln!(
            "ng-term: audio {} Hz, sound theme '{}' ({} events)",
            a.rate(),
            dir.file_name().and_then(|n| n.to_str()).unwrap_or("?"),
            a.event_count()
        );
    }
    let mut sfx: Vec<ng::sound::Event> = Vec::new();
    ng::sound::emit(ng::sound::Event::Boot);

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

    // One instance per registered widget, built by name from the set the
    // widgets crate provides. A described widget the set has no renderer
    // for stays None: it takes part in the layout and draws nothing.
    // A description with blocks wins over the compiled renderer, so any
    // built-in can be replaced by editing its file, and a widget can
    // exist with no code behind it at all.
    let widget_set = ng_builtins::default_set();
    let mut widget_inst: Vec<Option<Box<dyn widgets::Widget>>> = widgets::Panel::all()
        .into_iter()
        .map(|p| {
            // A script is the widget. Failing that, a block description;
            // failing that, the compiled renderer registered under the
            // same name — which is all the interactive widgets have.
            if let Some(script) = config::widget_script(p.name()) {
                return Some(Box::new(ng::script::ScriptWidget::new(script))
                    as Box<dyn widgets::Widget>);
            }
            match config::widget_desc(p.name()) {
                Some(d) => Some(Box::new(ng::desc::DescWidget::new(d))
                    as Box<dyn widgets::Widget>),
                None => widget_set.make(p.name()),
            }
        })
        .collect();

    let mut settings = widgets::settings::Settings::new();
    let mut editor = widgets::editor::Editor::new();
    let mut popup = widgets::popup::Popup::new();
    if let Some(w) = startup_warning {
        ng::sound::emit(ng::sound::Event::Alert);
        popup.show(w);
    }

    // Handles of the interactive built-ins, resolved once. A registry
    // without one of them yields an index past its end, whose rectangle
    // is off-screen — so the widget simply never appears.
    let pnl = |name: &str| {
        widgets::Panel::from_name(name).unwrap_or(widgets::Panel(u16::MAX))
    };
    let p_shell = pnl("shell");
    let p_keyboard = pnl("keyboard");
    let p_control = pnl("control");

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
                                widgets::shell::tab_rects(layout.p(p_shell), size.height as f32)
                                    .iter()
                                    .any(|tr| tr.contains(mouse.0, mouse.1));
                            let over_btn =
                                widgets::control::button_rects(layout.p(p_control), size.height as f32)
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
                        let hit = widgets::Panel::all()
                            .into_iter()
                            .find(|p| layout.p(*p).contains(mouse.0, mouse.1));
                        if let Some(panel) = hit {
                            let r = layout.p(panel);
                            let occupied: Vec<bool> =
                                (0..TAB_COUNT).map(|i| sessions[i].is_some()).collect();
                            let action = {
                                let host = widgets::Host {
                                    snap: &sys.lock().unwrap().clone(),
                                    term: sessions[active].as_ref().map(|s| &s.term),
                                    tabs: &occupied,
                                    tab_active: active,
                                    shell_cwd: None,
                                    t: start.elapsed().as_secs_f64(),
                                    window: (size.width as f32, size.height as f32),
                                };
                                widget_inst
                                    .get_mut(panel.idx())
                                    .and_then(|w| w.as_mut())
                                    .map(|w| w.wheel(dy, r, &host))
                                    .unwrap_or(widgets::Action::None)
                            };
                            if let widgets::Action::ScrollTerminal(n) = action {
                                if let Some(s) = sessions[active].as_mut() {
                                    s.term.scroll_view(n);
                                }
                            }
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
                                // Sizes travel with the layout, so a new
                                // layout brings its own.
                                ng::base::set_panel_sizes(&new_cfg.layout.sizes);
                                theme = new_cfg.theme;
                                layout_spec = new_cfg.layout;
                                active_ov =
                                    layout_spec.pick(screen_key(&window)).cloned();
                                // A new look or sound set means new clips.
                                if let (Some(a), Some(dir)) =
                                    (audio.as_mut(), config::active_sounds_dir())
                                {
                                    a.load_theme(&dir);
                                }
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
                                // Sizes travel with the layout, so a new
                                // layout brings its own.
                                ng::base::set_panel_sizes(&new_cfg.layout.sizes);
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
                        // One route for every widget: find the one under
                        // the cursor, hand it the click, act on what it
                        // asks for. The application does not know which
                        // widget it is talking to.
                        let hit = widgets::Panel::all()
                            .into_iter()
                            .find(|p| layout.p(*p).contains(mouse.0, mouse.1));
                        let Some(panel) = hit else { return };
                        let r = layout.p(panel);
                        let occupied: Vec<bool> =
                            (0..TAB_COUNT).map(|i| sessions[i].is_some()).collect();
                        let action = {
                            let snap = sys.lock().unwrap().clone();
                            let host = widgets::Host {
                                snap: &snap,
                                term: sessions[active].as_ref().map(|s| &s.term),
                                tabs: &occupied,
                                tab_active: active,
                                shell_cwd: sessions[active]
                                    .as_ref()
                                    .and_then(|s| s.pty.child_cwd()),
                                t: start.elapsed().as_secs_f64(),
                                window: (size.width as f32, size.height as f32),
                            };
                            widget_inst
                                .get_mut(panel.idx())
                                .and_then(|w| w.as_mut())
                                .map(|w| w.click(mouse.0, mouse.1, r, &host))
                                .unwrap_or(widgets::Action::None)
                        };
                        // The on-screen keyboard sounds its own keys, so a
                        // click on it must not also click.
                        if !matches!(
                            action,
                            widgets::Action::None | widgets::Action::Bytes(_)
                        ) {
                            ng::sound::emit(ng::sound::Event::Click);
                        }
                        match action {
                            widgets::Action::Bytes(bytes) => {
                                if let Some(s) = sessions[active].as_mut() {
                                    s.pty.write(&bytes);
                                    s.term.view_offset = 0;
                                }
                            }
                            widgets::Action::OpenDir(dir) => {
                                // Entering a directory = cd in the active
                                // tab (a leading space skips bash history).
                                if let Some(s) = sessions[active].as_mut() {
                                    let quoted =
                                        dir.display().to_string().replace('\'', "'\\''");
                                    s.pty.write(format!(" cd '{quoted}'\r").as_bytes());
                                    s.term.view_offset = 0;
                                }
                            }
                            widgets::Action::OpenFile(file) => {
                                // Application associated with the extension.
                                let _ = std::process::Command::new("xdg-open")
                                    .arg(&file)
                                    .stdin(std::process::Stdio::null())
                                    .stdout(std::process::Stdio::null())
                                    .stderr(std::process::Stdio::null())
                                    .spawn();
                            }
                            widgets::Action::SelectTab(i) => {
                                if sessions[i].is_some() {
                                    active = i;
                                } else {
                                    // A new tab starts where the active
                                    // shell is, which is what the file
                                    // panel is showing.
                                    let start = sessions[active]
                                        .as_ref()
                                        .and_then(|s| s.pty.child_cwd())
                                        .unwrap_or_else(|| home.clone());
                                    match Session::spawn(grid.0, grid.1, &start) {
                                        Ok(s) => {
                                            sessions[i] = Some(s);
                                            active = i;
                                        }
                                        Err(e) => {
                                            eprintln!("ng-term: cannot open PTY: {e}")
                                        }
                                    }
                                }
                            }
                            widgets::Action::Exit => {
                                eprintln!("ng-term: closed from the control panel");
                                elwt.exit();
                            }
                            widgets::Action::OpenSettings => settings.show(),
                            widgets::Action::ScrollTerminal(n) => {
                                if let Some(s) = sessions[active].as_mut() {
                                    s.term.scroll_view(n);
                                }
                            }
                            widgets::Action::None => {}
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
                            let (ch, label) = match &key_event.logical_key {
                                Key::Character(s) => (s.chars().next(), None),
                                Key::Named(NamedKey::Enter) => (None, Some("ENTER")),
                                Key::Named(NamedKey::Backspace) => (None, Some("BACK")),
                                Key::Named(NamedKey::Space) => (None, Some("SPACE")),
                                Key::Named(NamedKey::Tab) => (None, Some("TAB")),
                                Key::Named(NamedKey::Escape) => (None, Some("ESC")),
                                _ => (None, None),
                            };
                            if let Some(wg) =
                                widget_inst.get_mut(p_keyboard.idx()).and_then(|w| w.as_mut())
                            {
                                wg.key_feedback(ch, label);
                            }
                            // Typing: Enter and Backspace have their own
                            // sounds, every other key shares the rotating
                            // Key variants.
                            ng::sound::emit(match &key_event.logical_key {
                                Key::Named(NamedKey::Enter) => {
                                    ng::sound::Event::KeyReturn
                                }
                                Key::Named(NamedKey::Backspace) => {
                                    ng::sound::Event::KeyErase
                                }
                                _ => ng::sound::Event::Key,
                            });
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

                        // 2. Session state the widgets are given this frame.
                        let occupied: Vec<bool> =
                            (0..TAB_COUNT).map(|i| sessions[i].is_some()).collect();
                        let shell_cwd =
                            sessions[active].as_ref().and_then(|s| s.pty.child_cwd());

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
                            // Every widget drawn through the one contract:
                            // the application no longer knows which is
                            // which, only what the registry lists.
                            {
                                let host = widgets::Host {
                                    snap: &snap,
                                    term: sessions[active].as_ref().map(|s| &s.term),
                                    tabs: &occupied,
                                    tab_active: active,
                                    shell_cwd: shell_cwd.clone(),
                                    t: start.elapsed().as_secs_f64(),
                                    window: (w, h),
                                };
                                for panel in widgets::Panel::all() {
                                    let r = layout.p(panel);
                                    ctx.panel_scale = ctx.panel_font_scale(&r, panel);
                                    if let Some(wg) =
                                        widget_inst.get_mut(panel.idx()).and_then(|w| w.as_mut())
                                    {
                                        wg.draw(&mut ctx, r, &host);
                                    }
                                    ctx.panel_scale = 1.0;
                                }
                                // Grid overlay + editor controls on top of
                                // the live panels; the closure draws live
                                // miniatures in the ADD WIDGET window.
                                if editor.active {
                                    editor.draw(&mut ctx, |ctx, panel, r| {
                                        let p = widgets::Panel(panel as u16);
                                        ctx.panel_scale = ctx.panel_font_scale(&r, p);
                                        if let Some(wg) =
                                            widget_inst.get_mut(p.idx()).and_then(|w| w.as_mut())
                                        {
                                            wg.draw(ctx, r, &host);
                                        }
                                        ctx.panel_scale = 1.0;
                                    });
                                }
                            }
                            // The terminal reports the character grid it
                            // settled on, so the PTY can be resized to it.
                            let (cols, rows) = widget_inst
                                .get(p_shell.idx())
                                .and_then(|w| w.as_ref())
                                .and_then(|w| w.grid())
                                .unwrap_or(grid);
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

                        // 4. Sound preferences changed in the SOUND view
                        // apply immediately, so dragging the volume
                        // slider is audible while dragging.
                        if settings.sound_dirty {
                            settings.sound_dirty = false;
                            let (vol, typing, ambient) = settings.sound_settings();
                            if let Some(a) = audio.as_mut() {
                                a.set_volume(vol);
                                a.set_typing_enabled(typing);
                                a.set_ambient_enabled(ambient);
                            }
                        }

                        // 5. Play whatever this frame reported. The theme
                        // decides which file each event maps to; an event
                        // it says nothing about is silently skipped.
                        ng::sound::drain(&mut sfx);
                        if let Some(a) = audio.as_mut() {
                            for e in sfx.iter() {
                                a.play(*e);
                            }
                        }

                        // 6. Render.
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
                // One exit hook for every way out — the close button,
                // Ctrl+Shift+Q, the compositor, the last shell dying.
                // Blocking briefly is the point: the process would
                // otherwise cut the sound off as it goes.
                Event::LoopExiting => {
                    if let Some(a) = audio.as_mut() {
                        a.play_blocking(ng::sound::Event::Shutdown, 1400);
                    }
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
        ng::sound::emit(ng::sound::Event::Error);
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
                                // Sizes travel with the layout, so a new
                                // layout brings its own.
                                ng::base::set_panel_sizes(&new_cfg.layout.sizes);
    *theme = new_cfg.theme;
    *layout_spec = new_cfg.layout;
    *active_ov = layout_spec.pick(key).cloned();
    if let Some(wmsg) = warn {
        ng::sound::emit(ng::sound::Event::Alert);
        popup.show(wmsg);
    } else {
        ng::sound::emit(ng::sound::Event::Save);
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
