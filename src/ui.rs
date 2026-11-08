use gtk::prelude::*;
use gtk::{Application, ApplicationWindow, Box, Label, Orientation};

fn label(text: &str, class: &str) -> Label {
    let label = Label::new(Some(text));
    label.set_xalign(0.0);
    if !class.is_empty() {
        label.add_css_class(class);
    }
    label
}

fn panel_title(text: &str) -> Box {
    let bar = Box::new(Orientation::Horizontal, 6);
    bar.add_css_class("panel-title");
    let title = label(text, "heading");
    title.set_hexpand(true);
    bar.append(&title);
    bar
}

fn display_actions(
    app: &Application,
    window: &ApplicationWindow,
    tabs: &gtk::Notebook,
    controller: &crate::dialogs::Controller,
    panels: &[(&str, gtk::Widget)],
) {
    for (name, widget) in panels {
        let action = gtk::gio::SimpleAction::new_stateful(
            name,
            None,
            &widget.property::<bool>("visible").to_variant(),
        );
        let weak = widget.downgrade();
        action.connect_change_state(move |action, value| {
            if let Some(value) = value.and_then(|v| v.get::<bool>()) {
                if let Some(widget) = weak.upgrade() {
                    widget.set_visible(value);
                }
                action.set_state(&value.to_variant());
            }
        });
        action.connect_activate(|action, _| {
            let value = action.state().and_then(|v| v.get::<bool>()).unwrap_or(true);
            action.change_state(&(!value).to_variant());
        });
        app.add_action(&action);
    }
    let action = gtk::gio::SimpleAction::new_stateful("fullscreen", None, &false.to_variant());
    let weak = window.downgrade();
    action.connect_activate(move |_, _| {
        if let Some(w) = weak.upgrade() {
            if w.is_fullscreen() {
                w.unfullscreen();
            } else {
                w.fullscreen();
            }
        }
    });
    let state = action.clone();
    window.connect_fullscreened_notify(move |w| state.set_state(&w.is_fullscreen().to_variant()));
    app.add_action(&action);
    for (name, delta) in [("zoom-in", 1), ("zoom-out", -1), ("zoom-reset", 0)] {
        let action = gtk::gio::SimpleAction::new(name, None);
        let c = controller.clone();
        action.connect_activate(move |_, _| c.zoom_current(delta));
        app.add_action(&action);
    }
    let close = gtk::gio::SimpleAction::new("close-tab", None);
    let weak = tabs.downgrade();
    close.connect_activate(move |_, _| {
        if let Some(tabs) = weak.upgrade()
            && let Some(index) = tabs.current_page()
            && index > 0
        {
            tabs.remove_page(Some(index));
        }
    });
    app.add_action(&close);
    let c = controller.clone();
    tabs.connect_notify_local(Some("page"), move |_, _| c.sync_view_actions());
    controller.sync_view_actions();
    for (name, keys) in [
        ("app.fullscreen", &["F11"][..]),
        ("app.view-sessions", &["<Control>b"][..]),
        ("app.view-sftp", &["<Control><Shift>b"][..]),
        ("app.zoom-in", &["<Control>plus", "<Control>equal"][..]),
        ("app.zoom-out", &["<Control>minus"][..]),
        ("app.zoom-reset", &["<Control>0"][..]),
        ("app.close-tab", &["<Control><Shift>w"][..]),
    ] {
        app.set_accels_for_action(name, keys);
    }
}

