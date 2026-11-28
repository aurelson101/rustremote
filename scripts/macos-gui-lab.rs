//! Native GTK smoke test. Built only with the macos-gui-lab feature.
use crate::profiles::{ProtocolOptions, RdpSettings, SessionProfile, Workspace};
use gtk::prelude::*;
use std::{
    cell::Cell,
    rc::Rc,
    sync::Arc,
    time::{Duration, Instant},
};
use vte::prelude::*;

fn visit(widget: &gtk::Widget, f: &mut impl FnMut(&gtk::Widget)) {
    f(widget);
    let mut child = widget.first_child();
    while let Some(widget) = child {
        child = widget.next_sibling();
        visit(&widget, f);
    }
}

fn check_simulations(app: &gtk::Application, profile: &SessionProfile) {
    let demo = Workspace::decode(include_bytes!("../assets/simulations/serveur-ia.json")).unwrap();
    let ia = &demo.profiles[0];
    assert!(ia.simulation && matches!(ia.options, ProtocolOptions::Ssh { .. }));
    let window = crate::simulation::show(app, ia);
    assert!(window.is_visible());
    assert!(window.title().unwrap().contains("Serveur IA"));
    window.close();
    assert!(!window.is_visible());

    let mut rdp = profile.clone();
    rdp.name = "RDP lab".into();
    rdp.simulation = true;
    rdp.options = ProtocolOptions::Rdp {
        domain: String::new(),
        width: 1280,
        height: 720,
        fullscreen: false,
        clipboard: false,
        clipboard_files: false,
        settings: RdpSettings::default(),
    };
    for _ in 0..12 {
        let window = crate::simulation::show(app, &rdp);
        assert!(window.is_visible());
        let mut toggle = None;
        let mut close = None;
        let mut controls = Vec::new();
        visit(window.upcast_ref(), &mut |widget| {
            if let Ok(button) = widget.clone().downcast::<gtk::ToggleButton>()
                && button.label().as_deref()
                    == Some(&crate::i18n::text(
                        "Autoriser le presse-papiers de la simulation",
                    ))
            {
                toggle = Some(button);
            }
            if let Ok(button) = widget.clone().downcast::<gtk::Button>()
                && let Some(label) = button.label()
            {
                if label == crate::i18n::text("Déconnecter la simulation") {
                    close = Some(button);
                } else if [
                    "Copier local",
                    "Copier distant",
                    "Coller local",
                    "Coller distant",
                ]
                .iter()
                .any(|text| label == crate::i18n::text(text))
                {
                    controls.push(button);
                }
            }
        });
        let toggle = toggle.unwrap();
        assert_eq!(controls.len(), 4);
        assert!(!toggle.is_active());
        for enabled in [false, true, false] {
            toggle.set_active(enabled);
            assert!(
                controls
                    .iter()
                    .all(|button| button.is_sensitive() == enabled)
            );
        }
        close.unwrap().emit_clicked();
        assert!(!window.is_visible());
    }
    println!("Serveur IA and 12 simulated RDP cycles passed");
}

fn saw_reply(terminal: Option<&vte::Terminal>) -> bool {
    let Some(terminal) = terminal else {
        return false;
    };
    let stream = gtk::gio::MemoryOutputStream::new_resizable();
    terminal
        .write_contents_sync(
            &stream,
            vte::WriteFlags::Default,
            gtk::gio::Cancellable::NONE,
        )
        .unwrap();
    stream.close(gtk::gio::Cancellable::NONE).unwrap();
    String::from_utf8_lossy(stream.steal_as_bytes().as_ref()).contains("RÉPONSE_DISTANTE — été")
}

fn check_clipboard_shortcuts(terminal: &vte::Terminal) {
    let controllers = terminal.observe_controllers();
    let mut triggers = Vec::new();
    for item in (0..controllers.n_items()).filter_map(|i| controllers.item(i)) {
        if let Ok(shortcuts) = item.downcast::<gtk::ShortcutController>() {
            for shortcut in (0..shortcuts.n_items()).filter_map(|i| shortcuts.item(i)) {
                if let Some(trigger) = shortcut
                    .downcast::<gtk::Shortcut>()
                    .ok()
                    .and_then(|s| s.trigger())
                {
                    triggers.push(trigger.to_str().to_string());
                }
            }
        }
    }
    for expected in ["<Meta>c", "<Meta>v", "<Meta>a"] {
        assert!(
            triggers.iter().any(|t| t == expected),
            "missing {expected} in {triggers:?}"
        );
    }
    println!("Cmd+C/V/A terminal shortcuts attached");
}

fn check_keychain(reference: &str, secret: &str) {
    use crate::credentials::{read, remove, write};
    assert!(read(reference).expect("keychain preflight").is_none());
    write(reference, secret).expect("temporary keychain entry");
    let found = read(reference);
    remove(reference).expect("remove temporary keychain entry");
    assert!(
        found
            .expect("read temporary keychain entry")
            .is_some_and(|entry| entry.as_str() == secret)
    );
    assert!(read(reference).expect("check removal").is_none());
    println!("Temporary keychain entry written, read and removed");
}

