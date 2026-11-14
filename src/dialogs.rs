use crate::profiles::{ProtocolOptions, SessionProfile, SessionState, Workspace};
mod automation;
mod connections;
mod credentials;
mod desktop;
mod diagnostics;
mod discovery;
mod exchange;
mod rdp;
mod settings;
mod tree;
use crate::store::{self, Store};
use age::secrecy::SecretString;
use gtk::prelude::*;
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};

pub(crate) fn io<T: Send + 'static>(
    job: impl FnOnce() -> Result<T, String> + Send + 'static,
    done: impl FnOnce(Result<T, String>) + 'static,
) {
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let _ = tx.send(job());
    });
    let mut done = Some(done);
    gtk::glib::timeout_add_local(std::time::Duration::from_millis(50), move || {
        match rx.try_recv() {
            Ok(result) => {
                done.take().expect("callback disponible")(result);
                gtk::glib::ControlFlow::Break
            }
            Err(std::sync::mpsc::TryRecvError::Empty) => gtk::glib::ControlFlow::Continue,
            Err(_) => {
                done.take().expect("callback disponible")(Err("Opération interrompue".into()));
                gtk::glib::ControlFlow::Break
            }
        }
    });
}

pub(crate) fn defer_unparent(widget: &impl IsA<gtk::Widget>) {
    let widget = widget.as_ref().clone();
    gtk::glib::idle_add_local_once(move || {
        if widget.parent().is_some() {
            widget.unparent();
        }
    });
}

pub(crate) fn clear_window_later(window: &gtk::Window) {
    let window = window.clone();
    gtk::glib::idle_add_local_once(move || {
        gtk::prelude::GtkWindowExt::set_focus(&window, None::<&gtk::Widget>);
        window.set_child(None::<&gtk::Widget>);
    });
}

#[derive(Clone)]
pub struct Controller(Rc<Inner>);
struct Inner {
    app: gtk::glib::WeakRef<gtk::Application>,
    store: RefCell<Option<Store>>,
    profiles: RefCell<Vec<SessionProfile>>,
    folders: RefCell<Vec<String>>,
    profile_buttons: RefCell<Vec<(String, gtk::Button)>>,
    query: RefCell<String>,
    busy: Cell<bool>,
    offline: Cell<bool>,
    enforced_offline: bool,
    preferences: RefCell<crate::preferences::Preferences>,
    session_lock: RefCell<Option<std::fs::File>>,
    demo_root: RefCell<Option<tempfile::TempDir>>,
    rows: gtk::Box,
    status: gtk::Label,
    state: RefCell<SessionState>,
    simulations: RefCell<std::collections::HashMap<String, gtk::glib::WeakRef<gtk::Window>>>,
    tabs: gtk::glib::WeakRef<gtk::Notebook>,
    passwords:
        RefCell<std::collections::HashMap<String, std::sync::Arc<crate::credentials::Secret>>>,
    external: RefCell<std::collections::HashMap<String, crate::external::Client>>,
    terminals: RefCell<std::collections::HashMap<String, crate::terminal::Session>>,
}

