use super::*;
impl Controller {
    pub(super) fn drag_item(&self, widget: &impl IsA<gtk::Widget>, item: String) {
        let source = gtk::DragSource::builder()
            .actions(gtk::gdk::DragAction::MOVE)
            .build();
        source.set_propagation_phase(gtk::PropagationPhase::Capture);
        source.connect_prepare(move |_, _, _| {
            if std::env::var_os("RUSTREMOTE_TEST_TRACE").is_some() {
                eprintln!("Session drag prepared");
            }
            Some(gtk::gdk::ContentProvider::for_value(&item.to_value()))
        });
        widget.add_controller(source);
    }
    pub fn drop_folder(&self, widget: &impl IsA<gtk::Widget>, folder: &str) {
        let target = gtk::DropTarget::new(String::static_type(), gtk::gdk::DragAction::MOVE);
        let c = self.clone();
        let folder = folder.to_string();
        target.connect_drop(move |_, value, _, _| {
            if std::env::var_os("RUSTREMOTE_TEST_TRACE").is_some() {
                eprintln!("Session drop received (root: {})", folder.is_empty());
            }
            let Ok(item) = value.get::<String>() else {
                return false;
            };
            if c.0.busy.get() {
                return false;
            }
            let mut data = c.snapshot();
            if let Err(message) = data.move_item(&item, &folder) {
                c.0.status.set_text(&message);
                return false;
            }
            c.save_tree(data);
            true
        });
        widget.add_controller(target);
    }
    fn save_tree(&self, data: Workspace) {
        let Some(store) = self.0.store.borrow().clone() else {
            return;
        };
        if self.0.busy.replace(true) {
            return;
        }
        let c = self.clone();
        io(
            move || {
                store.save(&data, false)?;
                Ok(data)
            },
            move |result| {
                c.0.busy.set(false);
                match result {
                    Ok(data) => {
                        c.0.folders.replace(data.folders);
                        c.0.profiles.replace(data.profiles);
                        c.refresh();
                    }
                    Err(e) => c.0.status.set_text(&e),
                }
            },
        );
    }
    pub(super) fn move_dialog(&self, item: &str, target: Option<&str>) {
        let window = self.window("Déplacer dans un dossier", 480);
        let body = gtk::Box::new(gtk::Orientation::Vertical, 10);
        body.add_css_class("dialog-content");
        let folders = std::iter::once(String::new())
            .chain(self.0.folders.borrow().iter().cloned())
            .collect::<Vec<_>>();
        let names = folders
            .iter()
            .map(|f| {
                if f.is_empty() {
                    crate::i18n::text("Sans dossier")
                } else {
                    f.clone()
                }
            })
            .collect::<Vec<_>>();
        let choice =
            gtk::DropDown::from_strings(&names.iter().map(String::as_str).collect::<Vec<_>>());
        choice.set_selected(
            target
                .and_then(|t| folders.iter().position(|f| f == t))
                .unwrap_or(0) as u32,
        );
        choice.update_property(&[gtk::accessible::Property::Label(&crate::i18n::text(
            "Dossier destination",
        ))]);
        let name = gtk::Label::new(Some(item.split_once(':').map_or(item, |(_, n)| n)));
        name.add_css_class("user-content");
        body.append(&name);
        body.append(&choice);
        let error = gtk::Label::new(None);
        error.set_wrap(true);
        body.append(&error);
        let save = gtk::Button::with_label("Déplacer");
        let c = self.clone();
        let item = item.to_string();
        let w = window.clone();
        save.connect_clicked(move |b| {
            let mut data = c.snapshot();
            if let Err(e) = data.move_item(&item, &folders[choice.selected() as usize]) {
                error.set_text(&e);
                return;
            }
            c.persist(&w, b, &error, data);
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
    pub fn expand_folders(&self, expanded: bool) {
        fn apply(widget: &gtk::Widget, expanded: bool) {
            if let Some(e) = widget.downcast_ref::<gtk::Expander>() {
                e.set_expanded(expanded);
            }
            let mut child = widget.first_child();
            while let Some(c) = child {
                child = c.next_sibling();
                apply(&c, expanded);
            }
        }
        apply(self.0.rows.upcast_ref(), expanded);
    }
}
