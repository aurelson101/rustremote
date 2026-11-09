use super::*;
use crate::profiles::validate_folder;
use std::io::Read;
use std::os::unix::fs::OpenOptionsExt;

impl Controller {
    pub fn new_folder(&self) {
        self.new_subfolder("");
    }
    pub(super) fn new_subfolder(&self, parent: &str) {
        if self.0.store.borrow().is_none() {
            self.startup();
            return;
        }
        let window = self.window("Créer un dossier de sessions", 420);
        let body = gtk::Box::new(gtk::Orientation::Vertical, 12);
        body.add_css_class("dialog-content");
        let name = gtk::Entry::new();
        name.set_placeholder_text(Some("Linux"));
        name.set_text(&if parent.is_empty() {
            String::new()
        } else {
            format!("{parent}/")
        });
        name.update_property(&[gtk::accessible::Property::Label(&crate::i18n::text(
            "Nom du dossier",
        ))]);
        body.append(&name);
        let error = gtk::Label::new(None);
        error.set_wrap(true);
        error.add_css_class("error");
        body.append(&error);
        let submit = gtk::Button::with_label("Créer le dossier");
        body.append(&submit);
        let controller = self.clone();
        let w = window.clone();
        submit.connect_clicked(move |button| {
            let folder = name.text().to_string();
            if let Err(e) = validate_folder(&folder) {
                error.set_text(&e);
                return;
            }
            let mut data = controller.snapshot();
            if let Err(message) = data.add_folder(&folder) {
                error.set_text(&message);
                return;
            }
            controller.persist(&w, button, &error, data);
        });
        window.set_child(Some(&body));
        crate::i18n::present(&window);
    }
    pub fn import_profiles(&self) {
        if self.0.store.borrow().is_none() {
            self.startup();
            return;
        }
        let window = self.window("Importer des profils RustRemote", 560);
        let body = gtk::Box::new(gtk::Orientation::Vertical, 12);
        body.add_css_class("dialog-content");
        let hint = gtk::Label::new(Some(
            "JSON RustRemote uniquement (2 Mio max). Aucun secret accepté. Prévisualisation avant import, aucun remplacement ni connexion automatique.",
        ));
        hint.set_wrap(true);
        body.append(&hint);
        let path = gtk::Entry::new();
        path.set_placeholder_text(Some("Chemin du fichier .json"));
        path.update_property(&[gtk::accessible::Property::Label(&crate::i18n::text(
            "Fichier à importer",
        ))]);
        body.append(&path);
        let browse = gtk::Button::with_label("Parcourir…");
        body.append(&browse);
        let w = window.clone();
        let field = path.clone();
        browse.connect_clicked(move |_| {
            let chooser = gtk::FileChooserNative::new(
                Some("Importer un JSON RustRemote"),
                Some(&w),
                gtk::FileChooserAction::Open,
                Some("Choisir"),
                Some("Annuler"),
            );
            let field = field.clone();
            chooser.connect_response(move |dialog, response| {
                if response == gtk::ResponseType::Accept
                    && let Some(p) = dialog.file().and_then(|f| f.path())
                {
                    field.set_text(&p.to_string_lossy());
                }
                dialog.destroy();
            });
            chooser.show();
        });
        let preview = gtk::Button::with_label("Vérifier le fichier");
        body.append(&preview);
        let message = gtk::Label::new(None);
        message.set_wrap(true);
        message.set_lines(8);
        message.set_ellipsize(gtk::pango::EllipsizeMode::End);
        body.append(&message);
        let confirm = gtk::Button::with_label("Confirmer l’import");
        confirm.set_sensitive(false);
        body.append(&confirm);
        let staged = Rc::new(RefCell::new(None::<Workspace>));
        let example = gtk::Button::with_label("Charger l’exemple Serveur IA (simulation)");
        body.append(&example);
        let s = staged.clone();
        let c = confirm.clone();
        let m = message.clone();
        example.connect_clicked(move |_| {
            match Workspace::decode(include_bytes!("../../assets/simulations/serveur-ia.json")) {
                Ok(data) => {
                    m.set_text("Exemple validé : Serveur IA dans Linux, simulation sans réseau");
                    s.replace(Some(data));
                    c.set_sensitive(true);
                }
                Err(e) => m.set_text(&e),
            }
        });
        let s = staged.clone();
        let c = confirm.clone();
        path.connect_changed(move |_| {
            s.replace(None);
            c.set_sensitive(false);
        });
        let staged_copy = staged.clone();
        let browse_copy = browse.clone();
        let example_copy = example.clone();
        let m = message.clone();
        let c = confirm.clone();
        let w = window.clone();
        let path_copy = path.clone();
        preview.connect_clicked(move |button| {
            browse_copy.set_sensitive(false);
            example_copy.set_sensitive(false);
            staged_copy.replace(None);
            c.set_sensitive(false);
            button.set_sensitive(false);
            path_copy.set_sensitive(false);
            w.set_deletable(false);
            m.set_text("Vérification…");
            let path = std::path::PathBuf::from(path_copy.text().as_str());
            let staged = staged_copy.clone();
            let c = c.clone();
            let m = m.clone();
            let b = button.clone();
            let w = w.clone();
            let field = path_copy.clone();
            let browse = browse_copy.clone();
            let example = example_copy.clone();
            io(
                move || {
                    let file = std::fs::OpenOptions::new()
                        .read(true)
                        .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
                        .open(path)
                        .map_err(|_| "Fichier illisible ou lien symbolique refusé")?;
                    if !file
                        .metadata()
                        .map_err(|_| "Fichier inaccessible")?
                        .is_file()
                    {
                        return Err("Un fichier régulier est requis".into());
                    }
                    let mut bytes = Vec::new();
                    file.take(store::MAX_SIZE + 1)
                        .read_to_end(&mut bytes)
                        .map_err(|_| "Lecture impossible")?;
                    Workspace::decode(&bytes)
                },
                move |result| {
                    browse.set_sensitive(true);
                    example.set_sensitive(true);
                    b.set_sensitive(true);
                    field.set_sensitive(true);
                    w.set_deletable(true);
                    match result {
                        Ok(data) => {
                            m.set_text(&format!(
                                "{} profil(s), {} dossier(s) validés : {}",
                                data.profiles.len(),
                                data.folders.len(),
                                data.profiles
                                    .iter()
                                    .take(12)
                                    .map(|p| p.name.as_str())
                                    .collect::<Vec<_>>()
                                    .join(", ")
                            ));
                            staged.replace(Some(data));
                            c.set_sensitive(true);
                        }
                        Err(e) => m.set_text(&e),
                    }
                },
            );
        });
        let controller = self.clone();
        let w = window.clone();
        confirm.connect_clicked(move |button| {
            if let Some(incoming) = staged.borrow().as_ref() {
                match controller.snapshot().merge(incoming) {
                    Ok(data) => controller.persist(&w, button, &message, data),
                    Err(e) => message.set_text(&e),
                }
            }
        });
        let cancel = gtk::Button::with_label("Fermer l’import");
        let w = window.downgrade();
        cancel.connect_clicked(move |_| {
            if let Some(w) = w.upgrade()
                && w.is_deletable()
            {
                w.close();
            }
        });
        body.append(&cancel);
        window.set_child(Some(&body));
        crate::i18n::present(&window);
    }
}

