mod askpass_wire;
mod clipboard;
mod credentials;
mod dialogs;
mod external;
mod i18n;
mod icons;
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
mod ui;

use gtk::prelude::*;

fn main() -> gtk::glib::ExitCode {
    if std::env::args().any(|arg| arg == "--version") {
        println!(
            "RustRemote {} — Linux SSH/RDP client",
            env!("CARGO_PKG_VERSION")
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
