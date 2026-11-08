use crate::{
    profiles::SessionProfile,
    sftp::{Budget, Connection, Entry, Operation, Output},
};
use gtk::prelude::*;
use std::{
    cell::{Cell, RefCell},
    path::PathBuf,
    rc::{Rc, Weak},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
};

struct Browser {
    connection: Arc<Mutex<Option<Connection>>>,
    cancel: Arc<AtomicBool>,
    profile: SessionProfile,
    credential: Option<Arc<crate::credentials::Secret>>,
    busy: Cell<bool>,
    controls: gtk::Box,
    path: gtk::Entry,
    local: gtk::Entry,
    name: gtk::Entry,
    list: gtk::ListBox,
    selected: RefCell<Option<Entry>>,
    entries: RefCell<Vec<Entry>>,
    editor: gtk::TextView,
    editing: RefCell<Option<(PathBuf, Vec<u8>)>>,
    status: gtk::Label,
    progress: Arc<crate::sftp::Progress>,
    progress_bar: gtk::ProgressBar,
}
impl Browser {
    fn invalidate_listing(&self) {
        self.selected.replace(None);
        self.entries.borrow_mut().clear();
        while let Some(child) = self.list.first_child() {
            self.list.remove(&child);
        }
    }
    fn submit(self: &Rc<Self>, operation: Operation) {
        if self.busy.replace(true) {
            self.status.set_text("Attendez la fin de l’opération SFTP");
            return;
        }
        self.cancel.store(false, Ordering::Relaxed);
        self.progress.reset();
        self.progress_bar.set_fraction(0.0);
        self.progress_bar.set_text(Some("0 octets · 0 entrée(s)"));
        self.progress_bar
            .update_property(&[gtk::accessible::Property::Description(
                "0 octets · 0 entrée(s)",
            )]);
        let weak = Rc::downgrade(self);
        gtk::glib::timeout_add_local(std::time::Duration::from_millis(100), move || {
            let Some(state) = weak.upgrade() else {
                return gtk::glib::ControlFlow::Break;
            };
            if !state.busy.get() {
                return gtk::glib::ControlFlow::Break;
            }
            let (bytes, entries) = state.progress.snapshot();
            state
                .progress_bar
                .set_text(Some(&format!("{bytes} octets · {entries} entrée(s)")));
            state
                .progress_bar
                .update_property(&[gtk::accessible::Property::Description(&format!(
                    "{bytes} octets · {entries} entrée(s)"
                ))]);
            state.progress_bar.pulse();
            gtk::glib::ControlFlow::Continue
        });
        if matches!(&operation, Operation::List(_)) {
            self.invalidate_listing();
        }
        self.controls.set_sensitive(false);
        self.status
            .set_text("SFTP en cours · annulation disponible · limite 30 s / 16 Mio");
        let connection = self.connection.clone();
        let profile = self.profile.clone();
        let credential = self.credential.clone();
        let cancel = self.cancel.clone();
        let progress = self.progress.clone();
        let state = Rc::downgrade(self);
        crate::dialogs::io(
            move || {
                let mut guard = connection.lock().map_err(|_| "Session SFTP interrompue")?;
                if guard.is_none() {
                    *guard = Some(Connection::connect(
                        &profile,
                        credential.as_deref().map(|s| s.as_str()),
                    )?);
                }
                let mut budget = Budget::new(cancel).track(progress);
                let result = guard
                    .as_ref()
                    .ok_or("Session SFTP absente")?
                    .run(operation, &mut budget);
                if result.is_err()
                    && let Some(c) = guard.take()
                {
                    c.abort();
                }
                result
            },
            move |result| {
                let Some(state) = state.upgrade() else {
                    return;
                };
                state.busy.set(false);
                let (bytes, entries) = state.progress.snapshot();
                state
                    .progress_bar
                    .set_fraction(if result.is_ok() { 1.0 } else { 0.0 });
                state
                    .progress_bar
                    .set_text(Some(&format!("{bytes} octets · {entries} entrée(s)")));
                state.controls.set_sensitive(true);
                match result{
                Ok(Output::Entries(entries))=>{
                    while let Some(child)=state.list.first_child(){state.list.remove(&child);}
                    state.selected.replace(None);
                    state.entries.replace(entries.clone());
                    for entry in entries{
                        let row=gtk::ListBoxRow::new();row.set_widget_name(&entry.name);
                        let line=gtk::Box::new(gtk::Orientation::Horizontal,8);line.append(&crate::icons::image(if entry.directory{"folder"}else{"file"}));
                        let name=gtk::Label::new(Some(&format!("{}{} · {} o",entry.name,if !entry.directory && !entry.regular{" (lien/spécial)"}else{""},entry.size)));name.set_xalign(0.0);name.set_ellipsize(gtk::pango::EllipsizeMode::End);line.append(&name);row.set_child(Some(&line));
                        let weak=Rc::downgrade(&state);let copy=entry.clone();row.connect_activate(move |_|{if let Some(s)=weak.upgrade(){s.selected.replace(Some(copy.clone()));s.open_selected();}});
                        state.list.append(&row);
                    }
                    state.status.set_text("Listing SFTP authentifié chargé · liens non transférables");
                }
                Ok(Output::Text{path,bytes})=>{
                    let text=String::from_utf8(bytes.clone()).unwrap_or_default();state.editor.buffer().set_text(&text);state.editing.replace(Some((path,bytes)));state.status.set_text("Texte distant chargé ou enregistré · UTF-8, maximum 1 Mio");
                }
                Ok(Output::Done(message))=>state.status.set_text(&message),
                Err(message)=>state.status.set_text(&format!("{message}. Un envoi interrompu peut laisser une destination partielle ; aucun fichier existant n’a été écrasé par un transfert.")),
            }
            },
        );
    }
    fn open_selected(self: &Rc<Self>) {
        let Some(entry) = self.selected.borrow().clone() else {
            self.status
                .set_text("Sélectionnez un fichier ou dossier distant");
            return;
        };
        let path = PathBuf::from(self.path.text().as_str()).join(entry.name);
        if entry.directory {
            self.path.set_text(&path.to_string_lossy());
            self.submit(Operation::List(path));
        } else if entry.regular {
            self.submit(Operation::Read(path));
        } else {
            self.status.set_text("Liens et fichiers spéciaux refusés");
        }
    }
    fn selected_entries(&self) -> Vec<Entry> {
        let entries = self.entries.borrow();
        self.list
            .selected_rows()
            .iter()
            .filter_map(|row| {
                entries
                    .iter()
                    .find(|e| e.name == row.widget_name())
                    .cloned()
            })
            .collect()
    }
    fn change_selected(self: &Rc<Self>, permissions: bool) {
        let selected = self.selected_entries();
        if selected.len() != 1 {
            self.status.set_text("Sélectionnez un seul élément distant");
            return;
        }
        let Some(parent) = self.path.root().and_downcast::<gtk::Window>() else {
            return;
        };
        let path = PathBuf::from(self.path.text().as_str()).join(&selected[0].name);
        let dialog = gtk::Window::builder()
            .title(if permissions {
                "Permissions SFTP"
            } else {
                "Renommer SFTP"
            })
            .transient_for(&parent)
            .modal(true)
            .default_width(380)
            .build();
        let body = gtk::Box::new(gtk::Orientation::Vertical, 10);
        body.add_css_class("dialog-content");
        let input = gtk::Entry::new();
        input.set_max_length(255);
        input.update_property(&[gtk::accessible::Property::Label(if permissions {
            "Permissions octales SFTP"
        } else {
            "Nouveau nom SFTP"
        })]);
        input.set_text(if permissions {
            "600"
        } else {
            &selected[0].name
        });
        let label = gtk::Label::new(Some(if permissions {
            "Droits Unix (000 à 777), sans changement de propriétaire"
        } else {
            "Nouveau nom, sans écraser un élément existant"
        }));
        label.set_wrap(true);
        body.append(&label);
        body.append(&input);
        let apply = gtk::Button::with_label("Appliquer la modification SFTP");
        let weak = Rc::downgrade(self);
        let window = dialog.downgrade();
        apply.connect_clicked(move |_| {
            let Some(state) = weak.upgrade() else {
                return;
            };
            let value = input.text();
            let operation = if permissions {
                if value.len() != 3 || !value.bytes().all(|v| (b'0'..=b'7').contains(&v)) {
                    label.set_text("Saisissez trois chiffres octaux de 000 à 777");
                    return;
                }
                Operation::Permissions {
                    path: path.clone(),
                    mode: u32::from_str_radix(&value, 8).unwrap_or(0),
                }
            } else {
                if let Err(message) = crate::sftp::component(&value) {
                    label.set_text(&message);
                    return;
                }
                Operation::Rename {
                    from: path.clone(),
                    to: path.with_file_name(value.as_str()),
                }
            };
            state.submit(operation);
            if let Some(w) = window.upgrade() {
                w.close();
            }
        });
        body.append(&apply);
        dialog.set_child(Some(&body));
        dialog.present();
    }
    fn delete_selected(self: &Rc<Self>) {
        let selected = self.selected_entries();
        if selected.is_empty() {
            self.status
                .set_text("Sélectionnez les éléments à supprimer");
            return;
        }
        let Some(parent) = self.path.root().and_downcast::<gtk::Window>() else {
            return;
        };
        let dialog = gtk::Window::builder()
            .title("Confirmer la suppression SFTP")
            .transient_for(&parent)
            .modal(true)
            .default_width(420)
            .build();
        let body = gtk::Box::new(gtk::Orientation::Vertical, 10);
        body.add_css_class("dialog-content");
        let label = gtk::Label::new(Some(&format!(
            "Supprimer définitivement {} élément(s) et leur contenu ?\n{}\nLimite : 128 entrées, 8 niveaux ; liens refusés. Aucune corbeille distante.",
            selected.len(),
            selected
                .iter()
                .map(|e| e.name.as_str())
                .collect::<Vec<_>>()
                .join(", ")
        )));
        label.set_wrap(true);
        body.append(&label);
        let confirm = gtk::Button::with_label("Supprimer définitivement la sélection SFTP");
        confirm.add_css_class("destructive-action");
        let operations = selected
            .into_iter()
            .map(|e| Operation::Delete(PathBuf::from(self.path.text().as_str()).join(e.name)))
            .collect::<Vec<_>>();
        let operations = RefCell::new(Some(operations));
        let weak = Rc::downgrade(self);
        let window = dialog.downgrade();
        confirm.connect_clicked(move |_| {
            if let (Some(state), Some(operations)) =
                (weak.upgrade(), operations.borrow_mut().take())
            {
                state.submit(Operation::Batch(operations));
            }
            if let Some(w) = window.upgrade() {
                w.close();
            }
        });
        body.append(&confirm);
        let cancel = gtk::Button::with_label("Annuler la suppression SFTP");
        let window = dialog.downgrade();
        cancel.connect_clicked(move |_| {
            if let Some(w) = window.upgrade() {
                w.close();
            }
        });
        body.append(&cancel);
        dialog.set_child(Some(&body));
        dialog.present();
    }
    fn choose_upload(self: &Rc<Self>, folders: bool) {
        let Some(parent) = self.path.root().and_downcast::<gtk::Window>() else {
            return;
        };
        let chooser = gtk::FileChooserDialog::new(
            Some("Envoyer vers le dossier SFTP courant"),
            Some(&parent),
            if folders {
                gtk::FileChooserAction::SelectFolder
            } else {
                gtk::FileChooserAction::Open
            },
            &[
                ("Annuler", gtk::ResponseType::Cancel),
                ("Envoyer", gtk::ResponseType::Accept),
            ],
        );
        chooser.set_application(parent.application().as_ref());
        chooser.set_modal(true);
        chooser.set_destroy_with_parent(true);
        chooser.set_default_size(800, 600);
        chooser.set_select_multiple(true);
        let weak = Rc::downgrade(self);
        chooser.connect_response(move |chooser, response| {
            if response == gtk::ResponseType::Accept
                && let Some(state) = weak.upgrade()
            {
                let files = chooser.files();
                if files.n_items() > 128 {
                    state
                        .status
                        .set_text("Maximum 128 fichiers ou dossiers par lot");
                    return;
                }
                let mut operations = Vec::new();
                for i in 0..files.n_items() {
                    let Some(local) = files
                        .item(i)
                        .and_downcast::<gtk::gio::File>()
                        .and_then(|f| f.path())
                    else {
                        state
                            .status
                            .set_text("Seuls les chemins locaux sont acceptés");
                        return;
                    };
                    let Some(name) = local.file_name().and_then(|s| s.to_str()) else {
                        return;
                    };
                    if let Err(message) = crate::sftp::component(name) {
                        state.status.set_text(&message);
                        return;
                    }
                    let remote = PathBuf::from(state.path.text().as_str()).join(name);
                    operations.push(Operation::Upload { local, remote });
                }
                state.submit(Operation::Batch(operations));
            }
            chooser.destroy();
        });
        chooser.present();
    }
    fn resume_selected(self: &Rc<Self>, upload: bool) {
        let entries = self.selected_entries();
        if entries.len() != 1 || !entries[0].regular {
            self.status
                .set_text("Sélectionnez un seul fichier régulier pour la reprise");
            return;
        }
        let local = PathBuf::from(self.local.text().as_str());
        if !local.is_absolute() {
            self.status
                .set_text("Chemin local absolu requis pour la reprise");
            return;
        }
        let remote = PathBuf::from(self.path.text().as_str()).join(&entries[0].name);
        let Some(parent) = self.path.root().and_downcast::<gtk::Window>() else {
            return;
        };
        let dialog = gtk::Window::builder()
            .title("Confirmer la reprise SFTP")
            .transient_for(&parent)
            .modal(true)
            .default_width(460)
            .build();
        let body = gtk::Box::new(gtk::Orientation::Vertical, 10);
        body.add_css_class("dialog-content");
        let text = gtk::Label::new(Some(&format!(
            "Reprendre {} du fichier « {} » ? Le préfixe existant sera comparé ; seuls les octets manquants seront ajoutés. La destination peut rester partielle en cas d’interruption.",
            if upload {
                "l’envoi"
            } else {
                "le téléchargement"
            },
            entries[0].name
        )));
        text.set_wrap(true);
        body.append(&text);
        let operation = RefCell::new(Some(Operation::Resume {
            upload,
            local,
            remote,
        }));
        let apply = gtk::Button::with_label("Confirmer la reprise du fichier SFTP");
        let weak = Rc::downgrade(self);
        let window = dialog.downgrade();
        apply.connect_clicked(move |_| {
            if let (Some(state), Some(op)) = (weak.upgrade(), operation.borrow_mut().take()) {
                state.submit(op);
            }
            if let Some(w) = window.upgrade() {
                w.close();
            }
        });
        body.append(&apply);
        let cancel = gtk::Button::with_label("Annuler la reprise SFTP");
        let window = dialog.downgrade();
        cancel.connect_clicked(move |_| {
            if let Some(w) = window.upgrade() {
                w.close();
            }
        });
        body.append(&cancel);
        dialog.set_child(Some(&body));
        dialog.present();
    }
    fn stop(&self) {
        self.cancel.store(true, Ordering::Relaxed);
        if let Ok(mut guard) = self.connection.try_lock()
            && let Some(connection) = guard.take()
        {
            std::thread::spawn(move || connection.abort());
        }
        self.status
            .set_text("Déconnexion / annulation SFTP demandée");
    }
}
fn button<F: Fn(Rc<Browser>) + 'static>(
    body: &gtk::Box,
    label: &str,
    state: &Rc<Browser>,
    action: F,
) -> gtk::Button {
    let button = gtk::Button::with_label(label);
    let weak: Weak<Browser> = Rc::downgrade(state);
    button.connect_clicked(move |_| {
        if let Some(s) = weak.upgrade() {
            action(s);
        }
    });
    body.append(&button);
    button
}

