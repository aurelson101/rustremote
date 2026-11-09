//! Laboratoire local : aucun adaptateur réseau ni commande distante.
use crate::profiles::{ProtocolOptions, SessionProfile};
use gtk::prelude::*;
use std::io::{Read, Write};
use std::os::unix::fs::OpenOptionsExt;
use std::{
    cell::RefCell,
    path::{Component, Path, PathBuf},
    rc::Rc,
};

pub struct Sandbox {
    _root: tempfile::TempDir,
    roots: [PathBuf; 2],
}
impl Sandbox {
    pub fn new() -> Result<Self, String> {
        let root = tempfile::tempdir().map_err(|_| "Laboratoire temporaire impossible")?;
        let roots = [
            root.path().join("local"),
            root.path().join("serveur-simule"),
        ];
        for p in &roots {
            std::fs::create_dir(p).map_err(|_| "Création du laboratoire impossible")?;
        }
        std::fs::create_dir_all(roots[0].join("dossier-test/sous-dossier"))
            .map_err(|_| "Initialisation impossible")?;
        std::fs::write(roots[0].join("note.txt"), "Texte local — été\n")
            .map_err(|_| "Initialisation impossible")?;
        std::fs::write(
            roots[0].join("dossier-test/sous-dossier/exemple.txt"),
            "Dossier fictif UTF-8\n",
        )
        .map_err(|_| "Initialisation impossible")?;
        std::fs::write(
            roots[1].join("serveur.txt"),
            "Serveur IA simulé — aucune connexion réseau\n",
        )
        .map_err(|_| "Initialisation impossible")?;
        Ok(Self { _root: root, roots })
    }
    fn path(&self, side: usize, relative: &Path) -> Result<PathBuf, String> {
        let root = self.roots.get(side).ok_or("Côté invalide")?;
        if relative
            .components()
            .any(|c| !matches!(c, Component::Normal(_)))
        {
            return Err("Chemin non autorisé".into());
        }
        let mut path = root.clone();
        for component in relative.components() {
            path.push(component.as_os_str());
            if let Ok(meta) = path.symlink_metadata()
                && meta.file_type().is_symlink()
            {
                return Err("Liens symboliques refusés".into());
            }
        }
        Ok(path)
    }
    pub fn list(&self, side: usize, relative: &Path) -> Result<Vec<(String, bool)>, String> {
        let mut result = Vec::new();
        for item in
            std::fs::read_dir(self.path(side, relative)?).map_err(|_| "Listing impossible")?
        {
            let item = item.map_err(|_| "Entrée illisible")?;
            let kind = item.file_type().map_err(|_| "Type illisible")?;
            if kind.is_symlink() {
                return Err("Liens symboliques refusés".into());
            }
            result.push((
                item.file_name().to_string_lossy().into_owned(),
                kind.is_dir(),
            ));
        }
        result.sort();
        Ok(result)
    }
    pub fn read(&self, side: usize, relative: &Path) -> Result<String, String> {
        let file = std::fs::OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
            .open(self.path(side, relative)?)
            .map_err(|_| "Lecture impossible")?;
        if !file.metadata().map_err(|_| "Fichier illisible")?.is_file() {
            return Err("Fichier régulier requis".into());
        }
        let mut bytes = Vec::new();
        file.take(1024 * 1024 + 1)
            .read_to_end(&mut bytes)
            .map_err(|_| "Lecture impossible")?;
        if bytes.len() > 1024 * 1024 {
            return Err("Fichier trop volumineux pour la simulation".into());
        }
        String::from_utf8(bytes).map_err(|_| "Fichier binaire : édition texte refusée".into())
    }
    pub fn write(&self, side: usize, relative: &Path, text: &str) -> Result<(), String> {
        if text.len() > 1024 * 1024 {
            return Err("Texte trop volumineux".into());
        }
        let path = self.path(side, relative)?;
        if !path.is_file() {
            return Err("Sélectionnez un fichier existant".into());
        }
        let mut temp = tempfile::NamedTempFile::new_in(path.parent().ok_or("Chemin invalide")?)
            .map_err(|_| "Écriture impossible")?;
        temp.write_all(text.as_bytes())
            .map_err(|_| "Écriture impossible")?;
        temp.as_file()
            .sync_all()
            .map_err(|_| "Synchronisation impossible")?;
        temp.persist(path)
            .map_err(|_| "Enregistrement impossible")?;
        Ok(())
    }
    pub fn mkdir(&self, side: usize, relative: &Path, name: &str) -> Result<(), String> {
        crate::profiles::validate_folder(name)?;
        std::fs::create_dir(self.path(side, &relative.join(name))?)
            .map_err(|_| "Dossier existant ou création impossible".into())
    }
    pub fn copy(
        &self,
        from: usize,
        source: &Path,
        to: usize,
        destination: &Path,
    ) -> Result<(), String> {
        let source = self.path(from, source)?;
        let target = self.path(to, destination)?;
        if target.exists() {
            return Err("Destination existante : écrasement refusé".into());
        }
        // Préparer une copie bornée dans le laboratoire avant de la publier.
        let temporary = tempfile::tempdir_in(target.parent().ok_or("Destination invalide")?)
            .map_err(|_| "Copie impossible")?;
        fn tree(
            src: &Path,
            dst: &Path,
            depth: usize,
            count: &mut usize,
            total: &mut u64,
        ) -> Result<(), String> {
            *count += 1;
            if depth > 8 || *count > 128 {
                return Err("Limite de copie du laboratoire atteinte".into());
            }
            let meta = src.symlink_metadata().map_err(|_| "Source inaccessible")?;
            if meta.file_type().is_symlink() {
                return Err("Liens symboliques refusés".into());
            }
            if meta.is_dir() {
                std::fs::create_dir(dst).map_err(|_| "Création impossible")?;
                for entry in std::fs::read_dir(src).map_err(|_| "Lecture impossible")? {
                    let e = entry.map_err(|_| "Lecture impossible")?;
                    tree(&e.path(), &dst.join(e.file_name()), depth + 1, count, total)?;
                }
            } else if meta.is_file() {
                let remaining = 1024 * 1024 - *total;
                if meta.len() > remaining {
                    return Err("Copie limitée à 1 Mio".into());
                }
                let mut input = std::fs::OpenOptions::new()
                    .read(true)
                    .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
                    .open(src)
                    .map_err(|_| "Lecture impossible")?;
                let mut bytes = Vec::new();
                Read::by_ref(&mut input)
                    .take(remaining + 1)
                    .read_to_end(&mut bytes)
                    .map_err(|_| "Lecture impossible")?;
                if bytes.len() as u64 > remaining {
                    return Err("Copie limitée à 1 Mio".into());
                }
                *total += bytes.len() as u64;
                std::fs::write(dst, bytes).map_err(|_| "Écriture impossible")?;
            } else {
                return Err("Type de fichier refusé".into());
            }
            Ok(())
        }
        tree(&source, &temporary.path().join("copie"), 0, &mut 0, &mut 0)?;
        std::fs::rename(temporary.path().join("copie"), target)
            .map_err(|_| "Publication impossible")?;
        Ok(())
    }
}