fn terminal() -> gtk::ScrolledWindow {
    let view = gtk::TextView::new();
    view.set_editable(false);
    view.set_cursor_visible(false);
    view.set_monospace(true);
    view.set_left_margin(14);
    view.set_top_margin(12);
    view.add_css_class("terminal");
    let buffer = view.buffer();
    let green = buffer.create_tag(Some("prompt"), &[("foreground", &"#82e34a")]);
    let red = buffer.create_tag(Some("accent"), &[("foreground", &"#ef6b61")]);
    let muted = buffer.create_tag(Some("muted"), &[("foreground", &"#929aa5")]);
    let mut end = buffer.end_iter();
    buffer.insert_with_tags(&mut end, "demo@laboratoire", &[&green.unwrap()]);
    buffer.insert(&mut end, ":~$ rustremote --apercu\n\n");
    buffer.insert_with_tags(
        &mut end,
        "RustRemote · laboratoire de connexions\n",
        &[&red.unwrap()],
    );
    buffer.insert(&mut end, "────────────────────────────────────────\n\n");
    buffer.insert_with_tags(
        &mut end,
        "Aucune session distante ouverte.\n\n",
        &[&muted.unwrap()],
    );
    buffer.insert(&mut end,"  • Configurez une session SSH ou RDP depuis la barre d’outils.\n  • Importez vos profils et classez-les dans des dossiers.\n  • Testez l’édition et les transferts dans le laboratoire local.\n\n");
    buffer.insert(
        &mut end,
        "Les onglets de démonstration ne sont pas des terminaux SSH.\n",
    );
    buffer.insert(
        &mut end,
        "Données de démonstration — aucun serveur connecté.\n\nsimulation@laboratoire:~$ ▌",
    );
    gtk::ScrolledWindow::builder()
        .child(&view)
        .vexpand(true)
        .hexpand(true)
        .build()
}

fn open_demo(tabs: &gtk::Notebook, status: &Label, title: &str) {
    let page = terminal();
    let tab_header = Box::new(Orientation::Horizontal, 8);
    tab_header.append(&gtk::Image::from_icon_name("utilities-terminal"));
    tab_header.append(&Label::new(Some(title)));
    let close = gtk::Button::from_icon_name("window-close-symbolic");
    close.set_tooltip_text(Some("Fermer l’onglet"));
    close.add_css_class("tab-close");
    tab_header.append(&close);
    let notebook = tabs.clone();
    let page_copy = page.clone();
    let status_copy = status.clone();
    close.connect_clicked(move |_| {
        if let Some(index) = notebook.page_num(&page_copy) {
            notebook.remove_page(Some(index));
        }
        status_copy.set_text("Hors ligne · aperçu de démonstration");
    });
    let index = tabs.append_page(&page, Some(&tab_header));
    tabs.set_tab_reorderable(&page, true);
    tabs.set_current_page(Some(index));
    status.set_text("Hors ligne · aperçu de démonstration");
}

