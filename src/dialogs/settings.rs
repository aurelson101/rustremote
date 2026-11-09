use super::*;
use crate::preferences::Preferences;
impl Controller {
    fn preferences_path(&self) -> Result<std::path::PathBuf, String> {
        Ok(self
            .0
            .demo_root
            .borrow()
            .as_ref()
            .map(|d| Ok(d.path().to_path_buf()))
            .unwrap_or_else(Store::directory)?
            .join("settings.json"))
    }
    pub fn initialize(&self, demo: bool) {
        if demo {
            self.start_demo();
        }
        self.load_preferences(!demo);
    }
    fn load_preferences(&self, open_profiles: bool) {
        let path = match self.preferences_path() {
            Ok(path) => path,
            Err(e) => {
                self.information("Paramètres indisponibles", &e);
                return;
            }
        };
        self.0.busy.set(true);
        let controller = self.clone();
        io(
            move || Preferences::load(&path),
            move |result| {
                controller.0.busy.set(false);
                match result {
                    Ok(prefs) => controller.apply_preferences(prefs),
                    Err(e) => controller
                        .0
                        .status
                        .set_text(&format!("{e} · paramètres par défaut conservés")),
                }
                if open_profiles {
                    controller.startup();
                }
            },
        );
    }
    fn apply_preferences(&self, preferences: Preferences) {
        for terminal in self.0.terminals.borrow().values() {
            terminal.configure(&preferences);
        }
        self.0.preferences.replace(preferences);
    }
    pub fn settings(&self) {
        if self.0.busy.get() {
            self.information(
                "Opération en cours",
                "Attendez la fin de l’opération avant de modifier les paramètres.",
            );
            return;
        }
        let window = self.window("Paramètres et sécurité", 540);
        let body = gtk::Box::new(gtk::Orientation::Vertical, 12);
        body.add_css_class("dialog-content");
        let preferences = self.0.preferences.borrow().clone();
        body.append(&gtk::Label::new(Some(
            "Langue de l’interface (redémarrage nécessaire)",
        )));
        let language = crate::i18n::dropdown(&["English", "Français"]);
        language.set_selected(u32::from(preferences.language == "fr"));
        language.update_property(&[gtk::accessible::Property::Label(&crate::i18n::text(
            "Interface language",
        ))]);
        body.append(&language);
        let note = gtk::Label::new(Some(
            "Réglages appliqués aux terminaux ouverts et aux nouvelles sessions. Les paramètres de démonstration restent temporaires.",
        ));
        note.set_wrap(true);
        body.append(&note);
        fn number(body: &gtk::Box, name: &str, min: f64, max: f64, value: f64) -> gtk::SpinButton {
            let row = gtk::Box::new(gtk::Orientation::Horizontal, 12);
            let label = gtk::Label::new(Some(name));
            label.set_hexpand(true);
            label.set_xalign(0.0);
            row.append(&label);
            let field = gtk::SpinButton::with_range(min, max, 1.0);
            field.set_value(value);
            field.update_property(&[gtk::accessible::Property::Label(&crate::i18n::text(name))]);
            row.append(&field);
            body.append(&row);
            field
        }
        let font = number(
            &body,
            "Taille de police du terminal",
            8.,
            24.,
            preferences.font_size as f64,
        );
        let history = number(
            &body,
            "Lignes d’historique",
            100.,
            20000.,
            preferences.scrollback as f64,
        );
        let black = gtk::ToggleButton::with_label("Fond noir du terminal");
        black.set_active(preferences.black_terminal);
        body.append(&black);
        let ping = gtk::ToggleButton::with_label("Activer l’outil Ping");
        ping.set_active(preferences.ping_enabled);
        body.append(&ping);
        let trace = gtk::ToggleButton::with_label("Activer l’outil Traceroute");
        trace.set_active(preferences.traceroute_enabled);
        body.append(&trace);
        let error = gtk::Label::new(None);
        error.set_wrap(true);
        error.add_css_class("error");
        body.append(&error);
        let save = gtk::Button::with_label("Enregistrer les paramètres");
        save.add_css_class("suggested-action");
        body.append(&save);
        let controller = self.clone();
        let w = window.clone();
        save.connect_clicked(move |button| {
            if controller.0.busy.get() {
                return;
            }
            font.update();
            history.update();
            let preferences = Preferences {
                language: if language.selected() == 1 { "fr" } else { "en" }.into(),
                font_size: font.value_as_int() as u8,
                scrollback: history.value_as_int() as u32,
                black_terminal: black.is_active(),
                ping_enabled: ping.is_active(),
                traceroute_enabled: trace.is_active(),
                ..Preferences::default()
            };
            let path = match controller.preferences_path() {
                Ok(path) => path,
                Err(e) => {
                    error.set_text(&e);
                    return;
                }
            };
            if let Err(e) = preferences.validate() {
                error.set_text(&e);
                return;
            }
            controller.0.busy.set(true);
            button.set_sensitive(false);
            w.set_deletable(false);
            if let Some(child) = w.child() {
                child.set_sensitive(false);
            }
            let c = controller.clone();
            let w = w.clone();
            let e = error.clone();
            let b = button.clone();
            let saved = preferences.clone();
            io(
                move || preferences.save(&path),
                move |result| {
                    c.0.busy.set(false);
                    w.set_deletable(true);
                    if let Some(child) = w.child() {
                        child.set_sensitive(true);
                    }
                    b.set_sensitive(true);
                    match result {
                        Ok(()) => {
                            c.apply_preferences(saved);
                            c.0.status.set_text("Paramètres enregistrés et appliqués");
                            w.close();
                        }
                        Err(message) => e.set_text(&message),
                    }
                },
            );
        });
        let security = gtk::Button::with_label("Sécurité et stockage");
        let c = self.clone();
        security.connect_clicked(move |_| c.security_details());
        body.append(&security);
        if self.0.demo_root.borrow().is_some() {
            let exit = gtk::Button::with_label("Quitter la démonstration");
            exit.set_action_name(Some("app.exit-demo"));
            body.append(&exit);
        }
        let close = gtk::Button::with_label("Fermer");
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
    pub fn exit_demo(&self, after: impl Fn() + 'static) {
        if self.0.demo_root.borrow().is_none() {
            return;
        }
        if self.0.busy.get() {
            self.information(
                "Opération en cours",
                "Attendez la fin de l’opération avant de quitter la démonstration.",
            );
            return;
        }
        let window = self.window("Quitter la démonstration", 500);
        let body = gtk::Box::new(gtk::Orientation::Vertical, 12);
        body.add_css_class("dialog-content");
        let mode = if self.0.enforced_offline {
            "Vos profils persistants seront ouverts, mais --offline continue d’interdire les connexions réseau."
        } else {
            "Le mode normal utilise vos profils persistants et autorise les connexions après confirmation."
        };
        let note = gtk::Label::new(Some(&format!(
            "{mode} Les profils temporaires ne sont pas copiés automatiquement : exportez-les via Fichier → Exporter pour les conserver."
        )));
        note.set_wrap(true);
        body.append(&note);
        let confirm = gtk::Button::with_label("Ouvrir mes profils persistants");
        confirm.add_css_class("suggested-action");
        let c = self.clone();
        confirm.connect_clicked(move |_| {
            if c.0.busy.get() || c.0.demo_root.borrow().is_none() {
                return;
            }
            if let Some(app) = c.0.app.upgrade() {
                for window in app.windows() {
                    if !window.is::<gtk::ApplicationWindow>() {
                        window.close();
                    }
                }
            }
            c.0.passwords.borrow_mut().clear();
            c.0.store.replace(None);
            c.0.session_lock.replace(None);
            c.0.demo_root.replace(None);
            c.0.profiles.borrow_mut().clear();
            c.0.folders.borrow_mut().clear();
            c.0.simulations.borrow_mut().clear();
            c.0.offline.set(c.0.enforced_offline);
            c.refresh();
            after();
            c.0.status.set_text(if c.0.enforced_offline {
                "Profils persistants · réseau bloqué par --offline"
            } else {
                "Mode normal · ouvrez ou créez vos profils persistants"
            });
            c.load_preferences(true);
        });
        body.append(&confirm);
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