pub fn show(app: &gtk::Application, profile: &SessionProfile) -> gtk::Window {
    if matches!(profile.options, ProtocolOptions::Serial { .. }) {
        return switch(app, profile);
    }
    let window = gtk::Window::builder()
        .application(app)
        .title(format!("Simulation — {}", profile.name))
        .default_width(1050)
        .default_height(760)
        .build();
    let body = gtk::Box::new(gtk::Orientation::Vertical, 10);
    body.add_css_class("dialog-content");
    let warning = gtk::Label::new(Some(&format!(
        "{} · {} · SIMULATION LOCALE — aucun réseau, fichiers temporaires uniquement",
        profile.name, profile.host
    )));
    warning.set_wrap(true);
    body.append(&warning);
    let tabs = gtk::Notebook::new();
    tabs.set_vexpand(true);
    body.append(&tabs);
    let status = gtk::Label::new(Some(
        "Laboratoire prêt · pas de session SSH/RDP/SFTP réelle",
    ));
    status.set_wrap(true);
    body.append(&status);
    match Sandbox::new() {
        Ok(sandbox) => {
            tabs.append_page(
                &files(Rc::new(sandbox), &status),
                Some(&gtk::Label::new(Some("Fichiers / SFTP simulé"))),
            );
        }
        Err(e) => status.set_text(&e),
    }
    tabs.append_page(
        &clipboard(profile, &status),
        Some(&gtk::Label::new(Some("Presse-papiers simulé"))),
    );
    let close = gtk::Button::with_label("Déconnecter la simulation");
    let w = window.downgrade();
    close.connect_clicked(move |_| {
        if let Some(w) = w.upgrade() {
            w.close();
        }
    });
    body.append(&close);
    window.set_child(Some(&body));
    crate::i18n::present(&window);
    window
}