impl Controller {
    pub fn export_profiles(&self) {
        if self.0.store.borrow().is_none() {
            self.startup();
            return;
        }
        let window = self.window("Exporter les profils RustRemote", 550);
        let body = gtk::Box::new(gtk::Orientation::Vertical, 12);
        body.add_css_class("dialog-content");
        let note = gtk::Label::new(Some(
            "L’export JSON contient les hôtes, utilisateurs, dossiers et chemins de clés. Il ne contient ni mot de passe ni clé privée, mais ses métadonnées ne sont pas chiffrées. Aucun fichier existant n’est remplacé.",
        ));
        note.set_wrap(true);
        body.append(&note);
        let path = gtk::Entry::new();
        path.set_placeholder_text(Some("Chemin absolu du fichier .json à créer"));
        path.update_property(&[gtk::accessible::Property::Label(&crate::i18n::text(
            "Fichier d’export",
        ))]);
        body.append(&path);
        let accept =
            gtk::ToggleButton::with_label("J’accepte l’export de métadonnées non chiffrées");
        body.append(&accept);
        let error = gtk::Label::new(None);
        error.set_wrap(true);
        body.append(&error);
        let save = gtk::Button::with_label("Créer l’export JSON");
        save.set_sensitive(false);
        body.append(&save);
        let s = save.clone();
        accept.connect_toggled(move |b| s.set_sensitive(b.is_active()));
        let controller = self.clone();
        let w = window.clone();
        save.connect_clicked(move |button| {
            let data = controller.snapshot();
            let path = std::path::PathBuf::from(path.text().as_str());
            button.set_sensitive(false);
            w.set_deletable(false);
            let b = button.clone();
            let w = w.clone();
            let e = error.clone();
            let status = controller.0.status.clone();
            io(
                move || store::export_json(&path, &data),
                move |result| {
                    b.set_sensitive(true);
                    w.set_deletable(true);
                    match result {
                        Ok(()) => {
                            status.set_text("Export JSON créé, métadonnées non chiffrées");
                            w.close();
                        }
                        Err(message) => e.set_text(&message),
                    }
                },
            );
        });
        let close = gtk::Button::with_label("Annuler l’export");
        let w = window.downgrade();
        close.connect_clicked(move |_| {
            if let Some(w) = w.upgrade()
                && w.is_deletable()
            {
                w.close();
            }
        });
        body.append(&close);
        window.set_child(Some(&body));
        crate::i18n::present(&window);
    }
}

