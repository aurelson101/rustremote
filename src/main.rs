mod askpass_wire;
mod clipboard;
mod credentials;
mod dialogs;
mod discovery;
mod external;
mod i18n;
mod icons;
#[cfg(target_os = "macos")]
mod macos_bundle;
mod preferences;
mod profiles;
mod session_tree;
mod sftp;
mod sftp_ui;
mod simulation;
mod ssh_auth;
mod ssh_route;
mod store;
mod terminal;
mod tools;
mod transfers;
mod ui;

#[cfg(all(target_os = "macos", feature = "macos-gui-lab"))]
#[path = "../scripts/macos-gui-lab.rs"]
mod macos_gui_lab;

use gtk::prelude::*;

fn main() -> gtk::glib::ExitCode {
    #[cfg(target_os = "macos")]
    macos_bundle::configure();
    #[cfg(all(target_os = "macos", feature = "macos-gui-lab"))]
    match std::env::args().nth(1).as_deref() {
        Some("--macos-gui-lab") => return macos_gui_lab::run(),
        Some("--macos-rdp-lab") => return macos_gui_lab::rdp(),
        _ => {}
    }
    if std::env::args().any(|arg| arg == "--version") {
        println!(
            "RustRemote {} — {}",
            env!("CARGO_PKG_VERSION"),
            if cfg!(target_os = "macos") {
                "macOS SSH/SFTP/RDP preview"
            } else {
                "Linux SSH/RDP client"
            }
        );
        return gtk::glib::ExitCode::SUCCESS;
    }
    i18n::initialize();
    let app = gtk::Application::builder()
        .application_id("com.rustremote.Preview")
        .build();
    let args: Vec<String> = std::env::args().collect();
    let demo = args.iter().any(|arg| arg == "--demo");
    let enforced_offline = args.iter().any(|arg| arg == "--offline");
    let offline = demo || enforced_offline;
    let args: Vec<_> = args
        .into_iter()
        .filter(|arg| arg != "--offline" && arg != "--demo")
        .collect();
    app.connect_activate(move |app| ui::build(app, offline, demo, enforced_offline));
    if offline {
        if app.register(None::<&gtk::gio::Cancellable>).is_err() {
            eprintln!("Impossible d’initialiser le mode hors ligne.");
            return gtk::glib::ExitCode::FAILURE;
        }
        if app.is_remote() {
            eprintln!("Une instance est déjà ouverte. Fermez-la avant de lancer --offline.");
            return gtk::glib::ExitCode::FAILURE;
        }
    }
    app.run_with_args(&args)
}