impl Controller {
    pub fn new(
        app: &gtk::Application,
        status: &gtk::Label,
        offline: bool,
        tabs: &gtk::Notebook,
        enforced_offline: bool,
    ) -> Self {
        let controller = Self(Rc::new(Inner {
            app: app.downgrade(),
            store: RefCell::new(None),
            profiles: RefCell::new(Vec::new()),
            folders: RefCell::new(Vec::new()),
            profile_buttons: RefCell::new(Vec::new()),
            query: RefCell::new(String::new()),
            busy: Cell::new(false),
            offline: Cell::new(offline),
            enforced_offline,
            preferences: RefCell::new(crate::preferences::Preferences::default()),
            session_lock: RefCell::new(None),
            demo_root: RefCell::new(None),
            rows: gtk::Box::new(gtk::Orientation::Vertical, 3),
            status: status.clone(),
            state: RefCell::new(SessionState::Disconnected),
            simulations: RefCell::new(std::collections::HashMap::new()),
            tabs: tabs.downgrade(),
            passwords: RefCell::new(std::collections::HashMap::new()),
            external: RefCell::new(std::collections::HashMap::new()),
            terminals: RefCell::new(std::collections::HashMap::new()),
        }));
        let weak = Rc::downgrade(&controller.0);
        app.connect_shutdown(move |_| {
            if let Some(inner) = weak.upgrade() {
                for terminal in inner.terminals.borrow().values() {
                    terminal.disconnect();
                }
                for client in inner.external.borrow().values() {
                    client.disconnect();
                }
                inner.external.borrow_mut().clear();
                inner.passwords.borrow_mut().clear();
            }
        });
        controller
    }
    pub fn start_demo(&self) {
        let result = (|| -> Result<(), String> {
            let root = tempfile::tempdir()
                .map_err(|_| "Dossier temporaire de démonstration impossible")?;
            let mut data =
                Workspace::decode(include_bytes!("../assets/simulations/serveur-ia.json"))?;
            data.folders.push("Windows".into());
            data.profiles.push(SessionProfile {
                name: "Windows — simulation".into(),
                host: "windows.example.invalid".into(),
                port: 3389,
                username: "simulation".into(),
                credential_ref: None,
                favorite: false,
                folder: "Windows".into(),
                platform: crate::profiles::Platform::Windows,
                simulation: true,
                options: ProtocolOptions::Rdp {
                    domain: String::new(),
                    width: 1280,
                    height: 720,
                    fullscreen: false,
                    clipboard: false,
                    clipboard_files: false,
                    settings: crate::profiles::RdpSettings::default(),
                },
            });
            let store = Store::new(root.path().join("profiles.json"), None);
            let lease = store::lock_directory(root.path())?;
            store.save(&data, true)?;
            self.0.folders.replace(data.folders);
            self.0.profiles.replace(data.profiles);
            self.0.store.replace(Some(store));
            self.0.session_lock.replace(Some(lease));
            self.0.demo_root.replace(Some(root));
            self.refresh();
            Ok(())
        })();
        if let Err(e) = result {
            self.information("Démonstration indisponible", &e);
        }
        if let Some(app) = self.0.app.upgrade() {
            let inner = Rc::downgrade(&self.0);
            app.connect_shutdown(move |_| {
                if let Some(inner) = inner.upgrade() {
                    inner.store.replace(None);
                    inner.session_lock.replace(None);
                    inner.demo_root.replace(None);
                }
            });
        }
    }
    pub fn rows(&self) -> gtk::Box {
        self.0.rows.clone()
    }
    fn window(&self, title: &str, width: i32) -> gtk::Window {
        let app = self.0.app.upgrade().expect("Application active");
        let window = gtk::Window::builder()
            .application(&app)
            .title(title)
            .default_width(width)
            .modal(true)
            .destroy_with_parent(true)
            .build();
        if let Some(parent) = app
            .windows()
            .iter()
            .find(|w| w.is::<gtk::ApplicationWindow>())
        {
            window.set_transient_for(Some(parent));
        }
        window.connect_close_request(|w| {
            gtk::prelude::GtkWindowExt::set_focus(w, None::<&gtk::Widget>);
            gtk::glib::Propagation::Proceed
        });
        window.connect_hide(crate::dialogs::clear_window_later);
        window
    }
    pub fn information(&self, title: &str, text: &str) {
        let window = self.window(title, 480);
        let content = gtk::Box::new(gtk::Orientation::Vertical, 14);
        content.add_css_class("dialog-content");
        let label = gtk::Label::new(Some(text));
        label.set_wrap(true);
        label.set_xalign(0.0);
        content.append(&label);
        let close = gtk::Button::with_label("Fermer");
        let w = window.clone();
        close.connect_clicked(move |_| w.close());
        content.append(&close);
        window.set_child(Some(&content));
        crate::i18n::present(&window);
    }
    pub fn startup(&self) {
        if self.0.busy.get() {
            self.information(
                "Opération en cours",
                "Attendez la fin du chargement ou de l’enregistrement.",
            );
            return;
        }
        if self.0.store.borrow().is_some() {
            return;
        }
        let directory = self
            .0
            .demo_root
            .borrow()
            .as_ref()
            .map(|d| Ok(d.path().to_path_buf()))
            .unwrap_or_else(Store::directory);
        let dir = match directory {
            Ok(v) => v,
            Err(e) => {
                self.information("Profils indisponibles", &e);
                return;
            }
        };
        let existing = match store::existing(&dir) {
            Ok(v) => v,
            Err(e) => {
                self.information("Profils indisponibles", &e);
                return;
            }
        };
        let window = self.window(
            if existing.is_some() {
                "Ouvrir les profils"
            } else {
                "Protection des profils"
            },
            490,
        );
        let content = gtk::Box::new(gtk::Orientation::Vertical, 14);
        content.add_css_class("dialog-content");
        let hint = gtk::Label::new(Some(
            "Les profils contiennent les hôtes, utilisateurs et options, jamais les mots de passe ni les clés privées. Le chiffrement protège ces métadonnées sur disque. Sans la phrase secrète, aucune récupération n’est possible.",
        ));
        hint.set_wrap(true);
        hint.set_xalign(0.0);
        content.append(&hint);
        let encrypted = gtk::ToggleButton::with_label("Chiffrer les profils (recommandé)");
        encrypted.set_active(existing.unwrap_or(true));
        encrypted.set_sensitive(existing.is_none());
        content.append(&encrypted);
        let password = gtk::PasswordEntry::new();
        password.set_placeholder_text(Some("Phrase secrète (12 caractères minimum)"));
        password.set_show_peek_icon(true);
        password.update_property(&[gtk::accessible::Property::Label(&crate::i18n::text(
            "Phrase secrète des profils",
        ))]);
        let confirmation = gtk::PasswordEntry::new();
        confirmation.set_placeholder_text(Some("Confirmer la phrase secrète"));
        confirmation.update_property(&[gtk::accessible::Property::Label(&crate::i18n::text(
            "Confirmation de la phrase secrète",
        ))]);
        content.append(&password);
        content.append(&confirmation);
        password.set_visible(encrypted.is_active());
        confirmation.set_visible(encrypted.is_active() && existing.is_none());
        let p = password.clone();
        let c = confirmation.clone();
        encrypted.connect_toggled(move |check| {
            p.set_visible(check.is_active());
            c.set_visible(check.is_active() && existing.is_none());
        });
        let error = gtk::Label::new(None);
        error.set_wrap(true);
        error.add_css_class("error");
        content.append(&error);
        let submit = gtk::Button::with_label(if existing.is_some() {
            "Déverrouiller / ouvrir"
        } else {
            "Créer le magasin de profils"
        });
        submit.add_css_class("suggested-action");
        content.append(&submit);
        let controller = self.clone();
        let w = window.clone();
        submit.connect_clicked(move |button| {
            let encrypt = encrypted.is_active();
            if encrypt
                && (password.text().is_empty()
                    || (existing.is_none()
                        && (password.text().chars().count() < 12
                            || password.text() != confirmation.text())))
            {
                error.set_text("Utilisez au moins 12 caractères et une confirmation identique.");
                return;
            }
            let secret = if encrypt {
                Some(SecretString::from(password.text().to_string()))
            } else {
                None
            };
            password.set_text("");
            confirmation.set_text("");
            let store = Store::new(
                dir.join(if encrypt {
                    "profiles.age"
                } else {
                    "profiles.json"
                }),
                secret,
            );
            button.set_sensitive(false);
            w.set_deletable(false);
            error.set_text("Traitement en cours…");
            if controller.0.busy.replace(true) {
                button.set_sensitive(true);
                w.set_deletable(true);
                error.set_text("Une autre opération est en cours.");
                return;
            }
            let worker_store = store.clone();
            let controller = controller.clone();
            let w = w.clone();
            let button = button.clone();
            let error = error.clone();
            io(
                move || {
                    let dir = worker_store
                        .path
                        .parent()
                        .ok_or("Dossier de profils invalide")?;
                    let lease = store::lock_directory(dir)?;
                    if existing.is_none() {
                        worker_store.save(&Workspace::default(), true)?;
                    }
                    Ok((worker_store.load()?, lease))
                },
                move |result| {
                    controller.0.busy.set(false);
                    button.set_sensitive(true);
                    w.set_deletable(true);
                    match result {
                        Ok((data, lease)) => {
                            controller.0.session_lock.replace(Some(lease));
                            controller.0.profiles.replace(data.profiles);
                            controller.0.folders.replace(data.folders);
                            controller.0.store.replace(Some(store));
                            controller.refresh();
                            w.close();
                        }
                        Err(e) => error.set_text(&e),
                    }
                },
            );
        });
        window.set_child(Some(&content));
        crate::i18n::present(&window);
    }
    pub fn filter(&self, query: &str) {
        self.0.query.replace(query.to_string());
        if !query.is_empty() {
            self.expand_folders(true);
        }
        for (search, button) in self.0.profile_buttons.borrow().iter() {
            button.set_visible(search.contains(query));
        }
    }
    fn snapshot(&self) -> Workspace {
        Workspace {
            version: 1,
            folders: self.0.folders.borrow().clone(),
            profiles: self.0.profiles.borrow().clone(),
        }
    }
    fn refresh(&self) {
        self.0.profile_buttons.borrow_mut().clear();
        while let Some(child) = self.0.rows.first_child() {
            self.0.rows.remove(&child);
        }
        let data = self.snapshot();
        if data.profiles.is_empty() && data.folders.is_empty() {
            self.0
                .rows
                .append(&gtk::Label::new(Some("Aucun profil enregistré")));
        }
        let root_drop = gtk::Box::new(gtk::Orientation::Horizontal, 6);
        root_drop.append(&crate::icons::image("folder"));
        root_drop.append(&gtk::Label::new(Some(&crate::i18n::text(
            "Racine / sans dossier",
        ))));
        root_drop.add_css_class("session-drop-root");
        root_drop.set_tooltip_text(Some(&crate::i18n::text(
            "Déposez ici pour déplacer à la racine",
        )));
        self.drop_folder(&root_drop, "");
        self.0.rows.append(&root_drop);
        let mut groups = std::collections::HashMap::new();
        let mut sorted_folders = data.folders.clone();
        sorted_folders.sort();
        for folder in &sorted_folders {
            let expander = gtk::Expander::new(None);
            let title = gtk::Box::new(gtk::Orientation::Horizontal, 7);
            title.append(&crate::icons::image("folder"));
            title.append(&gtk::Label::new(Some(
                folder.rsplit('/').next().unwrap_or(folder),
            )));
            expander.set_tooltip_text(Some(folder));
            title.add_css_class("user-content");
            self.drag_item(&title, format!("folder:{folder}"));
            self.drop_folder(&expander, folder);
            expander.set_label_widget(Some(&title));
            // Expander labels are non-targetable by default; the header owns DND.
            title.set_can_target(true);
            let gesture = gtk::GestureClick::new();
            gesture.set_button(3);
            let c = self.clone();
            let folder_name = folder.clone();
            let anchor = expander.downgrade();
            let header = title.downgrade();
            gesture.set_propagation_phase(gtk::PropagationPhase::Capture);
            gesture.connect_pressed(move |gesture, _, x, y| {
                if let (Some(anchor), Some(header)) = (anchor.upgrade(), header.upgrade())
                    && let Some(bounds) = header.compute_bounds(&anchor)
                    && y >= f64::from(bounds.y())
                    && y <= f64::from(bounds.y() + bounds.height())
                {
                    gesture.set_state(gtk::EventSequenceState::Claimed);
                    c.folder_menu(&folder_name, &anchor, x, y);
                }
            });
            expander.add_controller(gesture);
            expander.set_expanded(true);
            expander.add_css_class("session-group");
            expander.add_css_class("user-content");
            let children = gtk::Box::new(gtk::Orientation::Vertical, 3);
            expander.set_child(Some(&children));
            if let Some((parent, _)) = folder.rsplit_once('/') {
                if let Some(group) = groups.get(parent) {
                    let group: &gtk::Box = group;
                    group.append(&expander);
                } else {
                    self.0.rows.append(&expander);
                }
            } else {
                self.0.rows.append(&expander);
            }
            groups.insert(folder.clone(), children);
        }
        for (index, profile) in data.profiles.iter().enumerate() {
            let button = gtk::Button::new();
            let caption = format!(
                "{} {}",
                if profile.favorite { "★" } else { "•" },
                profile.name
            );
            let content = gtk::Box::new(gtk::Orientation::Horizontal, 8);
            content.append(&crate::icons::image(match profile.kind() {
                crate::profiles::SessionKind::Serial => "switch",
                crate::profiles::SessionKind::Vnc
                    if profile.platform == crate::profiles::Platform::MacOs =>
                {
                    "macos"
                }
                crate::profiles::SessionKind::Vnc => "vnc",
                _ => match profile.platform {
                    crate::profiles::Platform::Linux => "linux",
                    crate::profiles::Platform::Windows => "windows",
                    crate::profiles::Platform::MacOs => "macos",
                    _ if profile.kind() == crate::profiles::SessionKind::Rdp => "windows",
                    _ => "ssh",
                },
            }));
            let label = gtk::Label::new(Some(&caption));
            label.set_xalign(0.0);
            label.set_ellipsize(gtk::pango::EllipsizeMode::End);
            label.set_max_width_chars(25);
            content.append(&label);
            content.add_css_class("user-content");
            button.set_child(Some(&content));
            button.update_property(&[gtk::accessible::Property::Label(&caption)]);
            button.set_tooltip_text(Some(&profile.name));
            button.add_css_class("session-row");
            button.add_css_class("user-content");
            let controller = self.clone();
            button.connect_clicked(move |_| controller.editor(Some(index), false));
            let context = gtk::GestureClick::new();
            context.set_button(3);
            let controller = self.clone();
            let anchor = button.downgrade();
            context.connect_pressed(move |_, _, x, y| {
                if let Some(anchor) = anchor.upgrade() {
                    controller.context_menu(index, &anchor, x, y);
                }
            });
            button.add_controller(context);
            self.drag_item(&button, format!("profile:{}", profile.name));
            let row = gtk::Box::new(gtk::Orientation::Horizontal, 2);
            button.set_hexpand(true);
            row.append(&button);
            let more = gtk::Button::from_icon_name("view-more-symbolic");
            more.set_tooltip_text(Some(&format!("Actions {}", profile.name)));
            more.update_property(&[gtk::accessible::Property::Label(&format!(
                "Actions {}",
                profile.name
            ))]);
            button
                .bind_property("visible", &more, "visible")
                .sync_create()
                .build();
            let controller = self.clone();
            more.connect_clicked(move |anchor| controller.context_menu(index, anchor, 0.0, 0.0));
            row.append(&more);
            if let Some(group) = groups.get(&profile.folder) {
                group.append(&row);
            } else {
                self.0.rows.append(&row);
            }
            self.0.profile_buttons.borrow_mut().push((
                format!("{} {} {}", profile.name, profile.host, profile.folder).to_lowercase(),
                button,
            ));
        }
        let query = self.0.query.borrow().clone();
        self.filter(&query);
        crate::i18n::translate(self.0.rows.upcast_ref());
        self.0.status.set_text(
            if self.0.store.borrow().as_ref().is_some_and(Store::encrypted) {
                "Profils chiffrés · déverrouillés"
            } else {
                "Profils non chiffrés"
            },
        );
    }
    fn persist(
        &self,
        window: &gtk::Window,
        button: &gtk::Button,
        error: &gtk::Label,
        data: Workspace,
    ) {
        self.persist_with_credentials(window, button, error, data, None);
    }
    fn persist_with_credentials(
        &self,
        window: &gtk::Window,
        button: &gtk::Button,
        error: &gtk::Label,
        data: Workspace,
        credential: Option<credentials::Change>,
    ) {
        if let Err(e) = data.validate() {
            error.set_text(&e);
            return;
        }
        let Some(store) = self.0.store.borrow().clone() else {
            error.set_text("Magasin verrouillé");
            return;
        };
        if self.0.busy.replace(true) {
            error.set_text("Un enregistrement est déjà en cours. Réessayez ensuite.");
            return;
        }
        button.set_sensitive(false);
        if let Some(content) = window.child() {
            content.set_sensitive(false);
        }
        window.set_deletable(false);
        error.set_text("Enregistrement…");
        let controller = self.clone();
        let w = window.clone();
        let b = button.clone();
        let e = error.clone();
        let cache = credential
            .as_ref()
            .map(|c| (c.old_cache.clone(), c.new_cache.clone(), c.password.clone()));
        io(
            move || {
                let mut created = None;
                if let Some(c) = &credential
                    && c.remember
                    && c.after != c.before
                    && let (Some(reference), Some(password)) = (&c.after, &c.password)
                {
                    crate::credentials::write(reference, password)?;
                    created = Some(reference.clone());
                }
                if let Err(message) = store.save(&data, false) {
                    if let Some(reference) = created {
                        let _ = crate::credentials::remove(&reference);
                    }
                    return Err(message);
                }
                let warning = credential
                    .as_ref()
                    .and_then(|c| c.before.as_ref().filter(|r| Some(*r) != c.after.as_ref()))
                    .and_then(|reference| crate::credentials::remove(reference).err());
                Ok((data, warning))
            },
            move |result| {
                controller.0.busy.set(false);
                if let Some(content) = w.child() {
                    content.set_sensitive(true);
                }
                b.set_sensitive(true);
                w.set_deletable(true);
                match result {
                    Ok((data, warning)) => {
                        if let Some((old, new, password)) = cache {
                            let mut cache = controller.0.passwords.borrow_mut();
                            if let Some(old) = old {
                                cache.remove(&old);
                            }
                            if let Some(password) = password {
                                cache.insert(new, password);
                            }
                        }
                        controller.0.profiles.replace(data.profiles);
                        controller.0.folders.replace(data.folders);
                        controller.refresh();
                        w.close();
                        if let Some(message) = warning {
                            controller.information("Nettoyage du coffre", &message);
                        }
                    }
                    Err(message) => e.set_text(&message),
                }
            },
        );
    }
    pub fn manager(&self) {
        if self.0.store.borrow().is_none() {
            self.startup();
            return;
        }
        let window = self.window("Gestionnaire de profils", 520);
        let content = gtk::Box::new(gtk::Orientation::Vertical, 10);
        content.add_css_class("dialog-content");
        let new = gtk::Button::with_label("Nouvelle connexion SSH / RDP / VNC / série");
        let controller = self.clone();
        let w = window.clone();
        new.connect_clicked(move |_| {
            w.close();
            controller.editor(None, false);
        });
        content.append(&new);
        for (index, profile) in self.0.profiles.borrow().iter().enumerate() {
            let target = match &profile.options {
                ProtocolOptions::Serial { device, baud, .. } => format!("{device} · {baud} bauds"),
                _ => format!("{}:{}", profile.host, profile.port),
            };
            let button = gtk::Button::with_label(&format!(
                "{} ({:?}) — {}",
                profile.name,
                profile.kind(),
                target
            ));
            let controller = self.clone();
            let w = window.clone();
            button.connect_clicked(move |_| {
                w.close();
                controller.editor(Some(index), false);
            });
            content.append(&button);
        }
        let scroll = gtk::ScrolledWindow::builder()
            .child(&content)
            .min_content_height(240)
            .max_content_height(500)
            .build();
        window.set_child(Some(&scroll));
        crate::i18n::present(&window);
    }
    pub fn lock(&self) {
        if self.0.busy.get() {
            self.information(
                "Enregistrement en cours",
                "Attendez la fin de l’opération avant de verrouiller les profils.",
            );
            return;
        }
        self.0.passwords.borrow_mut().clear();
        self.0.store.replace(None);
        self.0.session_lock.replace(None);
        self.0.profiles.borrow_mut().clear();
        self.0.folders.borrow_mut().clear();
        self.0.profile_buttons.borrow_mut().clear();
        while let Some(child) = self.0.rows.first_child() {
            self.0.rows.remove(&child);
        }
        self.0
            .status
            .set_text("Profils verrouillés · sessions ouvertes conservées");
        self.startup();
    }
    fn security_details(&self) {
        let mode = self
            .0
            .store
            .borrow()
            .as_ref()
            .map(|s| {
                if s.encrypted() {
                    "Chiffré avec age / scrypt (déverrouillé pour cette session)"
                } else {
                    "Non chiffré, permissions privées (0600)"
                }
            })
            .unwrap_or("Non ouvert");
        self.information("Informations de sécurité", &format!("Stockage des profils : {mode}\n\nLes mots de passe peuvent être liés aux profils dans le coffre système. Ils ne sont jamais inscrits dans le JSON ou les exports. La clé SSH reste dans son fichier d’origine.\nSSH : empreinte hôte à vérifier, transfert d’agent désactivé.\nRDP : NLA et certificat valide requis ; presse-papiers désactivé par défaut.\n\nSSH et série : terminaux VTE intégrés, arrêt à la fermeture de leur onglet. SFTP natif : clé, agent ou mot de passe, empreinte known_hosts vérifiée ; connexion directe sans appliquer les passerelles OpenSSH. RDP/VNC : fenêtres externes, à fermer dans leurs clients.\n\nLe changement du mode de chiffrement d’un magasin existant n’est pas encore disponible."));
    }
    pub fn editor(&self, index: Option<usize>, rdp_default: bool) {
        self.editor_protocol(index, u32::from(rdp_default));
    }
    pub fn editor_protocol(&self, index: Option<usize>, default_protocol: u32) {
        self.editor_details(index, default_protocol, None);
    }
    fn editor_details(
        &self,
        index: Option<usize>,
        default_protocol: u32,
        default_folder: Option<&str>,
    ) {
        self.editor_prefilled(index, default_protocol, default_folder, None);
    }
    fn editor_prefilled(
        &self,
        index: Option<usize>,
        default_protocol: u32,
        default_folder: Option<&str>,
        seed: Option<SessionProfile>,
    ) {
        if self.0.store.borrow().is_none() {
            self.startup();
            return;
        }
        let original = index
            .and_then(|i| self.0.profiles.borrow().get(i).cloned())
            .or(seed);
        let window = self.window("Configuration de connexion", 580);
        window.set_default_height(640);
        let body = gtk::Box::new(gtk::Orientation::Vertical, 10);
        body.add_css_class("dialog-content");
        let grid = gtk::Grid::new();
        grid.set_row_spacing(10);
        grid.set_column_spacing(16);
        fn entry(grid: &gtk::Grid, row: i32, title: &str, value: &str) -> gtk::Entry {
            let label = gtk::Label::new(Some(title));
            label.set_xalign(0.0);
            label.set_wrap(true);
            label.set_max_width_chars(24);
            grid.attach(&label, 0, row, 1, 1);
            let entry = gtk::Entry::new();
            entry.set_width_chars(12);
            entry.set_text(value);
            entry.set_hexpand(true);
            entry.update_property(&[gtk::accessible::Property::Label(&crate::i18n::text(title))]);
            grid.attach(&entry, 1, row, 1, 1);
            entry
        }
        let name = entry(
            &grid,
            0,
            "Nom du profil",
            original.as_ref().map_or("", |p| &p.name),
        );
        let host = entry(
            &grid,
            1,
            "Hôte / adresse IP",
            original.as_ref().map_or("", |p| &p.host),
        );
        let user = entry(
            &grid,
            2,
            "Utilisateur",
            original.as_ref().map_or("", |p| &p.username),
        );
        let domain = entry(&grid, 4, "Domaine AD / realm", "");
        let domain_label = grid.child_at(0, 4).expect("libellé de domaine");
        let macos_preset = default_protocol == 4;
        let default_protocol = if macos_preset { 2 } else { default_protocol };
        let selected_protocol = original
            .as_ref()
            .map_or(default_protocol, |p| match p.options {
                ProtocolOptions::Ssh { .. } => 0,
                ProtocolOptions::Rdp { .. } => 1,
                ProtocolOptions::Vnc { .. } => 2,
                ProtocolOptions::Serial { .. } => 3,
            });
        let rdp = selected_protocol == 1;
        let port = entry(
            &grid,
            3,
            "Port",
            &original
                .as_ref()
                .map_or(
                    if rdp {
                        3389
                    } else if selected_protocol == 2 {
                        5900
                    } else {
                        22
                    },
                    |p| p.port,
                )
                .to_string(),
        );
        body.append(&grid);
        let platform =
            crate::i18n::dropdown(&["Système non précisé", "Linux", "Windows", "macOS / Apple"]);
        platform.set_selected(original.as_ref().map_or(
            if macos_preset {
                3
            } else if selected_protocol == 0 {
                1
            } else if selected_protocol == 1 {
                2
            } else {
                0
            },
            |p| match p.platform {
                crate::profiles::Platform::Unknown => 0,
                crate::profiles::Platform::Linux => 1,
                crate::profiles::Platform::Windows => 2,
                crate::profiles::Platform::MacOs => 3,
            },
        ));
        platform.update_property(&[gtk::accessible::Property::Label(&crate::i18n::text(
            "Type de machine",
        ))]);
        body.append(&platform);
        let protocol = crate::i18n::dropdown(&["SSH", "RDP", "VNC", "Série / switch"]);
        protocol.set_selected(selected_protocol);
        protocol.update_property(&[gtk::accessible::Property::Label(&crate::i18n::text(
            "Protocole",
        ))]);
        body.append(&protocol);
        let favorite = gtk::ToggleButton::with_label("Favori");
        favorite.set_active(original.as_ref().is_some_and(|p| p.favorite));
        body.append(&favorite);
        let folder_names: Vec<String> = std::iter::once("Sans dossier".to_string())
            .chain(self.0.folders.borrow().iter().cloned())
            .collect();
        body.append(&gtk::Label::new(Some("Dossier de sessions")));
        let folder = gtk::DropDown::from_strings(
            &folder_names.iter().map(String::as_str).collect::<Vec<_>>(),
        );
        folder.set_selected(
            original
                .as_ref()
                .and_then(|p| folder_names.iter().position(|f| f == &p.folder))
                .or_else(|| {
                    default_folder.and_then(|f| folder_names.iter().position(|name| name == f))
                })
                .or_else(|| {
                    if original.is_none() {
                        folder_names.iter().position(|f| {
                            f == if selected_protocol == 1 {
                                "Windows"
                            } else if selected_protocol == 0 {
                                "Linux"
                            } else {
                                ""
                            }
                        })
                    } else {
                        None
                    }
                })
                .unwrap_or(0) as u32,
        );
        folder.update_property(&[gtk::accessible::Property::Label(&crate::i18n::text(
            "Dossier de sessions",
        ))]);
        body.append(&folder);
        let new_folder = gtk::Entry::new();
        new_folder.set_placeholder_text(Some("Ou créer un nouveau dossier pour cette connexion"));
        new_folder.update_property(&[gtk::accessible::Property::Label(&crate::i18n::text(
            "Nouveau dossier du profil",
        ))]);
        body.append(&new_folder);
        let authentication = gtk::Box::new(gtk::Orientation::Vertical, 8);
        let auth_title = gtk::Label::new(Some("Identifiants de connexion"));
        auth_title.add_css_class("heading");
        authentication.append(&auth_title);
        let password = gtk::PasswordEntry::builder()
            .show_peek_icon(true)
            .hexpand(true)
            .build();
        password.update_property(&[gtk::accessible::Property::Label(&crate::i18n::text(
            "Mot de passe de la connexion",
        ))]);
        let password_label = gtk::Label::new(Some(
            if original
                .as_ref()
                .is_some_and(|p| p.credential_ref.is_some())
            {
                "Mot de passe enregistré : laissez vide pour le conserver"
            } else {
                "Mot de passe (facultatif, jamais exporté)"
            },
        ));
        password_label.set_wrap(true);
        authentication.append(&password_label);
        authentication.append(&password);
        let remember =
            gtk::ToggleButton::with_label("Enregistrer le mot de passe avec cette connexion");
        remember.set_active(
            original
                .as_ref()
                .is_some_and(|p| p.credential_ref.is_some()),
        );
        remember.set_sensitive(self.0.demo_root.borrow().is_none());
        remember.set_tooltip_text(Some("Protégé par le coffre système ; sinon conservé uniquement pendant cette ouverture de RustRemote"));
        authentication.append(&remember);
        authentication.set_visible(selected_protocol <= 1);
        body.append(&authentication);
        let ssh = gtk::Box::new(gtk::Orientation::Vertical, 8);
        let agent =
            gtk::ToggleButton::with_label("Utiliser SSH Agent (sans transfert vers le serveur)");
        agent.set_active(true);
        ssh.append(&agent);
        let key = gtk::Entry::new();
        key.set_placeholder_text(Some("Chemin absolu de la clé privée (facultatif)"));
        key.update_property(&[gtk::accessible::Property::Label(&crate::i18n::text(
            "Clé SSH",
        ))]);
        let known_hosts = gtk::Entry::new();
        known_hosts.set_placeholder_text(Some(
            "Facultatif : known_hosts propre à ce profil (chemin absolu)",
        ));
        known_hosts.update_property(&[gtk::accessible::Property::Label(&crate::i18n::text(
            "Fichier known_hosts SSH",
        ))]);
        let config_file = gtk::Entry::new();
        config_file.set_placeholder_text(Some(
            "Facultatif : fichier de configuration OpenSSH (ex. /dev/null pour l’ignorer)",
        ));
        config_file.update_property(&[gtk::accessible::Property::Label(&crate::i18n::text(
            "Configuration OpenSSH",
        ))]);
        if let Some(SessionProfile {
            options:
                ProtocolOptions::Ssh {
                    identity_file,
                    agent: enabled,
                    known_hosts_file,
                    config_file: configuration,
                },
            ..
        }) = &original
        {
            agent.set_active(*enabled);
            if let Some(path) = known_hosts_file {
                known_hosts.set_text(path);
            }
            if let Some(path) = configuration {
                config_file.set_text(path);
            }
            if let Some(path) = identity_file {
                key.set_text(path);
            }
        }
        let keybar = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        key.set_hexpand(true);
        keybar.append(&key);
        let choose = gtk::Button::with_label("Parcourir…");
        keybar.append(&choose);
        ssh.append(&keybar);
        ssh.append(&known_hosts);
        ssh.append(&config_file);
        let w = window.clone();
        let field = key.clone();
        choose.connect_clicked(move |_| {
            let chooser = gtk::FileChooserNative::new(
                Some("Choisir une clé SSH existante"),
                Some(&w),
                gtk::FileChooserAction::Open,
                Some("Choisir"),
                Some("Annuler"),
            );
            let field = field.clone();
            chooser.connect_response(move |dialog, response| {
                if response == gtk::ResponseType::Accept
                    && let Some(path) = dialog.file().and_then(|f| f.path())
                {
                    field.set_text(&path.to_string_lossy());
                }
                dialog.destroy();
            });
            chooser.show();
        });
        ssh.append(&gtk::Label::new(Some(
            "Clé chiffrée : utilisez SSH Agent. Le mot de passe du compte se configure ci-dessus. AD/LDAP : identifiant user@domaine ou DOMAINE\\user selon le serveur.",
        )));
        body.append(&ssh);
        let rdp_form = rdp::RdpForm::new(original.as_ref(), &domain);
        if selected_protocol == 0
            && let Some(profile) = &original
            && profile.username.matches('@').count() == 1
            && let Some((login, realm)) = profile.username.rsplit_once('@')
        {
            user.set_text(login);
            domain.set_text(realm);
        }
        domain.set_visible(selected_protocol <= 1);
        domain_label.set_visible(selected_protocol <= 1);
        body.append(&rdp_form.widget);
        let extra = connections::ConnectionsForm::new(original.as_ref());
        body.append(&extra.vnc);
        body.append(&extra.serial);
        extra.vnc.set_visible(selected_protocol == 2);
        extra.serial.set_visible(selected_protocol == 3);
        ssh.set_visible(selected_protocol == 0);
        rdp_form.widget.set_visible(rdp);
        let ssh_copy = ssh.clone();
        let rdp_copy = rdp_form.widget.clone();
        let port_copy = port.clone();
        let vnc_copy = extra.vnc.clone();
        let serial_copy = extra.serial.clone();
        let common = [1, 2, 3]
            .into_iter()
            .flat_map(|row| {
                [
                    grid.child_at(0, row).unwrap(),
                    grid.child_at(1, row).unwrap(),
                ]
            })
            .collect::<Vec<_>>();
        for item in &common {
            item.set_visible(selected_protocol != 3);
        }
        let domain_copy = domain.clone();
        let domain_caption = domain_label.clone();
        let authentication_copy = authentication.clone();
        protocol.connect_selected_notify(move |drop| {
            authentication_copy.set_visible(drop.selected() <= 1);
            domain_copy.set_visible(drop.selected() <= 1);
            domain_caption.set_visible(drop.selected() <= 1);
            let rdp = drop.selected() == 1;
            ssh_copy.set_visible(drop.selected() == 0);
            rdp_copy.set_visible(rdp);
            vnc_copy.set_visible(drop.selected() == 2);
            serial_copy.set_visible(drop.selected() == 3);
            for item in &common {
                item.set_visible(drop.selected() != 3);
            }
            if matches!(port_copy.text().as_str(), "22" | "3389" | "5900") {
                port_copy.set_text(match drop.selected() {
                    1 => "3389",
                    2 => "5900",
                    _ => "22",
                });
            }
        });
        let simulation = gtk::ToggleButton::with_label("Simulation locale (aucun réseau)");
        simulation.set_active(original.as_ref().is_some_and(|p| p.simulation));
        body.append(&simulation);
        let error = gtk::Label::new(None);
        error.set_wrap(true);
        error.add_css_class("error");
        let footer_error = error.clone();
        let buttons = gtk::FlowBox::builder()
            .selection_mode(gtk::SelectionMode::None)
            .min_children_per_line(1)
            .max_children_per_line(3)
            .column_spacing(8)
            .row_spacing(6)
            .build();
        let save = gtk::Button::with_label("Enregistrer le profil");
        save.add_css_class("suggested-action");
        buttons.insert(&save, -1);
        let cancel = gtk::Button::with_label("Annuler");
        let w = window.clone();
        cancel.connect_clicked(move |_| {
            if w.is_deletable() {
                w.close();
            }
        });
        buttons.insert(&cancel, -1);
        if let Some(i) = index {
            let connect = gtk::Button::with_label("Se connecter…");
            let controller = self.clone();
            let w = window.clone();
            connect.connect_clicked(move |_| {
                if w.is_deletable() {
                    w.close();
                    controller.connect(i);
                }
            });
            buttons.insert(&connect, -1);
        }
        let controller = self.clone();
        let w = window.clone();
        let original_name = original.as_ref().map(|p| p.name.clone());
        let original_profile = original.clone();
        save.connect_clicked(move |button| {
            let mut options = if protocol.selected() == 1 {
                match rdp_form.options() {
                    Ok(options) => options,
                    Err(e) => {
                        error.set_text(&e);
                        return;
                    }
                }
            } else if protocol.selected() >= 2 {
                extra.options(protocol.selected())
            } else {
                ProtocolOptions::Ssh {
                    identity_file: if key.text().is_empty() {
                        None
                    } else {
                        Some(key.text().to_string())
                    },
                    agent: agent.is_active(),
                    known_hosts_file: (!known_hosts.text().trim().is_empty())
                        .then(|| known_hosts.text().trim().to_string()),
                    config_file: (!config_file.text().trim().is_empty())
                        .then(|| config_file.text().trim().to_string()),
                }
            };
            if platform.selected() == 3
                && let ProtocolOptions::Vnc {
                    tunnel: Some(t), ..
                } = &mut options
            {
                if t.host.is_empty() {
                    t.host = host.text().trim().into();
                }
                if t.username.is_empty() {
                    t.username = user.text().trim().into();
                }
            }
            let mut profile = SessionProfile {
                name: name.text().trim().into(),
                host: host.text().trim().into(),
                port: port.text().parse().unwrap_or(0),
                username: if protocol.selected() == 0 {
                    match crate::profiles::ssh_identity(&user.text(), &domain.text()) {
                        Ok(value) => value,
                        Err(message) => {
                            error.set_text(&message);
                            return;
                        }
                    }
                } else {
                    user.text().trim().into()
                },
                credential_ref: None,
                favorite: favorite.is_active(),
                platform: match platform.selected() {
                    1 => crate::profiles::Platform::Linux,
                    2 => crate::profiles::Platform::Windows,
                    3 => crate::profiles::Platform::MacOs,
                    _ => crate::profiles::Platform::Unknown,
                },
                folder: if !new_folder.text().trim().is_empty() {
                    new_folder.text().trim().to_string()
                } else if folder.selected() == 0 {
                    String::new()
                } else {
                    folder_names[folder.selected() as usize].clone()
                },
                simulation: simulation.is_active(),
                options,
            };
            if let Err(e) = profile.validate() {
                error.set_text(&e);
                return;
            }
            if let ProtocolOptions::Ssh {
                identity_file: Some(path),
                ..
            } = &profile.options
                && !std::path::Path::new(path).is_file()
            {
                error.set_text("Le fichier de clé SSH n’existe pas.");
                return;
            }
            let mut profiles = controller.0.profiles.borrow().clone();
            if profiles
                .iter()
                .enumerate()
                .any(|(i, p)| Some(i) != index && p.name == profile.name)
            {
                error.set_text("Ce nom de profil existe déjà.");
                return;
            }
            let change = match controller.credential_change(
                original_profile.as_ref(),
                &mut profile,
                password.text().as_str(),
                remember.is_active(),
            ) {
                Ok(change) => change,
                Err(message) => {
                    error.set_text(&message);
                    return;
                }
            };
            let profile_folder = profile.folder.clone();
            if let Some(i) = index {
                let Some(current) = profiles
                    .get_mut(i)
                    .filter(|p| Some(p.name.as_str()) == original_name.as_deref())
                else {
                    error.set_text(
                        "Ce profil a changé depuis l’ouverture. Rouvrez-le avant de modifier.",
                    );
                    return;
                };
                *current = profile;
            } else {
                profiles.push(profile);
            }
            let mut folders = controller.0.folders.borrow().clone();
            if !profile_folder.is_empty() && !folders.contains(&profile_folder) {
                let mut tree = Workspace {
                    folders,
                    profiles: Vec::new(),
                    ..Workspace::default()
                };
                if let Err(message) = tree.add_folder(&profile_folder) {
                    error.set_text(&message);
                    return;
                }
                folders = tree.folders;
            }
            let data = Workspace {
                profiles,
                folders,
                version: 1,
            };
            controller.persist_with_credentials(&w, button, &error, data, Some(change));
        });
        let scroll = gtk::ScrolledWindow::builder()
            .child(&body)
            .min_content_height(120)
            .hscrollbar_policy(gtk::PolicyType::Never)
            .vexpand(true)
            .build();
        let content = gtk::Box::new(gtk::Orientation::Vertical, 0);
        content.append(&scroll);
        let footer = gtk::Box::new(gtk::Orientation::Vertical, 6);
        footer.add_css_class("dialog-content");
        footer.append(&footer_error);
        footer.append(&buttons);
        content.append(&footer);
        window.set_child(Some(&content));
        crate::i18n::present(&window);
    }
    fn context_menu(&self, index: usize, anchor: &gtk::Button, x: f64, y: f64) {
        let Some(profile) = self.0.profiles.borrow().get(index).cloned() else {
            return;
        };
        let popover = gtk::Popover::new();
        popover.set_parent(anchor);
        popover.set_pointing_to(Some(&gtk::gdk::Rectangle::new(x as i32, y as i32, 1, 1)));
        let content = gtk::Box::new(gtk::Orientation::Vertical, 3);
        for action in ["Se connecter", "Modifier", "Déplacer…", "Déconnecter"] {
            let item = gtk::Button::with_label(action);
            item.add_css_class("menu-item");
            if action == "Déconnecter" {
                item.set_sensitive(
                    self.0
                        .simulations
                        .borrow()
                        .get(&profile.name)
                        .and_then(|w| w.upgrade())
                        .is_some()
                        || self
                            .0
                            .terminals
                            .borrow()
                            .get(&profile.name)
                            .is_some_and(|s| s.active())
                        || self
                            .0
                            .external
                            .borrow()
                            .get(&profile.name)
                            .is_some_and(|client| client.active()),
                );
                item.set_tooltip_text(Some(
                    "Déconnecter le terminal, la simulation ou le client RDP suivi par RustRemote.",
                ));
            }
            let controller = self.clone();
            let popup = popover.downgrade();
            let name = profile.name.clone();
            item.connect_clicked(move |_| {
                if let Some(p) = popup.upgrade() {
                    p.popdown();
                }
                match action {
                    "Se connecter" => controller.connect(index),
                    "Modifier" => controller.editor(Some(index), false),
                    "Déplacer…" => controller.move_dialog(&format!("profile:{name}"), None),
                    _ => {
                        if let Some(session) = controller.0.terminals.borrow().get(&name) {
                            session.disconnect();
                        }
                        if let Some(window) = controller
                            .0
                            .simulations
                            .borrow_mut()
                            .remove(&name)
                            .and_then(|w| w.upgrade())
                        {
                            window.close();
                        }
                        if let Some(client) = controller.0.external.borrow_mut().remove(&name) {
                            client.disconnect();
                        }
                        controller.0.status.set_text("Déconnexion demandée");
                    }
                }
            });
            content.append(&item);
        }
        popover.set_child(Some(&content));
        popover.connect_closed(crate::dialogs::defer_unparent);
        crate::i18n::translate(popover.upcast_ref());
        popover.popup();
    }
    pub fn simulation_lab(&self) {
        let data = Workspace::decode(include_bytes!("../assets/simulations/serveur-ia.json"));
        match data {
            Ok(data) => {
                if let Some(app) = self.0.app.upgrade() {
                    crate::simulation::show(&app, &data.profiles[0]);
                }
            }
            Err(e) => self.information("Simulation indisponible", &e),
        }
    }
    pub fn sftp_current(&self) {
        if self.0.offline.get() {
            self.simulation_lab();
            return;
        }
        if let Some(tabs) = self.0.tabs.upgrade()
            && let Some(session) = self
                .0
                .terminals
                .borrow()
                .values()
                .find(|s| s.can_sftp() && s.selected(&tabs))
        {
            session.open_sftp();
            return;
        }
        self.information("SFTP", "Ouvrez et sélectionnez d’abord un terminal SSH. Le navigateur SFTP utilise la cible enregistrée au début de cette session.");
    }
    pub fn zoom_current(&self, direction: i32) {
        if let Some(tabs) = self.0.tabs.upgrade()
            && let Some(session) = self
                .0
                .terminals
                .borrow()
                .values()
                .find(|s| s.selected(&tabs))
            && let Some(scale) = session.zoom(direction)
        {
            self.0
                .status
                .set_text(&format!("Zoom du terminal : {:.0} %", scale * 100.0));
        }
    }
    pub fn sync_view_actions(&self) {
        let Some(app) = self.0.app.upgrade() else {
            return;
        };
        let tabs = self.0.tabs.upgrade();
        let terminal = tabs
            .as_ref()
            .is_some_and(|t| self.0.terminals.borrow().values().any(|s| s.selected(t)));
        let closable = tabs.is_some_and(|t| t.current_page().is_some_and(|i| i > 0));
        for (name, enabled) in [
            ("zoom-in", terminal),
            ("zoom-out", terminal),
            ("zoom-reset", terminal),
            ("close-tab", closable),
        ] {
            if let Some(action) = app
                .lookup_action(name)
                .and_downcast::<gtk::gio::SimpleAction>()
            {
                action.set_enabled(enabled);
            }
        }
    }
    fn connect(&self, index: usize) {
        if self.0.busy.get() {
            self.information(
                "Opération en cours",
                "Attendez la fin de l’enregistrement avant de vous connecter.",
            );
            return;
        }
        let Some(profile) = self.0.profiles.borrow().get(index).cloned() else {
            return;
        };
        if profile.simulation {
            if let Some(window) = self
                .0
                .simulations
                .borrow()
                .get(&profile.name)
                .and_then(|w| w.upgrade())
            {
                crate::i18n::present(&window);
                return;
            }
            if let Some(app) = self.0.app.upgrade() {
                let window = crate::simulation::show(&app, &profile);
                self.0
                    .simulations
                    .borrow_mut()
                    .insert(profile.name.clone(), window.downgrade());
            }
            return;
        }
        if self.0.offline.get() {
            self.information("Connexion bloquée hors ligne","Le mode --offline interdit toute connexion réelle. Activez la simulation dans le profil pour ouvrir le laboratoire local.");
            return;
        }
        if let Some(session) = self.0.terminals.borrow().get(&profile.name)
            && session.active()
        {
            if let Some(tabs) = self.0.tabs.upgrade() {
                session.present(&tabs);
            }
            return;
        }
        let window = self.window("Confirmer la connexion", 480);
        let body = gtk::Box::new(gtk::Orientation::Vertical, 14);
        body.add_css_class("dialog-content");
        let target = if let ProtocolOptions::Serial { device, .. } = &profile.options {
            format!("Port série {device}")
        } else {
            format!(
                "{}:{} pour {}",
                profile.host, profile.port, profile.username
            )
        };
        let presentation = if matches!(
            profile.options,
            ProtocolOptions::Ssh { .. } | ProtocolOptions::Serial { .. }
        ) {
            "Le terminal s’ouvrira dans un onglet intégré."
        } else {
            "Le client s’ouvrira dans une fenêtre externe."
        };
        let effect = if matches!(profile.options, ProtocolOptions::Serial { .. }) {
            "Cette action ouvre le périphérique série local."
        } else {
            "Cette action contacte le serveur."
        };
        let text = gtk::Label::new(Some(&format!(
            "{} {target} ?\n\n{} {} {}",
            crate::i18n::text("Ouvrir une connexion vers"),
            crate::i18n::text(presentation),
            crate::i18n::text(effect),
            crate::i18n::text("Les changements non enregistrés ne seront pas utilisés.")
        )));
        text.set_wrap(true);
        body.append(&text);
        let start = gtk::Button::with_label("Autoriser et connecter");
        body.append(&start);
        let cancel = gtk::Button::with_label("Annuler la connexion");
        let cancel_window = window.clone();
        cancel.connect_clicked(move |_| cancel_window.close());
        body.append(&cancel);
        let controller = self.clone();
        let w = window.clone();
        let password = gtk::PasswordEntry::builder().show_peek_icon(true).build();
        password.update_property(&[gtk::accessible::Property::Label(&crate::i18n::text(
            "Mot de passe pour cette connexion",
        ))]);
        if matches!(
            profile.options,
            ProtocolOptions::Ssh { .. } | ProtocolOptions::Rdp { .. }
        ) {
            let note = gtk::Label::new(Some(
                "Mot de passe : laissez vide pour utiliser celui du profil ou l’authentification SSH par clé.",
            ));
            note.set_wrap(true);
            body.prepend(&password);
            body.prepend(&note);
        }
        let error = gtk::Label::new(None);
        error.set_wrap(true);
        error.add_css_class("error");
        body.append(&error);
        let button = start.clone();
        start.connect_clicked(move |_| {
            let entered=if password.text().is_empty(){None}else{
                if let Err(message)=crate::credentials::validate_password(password.text().as_str()){error.set_text(&message);return;}
                Some(std::sync::Arc::new(crate::credentials::Secret::new(password.text().to_string())))
            };
            button.set_sensitive(false);password.set_sensitive(false);
            let c=controller.clone();let profile=profile.clone();let w=w.clone();let b=button.clone();let e=error.clone();let p=password.clone();
            controller.with_password(profile.clone(),entered,move|result|{
                if !w.is_visible(){return;}
                b.set_sensitive(true);p.set_sensitive(true);
                let secret=match result {Ok(value)=>value,Err(message)=>{e.set_text(&message);return;}};
                if c.0.offline.get(){e.set_text("Les connexions réelles sont interdites hors ligne");return;}
                if matches!(profile.options,ProtocolOptions::Ssh{..}|ProtocolOptions::Serial{..}){
                    if let Some(tabs)=c.0.tabs.upgrade(){
                        match crate::terminal::open(&tabs,&profile,&c.0.status,&c.0.preferences.borrow(),secret){
                            Ok(session)=>{c.0.terminals.borrow_mut().retain(|_,s|s.active());c.0.terminals.borrow_mut().insert(profile.name.clone(),session.clone());w.close();session.present(&tabs);c.sync_view_actions();}
                            Err(message)=>e.set_text(&message),
                        }
                    }
                    return;
                }
                if matches!(profile.options,ProtocolOptions::Rdp{..}){
                    let Some(secret)=secret else{e.set_text("Saisissez le mot de passe RDP ici ou dans le profil");return;};
                    if c.0.external.borrow().get(&profile.name).is_some_and(|client|client.active()){e.set_text("Cette connexion RDP est déjà ouverte");return;}
                    b.set_sensitive(false);let name=profile.name.clone();let b=b.clone();let w=w.clone();let e=e.clone();let c=c.clone();
                    io(move||crate::external::Client::spawn(&profile,crate::credentials::Secret::new(secret.as_str().to_owned())),move|result|{
                        b.set_sensitive(true);
                        match result {Ok(client)=>{
                            c.0.external.borrow_mut().retain(|_,client|client.active());
                            c.0.external.borrow_mut().insert(name.clone(),client.clone());
                            c.0.status.set_text("Client RDP lancé · connexion en cours avec contrôle NLA et certificat");
                            let weak=std::rc::Rc::downgrade(&c.0);
                            gtk::glib::timeout_add_local(std::time::Duration::from_millis(400),move||{
                                let Some(inner)=weak.upgrade() else{return gtk::glib::ControlFlow::Break;};
                                if let Some(code)=client.exit_code(){
                                    inner.status.set_text(&if code==0{format!("{name} · client RDP fermé")}else{format!("{name} · client RDP arrêté (code {code}) · vérifiez l’authentification, le certificat et la disponibilité du serveur")});
                                    gtk::glib::ControlFlow::Break
                                }else{gtk::glib::ControlFlow::Continue}
                            });
                            w.close();
                        },Err(message)=>e.set_text(&message)}
                    });
                    return;
                }
                let controller=c;
            let (binary,args)=match profile.command() {Ok(v)=>v,Err(e)=>{controller.information("Connexion refusée",&e);return;}};
            if !std::path::Path::new(binary).is_file() { controller.information("Client manquant","Installez openssh-client (SSH), freerdp3-x11 (RDP), tigervnc-viewer (VNC) ou picocom (série). Aucun téléchargement automatique.");return; }
            let terminal=if std::path::Path::new("/usr/bin/ptyxis").is_file() {"/usr/bin/ptyxis"} else {"/usr/bin/xterm"};
            let mut command=std::process::Command::new(terminal); if terminal.ends_with("ptyxis") { command.arg("--standalone").arg("--"); } else { command.arg("-e"); } command.arg(binary).args(args);
            // Ne pas transmettre les bibliothèques privées AppImage aux clients système.
            command.env_remove("LD_LIBRARY_PATH").env_remove("LD_PRELOAD").env_remove("VNC_VIA_CMD").env_remove("GDK_PIXBUF_MODULE_FILE").env_remove("GIO_MODULE_DIR").env_remove("GTK_PATH");
            match profile.vnc_tunnel_command() {
                Ok(Some(tunnel)) => { command.env("VNC_VIA_CMD", tunnel); }
                Ok(None) => {}
                Err(e) => { controller.information("Tunnel refusé", &e); return; }
            }
            match command.spawn() {
                Ok(mut child)=> { controller.0.state.replace(SessionState::Launching); controller.0.status.set_text("Client externe lancé · état de connexion non suivi"); std::thread::spawn(move ||{let _=child.wait();}); w.close(); }
                Err(_)=>controller.information("Lancement impossible","Terminal système introuvable ou client impossible à démarrer."),
            }
            });
        });
        window.set_child(Some(&body));
        crate::i18n::present(&window);
    }
}
