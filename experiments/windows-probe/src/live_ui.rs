use crate::{live::Session, terminal, terminal_keys};
use gtk::prelude::*;
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
    sync::Arc,
    time::Duration,
};

pub struct SmokeOptions {
    pub enabled: bool,
    pub capture: bool,
    pub result: Rc<Cell<bool>>,
}

pub fn attach(
    body: &gtk::Box,
    screen: &gtk::Label,
    viewport: &gtk::ScrolledWindow,
    status: &gtk::Label,
    window: &gtk::ApplicationWindow,
    smoke: SmokeOptions,
) {
    let session: Rc<RefCell<Option<Session>>> = Rc::new(RefCell::new(None));
    let bar = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    let start = gtk::Button::with_label("Open local terminal");
    start.set_sensitive(cfg!(windows));
    let stop = gtk::Button::with_label("Disconnect");
    stop.set_sensitive(false);
    let copy = gtk::Button::with_label("Copy selection");
    let paste = gtk::Button::with_label("Paste…");
    paste.set_sensitive(false);
    for button in [&start, &stop, &copy, &paste] {
        bar.append(button);
    }
    body.append(&bar);
    let menu = gtk::gio::Menu::new();
    menu.append(Some("Paste…"), Some("terminal.paste"));
    menu.append(Some("Disconnect"), Some("terminal.disconnect"));
    let actions = gtk::gio::SimpleActionGroup::new();
    for (name, button) in [("paste", &paste), ("disconnect", &stop)] {
        let action = gtk::gio::SimpleAction::new(name, None);
        let button = button.downgrade();
        action.connect_activate(move |_, _| {
            if let Some(button) = button.upgrade()
                && button.is_sensitive()
            {
                button.emit_clicked();
            }
        });
        actions.add_action(&action);
    }
    screen.insert_action_group("terminal", Some(&actions));
    screen.set_extra_menu(Some(&menu));

    screen.set_focusable(true);
    screen.set_tooltip_text(Some(
        "Click to type. Ctrl+Shift+C copies; paste requires confirmation. Ctrl+C interrupts.",
    ));

    let sessions = session.clone();
    let output = status.downgrade();
    let target = screen.downgrade();
    let stop_button = stop.downgrade();
    let paste_button = paste.downgrade();
    start.connect_clicked(move |button| {
        if sessions.borrow().is_some() {
            return;
        }
        match Session::start() {
            Ok(value) => {
                *sessions.borrow_mut() = Some(value);
                button.set_sensitive(false);
                if let Some(stop) = stop_button.upgrade() {
                    stop.set_sensitive(true);
                }
                if let Some(paste) = paste_button.upgrade() {
                    paste.set_sensitive(true);
                }
                if let Some(target) = target.upgrade() {
                    target.grab_focus();
                }
            }
            Err(error) => {
                if let Some(output) = output.upgrade() {
                    output.set_text(&error);
                }
            }
        }
    });
    let sessions = session.clone();
    stop.connect_clicked(move |_| {
        if let Some(session) = sessions.borrow().as_ref() {
            session.close();
        }
    });
    let target = screen.downgrade();
    copy.connect_clicked(move |_| {
        if let Some(target) = target.upgrade() {
            target.emit_copy_clipboard();
        }
    });

    let sessions = session.clone();
    let parent = window.downgrade();
    let output = status.downgrade();
    paste.connect_clicked(move |button| {
        let expected = sessions
            .borrow()
            .as_ref()
            .map(|session| session.state.clone());
        let sessions = sessions.clone();
        let parent = parent.clone();
        let output = output.clone();
        button
            .clipboard()
            .read_text_async(None::<&gtk::gio::Cancellable>, move |result| {
                let Some(parent) = parent.upgrade() else {
                    return;
                };
                let Ok(Some(text)) = result else {
                    return;
                };
                if text.len() > 4000
                    || text
                        .chars()
                        .any(|c| c.is_control() && !matches!(c, '\r' | '\n' | '\t'))
                {
                    if let Some(output) = output.upgrade() {
                        output.set_text(
                            "Paste refused: text over 4000 bytes or containing control sequences",
                        );
                    }
                    return;
                }
                let dialog = gtk::MessageDialog::builder()
                    .transient_for(&parent)
                    .modal(true)
                    .message_type(gtk::MessageType::Question)
                    .buttons(gtk::ButtonsType::OkCancel)
                    .text("Paste into this local terminal?")
                    .secondary_text(
                        "Pasted lines can execute commands. Confirm only text you trust.",
                    )
                    .build();
                dialog.connect_response(move |dialog, response| {
                    if response == gtk::ResponseType::Ok
                        && let Some(session) = sessions.borrow().as_ref()
                    {
                        // Delayed clipboard/dialog responses must never target a replacement session.
                        if !expected
                            .as_ref()
                            .is_some_and(|state| Arc::ptr_eq(state, &session.state))
                        {
                            dialog.close();
                            return;
                        }
                        let mut text = text
                            .replace("\r\n", "\n")
                            .replace('\r', "\n")
                            .replace('\n', "\r");
                        if session
                            .snapshot(0)
                            .is_some_and(|s| s.screen.bracketed_paste())
                        {
                            text = format!("\x1b[200~{text}\x1b[201~");
                        }
                        if let Err(error) = session.input(text.into_bytes())
                            && let Some(output) = output.upgrade()
                        {
                            output.set_text(&error);
                        }
                    }
                    dialog.close();
                });
                dialog.present();
            });
    });

    let input = gtk::IMMulticontext::new();
    input.set_client_widget(Some(screen));
    let sessions = session.clone();
    let output = status.downgrade();
    input.connect_commit(move |_, text| {
        if let Some(session) = sessions.borrow().as_ref()
            && let Err(error) = session.input(text.as_bytes().to_vec())
            && let Some(output) = output.upgrade()
        {
            output.set_text(&error);
        }
    });
    let focus = gtk::EventControllerFocus::new();
    let ime = input.clone();
    focus.connect_enter(move |_| ime.focus_in());
    let ime = input.clone();
    focus.connect_leave(move |_| ime.focus_out());
    screen.add_controller(focus);
    let keys = gtk::EventControllerKey::new();
    keys.set_propagation_phase(gtk::PropagationPhase::Capture);
    keys.set_im_context(Some(&input));
    let sessions = session.clone();
    let output = status.downgrade();
    let target = screen.downgrade();
    let paste_shortcut = paste.downgrade();
    keys.connect_key_pressed(move |_, key, _, modifiers| {
        use gtk::{
            gdk::{Key, ModifierType},
            glib::Propagation,
        };
        if modifiers.contains(ModifierType::CONTROL_MASK | ModifierType::SHIFT_MASK)
            && matches!(key, Key::c | Key::C)
        {
            if let Some(target) = target.upgrade() {
                target.emit_copy_clipboard();
            }
            return Propagation::Stop;
        }
        if modifiers.contains(ModifierType::CONTROL_MASK | ModifierType::SHIFT_MASK)
            && matches!(key, Key::v | Key::V)
        {
            if let Some(paste) = paste_shortcut.upgrade()
                && paste.is_sensitive()
            {
                paste.emit_clicked();
            }
            return Propagation::Stop;
        }
        let sessions = sessions.borrow();
        let Some(session) = sessions.as_ref() else {
            return Propagation::Proceed;
        };
        let app_cursor = session
            .snapshot(0)
            .is_some_and(|s| s.screen.application_cursor());
        if let Some(bytes) = terminal_keys::encode(key, modifiers, app_cursor) {
            if let Err(error) = session.input(bytes)
                && let Some(output) = output.upgrade()
            {
                output.set_text(&error);
            }
            Propagation::Stop
        } else {
            Propagation::Proceed
        }
    });
    screen.add_controller(keys);

    let sessions = session.clone();
    window.connect_close_request(move |_| {
        sessions.borrow_mut().take(); // Signals stop without joining on GTK.
        gtk::glib::Propagation::Proceed
    });
    if smoke.enabled {
        let capture_ui = smoke.capture;
        let smoke_ok = smoke.result;
        start.emit_clicked();
        let sessions = session.clone();
        let target = screen.downgrade();
        let output = status.downgrade();
        let window = window.downgrade();
        let stop_button = stop.downgrade();
        let input = input.clone();
        let deadline = std::time::Instant::now() + Duration::from_secs(20);
        let mut phase = 0;
        let mut before_size = None;
        gtk::glib::timeout_add_local(Duration::from_millis(100), move || {
            let (Some(target), Some(window)) = (target.upgrade(), window.upgrade()) else {
                return gtk::glib::ControlFlow::Break;
            };
            if std::time::Instant::now() > deadline {
                eprintln!("GUI live terminal timeout at phase {phase}");
                if let Some(session) = sessions.borrow().as_ref() {
                    session.close();
                }
                if let Some(app) = window.application() {
                    app.quit();
                }
                return gtk::glib::ControlFlow::Break;
            }
            match phase {
                0 => {
                    if sessions
                        .borrow()
                        .as_ref()
                        .is_some_and(|s| s.snapshot(0).is_some_and(|s| s.running))
                    {
                        // Exercise the actual IM commit -> queue -> ConPTY -> GTK path.
                        input.emit_by_name::<()>("commit", &[&"set RR_A=RUSTREMOTE_CONPTY_\r\nset RR_B=OK_été\r\necho %RR_A%%RR_B%\r\n"]);
                        phase = 1;
                    }
                }
                1 if target.text().contains("RUSTREMOTE_CONPTY_OK_été") => {
                    before_size = sessions
                        .borrow()
                        .as_ref()
                        .and_then(|s| s.snapshot(0))
                        .map(|s| s.screen.size());
                    window.set_default_size(1000, 740);
                    phase = 2;
                }
                2 => {
                    let resized = sessions
                        .borrow()
                        .as_ref()
                        .and_then(|s| s.snapshot(0))
                        .is_some_and(|s| Some(s.screen.size()) != before_size);
                    if resized {
                        if capture_ui
                            && let Err(error) = crate::capture::window(
                                &window,
                                std::path::Path::new("dist/windows-probe/terminal-live.png"),
                            )
                        {
                            eprintln!("{error}");
                            if let Some(session) = sessions.borrow().as_ref() {
                                session.close();
                            }
                            if let Some(app) = window.application() {
                                app.quit();
                            }
                            return gtk::glib::ControlFlow::Break;
                        }
                        if let Some(stop) = stop_button.upgrade() {
                            stop.emit_clicked();
                        }
                        phase = 3;
                    }
                }
                3 if sessions.borrow().is_none() => {
                    let stopped = output
                        .upgrade()
                        .is_some_and(|output| output.text() == "Local terminal stopped");
                    smoke_ok.set(stopped);
                    if !stopped {
                        eprintln!("GUI disconnect did not complete cleanly");
                    }
                    if stopped {
                        println!(
                            "GUI_LIVE_OK: IM commit, live ANSI display, window resize, disconnect"
                        );
                    }
                    if let Some(app) = window.application() {
                        app.quit();
                    }
                    return gtk::glib::ControlFlow::Break;
                }
                _ => {}
            }
            gtk::glib::ControlFlow::Continue
        });
    }

    let target = screen.downgrade();
    let output = status.downgrade();
    let viewport = viewport.downgrade();
    let start = start.downgrade();
    let stop = stop.downgrade();
    let paste = paste.downgrade();
    let mut revision = 0;
    let mut dimensions = (0, 0);
    gtk::glib::timeout_add_local(Duration::from_millis(50), move || {
        let (Some(target), Some(output), Some(viewport)) =
            (target.upgrade(), output.upgrade(), viewport.upgrade())
        else {
            session.borrow_mut().take();
            return gtk::glib::ControlFlow::Break;
        };
        let mut ended = false;
        if let Some(session) = session.borrow().as_ref() {
            // Measure the actual monospace font at the current scale/DPI.
            let measure = target.create_pango_layout(None);
            measure.set_markup("<tt>M</tt>");
            let (cw, ch) = measure.pixel_size();
            let cols = ((viewport.width() - 40) / cw.max(1)).clamp(20, 160) as u16;
            let rows = ((viewport.height() - 24) / ch.max(1)).clamp(5, 60) as u16;
            if dimensions != (cols, rows) && session.resize(cols, rows).is_ok() {
                dimensions = (cols, rows);
            }
            if let Some(snapshot) = session.snapshot(revision) {
                revision = snapshot.revision;
                if let Ok(frame) = terminal::frame(&snapshot.screen) {
                    target.set_markup(&frame.markup);
                }
                output.set_text(&snapshot.status);
                ended = !snapshot.running;
            }
        }
        if ended {
            session.borrow_mut().take();
            revision = 0;
            dimensions = (0, 0);
            if let Some(start) = start.upgrade() {
                start.set_sensitive(cfg!(windows));
            }
            if let Some(stop) = stop.upgrade() {
                stop.set_sensitive(false);
            }
            if let Some(paste) = paste.upgrade() {
                paste.set_sensitive(false);
            }
        }
        gtk::glib::ControlFlow::Continue
    });
}