impl Controller {
    pub(super) fn folder_menu(&self, folder: &str, anchor: &impl IsA<gtk::Widget>, x: f64, y: f64) {
        let popover = gtk::Popover::new();
        popover.set_parent(anchor);
        popover.set_pointing_to(Some(&gtk::gdk::Rectangle::new(x as i32, y as i32, 1, 1)));
        let body = gtk::Box::new(gtk::Orientation::Vertical, 4);
        for (title, operation) in [
            ("Nouvelle connexion SSH dans ce dossier", 0),
            ("Nouvelle connexion RDP dans ce dossier", 1),
            ("Nouveau sous-dossier", 2),
            ("Déplacer ce dossier…", 5),
            ("Renommer ce dossier", 3),
            ("Supprimer ce dossier vide", 4),
        ] {
            let item = gtk::Button::with_label(title);
            let c = self.clone();
            let folder = folder.to_string();
            let p = popover.downgrade();
            if operation == 4
                && self
                    .0
                    .profiles
                    .borrow()
                    .iter()
                    .any(|profile| profile.folder == folder)
            {
                item.set_sensitive(false);
                item.set_tooltip_text(Some("Déplacez d’abord les connexions de ce dossier"));
            }
            item.connect_clicked(move |_| {
                if let Some(p) = p.upgrade() {
                    p.popdown();
                }
                match operation {
                    0 | 1 => c.editor_details(None, operation, Some(&folder)),
                    2 => c.new_subfolder(&folder),
                    5 => c.move_dialog(&format!("folder:{folder}"), None),
                    _ => c.edit_folder(&folder, operation == 4),
                }
            });
            body.append(&item);
        }
        popover.set_child(Some(&body));
        popover.connect_closed(crate::dialogs::defer_unparent);
        crate::i18n::translate(popover.upcast_ref());
        popover.popup();
    }
    fn edit_folder(&self, old: &str, remove: bool) {
        let window = self.window(
            if remove {
                "Supprimer un dossier de sessions"
            } else {
                "Renommer un dossier de sessions"
            },
            460,
        );
        let body = gtk::Box::new(gtk::Orientation::Vertical, 10);
        body.add_css_class("dialog-content");
        let name = gtk::Entry::new();
        name.set_text(old);
        name.update_property(&[gtk::accessible::Property::Label(&crate::i18n::text(
            "Nouveau nom du dossier",
        ))]);
        if remove {
            let note = gtk::Label::new(Some(&format!(
                "Supprimer le dossier vide « {old} » ? Aucune connexion ne sera supprimée."
            )));
            note.set_wrap(true);
            body.append(&note);
        } else {
            body.append(&name);
        }
        let error = gtk::Label::new(None);
        error.set_wrap(true);
        error.add_css_class("error");
        body.append(&error);
        let save = gtk::Button::with_label(if remove {
            "Confirmer la suppression du dossier vide"
        } else {
            "Renommer le dossier"
        });
        let c = self.clone();
        let old = old.to_string();
        let w = window.downgrade();
        save.connect_clicked(move |button| {
            let Some(w) = w.upgrade() else {
                return;
            };
            let mut data = c.snapshot();
            if !data.folders.contains(&old) {
                error.set_text("Ce dossier n’existe plus");
                return;
            }
            let result = if remove {
                data.remove_empty_folder(&old)
            } else {
                data.rename_folder(&old, name.text().trim())
            };
            if let Err(message) = result {
                error.set_text(&message);
                return;
            }
            c.persist(&w, button, &error, data);
        });
        body.append(&save);
        let cancel = gtk::Button::with_label("Annuler");
        let w = window.downgrade();
        cancel.connect_clicked(move |_| {
            if let Some(w) = w.upgrade() {
                w.close();
            }
        });
        body.append(&cancel);
        window.set_child(Some(&body));
        crate::i18n::present(&window);
    }
}