fn switch(app: &gtk::Application, profile: &SessionProfile) -> gtk::Window {
    let window = gtk::Window::builder()
        .application(app)
        .title(format!("Simulation — {}", profile.name))
        .default_width(760)
        .default_height(520)
        .build();
    let body = gtk::Box::new(gtk::Orientation::Vertical, 10);
    body.add_css_class("dialog-content");
    body.append(&crate::icons::image("switch"));
    body.append(&gtk::Label::new(Some(
        "SWITCH SIMULÉ — aucun port physique ouvert, aucune commande système",
    )));
    let output = gtk::TextView::new();
    output.set_editable(false);
    output.set_monospace(true);
    output.update_property(&[gtk::accessible::Property::Label(&crate::i18n::text(
        "Console du switch simulé",
    ))]);
    output.buffer().set_text("RustRemote Switch Lab\nswitch-lab> help\nCommandes : show version, show interfaces, show running-config, help\nswitch-lab> ");
    body.append(
        &gtk::ScrolledWindow::builder()
            .child(&output)
            .vexpand(true)
            .build(),
    );
    let input = gtk::Entry::new();
    input.set_max_length(256);
    input.update_property(&[gtk::accessible::Property::Label(
        "Commande du switch simulé",
    )]);
    body.append(&input);
    let send = gtk::Button::with_label("Envoyer au switch simulé");
    let e = input.clone();
    let buffer = output.buffer();
    send.connect_clicked(move |_| {
        let command = e.text();
        let response = switch_response(command.trim());
        if buffer.char_count() > 32768 {
            buffer.set_text("[Historique limité]\n");
        }
        buffer.insert(
            &mut buffer.end_iter(),
            &format!("{}\n{response}\nswitch-lab> ", command.trim()),
        );
        e.set_text("");
    });
    let button = send.downgrade();
    input.connect_activate(move |_| {
        if let Some(button) = button.upgrade() {
            button.emit_clicked();
        }
    });
    body.append(&send);
    let close = gtk::Button::with_label("Déconnecter la simulation");
    let w = window.downgrade();
    close.connect_clicked(move |_| {
        if let Some(w) = w.upgrade() {
            w.close();
        }
    });
    body.append(&close);
    window.set_child(Some(&body));
    crate::i18n::present(&window);
    window
}

pub(crate) fn switch_response(command: &str) -> &'static str {
    match command {
        "show version" => "RustRemote Switch Simulator 1.0 — équipement fictif",
        "show interfaces" => "Gi0/1 up 1000Mb/s\nGi0/2 down\nVLAN1 up — simulation",
        "show running-config" => {
            "hostname switch-lab\ninterface Gi0/1\n description Laboratoire local\n! Aucun secret ni configuration réelle"
        }
        "help" | "?" => "Commandes : show version, show interfaces, show running-config, help",
        _ => "% Commande non prise en charge — simulation en lecture seule",
    }
}

