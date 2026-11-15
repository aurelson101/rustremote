//! Feasibility probe, not the full RustRemote Windows application.
use gtk::prelude::*;
use std::{cell::Cell, rc::Rc, time::Duration};
#[cfg(windows)]
mod conpty;

fn probe() -> Result<String, String> {
    #[cfg(windows)]
    {
        conpty::run()
    }
    #[cfg(not(windows))]
    {
        Err("ConPTY requires Windows; the Linux GUI smoke test does not validate it.".into())
    }
}
fn main() -> std::process::ExitCode {
    let args: Vec<String> = std::env::args().collect();
    if args.iter().any(|a| a == "--self-test") {
        return match probe() {
            Ok(report) => {
                println!("{report}");
                std::process::ExitCode::SUCCESS
            }
            Err(error) => {
                eprintln!("{error}");
                std::process::ExitCode::FAILURE
            }
        };
    }
    let smoke = args.iter().any(|a| a == "--smoke-ui");
    let smoke_ok = Rc::new(Cell::new(false));
    let seen = smoke_ok.clone();
    let app = gtk::Application::builder()
        .application_id("com.rustremote.WindowsProbe")
        .build();
    app.connect_activate(move |app| {
        let body = gtk::Box::new(gtk::Orientation::Vertical, 16);
        for side in [gtk::PositionType::Top, gtk::PositionType::Bottom, gtk::PositionType::Left, gtk::PositionType::Right] {
            match side {
                gtk::PositionType::Top => body.set_margin_top(24),
                gtk::PositionType::Bottom => body.set_margin_bottom(24),
                gtk::PositionType::Left => body.set_margin_start(24),
                _ => body.set_margin_end(24),
            }
        }
        let title = gtk::Label::new(Some("RustRemote — Windows feasibility probe"));
        title.add_css_class("title-1"); body.append(&title);
        let note = gtk::Label::new(Some("Technical prototype: GTK window and bounded local ConPTY test.\nNo remote connection, MSI, credentials or interactive ANSI renderer yet."));
        note.set_wrap(true); body.append(&note);
        let status = gtk::Label::new(Some("Ready — local test only"));
        status.set_wrap(true); status.set_selectable(true); body.append(&status);
        let run = gtk::Button::with_label("Run local ConPTY test");
        let output = status.clone();
        run.connect_clicked(move |button| {
            button.set_sensitive(false); output.set_text("Testing on a worker thread…");
            let (tx, rx) = std::sync::mpsc::sync_channel(1);
            std::thread::spawn(move || { let _ = tx.send(probe()); });
            let button = button.downgrade(); let output = output.downgrade();
            gtk::glib::timeout_add_local(Duration::from_millis(50), move || {
                match rx.try_recv() {
                    Ok(result) => {
                        if let Some(output) = output.upgrade() { output.set_text(&result.unwrap_or_else(|e| e)); }
                        if let Some(button) = button.upgrade() { button.set_sensitive(true); }
                        gtk::glib::ControlFlow::Break
                    }
                    Err(std::sync::mpsc::TryRecvError::Empty) => gtk::glib::ControlFlow::Continue,
                    Err(_) => {
                        if let Some(output) = output.upgrade() { output.set_text("Worker interrupted"); }
                        if let Some(button) = button.upgrade() { button.set_sensitive(true); }
                        gtk::glib::ControlFlow::Break
                    }
                }
            });
        });
        body.append(&run);
        let window = gtk::ApplicationWindow::builder().application(app).title("RustRemote Windows probe").default_width(760).default_height(300).child(&body).build();
        window.present();
        if smoke {
            let app = app.downgrade(); let seen = seen.clone();
            gtk::glib::timeout_add_local_once(Duration::from_secs(1), move || {
                seen.set(window.is_mapped() && window.width() > 0);
                if seen.get() { println!("GUI_SMOKE_OK"); }
                if let Some(app) = app.upgrade() { app.quit(); }
            });
        }
    });
    let result = app.run_with_args(&["rustremote-windows-probe"]);
    if smoke && !smoke_ok.get() {
        std::process::ExitCode::FAILURE
    } else {
        std::process::ExitCode::from(result.get())
    }
}
