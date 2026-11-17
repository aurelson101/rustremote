use gtk::prelude::*;
use rustremote_core::profiles::{ProtocolOptions, SessionProfile};
use std::{cell::Cell, rc::Rc};

pub fn options(
    parent: &gtk::ApplicationWindow,
    profile: SessionProfile,
    save: impl Fn(SessionProfile) + 'static,
) {
    let ProtocolOptions::Rdp {
        width,
        height,
        fullscreen,
        clipboard,
        clipboard_files,
        settings,
        ..
    } = &profile.options
    else {
        return;
    };
    let dialog = gtk::Dialog::builder()
        .transient_for(parent)
        .destroy_with_parent(true)
        .modal(true)
        .title("RDP options")
        .default_width(560)
        .default_height(600)
        .build();
    let form = gtk::Box::new(gtk::Orientation::Vertical, 10);
    let size = gtk::DropDown::from_strings(&[
        "Fixed resolution",
        "Automatic resize",
        "Scale remote desktop",
    ]);
    size.set_selected(settings.size_mode.into());
    let audio = gtk::DropDown::from_strings(&[
        "Play on this computer",
        "Play on remote computer",
        "No sound",
    ]);
    audio.set_selected(settings.audio_mode.into());
    audio.set_widget_name("rdp-audio");
    let w = gtk::SpinButton::with_range(640.0, 8192.0, 1.0);
    w.set_value(f64::from(*width));
    let h = gtk::SpinButton::with_range(480.0, 8192.0, 1.0);
    h.set_value(f64::from(*height));
    for (name, widget) in [
        ("Display mode", size.upcast_ref::<gtk::Widget>()),
        ("Width", w.upcast_ref()),
        ("Height", h.upcast_ref()),
        ("Remote audio", audio.upcast_ref()),
    ] {
        form.append(&gtk::Label::new(Some(name)));
        form.append(widget);
    }
    let full = gtk::CheckButton::with_label("Full screen");
    full.set_active(*fullscreen);
    let multi = gtk::CheckButton::with_label("Use multiple monitors");
    multi.set_active(settings.multi_monitor);
    let monitors = gtk::Entry::new();
    monitors.set_widget_name("rdp-monitors");
    monitors.set_placeholder_text(Some("Monitor IDs, for example 0,1 (blank = all)"));
    monitors.set_text(
        &settings
            .monitors
            .iter()
            .map(u16::to_string)
            .collect::<Vec<_>>()
            .join(","),
    );
    let clip = gtk::CheckButton::with_label("Allow clipboard (text and files with mstsc)");
    clip.set_active(*clipboard && *clipboard_files);
    let mic = gtk::CheckButton::with_label("Redirect microphone");
    mic.set_active(settings.microphone);
    let printers = gtk::CheckButton::with_label("Redirect local printers");
    printers.set_active(settings.printers);
    for check in [&full, &multi, &clip, &mic, &printers] {
        form.append(check);
    }
    form.append(&monitors);
    let note = gtk::Label::new(Some(
        "NLA/CredSSP and strict certificate validation are enabled. Drive, USB, smart-card and serial redirection remain disabled. Text-only clipboard and SHA256 certificate pins cannot be enforced by this adapter. A saved profile password is read from Windows Credential Manager and encrypted with current-user DPAPI in the private temporary launch file. Windows policies may still require a prompt.",
    ));
    note.set_wrap(true);
    form.append(&note);
    let status = gtk::Label::new(None);
    status.set_wrap(true);
    form.append(&status);
    dialog.content_area().append(
        &gtk::ScrolledWindow::builder()
            .vexpand(true)
            .child(&form)
            .build(),
    );
    dialog.add_button("Cancel", gtk::ResponseType::Cancel);
    dialog.add_button("Save options", gtk::ResponseType::Ok);
    dialog.connect_response(move |dialog, response| {
        if response != gtk::ResponseType::Ok {
            gtk::prelude::GtkWindowExt::set_focus(dialog, None::<&gtk::Widget>);
            dialog.close();
            return;
        }
        let monitor_ids = if monitors.text().trim().is_empty() {
            Ok(Vec::new())
        } else {
            monitors
                .text()
                .split(',')
                .map(|value| value.trim().parse::<u16>())
                .collect::<Result<Vec<_>, _>>()
        };
        let Ok(ids) = monitor_ids else {
            status.set_text("Enter numeric monitor IDs separated by commas");
            return;
        };
        let mut profile = profile.clone();
        if let ProtocolOptions::Rdp {
            width,
            height,
            fullscreen,
            clipboard,
            clipboard_files,
            settings,
            ..
        } = &mut profile.options
        {
            *width = w.value_as_int() as u16;
            *height = h.value_as_int() as u16;
            *fullscreen = full.is_active();
            *clipboard = clip.is_active();
            *clipboard_files = clip.is_active();
            settings.size_mode = size.selected() as u8;
            settings.audio_mode = audio.selected() as u8;
            settings.multi_monitor = multi.is_active();
            settings.monitors = ids;
            settings.microphone = mic.is_active();
            settings.printers = printers.is_active();
        }
        match profile.validate() {
            Ok(()) => {
                gtk::prelude::GtkWindowExt::set_focus(dialog, None::<&gtk::Widget>);
                save(profile);
                dialog.close();
            }
            Err(error) => status.set_text(&error),
        }
    });
    dialog.present();
}