fn files(sandbox: Rc<Sandbox>, status: &gtk::Label) -> gtk::Box {
    let body = gtk::Box::new(gtk::Orientation::Vertical, 8);
    let panels = gtk::Box::new(gtk::Orientation::Horizontal, 12);
    panels.set_vexpand(true);
    body.append(&panels);
    let paths = Rc::new([RefCell::new(PathBuf::new()), RefCell::new(PathBuf::new())]);
    let selected = Rc::new([RefCell::new(None::<PathBuf>), RefCell::new(None::<PathBuf>)]);
    let editing = Rc::new(RefCell::new(None::<(usize, PathBuf)>));
    let editor = gtk::TextView::new();
    editor.set_monospace(true);
    bound_text(&editor, status, 1024 * 1024);
    editor.update_property(&[gtk::accessible::Property::Label(&crate::i18n::text(
        "Éditeur SFTP simulé",
    ))]);
    let listboxes = [gtk::ListBox::new(), gtk::ListBox::new()];
    let refresh: Rc<dyn Fn()> = {
        let sandbox = sandbox.clone();
        let paths = paths.clone();
        let lists = listboxes.each_ref().map(|w| w.downgrade());
        let status = status.clone();
        Rc::new(move || {
            for side in 0..2 {
                let Some(list) = lists[side].upgrade() else {
                    continue;
                };
                while let Some(child) = list.first_child() {
                    list.remove(&child);
                }
                match sandbox.list(side, &paths[side].borrow()) {
                    Ok(entries) => {
                        for (name, is_dir) in entries {
                            let label = gtk::Label::new(Some(&name));
                            label.set_xalign(0.0);
                            let content = gtk::Box::new(gtk::Orientation::Horizontal, 8);
                            content.add_css_class("file-cell");
                            content.append(&crate::icons::image(if is_dir {
                                "folder"
                            } else {
                                "file"
                            }));
                            content.append(&label);
                            let row = gtk::ListBoxRow::new();
                            row.set_widget_name(&name);
                            row.set_child(Some(&content));
                            list.append(&row);
                        }
                    }
                    Err(e) => status.set_text(&e),
                }
            }
        })
    };
    for side in 0..2 {
        let column = gtk::Box::new(gtk::Orientation::Vertical, 6);
        column.set_hexpand(true);
        column.append(&gtk::Label::new(Some(if side == 0 {
            "Poste local (bac à sable)"
        } else {
            "Serveur IA simulé"
        })));
        let up = gtk::Button::with_label(if side == 0 {
            "Dossier parent local"
        } else {
            "Dossier parent distant"
        });
        column.append(&up);
        let p = paths.clone();
        let r = refresh.clone();
        up.connect_clicked(move |_| {
            p[side].borrow_mut().pop();
            r();
        });
        let scroll = gtk::ScrolledWindow::builder()
            .child(&listboxes[side])
            .min_content_height(180)
            .vexpand(true)
            .build();
        column.append(&scroll);
        let sel = selected.clone();
        let p = paths.clone();
        listboxes[side].connect_row_selected(move |_, row| {
            sel[side].replace(row.map(|row| p[side].borrow().join(row.widget_name().as_str())));
        });
        let sb = sandbox.clone();
        let p = paths.clone();
        let r = refresh.clone();
        let e = editor.clone();
        let ed = editing.clone();
        let s = status.clone();
        listboxes[side].connect_row_activated(move |_, row| {
            let relative = p[side].borrow().join(row.widget_name().as_str());
            match sb.path(side, &relative) {
                Ok(path) if path.is_dir() => {
                    p[side].replace(relative);
                    r();
                }
                Ok(_) => match sb.read(side, &relative) {
                    Ok(text) => {
                        e.buffer().set_text(&text);
                        ed.replace(Some((side, relative)));
                        s.set_text("Fichier chargé dans l’éditeur local de simulation");
                    }
                    Err(err) => s.set_text(&err),
                },
                Err(err) => s.set_text(&err),
            }
        });
        let open = gtk::Button::with_label(if side == 0 {
            "Ouvrir la sélection locale"
        } else {
            "Ouvrir la sélection distante"
        });
        let sb = sandbox.clone();
        let sel = selected.clone();
        let p = paths.clone();
        let r = refresh.clone();
        let e = editor.clone();
        let ed = editing.clone();
        let s = status.clone();
        open.connect_clicked(move |_| {
            let Some(relative) = sel[side].borrow().clone() else {
                s.set_text("Sélectionnez un élément");
                return;
            };
            match sb.path(side, &relative) {
                Ok(path) if path.is_dir() => {
                    p[side].replace(relative);
                    r();
                }
                Ok(_) => match sb.read(side, &relative) {
                    Ok(text) => {
                        e.buffer().set_text(&text);
                        ed.replace(Some((side, relative)));
                        s.set_text("Fichier chargé dans l’éditeur local de simulation");
                    }
                    Err(err) => s.set_text(&err),
                },
                Err(err) => s.set_text(&err),
            }
        });
        column.append(&open);
        let create = gtk::Button::with_label(if side == 0 {
            "Créer dossier local…"
        } else {
            "Créer dossier distant…"
        });
        column.append(&create);
        let sb = sandbox.clone();
        let p = paths.clone();
        let r = refresh.clone();
        let s = status.clone();
        create.connect_clicked(move |button| {
            let dialog = gtk::Window::builder()
                .title("Nouveau dossier simulé")
                .modal(true)
                .build();
            if let Some(parent) = button.root().and_downcast::<gtk::Window>() {
                dialog.set_transient_for(Some(&parent));
                dialog.set_application(parent.application().as_ref());
                dialog.set_destroy_with_parent(true);
            }
            let content = gtk::Box::new(gtk::Orientation::Vertical, 8);
            content.add_css_class("dialog-content");
            let name = gtk::Entry::new();
            name.update_property(&[gtk::accessible::Property::Label(&crate::i18n::text(
                "Nom du dossier simulé",
            ))]);
            content.append(&name);
            let ok = gtk::Button::with_label("Créer dans le laboratoire");
            content.append(&ok);
            let sb = sb.clone();
            let p = p.clone();
            let r = r.clone();
            let s = s.clone();
            let w = dialog.downgrade();
            ok.connect_clicked(move |_| {
                match sb.mkdir(side, &p[side].borrow(), name.text().as_str()) {
                    Ok(()) => {
                        r();
                        s.set_text("Dossier simulé créé");
                        if let Some(w) = w.upgrade() {
                            w.close();
                        }
                    }
                    Err(e) => s.set_text(&e),
                }
            });
            dialog.set_child(Some(&content));
            dialog.present();
        });
        panels.append(&column);
    }
    let name = gtk::Entry::new();
    name.set_placeholder_text(Some("Nom à destination (vide = même nom)"));
    name.update_property(&[gtk::accessible::Property::Label(&crate::i18n::text(
        "Nom à destination",
    ))]);
    body.append(&name);
    let actions = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    body.append(&actions);
    for (from, to, title) in [
        (0, 1, "Envoyer vers le serveur simulé →"),
        (1, 0, "← Télécharger vers le poste local"),
    ] {
        let button = gtk::Button::with_label(title);
        let sb = sandbox.clone();
        let sel = selected.clone();
        let p = paths.clone();
        let s = status.clone();
        let r = refresh.clone();
        let name = name.clone();
        button.connect_clicked(move |_| {
            let Some(source) = sel[from].borrow().clone() else {
                s.set_text("Sélectionnez un fichier ou dossier source");
                return;
            };
            let basename = source
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .into_owned();
            let target_name = if name.text().is_empty() {
                basename
            } else {
                name.text().to_string()
            };
            if let Err(e) = crate::profiles::validate_folder(&target_name) {
                s.set_text(&e);
                return;
            }
            match sb.copy(from, &source, to, &p[to].borrow().join(target_name)) {
                Ok(()) => {
                    r();
                    s.set_text(
                        "Transfert simulé terminé · fichiers/dossiers copiés dans le laboratoire",
                    );
                }
                Err(e) => s.set_text(&e),
            }
        });
        actions.append(&button);
    }
    body.append(
        &gtk::ScrolledWindow::builder()
            .child(&editor)
            .min_content_height(150)
            .build(),
    );
    let save = gtk::Button::with_label("Enregistrer le fichier simulé");
    let sb = sandbox;
    let s = status.clone();
    save.connect_clicked(move |_| {
        if let Some((side, path)) = &*editing.borrow() {
            let b = editor.buffer();
            let text = b.text(&b.start_iter(), &b.end_iter(), false);
            match sb.write(*side, path, &text) {
                Ok(()) => s.set_text("Fichier simulé enregistré"),
                Err(e) => s.set_text(&e),
            }
        } else {
            s.set_text("Ouvrez un fichier par double-clic pour l’éditer");
        }
    });
    body.append(&save);
    refresh();
    body
}

