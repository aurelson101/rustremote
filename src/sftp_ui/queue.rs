use super::*;
impl Browser {
    pub(super) fn show_queue(&self, snapshot: crate::transfers::Queue) {
        if snapshot.revision < self.queue_revision.get() {
            return;
        }
        self.queue_revision.set(snapshot.revision);
        self.queue_pending.set(snapshot.pending());
        let text = snapshot
            .jobs
            .iter()
            .map(|j| {
                format!(
                    "#{} · {} · {} · {} → {}",
                    j.id,
                    crate::i18n::text(j.state.label()),
                    if j.upload { "↑" } else { "↓" },
                    if j.upload { &j.local } else { &j.remote }.display(),
                    if j.upload { &j.remote } else { &j.local }.display()
                )
            })
            .collect::<Vec<_>>()
            .join("\n");
        self.queue_view.buffer().set_text(&text);
    }
    pub(super) fn load_queue(self: &Rc<Self>) {
        if self.queue_ready.get() || self.queue_loading.replace(true) {
            return;
        }
        let store = self.queue_store.clone();
        let profile = self.profile.clone();
        let queue = self.queue.clone();
        let weak = Rc::downgrade(self);
        crate::dialogs::io(
            move || {
                let mut guard = queue.lock().map_err(|_| "File indisponible")?;
                if guard.is_none() {
                    *guard = Some(crate::transfers::Engine::open(
                        store.ok_or("Magasin de profils requis pour la file persistante")?,
                        &profile,
                    )?);
                }
                Ok(guard.as_ref().ok_or("File indisponible")?.queue.clone())
            },
            move |result| {
                if let Some(s) = weak.upgrade() {
                    s.queue_loading.set(false);
                    match result {
                        Ok(snapshot) => {
                            s.queue_paused.set(snapshot.unfinished());
                            s.queue_ready.set(true);
                            s.show_queue(snapshot);
                        }
                        Err(e) => s.status.set_text(&e),
                    }
                }
            },
        );
    }
    pub(super) fn enqueue_transfers(self: &Rc<Self>, operations: Vec<Operation>) {
        if !self.queue_ready.get() {
            self.status
                .set_text("Attendez l’ouverture de la file persistante");
            self.load_queue();
            return;
        }
        if self
            .path
            .root()
            .and_downcast::<gtk::Window>()
            .is_none_or(|w| !w.is_visible())
        {
            return;
        }
        let queue = self.queue.clone();
        let weak = Rc::downgrade(self);
        crate::dialogs::io(
            move || {
                queue
                    .lock()
                    .map_err(|_| "File indisponible")?
                    .as_mut()
                    .ok_or("File non ouverte")?
                    .enqueue(operations)
            },
            move |result| {
                if let Some(s) = weak.upgrade() {
                    match result {
                        Ok(snapshot) => {
                            s.show_queue(snapshot);
                            if !s.queue_paused.get() {
                                s.queue_auto.set(true);
                            }
                            s.pump_queue();
                        }
                        Err(e) => s.status.set_text(&e),
                    }
                }
            },
        );
    }
    pub(super) fn pump_queue(self: &Rc<Self>) {
        if self.queue_auto.get()
            && self.queue_ready.get()
            && self.queue_pending.get()
            && !self.busy.get()
        {
            self.execute(None);
        }
    }
    fn edit_queue(self: &Rc<Self>, retry: bool) {
        if !self.queue_ready.get() {
            self.load_queue();
            return;
        }
        let queue = self.queue.clone();
        let weak = Rc::downgrade(self);
        crate::dialogs::io(
            move || {
                let mut guard = queue.lock().map_err(|_| "File indisponible")?;
                let engine = guard.as_mut().ok_or("File non ouverte")?;
                if retry {
                    engine.retry()
                } else {
                    engine.clear_completed()
                }
            },
            move |result| {
                if let Some(s) = weak.upgrade() {
                    match result {
                        Ok(snapshot) => {
                            s.show_queue(snapshot);
                            if retry {
                                s.queue_paused.set(false);
                                s.queue_auto.set(true);
                                s.pump_queue();
                            }
                        }
                        Err(e) => s.status.set_text(&e),
                    }
                }
            },
        );
    }
    pub(super) fn queue_panel(self: &Rc<Self>, body: &gtk::Box) {
        let pane = gtk::Box::new(gtk::Orientation::Vertical, 6);
        let info = gtk::Label::new(Some(
            "File privée par profil, chiffrée comme les profils. Après réouverture, vérifiez les chemins puis démarrez explicitement. Une destination partielle peut nécessiter une reprise manuelle.",
        ));
        info.set_wrap(true);
        pane.append(&info);
        self.queue_view
            .update_property(&[gtk::accessible::Property::Label(&crate::i18n::text(
                "File de transferts SFTP",
            ))]);
        pane.append(
            &gtk::ScrolledWindow::builder()
                .child(&self.queue_view)
                .min_content_height(90)
                .max_content_height(140)
                .build(),
        );
        let row = gtk::Box::new(gtk::Orientation::Horizontal, 6);
        button(&row, "Ajouter des fichiers…", self, |s| {
            s.choose_upload(false)
        });
        button(&row, "Ajouter des dossiers…", self, |s| {
            s.choose_upload(true)
        });
        button(&row, "Démarrer la file", self, |s| {
            if !s.queue_ready.get() {
                s.load_queue();
                return;
            }
            s.queue_paused.set(false);
            s.queue_auto.set(true);
            s.pump_queue();
        });
        pane.append(&row);
        let row = gtk::Box::new(gtk::Orientation::Horizontal, 6);
        button(&row, "Pause après le transfert", self, |s| {
            s.queue_paused.set(true);
            s.queue_auto.set(false);
        });
        button(&row, "Réessayer les interrompus", self, |s| {
            s.edit_queue(true)
        });
        button(&row, "Effacer les terminés", self, |s| s.edit_queue(false));
        pane.append(&row);
        let expand = gtk::Expander::new(Some("File de transferts persistante"));
        expand.set_child(Some(&pane));
        let editor = self.editor_expander.downgrade();
        expand.connect_expanded_notify(move |w| {
            if w.is_expanded()
                && let Some(editor) = editor.upgrade()
            {
                editor.set_expanded(false);
            }
        });
        let queue = expand.downgrade();
        self.editor_expander.connect_expanded_notify(move |w| {
            if w.is_expanded()
                && let Some(queue) = queue.upgrade()
            {
                queue.set_expanded(false);
            }
        });
        body.append(&expand);
    }
}