pub fn build(app: &Application, offline: bool, demo: bool, enforced_offline: bool) {
    if let Some(window) = app.active_window() {
        window.present();
        return;
    }
    gtk::gio::resources_register_include!("rustremote.gresource")
        .expect("Ressources graphiques intégrées");
    if let Some(settings) = gtk::Settings::default() {
        settings.set_gtk_application_prefer_dark_theme(true);
    }
    let css = gtk::CssProvider::new();
    css.load_from_data(include_str!("style.css"));
    gtk::style_context_add_provider_for_display(
        &gtk::gdk::Display::default().expect("Écran GTK disponible"),
        &css,
        gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
    );
    let root = Box::new(Orientation::Vertical, 0);
    let header = gtk::HeaderBar::new();
    header.set_title_widget(Some(&Label::new(Some(
        "RustRemote — Terminal / SSH / RDP / SFTP",
    ))));
    header.add_css_class("titlebar");
    let tabs = gtk::Notebook::new();
    tabs.set_hexpand(true);
    tabs.set_vexpand(true);
    tabs.set_scrollable(true);
    let welcome = Box::new(Orientation::Vertical, 12);
    welcome.add_css_class("welcome");
    welcome.append(&label("Bienvenue dans RustRemote", "heading"));
    let intro = label(
        "SSH et série : terminaux intégrés. SFTP : navigateur natif depuis une session SSH.\nRDP/VNC utilisent les clients système ; les onglets de démonstration restent hors ligne.",
        "dim-label",
    );
    intro.set_wrap(true);
    welcome.append(&intro);
    for (name, action) in [
        ("Nouveau dossier", "app.new-folder"),
        ("Connexion Linux (SSH)", "app.new-ssh"),
        ("Connexion Windows (RDP)", "app.new-rdp"),
    ] {
        let button = gtk::Button::with_label(name);
        button.set_action_name(Some(action));
        welcome.append(&button);
    }
    tabs.append_page(&welcome, Some(&Label::new(Some("Accueil"))));
    let status = label(
        if offline {
            "Hors ligne · aperçu de démonstration"
        } else {
            "Profils persistants · aucune session connectée"
        },
        "status-text",
    );

    let profiles = crate::dialogs::Controller::new(app, &status, offline, &tabs, enforced_offline);
    for name in [
        "new-profile",
        "new-ssh",
        "new-rdp",
        "new-vnc",
        "new-serial",
        "profiles",
        "settings",
        "vault",
        "tools",
        "about",
        "lock",
        "new-folder",
        "import",
        "export",
        "simulation",
        "sftp",
        "diagnostics",
    ] {
        let action = gtk::gio::SimpleAction::new(name, None);
        let controller = profiles.clone();
        action.connect_activate(move |_, _| match name {
            "new-profile" | "new-ssh" => controller.editor(None, false),
            "new-rdp" => controller.editor(None, true),
            "new-vnc" => controller.editor_protocol(None, 2),
            "new-serial" => controller.editor_protocol(None, 3),
            "profiles" => controller.manager(),
            "settings" => controller.settings(),
            "vault" => controller.vault(),
            "lock" => controller.lock(),
            "new-folder" => controller.new_folder(),
            "import" => controller.import_profiles(),
            "export" => controller.export_profiles(),
            "simulation" => controller.simulation_lab(),
            "sftp" => controller.sftp_current(),
            "diagnostics" => controller.diagnostics(),
            "tools" => controller.information("Outils", "Accès disponibles : profils SSH et RDP configurables.

VNC dispose d’un tunnel SSH optionnel ; série utilise picocom. SFTP natif est accessible depuis le terminal SSH. Ping et Traceroute sont configurables dans Paramètres. Les tunnels généraux, macros et Multi-Exec restent à implémenter et leurs boutons sont désactivés.

Les accès actuels utilisent les clients système OpenSSH / FreeRDP 3 ; aucune connexion ne part sans confirmation de sa cible."),
            _ => controller.information("À propos de RustRemote", &format!("RustRemote {} · Linux / Rust / GTK4 / VTE
Client indépendant, sans code propriétaire MobaXterm.
Gestion des profils SSH/RDP, chiffrement age optionnel.
SSH et série dans les onglets ; navigateur SFTP natif. RDP/VNC restent externes.", env!("CARGO_PKG_VERSION"))),
        });
        app.add_action(&action);
    }
    let new_action = gtk::gio::SimpleAction::new("new-demo", None);
    let notebook = tabs.clone();
    let status_copy = status.clone();
    new_action.connect_activate(move |_, _| open_demo(&notebook, &status_copy, "SSH · Aperçu"));
    app.add_action(&new_action);
    let quit = gtk::gio::SimpleAction::new("quit", None);
    let weak_app = app.downgrade();
    quit.connect_activate(move |_, _| {
        if let Some(app) = weak_app.upgrade() {
            app.quit();
        }
    });
    app.add_action(&quit);
    app.set_accels_for_action("app.new-profile", &["<Primary>n"]);
    app.set_accels_for_action("app.new-demo", &["<Primary>t"]);
    app.set_accels_for_action("app.quit", &["<Primary>q"]);
    let menubar = Box::new(Orientation::Horizontal, 2);
    menubar.add_css_class("menubar");
    for title in ["Fichier", "Sessions", "Outils", "Affichage", "Aide"] {
        let entries: &[(&str, &str)] = match title {
            "Fichier" => &[
                ("Nouveau profil…", "app.new-profile"),
                ("Gestionnaire de profils…", "app.profiles"),
                ("Nouveau dossier…", "app.new-folder"),
                ("Importer des profils…", "app.import"),
                ("Exporter les profils…", "app.export"),
                ("Verrouiller les profils…", "app.lock"),
                ("Quitter", "app.quit"),
            ],
            "Sessions" => &[
                ("Configurer SSH…", "app.new-ssh"),
                ("Configurer RDP…", "app.new-rdp"),
                ("Configurer VNC…", "app.new-vnc"),
                ("Configurer série / switch…", "app.new-serial"),
                ("Profils enregistrés…", "app.profiles"),
            ],
            "Outils" => &[
                ("Ping / Traceroute…", "app.diagnostics"),
                ("Laboratoire SFTP / presse-papiers…", "app.simulation"),
                ("Outils disponibles…", "app.tools"),
                ("Paramètres et protection…", "app.settings"),
            ],
            "Affichage" => &[
                ("Panneau Sessions", "app.view-sessions"),
                ("Panneau SFTP", "app.view-sftp"),
                ("Barre d’outils", "app.view-toolbar"),
                ("Barre d’état", "app.view-status"),
                ("Plein écran — F11", "app.fullscreen"),
                ("Agrandir le terminal", "app.zoom-in"),
                ("Réduire le terminal", "app.zoom-out"),
                ("Zoom 100 %", "app.zoom-reset"),
                ("Fermer l’onglet courant", "app.close-tab"),
                ("Ouvrir un onglet de démonstration", "app.new-demo"),
            ],
            _ => &[("À propos…", "app.about")],
        };
        let popover = gtk::Popover::new();
        let items = Box::new(Orientation::Vertical, 3);
        for (title, action) in entries {
            if action.starts_with("app.view-") || *action == "app.fullscreen" {
                let item = gtk::ToggleButton::with_label(title);
                item.set_action_name(Some(action));
                item.add_css_class("menu-item");
                let popup = popover.downgrade();
                item.connect_clicked(move |_| {
                    if let Some(p) = popup.upgrade() {
                        p.popdown();
                    }
                });
                items.append(&item);
                continue;
            }
            let item = gtk::Button::with_label(title);
            item.set_action_name(Some(action));
            item.add_css_class("menu-item");
            let popover = popover.clone();
            item.connect_clicked(move |_| popover.popdown());
            items.append(&item);
        }
        popover.set_child(Some(&items));
        let button = gtk::MenuButton::builder()
            .label(title)
            .popover(&popover)
            .build();
        button.add_css_class("menu-button");
        menubar.append(&button);
    }
    root.append(&menubar);
    let mode_banner = Box::new(Orientation::Vertical, 4);
    if demo {
        mode_banner.append(&label(
            "DÉMONSTRATION · profils temporaires · aucune connexion réelle",
            "preview-note",
        ));
    }
    if offline {
        let badge = label(
            "MODE HORS LIGNE · connexions réseau et diagnostics réels interdits",
            "preview-note",
        );
        mode_banner.append(&badge);
    }
    if demo {
        let exit = gtk::Button::with_label("Quitter la démonstration");
        exit.set_action_name(Some("app.exit-demo"));
        mode_banner.append(&exit);
    }
    mode_banner.set_visible(demo || offline);
    root.append(&mode_banner);
    let toolbar = Box::new(Orientation::Horizontal, 2);
    toolbar.add_css_class("toolbar");
    for (name, icon, class) in [
        ("Nouvelle session", "list-add", "blue"),
        ("SSH", "utilities-terminal", "gold"),
        ("RDP", "computer", "blue"),
        ("VNC", "video-display", "gold"),
        ("Série", "network-wired", "silver"),
        ("SFTP", "folder-remote", "gold"),
        ("Port Forward", "network-transmit-receive", "blue"),
        ("Tunnels", "network-wired", "blue"),
        ("Macros", "media-playlist-repeat", "blue"),
        ("Multi-Exec", "view-grid", "blue"),
        ("Coffre", "changes-prevent", "silver"),
        ("Paramètres", "emblem-system", "silver"),
    ] {
        let button = gtk::Button::new();
        button.add_css_class("tool");
        let contents = Box::new(Orientation::Vertical, 6);
        let image = match name {
            "SSH" => crate::icons::image("ssh"),
            "RDP" => crate::icons::image("windows"),
            "SFTP" => crate::icons::image("folder"),
            "VNC" => crate::icons::image("vnc"),
            "Série" => crate::icons::image("switch"),
            _ => gtk::Image::from_icon_name(icon),
        };
        image.set_pixel_size(24);
        image.add_css_class(class);
        contents.append(&image);
        contents.append(&Label::new(Some(name)));
        button.set_child(Some(&contents));
        let action = match name {
            "Nouvelle session" => Some("app.new-profile"),
            "SSH" => Some("app.new-ssh"),
            "RDP" => Some("app.new-rdp"),
            "VNC" => Some("app.new-vnc"),
            "Série" => Some("app.new-serial"),
            "Coffre" => Some("app.vault"),
            "Paramètres" => Some("app.settings"),
            "SFTP" => Some("app.sftp"),
            _ => None,
        };
        if let Some(action) = action {
            button.set_action_name(Some(action));
        } else {
            button.set_sensitive(false);
            button.set_tooltip_text(Some("À venir — non implémenté"));
        }
        toolbar.append(&button);
    }
    let toolbar_scroll = gtk::ScrolledWindow::builder()
        .hscrollbar_policy(gtk::PolicyType::Automatic)
        .vscrollbar_policy(gtk::PolicyType::Never)
        .child(&toolbar)
        .build();
    root.append(&toolbar_scroll);

    let sessions = Box::new(Orientation::Vertical, 0);
    sessions.add_css_class("panel");
    sessions.set_width_request(240);
    let sessions_header = panel_title("Sessions");
    let create_folder = gtk::Button::from_icon_name("folder-new-symbolic");
    create_folder.set_tooltip_text(Some("Nouveau dossier de connexions"));
    create_folder.update_property(&[gtk::accessible::Property::Label(
        "Nouveau dossier de connexions",
    )]);
    create_folder.set_action_name(Some("app.new-folder"));
    sessions_header.append(&create_folder);
    sessions.append(&sessions_header);
    let search = gtk::SearchEntry::new();
    search.set_placeholder_text(Some("Rechercher une session"));
    search.add_css_class("session-search");
    sessions.append(&search);
    let tree = Box::new(Orientation::Vertical, 2);
    tree.append(&label("★  Mes profils", "favorite"));
    let manage = gtk::Button::with_label("Gérer les profils…");
    manage.set_action_name(Some("app.profiles"));
    manage.add_css_class("profile-manage");
    tree.append(&manage);
    tree.append(&profiles.rows());
    let example_tree = Box::new(Orientation::Vertical, 2);
    example_tree.set_visible(demo || offline);
    tree.append(&example_tree);
    example_tree.append(&label("Exemples hors ligne", "preview-note"));
    let mut rows = Vec::new();
    for (group, examples) in [
        (
            "Laboratoire",
            vec![
                ("Linux démo (SSH)", "utilities-terminal"),
                ("Serveur démo (SSH)", "utilities-terminal"),
                ("Windows démo (RDP)", "computer"),
            ],
        ),
        ("Cloud", vec![("Bastion démo (SSH)", "utilities-terminal")]),
        ("Réseau", vec![("Routeur démo (SSH)", "network-wired")]),
        ("Console", vec![("Console démo (VNC)", "video-display")]),
    ] {
        let expander = gtk::Expander::new(Some(group));
        expander.set_expanded(true);
        expander.add_css_class("session-group");
        let children = Box::new(Orientation::Vertical, 0);
        for (name, icon) in examples {
            let row = gtk::Button::new();
            row.add_css_class("session-row");
            let contents = Box::new(Orientation::Horizontal, 8);
            contents.append(&if name.contains("Windows") {
                crate::icons::image("windows")
            } else if name.contains("Linux") {
                crate::icons::image("linux")
            } else {
                gtk::Image::from_icon_name(icon)
            });
            contents.append(&label(name, "session-label"));
            row.set_child(Some(&contents));
            if name.contains("SSH") {
                row.set_action_name(Some("app.new-demo"));
            } else {
                row.set_sensitive(false);
            }
            children.append(&row);
            rows.push((name.to_lowercase(), row));
        }
        expander.set_child(Some(&children));
        example_tree.append(&expander);
    }
    let profile_search = profiles.clone();
    search.connect_search_changed(move |entry| {
        let query = entry.text().to_lowercase();
        profile_search.filter(&query);
        for (name, row) in &rows {
            row.set_visible(name.contains(&query));
        }
    });
    sessions.append(
        &gtk::ScrolledWindow::builder()
            .child(&tree)
            .vexpand(true)
            .build(),
    );

    let sftp = Box::new(Orientation::Vertical, 0);
    sftp.add_css_class("panel");
    sftp.set_width_request(290);
    sftp.append(&panel_title("SFTP"));
    let sftp_preview = Box::new(Orientation::Vertical, 0);
    sftp_preview.set_vexpand(true);
    sftp_preview.set_visible(demo || offline);
    sftp.append(&sftp_preview);
    let sftp_real = Box::new(Orientation::Vertical, 12);
    let browser = gtk::Button::with_label("Ouvrir le navigateur SFTP de l’onglet SSH");
    browser.set_action_name(Some("app.sftp"));
    sftp_real.append(&browser);
    let hint = label(
        "Connectez un profil SSH, puis ouvrez son navigateur pour éditer et transférer vos fichiers et dossiers.",
        "preview-note",
    );
    hint.set_wrap(true);
    sftp_real.append(&hint);
    sftp_real.set_visible(!demo && !offline);
    sftp.append(&sftp_real);
    let lab = gtk::Button::with_label("Éditer / transférer en simulation…");
    lab.set_action_name(Some("app.simulation"));
    lab.set_tooltip_text(Some(
        "Fichiers temporaires du laboratoire, aucun serveur connecté",
    ));
    sftp_preview.append(&lab);
    let pathbar = Box::new(Orientation::Horizontal, 6);
    pathbar.add_css_class("pathbar");
    for icon in ["go-previous-symbolic", "go-next-symbolic", "go-up-symbolic"] {
        let button = gtk::Button::from_icon_name(icon);
        button.set_sensitive(false);
        pathbar.append(&button);
    }
    let path = gtk::Entry::new();
    path.set_text("/demo");
    path.set_editable(false);
    path.set_hexpand(true);
    path.set_width_chars(6);
    pathbar.append(&path);
    sftp_preview.append(&pathbar);
    let listing = gtk::Grid::new();
    listing.set_column_spacing(14);
    listing.add_css_class("file-list");
    for (column, title) in ["Nom", "Taille", "Modifié"].iter().enumerate() {
        listing.attach(&label(title, "column-title"), column as i32, 0, 1, 1);
    }
    for (index, (name, size)) in [
        ("..", "—"),
        ("documents", "4,0 Ko"),
        ("projets", "4,0 Ko"),
        ("scripts", "4,0 Ko"),
        ("README.md", "1,2 Ko"),
    ]
    .iter()
    .enumerate()
    {
        let item = Box::new(Orientation::Horizontal, 7);
        item.add_css_class("file-cell");
        item.append(&crate::icons::image(if name.ends_with(".md") {
            "file"
        } else {
            "folder"
        }));
        item.append(&label(name, ""));
        listing.attach(&item, 0, index as i32 + 1, 1, 1);
        listing.attach(&label(size, "file-cell"), 1, index as i32 + 1, 1, 1);
        listing.attach(&label("—", "file-cell"), 2, index as i32 + 1, 1, 1);
    }
    sftp_preview.append(&listing);
    sftp_preview.append(&label(
        "Fichiers fictifs · aucune session connectée",
        "preview-note",
    ));
    let spacer = Box::new(Orientation::Vertical, 0);
    spacer.set_vexpand(true);
    sftp_preview.append(&spacer);
    let transfers = gtk::Notebook::new();
    transfers.add_css_class("transfers");
    let table = Box::new(Orientation::Vertical, 0);
    table.append(&label(
        "Fichier       Progression       Statut",
        "column-title",
    ));
    table.append(&label("Aucun transfert en cours", "preview-note"));
    transfers.append_page(&table, Some(&Label::new(Some("Transferts"))));
    transfers.append_page(
        &label("File d’attente vide", "preview-note"),
        Some(&Label::new(Some("File d’attente"))),
    );
    transfers.set_height_request(160);
    sftp_preview.append(&transfers);
    let right_split = gtk::Paned::new(Orientation::Horizontal);
    right_split.set_start_child(Some(&tabs));
    right_split.set_end_child(Some(&sftp));
    right_split.set_position(760);
    right_split.set_resize_end_child(false);
    right_split.set_shrink_start_child(false);
    right_split.set_shrink_end_child(false);
    let split = gtk::Paned::new(Orientation::Horizontal);
    split.set_start_child(Some(&sessions));
    split.set_end_child(Some(&right_split));
    split.set_position(260);
    split.set_resize_start_child(false);
    split.set_shrink_start_child(false);
    split.set_vexpand(true);
    root.append(&split);
    let statusbar = Box::new(Orientation::Horizontal, 12);
    statusbar.add_css_class("statusbar");
    status.set_hexpand(true);
    statusbar.append(&status);
    let protocol_status = label(
        if offline {
            "SSH  │  SFTP  │  Aucun accès réseau"
        } else {
            "SSH  │  SFTP  │  Vérification des empreintes"
        },
        "dim-label",
    );
    statusbar.append(&protocol_status);
    root.append(&statusbar);
    if demo {
        open_demo(&tabs, &status, "Laboratoire (SSH)");
    }
    let exit_action = gtk::gio::SimpleAction::new("exit-demo", None);
    let controller = profiles.clone();
    let preview = sftp_preview.downgrade();
    let real_browser = sftp_real.downgrade();
    let banner = mode_banner.downgrade();
    let examples = example_tree.downgrade();
    let notebook = tabs.downgrade();
    let protocol = protocol_status.downgrade();
    let application = app.downgrade();
    exit_action.connect_activate(move |_, _| {
        let preview = preview.clone();
        let real_browser = real_browser.clone();
        let banner = banner.clone();
        let examples = examples.clone();
        let notebook = notebook.clone();
        let protocol = protocol.clone();
        let application = application.clone();
        controller.exit_demo(move || {
            if let Some(preview) = preview.upgrade() {
                preview.set_visible(enforced_offline);
            }
            if let Some(real) = real_browser.upgrade() {
                real.set_visible(!enforced_offline);
            }
            if let Some(banner) = banner.upgrade() {
                while let Some(child) = banner.first_child() {
                    banner.remove(&child);
                }
                if enforced_offline {
                    banner.append(&label(
                        "MODE HORS LIGNE · connexions réseau interdites",
                        "preview-note",
                    ));
                }
                banner.set_visible(enforced_offline);
            }
            if let Some(examples) = examples.upgrade() {
                examples.set_visible(false);
            }
            if let Some(tabs) = notebook.upgrade() {
                while tabs.n_pages() > 1 {
                    tabs.remove_page(Some(tabs.n_pages() - 1));
                }
                tabs.set_current_page(Some(0));
            }
            if let Some(protocol) = protocol.upgrade() {
                protocol.set_text(if enforced_offline {
                    "Hors ligne · réseau bloqué"
                } else {
                    "SSH  │  SFTP  │  Vérification des empreintes"
                });
            }
            if let Some(app) = application.upgrade() {
                for w in app.windows() {
                    if w.is::<gtk::ApplicationWindow>() {
                        w.set_title(Some("RustRemote"));
                    }
                }
            }
        });
    });
    app.add_action(&exit_action);
    let window = ApplicationWindow::builder()
        .application(app)
        .title(if demo {
            "RustRemote — Démonstration"
        } else {
            "RustRemote"
        })
        .default_width(1400)
        .default_height(820)
        .child(&root)
        .build();
    window.set_titlebar(Some(&header));
    display_actions(
        app,
        &window,
        &tabs,
        &profiles,
        &[
            ("view-sessions", sessions.clone().upcast()),
            ("view-sftp", sftp.clone().upcast()),
            ("view-toolbar", toolbar_scroll.clone().upcast()),
            ("view-status", statusbar.clone().upcast()),
        ],
    );
    window.present();
    gtk::glib::idle_add_local_once(move || {
        profiles.initialize(demo);
    });
}