pub fn open(
    parent: &gtk::Window,
    profile: &SessionProfile,
    credential: Option<Arc<crate::credentials::Secret>>,
) -> gtk::Window {
    let window = gtk::Window::builder()
        .title(format!("SFTP — {}", profile.name))
        .transient_for(parent)
        .destroy_with_parent(true)
        .default_width(780)
        .default_height(720)
        .build();
    window.set_application(parent.application().as_ref());
    let header = gtk::HeaderBar::new();
    let title = gtk::Label::new(Some(&format!("SFTP — {}", profile.name)));
    title.set_ellipsize(gtk::pango::EllipsizeMode::End);
    title.set_max_width_chars(45);
    header.set_title_widget(Some(&title));
    window.set_titlebar(Some(&header));
    let body = gtk::Box::new(gtk::Orientation::Vertical, 8);
    body.add_css_class("dialog-content");
    let info = gtk::Label::new(Some(&format!(
        "SFTP · {}:{} · {}\nAgent, clés et mot de passe du profil. HostName et ProxyJump pris en charge ; bastion par clé/agent, empreinte déjà validée. ProxyCommand personnalisé refusé.",
        profile.host, profile.port, profile.username
    )));
    info.set_wrap(true);
    body.append(&info);
    let status = gtk::Label::new(Some("Aucune connexion SFTP ouverte"));
    status.set_wrap(true);
    status.update_property(&[gtk::accessible::Property::Label("État SFTP réel")]);
    body.append(&status);
    let progress_bar = gtk::ProgressBar::new();
    progress_bar.set_show_text(true);
    progress_bar.update_property(&[gtk::accessible::Property::Label("Progression SFTP")]);
    body.append(&progress_bar);
    let controls = gtk::Box::new(gtk::Orientation::Vertical, 8);
    fn field(body: &gtk::Box, label: &str, value: &str) -> gtk::Entry {
        let entry = gtk::Entry::new();
        entry.set_text(value);
        entry.set_placeholder_text(Some(label));
        entry.set_max_length(4096);
        entry.update_property(&[gtk::accessible::Property::Label(label)]);
        body.append(&entry);
        entry
    }
    let path = field(&controls, "Chemin distant SFTP", ".");
    let list = gtk::ListBox::new();
    list.set_selection_mode(gtk::SelectionMode::Multiple);
    controls.append(
        &gtk::ScrolledWindow::builder()
            .child(&list)
            .min_content_height(150)
            .vexpand(true)
            .build(),
    );
    let name = field(&controls, "Nom distant pour création ou envoi", "");
    let local = field(&controls, "Chemin local absolu pour transfert", "");
    let editor = gtk::TextView::new();
    editor.set_monospace(true);
    editor.update_property(&[gtk::accessible::Property::Label("Éditeur SFTP réel")]);
    crate::simulation::bound_text(&editor, &status, 1024 * 1024);
    controls.append(
        &gtk::ScrolledWindow::builder()
            .child(&editor)
            .min_content_height(150)
            .vexpand(true)
            .build(),
    );
    let state = Rc::new(Browser {
        connection: Arc::new(Mutex::new(None)),
        cancel: Arc::new(AtomicBool::new(false)),
        profile: profile.clone(),
        credential,
        busy: Cell::new(false),
        controls: controls.clone(),
        path,
        local,
        name,
        list,
        selected: RefCell::new(None),
        entries: RefCell::new(Vec::new()),
        editor,
        editing: RefCell::new(None),
        status,
        progress: Arc::new(crate::sftp::Progress::default()),
        progress_bar,
    });
    let weak = Rc::downgrade(&state);
    state.path.connect_changed(move |_| {
        if let Some(s) = weak.upgrade() {
            s.invalidate_listing();
            s.status.set_text(
                "Chemin modifié · actualisez le listing avant de sélectionner un élément",
            );
        }
    });
    let weak = Rc::downgrade(&state);
    state.list.connect_row_selected(move |_, row| {
        if let Some(s) = weak.upgrade() {
            let item = row.and_then(|row| {
                s.entries
                    .borrow()
                    .iter()
                    .find(|e| e.name == row.widget_name())
                    .cloned()
            });
            s.selected.replace(item);
        }
    });
    let connect = button(&controls, "Connecter / Actualiser SFTP", &state, |s| {
        let path = PathBuf::from(s.path.text().as_str());
        s.submit(Operation::List(path));
    });
    controls.reorder_child_after(&connect, Some(&state.path));
    let action_box = gtk::Box::new(gtk::Orientation::Vertical, 6);
    button(
        &action_box,
        "Reprendre l’envoi du fichier SFTP…",
        &state,
        |s| s.resume_selected(true),
    );
    button(
        &action_box,
        "Reprendre le téléchargement SFTP…",
        &state,
        |s| s.resume_selected(false),
    );
    button(&action_box, "Dossier parent SFTP", &state, |s| {
        let path = PathBuf::from(s.path.text().as_str()).join("..");
        s.path.set_text(&path.to_string_lossy());
        s.submit(Operation::List(path));
    });
    button(&action_box, "Renommer la sélection SFTP", &state, |s| {
        s.change_selected(false)
    });
    button(
        &action_box,
        "Permissions de la sélection SFTP",
        &state,
        |s| s.change_selected(true),
    );
    button(
        &action_box,
        "Supprimer la sélection SFTP…",
        &state,
        |s| s.delete_selected(),
    );
    button(
        &action_box,
        "Choisir les fichiers à envoyer…",
        &state,
        |s| s.choose_upload(false),
    );
    button(
        &action_box,
        "Choisir les dossiers à envoyer…",
        &state,
        |s| s.choose_upload(true),
    );
    button(&action_box, "Ouvrir la sélection SFTP", &state, |s| {
        s.open_selected()
    });
    button(&action_box, "Créer le dossier distant", &state, |s| {
        let name = s.name.text();
        if let Err(e) = crate::sftp::component(&name) {
            s.status.set_text(&e);
            return;
        }
        let path = PathBuf::from(s.path.text().as_str()).join(name.as_str());
        s.submit(Operation::Mkdir(path));
    });
    button(
        &action_box,
        "Envoyer le fichier ou dossier local",
        &state,
        |s| {
            let local = PathBuf::from(s.local.text().as_str());
            let name = s.name.text().to_string();
            if let Err(e) = crate::sftp::component(&name) {
                s.status.set_text(&e);
                return;
            }
            let remote = PathBuf::from(s.path.text().as_str()).join(name);
            s.submit(Operation::Upload { local, remote });
        },
    );
    button(
        &action_box,
        "Télécharger la sélection SFTP",
        &state,
        |s| {
            let entries = s.selected_entries();
            if entries.is_empty() {
                s.status
                    .set_text("Sélectionnez un ou plusieurs éléments distants");
                return;
            }
            let local = PathBuf::from(s.local.text().as_str());
            if entries.len() == 1 {
                let remote = PathBuf::from(s.path.text().as_str()).join(&entries[0].name);
                s.submit(Operation::Download { remote, local });
            } else {
                if !local.is_absolute()
                    || !std::fs::symlink_metadata(&local).is_ok_and(|m| m.is_dir())
                {
                    s.status.set_text("Pour plusieurs éléments, choisissez un dossier local existant sans lien symbolique");
                    return;
                }
                s.submit(Operation::Batch(
                    entries
                        .into_iter()
                        .map(|e| Operation::Download {
                            remote: PathBuf::from(s.path.text().as_str()).join(&e.name),
                            local: local.join(e.name),
                        })
                        .collect(),
                ));
            }
        },
    );
    button(&action_box, "Enregistrer le fichier distant", &state, |s| {
        let Some((path, original)) = s.editing.borrow().clone() else {
            s.status.set_text("Ouvrez un fichier avant d’enregistrer");
            return;
        };
        let b = s.editor.buffer();
        let text = b
            .text(&b.start_iter(), &b.end_iter(), false)
            .as_bytes()
            .to_vec();
        s.submit(Operation::Save {
            path,
            original,
            text,
        });
    });
    let actions = gtk::FlowBox::new();
    actions.set_selection_mode(gtk::SelectionMode::None);
    actions.set_max_children_per_line(2);
    actions.set_min_children_per_line(1);
    actions.set_row_spacing(6);
    actions.set_column_spacing(6);
    while let Some(child) = action_box.first_child() {
        action_box.remove(&child);
        actions.insert(&child, -1);
    }
    controls.append(&actions);
    body.append(
        &gtk::ScrolledWindow::builder()
            .child(&controls)
            .min_content_height(220)
            .vexpand(true)
            .build(),
    );
    let cancel = gtk::Button::with_label("Déconnecter / Annuler SFTP");
    let weak = Rc::downgrade(&state);
    cancel.connect_clicked(move |_| {
        if let Some(s) = weak.upgrade() {
            s.stop();
        }
    });
    body.append(&cancel);
    let close = gtk::Button::with_label("Fermer le navigateur SFTP");
    let w = window.downgrade();
    close.connect_clicked(move |_| {
        if let Some(w) = w.upgrade() {
            w.close();
        }
    });
    body.append(&close);
    let s = state.clone();
    window.connect_hide(move |w| {
        s.stop();
        w.set_child(None::<&gtk::Widget>);
    });
    window.set_child(Some(&body));
    window.present();
    window
}