pub fn run() -> gtk::glib::ExitCode {
    let mut args = std::env::args().skip(2);
    let profile: SessionProfile = serde_json::from_slice(
        &std::fs::read(args.next().expect("fixture profile")).expect("read fixture profile"),
    )
    .expect("parse fixture profile");
    assert_eq!(profile.host, "127.0.0.1");
    assert!(profile.port > 1024);
    assert!(matches!(
        &profile.options,
        ProtocolOptions::Ssh { identity_file: Some(_), agent: false, known_hosts_file: Some(_), config_file: Some(config) }
            if config == "/dev/null"
    ));
    let password = Arc::new(crate::credentials::Secret::new(
        std::fs::read_to_string(args.next().expect("fixture password path"))
            .expect("read fixture password"),
    ));
    if let Some(reference) = args.next() {
        check_keychain(&reference, password.as_str());
    }
    let app = gtk::Application::builder()
        .application_id("com.rustremote.MacGuiLab")
        .flags(gtk::gio::ApplicationFlags::NON_UNIQUE)
        .build();
    app.connect_activate(move |app| {
        let window = gtk::ApplicationWindow::builder()
            .application(app)
            .title("RustRemote loopback lab")
            .default_width(980)
            .default_height(660)
            .build();
        let tabs = gtk::Notebook::new();
        let status = gtk::Label::new(None);
        window.set_child(Some(&tabs));
        window.present();
        check_simulations(app, &profile);
        let preferences = crate::preferences::Preferences::default();
        let first = crate::terminal::open(&tabs, &profile, &status, &preferences, None).unwrap();
        let mut password_profile = profile.clone();
        password_profile.name = "SSH password lab".into();
        if let ProtocolOptions::Ssh { identity_file, .. } = &mut password_profile.options {
            *identity_file = None;
        }
        let password = password.clone();
        let started = Instant::now();
        let disconnected = Rc::new(Cell::new(false));
        let mut step = 0;
        let mut second = None;
        let app = app.clone();
        gtk::glib::timeout_add_local(Duration::from_millis(50), move || {
            assert!(
                started.elapsed() < Duration::from_secs(30),
                "SSH phase {step}: {}",
                status.text()
            );
            let mut terminals = Vec::new();
            visit(tabs.upcast_ref(), &mut |widget| {
                if let Ok(terminal) = widget.clone().downcast::<vte::Terminal>() {
                    terminals.push(terminal);
                }
            });
            match step {
                0 if first.can_dispatch() => {
                    check_clipboard_shortcuts(&terminals[0]);
                    first.send_command("echo RUSTREMOTE_UTF8").unwrap();
                    step = 1;
                }
                1 if saw_reply(terminals.first()) => {
                    let ended = disconnected.clone();
                    terminals[0].connect_child_exited(move |_, _| ended.set(true));
                    first.disconnect();
                    second = Some(
                        crate::terminal::open(
                            &tabs,
                            &password_profile,
                            &status,
                            &preferences,
                            Some(password.clone()),
                        )
                        .unwrap(),
                    );
                    step = 2;
                }
                2 if second.as_ref().unwrap().can_dispatch() => {
                    second
                        .as_ref()
                        .unwrap()
                        .send_command("echo RUSTREMOTE_UTF8")
                        .unwrap();
                    step = 3;
                }
                3 if disconnected.get() && saw_reply(terminals.get(1)) => {
                    second.as_ref().unwrap().disconnect();
                    println!("Native SSH key/password, UTF-8 and PTY hangup passed");
                    window.close();
                    app.quit();
                    return gtk::glib::ControlFlow::Break;
                }
                _ => {}
            }
            gtk::glib::ControlFlow::Continue
        });
    });
    app.run_with_args(&["rustremote-macos-lab"])
}

pub fn rdp() -> gtk::glib::ExitCode {
    let mut args = std::env::args().skip(2);
    let profile: SessionProfile = serde_json::from_slice(
        &std::fs::read(args.next().expect("RDP fixture profile")).expect("read RDP fixture"),
    )
    .expect("parse RDP fixture");
    assert_eq!(profile.host, "127.0.0.1");
    assert!(profile.port > 1024 && !profile.simulation);
    assert!(matches!(
        profile.options,
        ProtocolOptions::Rdp {
            clipboard: false,
            clipboard_files: false,
            ..
        }
    ));
    let hold = args.next().as_deref() == Some("--hold");

    let mut password = crate::credentials::Secret::new(String::new());
    std::io::stdin()
        .read_line(&mut password)
        .expect("read fixture password");
    let length = password.trim_end_matches(['\r', '\n']).len();
    password.truncate(length);
    let client = crate::external::Client::spawn(&profile, password).expect("launch native RDP");
    if hold {
        assert!(client.active());
        println!("RustRemote launched RDP");
        // The driver checks the SDL window before asking the launcher to disconnect.
        let mut stop = String::new();
        std::io::stdin()
            .read_line(&mut stop)
            .expect("wait for RDP driver");
        assert!(client.active(), "RDP ended before disconnect");
        client.disconnect();
    }
    let deadline = Instant::now() + Duration::from_secs(20);
    while Instant::now() < deadline {
        if let Some(code) = client.exit_code() {
            if !hold {
                assert_ne!(code, 0, "invalid RDP connection succeeded");
            }
            println!("RustRemote RDP ended ({code})");
            return gtk::glib::ExitCode::SUCCESS;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    client.disconnect();
    gtk::glib::ExitCode::FAILURE
}