pub fn open(parent: &gtk::ApplicationWindow, profile: SessionProfile) -> Result<(), String> {
    crate::rdp::configuration(&profile)?;
    if !cfg!(windows) {
        return Err("mstsc requires Windows".into());
    }
    let app = parent.application().ok_or("Application unavailable")?;
    if app.windows().len() >= 18 {
        return Err("Close an existing session first".into());
    }
    let body = gtk::Box::new(gtk::Orientation::Vertical, 12);
    body.set_margin_top(16);
    body.set_margin_bottom(16);
    body.set_margin_start(16);
    body.set_margin_end(16);
    if let ProtocolOptions::Rdp { domain, .. } = &profile.options {
        let destination = gtk::Label::new(Some(&format!(
            "Target: {} / port {}\nAccount: {} · Domain: {}",
            profile.host, profile.port, profile.username, domain
        )));
        destination.set_wrap(true);
        destination.set_selectable(true);
        body.append(&destination);
    }
    let status = gtk::Label::new(Some(
        "Windows may first ask permission to open RDP files, then ask you to confirm this destination. Check the address above and enable only the local resources you want to share. Windows may still request credentials. Close the native RDP window to disconnect.",
    ));
    status.set_wrap(true);
    body.append(&status);
    let start = gtk::Button::with_label("Launch Windows Remote Desktop");
    body.append(&start);
    let window = gtk::ApplicationWindow::builder()
        .application(&app)
        .transient_for(parent)
        .title(format!("RDP — {}", profile.name))
        .default_width(560)
        .default_height(240)
        .child(&body)
        .build();
    let (stop, rx) = std::sync::mpsc::channel();
    let rx = Rc::new(std::cell::RefCell::new(Some(rx)));
    let started = Rc::new(Cell::new(false));
    let weak = status.downgrade();
    start.connect_clicked(move |button| {
        if started.replace(true) {
            return;
        }
        button.set_sensitive(false);
        if let Some(status) = weak.upgrade() {
            status.set_text("Opening Windows Remote Desktop. Check its security dialogs and destination; authentication and connection are not yet confirmed.");
        }
        let Some(receiver) = rx.borrow_mut().take() else {
            return;
        };
        let profile = profile.clone();
        let weak = weak.clone();
        crate::profile_ui::worker(
            move || {
                #[cfg(windows)]
                {
                    crate::rdp::launch(profile, receiver)
                }
                #[cfg(not(windows))]
                {
                    let _ = (profile, receiver);
                    Err("mstsc requires Windows".into())
                }
            },
            move |result| {
                if let Some(status) = weak.upgrade() {
                    status.set_text(match &result {
                        Ok(()) => "Controller closed",
                        Err(error) => error,
                    });
                }
            },
        );
    });
    window.connect_close_request(move |_| {
        let _ = stop.send(());
        gtk::glib::Propagation::Proceed
    });
    window.present();
    Ok(())
}