fn paste_bounded(view: &gtk::TextView, status: &gtk::Label, limit: usize, replace: bool) {
    paste_from(view, status, view.clipboard(), limit, replace);
}
fn paste_from(
    view: &gtk::TextView,
    status: &gtk::Label,
    clipboard: gtk::gdk::Clipboard,
    limit: usize,
    replace: bool,
) {
    let view = view.downgrade();
    let status = status.downgrade();
    gtk::glib::MainContext::default().spawn_local(async move {
        let result = crate::clipboard::read_bounded(&clipboard, limit).await;
        if let (Some(view), Some(status)) = (view.upgrade(), status.upgrade()) {
            match result {
                Ok(text) => {
                    let b = view.buffer();
                    let current = b.text(&b.start_iter(), &b.end_iter(), false);
                    if !replace && current.len() + text.len() > limit {
                        status.set_text("Texte trop volumineux");
                        return;
                    }
                    if replace {
                        b.set_text(&text);
                    } else {
                        b.delete_selection(true, true);
                        b.insert_at_cursor(&text);
                    }
                    status.set_text("Texte collé via le presse-papiers système");
                }
                Err(e) => status.set_text(e),
            }
        }
    });
}

pub(crate) fn bound_text(view: &gtk::TextView, status: &gtk::Label, limit: usize) {
    let v = view.downgrade();
    let s = status.downgrade();
    crate::clipboard::controlled_primary_paste(view, move |clipboard| {
        if let (Some(v), Some(s)) = (v.upgrade(), s.upgrade())
            && v.is_editable()
            && v.is_sensitive()
        {
            paste_from(&v, &s, clipboard, limit, false);
        }
    });
    let s = status.clone();
    view.connect_paste_clipboard(move |v| {
        v.stop_signal_emission_by_name("paste-clipboard");
        paste_bounded(v, &s, limit, false);
    });
    let s = status.clone();
    view.buffer().connect_insert_text(move |b, _, text| {
        if text.len() > limit
            || b.text(&b.start_iter(), &b.end_iter(), false).len() + text.len() > limit
        {
            b.stop_signal_emission_by_name("insert-text");
            s.set_text("Taille de texte maximale atteinte");
        }
    });
}

