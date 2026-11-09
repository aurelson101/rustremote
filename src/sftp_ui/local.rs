use super::*;
#[derive(Clone)]
pub(super) struct LocalEntry {
    pub path: PathBuf,
    pub directory: bool,
    pub regular: bool,
    pub size: u64,
}
fn listing(path: &std::path::Path) -> Result<Vec<LocalEntry>, String> {
    if !path.is_absolute() {
        return Err("Chemin local absolu requis".into());
    }
    let mut entries = Vec::new();
    for item in std::fs::read_dir(path)
        .map_err(|_| "Dossier local illisible")?
        .take(2049)
    {
        let item = item.map_err(|_| "Lecture du dossier local impossible")?;
        if entries.len() == 2048 {
            return Err("Listing local limité à 2048 entrées".into());
        }
        let meta = item
            .path()
            .symlink_metadata()
            .map_err(|_| "Métadonnées locales indisponibles")?;
        entries.push(LocalEntry {
            path: item.path(),
            directory: meta.is_dir(),
            regular: meta.is_file(),
            size: meta.len(),
        });
    }
    entries.sort_by(|a, b| {
        b.directory
            .cmp(&a.directory)
            .then_with(|| a.path.cmp(&b.path))
    });
    Ok(entries)
}
impl Browser {
    pub(super) fn refresh_local(self: &Rc<Self>) {
        if self.local_loading.replace(true) {
            self.local_pending.set(true);
            return;
        }
        let path = PathBuf::from(self.local_path.text().as_str());
        let generation = self.local_generation.get().wrapping_add(1);
        self.local_generation.set(generation);
        let weak = Rc::downgrade(self);
        crate::dialogs::io(
            move || listing(&path),
            move |result| {
                let Some(s) = weak.upgrade() else {
                    return;
                };
                s.local_loading.set(false);
                if s.local_pending.replace(false) {
                    s.refresh_local();
                    return;
                }
                if s.local_generation.get() != generation {
                    return;
                }
                match result {
                    Ok(entries) => {
                        while let Some(child) = s.local_list.first_child() {
                            s.local_list.remove(&child);
                        }
                        for (index, item) in entries.iter().enumerate() {
                            let row = gtk::ListBoxRow::new();
                            row.set_widget_name(&index.to_string());
                            let line = gtk::Box::new(gtk::Orientation::Horizontal, 8);
                            line.append(&crate::icons::image(if item.directory {
                                "folder"
                            } else {
                                "file"
                            }));
                            let name = item.path.file_name().unwrap_or_default().to_string_lossy();
                            let label = gtk::Label::new(Some(&format!(
                                "{name} · {} B{}",
                                item.size,
                                if !item.directory && !item.regular {
                                    crate::i18n::text(" (lien/spécial)")
                                } else {
                                    String::new()
                                }
                            )));
                            label.add_css_class("user-content");
                            label.set_xalign(0.0);
                            label.set_hexpand(true);
                            label.set_ellipsize(gtk::pango::EllipsizeMode::End);
                            line.append(&label);
                            row.set_child(Some(&line));
                            let weak = Rc::downgrade(&s);
                            let path = item.path.clone();
                            let directory = item.directory;
                            row.connect_activate(move |_| {
                                if let Some(s) = weak.upgrade() {
                                    if directory {
                                        s.local_path.set_text(&path.to_string_lossy());
                                        s.refresh_local();
                                    } else {
                                        s.upload_paths(vec![path.clone()]);
                                    }
                                }
                            });
                            s.local_list.append(&row);
                        }
                        s.local_entries.replace(entries);
                    }
                    Err(e) => s.status.set_text(&e),
                }
            },
        );
    }
    fn local_selection(&self) -> Vec<PathBuf> {
        self.local_list
            .selected_rows()
            .iter()
            .filter_map(|row| {
                row.widget_name()
                    .parse::<usize>()
                    .ok()
                    .and_then(|i| self.local_entries.borrow().get(i).map(|e| e.path.clone()))
            })
            .collect()
    }
    pub(super) fn upload_paths(self: &Rc<Self>, paths: Vec<PathBuf>) {
        if paths.is_empty() || paths.len() > 128 {
            self.status
                .set_text("Sélectionnez de 1 à 128 fichiers ou dossiers locaux");
            return;
        }
        let mut ops = Vec::new();
        for local in paths {
            let Some(name) = local.file_name().and_then(|n| n.to_str()) else {
                self.status.set_text("Nom de fichier invalide");
                return;
            };
            if let Err(e) = crate::sftp::component(name) {
                self.status.set_text(&e);
                return;
            }
            ops.push(Operation::Upload {
                remote: PathBuf::from(self.path.text().as_str()).join(name),
                local,
            });
        }
        self.submit(Operation::Batch(ops));
    }
    pub(super) fn upload_selection(self: &Rc<Self>) {
        self.upload_paths(self.local_selection());
    }
    pub(super) fn download_here(self: &Rc<Self>) {
        let entries = self.selected_entries();
        if entries.is_empty() {
            self.status
                .set_text("Sélectionnez un ou plusieurs éléments distants");
            return;
        }
        let local = PathBuf::from(self.local_path.text().as_str());
        let remote = PathBuf::from(self.path.text().as_str());
        self.submit(Operation::Batch(
            entries
                .into_iter()
                .map(|e| Operation::Download {
                    remote: remote.join(&e.name),
                    local: local.join(e.name),
                })
                .collect(),
        ));
    }
    pub(super) fn graphical_actions(self: &Rc<Self>, toolbar: &gtk::Box) {
        button(toolbar, "Envoyer →", self, |s| s.upload_selection())
            .add_css_class("suggested-action");
        button(toolbar, "← Télécharger", self, |s| s.download_here())
            .add_css_class("suggested-action");
        button(toolbar, "Nouveau dossier distant…", self, |s| {
            s.mkdir_dialog()
        });
        let source = gtk::DragSource::builder()
            .actions(gtk::gdk::DragAction::COPY)
            .build();
        let weak = Rc::downgrade(self);
        source.set_propagation_phase(gtk::PropagationPhase::Capture);
        source.connect_prepare(move |_, _, _| {
            let s = weak.upgrade()?;
            let files = s
                .local_selection()
                .into_iter()
                .map(gtk::gio::File::for_path)
                .collect::<Vec<_>>();
            if files.is_empty() {
                None
            } else {
                Some(gtk::gdk::ContentProvider::for_value(
                    &gtk::gdk::FileList::from_array(&files).to_value(),
                ))
            }
        });
        self.local_list.add_controller(source);
        let drop = gtk::DropTarget::new(
            gtk::gdk::FileList::static_type(),
            gtk::gdk::DragAction::COPY,
        );
        let weak = Rc::downgrade(self);
        drop.connect_drop(move |_, value, _, _| {
            let Some(s) = weak.upgrade().filter(|s| !s.busy.get()) else {
                return false;
            };
            let Ok(files) = value.get::<gtk::gdk::FileList>() else {
                return false;
            };
            let files = files.files();
            if files.is_empty() || files.len() > 128 {
                return false;
            }
            let Some(paths) = files.iter().map(|f| f.path()).collect::<Option<Vec<_>>>() else {
                s.status.set_text("Seuls les chemins locaux sont acceptés");
                return false;
            };
            let weak = Rc::downgrade(&s);
            gtk::glib::idle_add_local_once(move || {
                if let Some(s) = weak.upgrade() {
                    s.upload_paths(paths);
                }
            });
            true
        });
        self.list.add_controller(drop);
    }
    fn mkdir_dialog(self: &Rc<Self>) {
        let Some(parent) = self.path.root().and_downcast::<gtk::Window>() else {
            return;
        };
        let w = gtk::Window::builder()
            .application(parent.application().as_ref().unwrap())
            .transient_for(&parent)
            .modal(true)
            .destroy_with_parent(true)
            .title("Nouveau dossier distant")
            .default_width(400)
            .build();
        let b = gtk::Box::new(gtk::Orientation::Vertical, 10);
        b.add_css_class("dialog-content");
        let name = gtk::Entry::new();
        name.set_placeholder_text(Some("Nom du dossier"));
        b.append(&name);
        let error = gtk::Label::new(None);
        error.set_wrap(true);
        b.append(&error);
        let save = gtk::Button::with_label("Créer le dossier");
        b.append(&save);
        let weak = Rc::downgrade(self);
        let window = w.downgrade();
        save.connect_clicked(move |_| {
            let Some(s) = weak.upgrade() else {
                return;
            };
            let n = name.text();
            if let Err(e) = crate::sftp::component(&n) {
                error.set_text(&e);
                return;
            }
            s.submit(Operation::Mkdir(
                PathBuf::from(s.path.text().as_str()).join(n.as_str()),
            ));
            if let Some(w) = window.upgrade() {
                w.close();
            }
        });
        w.set_child(Some(&b));
        crate::i18n::present(&w);
    }
}

