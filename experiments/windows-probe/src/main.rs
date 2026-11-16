//! Feasibility probe, not the full RustRemote Windows application.
use gtk::prelude::*;
use std::{cell::Cell, rc::Rc, time::Duration};
mod capture;
#[cfg(windows)]
mod conpty;
mod live;
mod live_ui;
mod probe_output;
mod profile_service;
mod profile_ui;
mod ssh_launch;
mod terminal;
mod terminal_keys;
#[cfg(windows)]
mod win_credentials;
#[cfg(windows)]
mod win_ssh;
#[cfg(windows)]
mod win_storage;

struct ProbeResult {
    report: String,
    screen: terminal::Frame,
}

fn probe() -> Result<ProbeResult, String> {
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
    #[cfg(windows)]
    if std::env::var_os(win_ssh::PROFILE_ENV).is_some() {
        return win_ssh::askpass();
    }
    let args: Vec<String> = std::env::args().collect();
    if args.iter().any(|a| a == "--self-test") {
        return match probe() {
            Ok(report) => {
                println!("{}", report.report);
                std::process::ExitCode::SUCCESS
            }
            Err(error) => {
                eprintln!("{error}");
                std::process::ExitCode::FAILURE
            }
        };
    }
    if args.iter().any(|a| a == "--self-test-live") {
        return match live::self_test() {
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
    if args.iter().any(|a| a == "--self-test-ssh") {
        #[cfg(windows)]
        let result = win_ssh::self_test();
        #[cfg(not(windows))]
        let result: Result<String, String> =
            Err("SSH native qualification requires Windows".into());
        return match result {
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
    if args.iter().any(|a| a == "--self-test-storage") {
        #[cfg(windows)]
        let result = win_storage::self_test().and_then(|storage| {
            win_credentials::self_test().map(|credentials| format!("{storage}\n{credentials}"))
        });
        #[cfg(not(windows))]
        let result: Result<String, String> = Err("Storage qualification requires Windows".into());
        return match result {
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
    let capture_ui = args.iter().any(|a| a == "--capture-ui");
    let smoke_profiles = args.iter().any(|a| a == "--smoke-profiles-ui");
    let smoke_live = args.iter().any(|a| a == "--smoke-live-ui");
    let smoke = args.iter().any(|a| a == "--smoke-ui");
    let smoke_ok = Rc::new(Cell::new(false));
    let seen = smoke_ok.clone();
    let app = gtk::Application::builder()
        .application_id("com.rustremote.WindowsProbe")
        .build();
    app.connect_activate(move |app| {
        if let Some(settings) = gtk::Settings::default() {
            if let Some(property) = settings.find_property("gtk-interface-color-scheme") {
                if let Some(value) = gtk::glib::EnumClass::with_type(property.value_type())
                    .and_then(|class| class.to_value_by_nick("dark"))
                {
                    settings.set_property_from_value("gtk-interface-color-scheme", &value);
                }
            } else {
                settings.set_gtk_application_prefer_dark_theme(true);
            }
        }
        let css = gtk::CssProvider::new();
        css.load_from_data("#ansi-screen { background: #151b24; padding: 12px; font-size: 14px; }
.profiles { background: #151b24; color: #e4e9f2; padding: 12px; }
.profiles entry, .profiles passwordentry { background: #0e141d; border: 1px solid #334254; border-radius: 7px; min-height: 32px; }
.profiles button { border-radius: 7px; padding: 8px 12px; }
.profiles button.suggested-action { background: #2463b6; color: white; }
.profiles list { background: #192332; border-radius: 8px; padding: 8px; }");
        if let Some(display) = gtk::gdk::Display::default() {
            gtk::style_context_add_provider_for_display(&display, &css, gtk::STYLE_PROVIDER_PRIORITY_APPLICATION);
        }
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
        title.set_wrap(true);
        title.add_css_class("title-1"); body.append(&title);
        let note = gtk::Label::new(Some("Windows preview: local terminal and saved connection profiles.\nRemote protocol launch and MSI packaging are still being integrated."));
        note.set_wrap(true); body.append(&note);
        let status = gtk::Label::new(Some("Ready — local test only"));
        status.set_wrap(true); status.set_selectable(true); body.append(&status);
        let screen = gtk::Label::new(None);
        screen.set_xalign(0.0); screen.set_yalign(0.0);
        screen.set_selectable(true); screen.set_wrap(false);
        let preview = terminal::preview();
        let preview_ok = preview.plain.contains("UTF-8: été");
        screen.set_markup(&preview.markup);
        screen.set_widget_name("ansi-screen");
        let viewport = gtk::ScrolledWindow::builder().hexpand(true).vexpand(true)
            .min_content_height(260).child(&screen).build();
        body.append(&viewport);
        let run = gtk::Button::with_label("Run local ConPTY test");
        let output = status.clone();
        let screen_output = screen.clone();
        run.connect_clicked(move |button| {
            button.set_sensitive(false); output.set_text("Testing on a worker thread…");
            let (tx, rx) = std::sync::mpsc::sync_channel(1);
            std::thread::spawn(move || { let _ = tx.send(probe()); });
            let button = button.downgrade(); let output = output.downgrade();
            let screen = screen_output.downgrade();
            gtk::glib::timeout_add_local(Duration::from_millis(50), move || {
                match rx.try_recv() {
                    Ok(result) => {
                        match result {
                            Ok(result) => {
                                if let Some(screen) = screen.upgrade() { screen.set_markup(&result.screen.markup); }
                                if let Some(output) = output.upgrade() { output.set_text(&result.report); }
                            }
                            Err(error) => if let Some(output) = output.upgrade() { output.set_text(&error); },
                        }
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
        let shell = gtk::ScrolledWindow::builder().hscrollbar_policy(gtk::PolicyType::Never).child(&body).build();
        let window = gtk::ApplicationWindow::builder().application(app).title("RustRemote Windows probe").default_width(760).default_height(600).child(&shell).build();
        live_ui::attach(&body, &screen, &viewport, &status, &window, live_ui::Options { target: ssh_launch::Target::Local, auto_start:false, enabled: smoke_live, capture: capture_ui, result: seen.clone() }, &run);
        profile_ui::attach(&body, &window, &status);
        window.present();
        if smoke_profiles { profile_ui::smoke(&window, seen.clone(), capture_ui); }
        if smoke {
            let app = app.downgrade(); let seen = seen.clone();
            gtk::glib::timeout_add_local_once(Duration::from_secs(1), move || {
                seen.set(window.is_mapped() && window.width() > 0
                    && preview_ok && screen.is_mapped() && screen.width() > 0
                    && screen.text().contains("Progress: 100%")
                    && !screen.text().contains('\u{1b}'));
                if capture_ui && let Err(error) = capture::window(&window, std::path::Path::new("dist/windows-probe/terminal-preview.png")) {
                    eprintln!("{error}"); seen.set(false);
                }
                if seen.get() { println!("ANSI_GUI_OK: mapped UTF-8/styled screen, escaped markup"); }
                if seen.get() { println!("GUI_SMOKE_OK"); }
                if let Some(app) = app.upgrade() { app.quit(); }
            });
        }
    });
    let result = app.run_with_args(&["rustremote-windows-probe"]);
    if (smoke || smoke_live || smoke_profiles) && !smoke_ok.get() {
        std::process::ExitCode::FAILURE
    } else {
        std::process::ExitCode::from(result.get())
    }
}