fn clipboard(profile: &SessionProfile, status: &gtk::Label) -> gtk::Box {
    let body = gtk::Box::new(gtk::Orientation::Vertical, 12);
    let note = gtk::Label::new(Some(
        "Test local du presse-papiers système, pas un canal SSH/RDP. Aucun texte collé n’est exécuté. Les dossiers passent par l’onglet Fichiers ; leur copie via un canal distant n’est pas validée.",
    ));
    note.set_wrap(true);
    body.append(&note);
    let allow = gtk::ToggleButton::with_label("Autoriser le presse-papiers de la simulation");
    allow.set_active(match profile.options {
        ProtocolOptions::Ssh { .. } => true,
        ProtocolOptions::Rdp { clipboard, .. } => clipboard,
        ProtocolOptions::Vnc { .. } | ProtocolOptions::Serial { .. } => false,
    });
    body.append(&allow);
    let panes = gtk::Box::new(gtk::Orientation::Horizontal, 12);
    body.append(&panes);
    let views = [gtk::TextView::new(), gtk::TextView::new()];
    for (i, name) in ["Texte local", "Texte distant simulé"].iter().enumerate() {
        views[i].set_wrap_mode(gtk::WrapMode::WordChar);
        bound_text(&views[i], status, 65536);
        views[i].update_property(&[gtk::accessible::Property::Label(name)]);
        let box_ = gtk::Box::new(gtk::Orientation::Vertical, 6);
        box_.set_hexpand(true);
        box_.append(&gtk::Label::new(Some(name)));
        box_.append(
            &gtk::ScrolledWindow::builder()
                .child(&views[i])
                .min_content_height(200)
                .build(),
        );
        panes.append(&box_);
    }
    views[0]
        .buffer()
        .set_text("Texte local UTF-8 — été\nDeuxième ligne");
    views[1].buffer().set_text("Texte du serveur IA simulé");
    let actions = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    body.append(&actions);
    for (side, paste, title) in [
        (0, false, "Copier local"),
        (1, true, "Coller distant"),
        (1, false, "Copier distant"),
        (0, true, "Coller local"),
    ] {
        let button = gtk::Button::with_label(title);
        button.set_sensitive(allow.is_active());
        let b = button.clone();
        allow.connect_toggled(move |c| b.set_sensitive(c.is_active()));
        let view = views[side].clone();
        let s = status.clone();
        button.connect_clicked(move |_| {
            let clipboard = view.clipboard();
            if paste {
                let view = view.clone();
                let s = s.clone();
                paste_bounded(&view, &s, 65536, true);
            } else {
                let b = view.buffer();
                let text = b.text(&b.start_iter(), &b.end_iter(), false);
                if text.len() <= 65536 {
                    clipboard.set_text(&text);
                    s.set_text("Texte copié dans le presse-papiers système");
                } else {
                    s.set_text("Texte limité à 64 Kio");
                }
            }
        });
        actions.append(&button);
    }
    body
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn files_folders_bidirectional_edit_and_collision() {
        let s = Sandbox::new().unwrap();
        s.copy(0, Path::new("dossier-test"), 1, Path::new("dossier-test"))
            .unwrap();
        let file = Path::new("dossier-test/sous-dossier/exemple.txt");
        s.write(1, file, "Modifié à distance simulée\n").unwrap();
        s.copy(1, Path::new("dossier-test"), 0, Path::new("retour-dossier"))
            .unwrap();
        assert_eq!(
            s.read(0, Path::new("retour-dossier/sous-dossier/exemple.txt"))
                .unwrap(),
            "Modifié à distance simulée\n"
        );
        assert!(
            s.copy(0, Path::new("note.txt"), 1, Path::new("dossier-test"))
                .is_err()
        );
    }
    #[test]
    fn rejects_traversal_links_and_large_files() {
        let s = Sandbox::new().unwrap();
        assert!(s.mkdir(0, Path::new(""), "../dehors").is_err());
        assert!(s.read(0, Path::new("../secret")).is_err());
        std::os::unix::fs::symlink("/etc/passwd", s.roots[0].join("lien")).unwrap();
        assert!(s.copy(0, Path::new("lien"), 1, Path::new("lien")).is_err());
        let f = std::fs::File::create(s.roots[0].join("grand")).unwrap();
        f.set_len(1024 * 1024 + 1).unwrap();
        assert!(
            s.copy(0, Path::new("grand"), 1, Path::new("grand"))
                .is_err()
        );
        assert!(!s.roots[1].join("grand").exists());
    }
}