impl Browser {
    pub(super) fn context_actions(self: &Rc<Self>, remote: bool) {
        let list = if remote {
            self.list.clone()
        } else {
            self.local_list.clone()
        };
        let gesture = gtk::GestureClick::new();
        gesture.set_button(3);
        let weak = Rc::downgrade(self);
        let anchor = list.downgrade();
        gesture.connect_pressed(move |gesture, _, x, y| {
            let (Some(s), Some(list)) = (weak.upgrade(), anchor.upgrade()) else {
                return;
            };
            if s.busy.get() {
                return;
            }
            if let Some(row) = list.row_at_y(y as i32)
                && !row.is_selected()
            {
                list.unselect_all();
                list.select_row(Some(&row));
            }
            gesture.set_state(gtk::EventSequenceState::Claimed);
            let p = gtk::Popover::new();
            p.set_parent(&list);
            p.set_pointing_to(Some(&gtk::gdk::Rectangle::new(x as i32, y as i32, 1, 1)));
            let b = gtk::Box::new(gtk::Orientation::Vertical, 4);
            if remote {
                button(&b, "Ouvrir la sélection SFTP", &s, |s| s.open_selected());
                button(&b, "← Télécharger", &s, |s| s.download_here());
                button(&b, "Nouveau dossier distant…", &s, |s| s.mkdir_dialog());
                button(&b, "Renommer la sélection SFTP", &s, |s| {
                    s.change_selected(false)
                });
                button(&b, "Permissions de la sélection SFTP", &s, |s| {
                    s.change_selected(true)
                });
                button(&b, "Supprimer la sélection SFTP…", &s, |s| {
                    s.delete_selected()
                });
            } else {
                button(&b, "Envoyer →", &s, |s| s.upload_selection());
            }
            let mut child = b.first_child();
            while let Some(w) = child {
                child = w.next_sibling();
                if let Ok(button) = w.downcast::<gtk::Button>() {
                    let weak = p.downgrade();
                    button.connect_clicked(move |_| {
                        if let Some(p) = weak.upgrade() {
                            p.popdown();
                        }
                    });
                }
            }
            p.set_child(Some(&b));
            p.connect_closed(crate::dialogs::defer_unparent);
            crate::i18n::translate(p.upcast_ref());
            p.popup();
        });
        list.add_controller(gesture);
    }
}
