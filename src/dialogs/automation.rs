use super::*;

impl Controller {
    pub fn commands(&self, multiple: bool) {
        let window = self.window(
            if multiple {
                "Multi-Exec SSH"
            } else {
                "Macros SSH"
            },
            620,
        );
        let body = gtk::Box::new(gtk::Orientation::Vertical, 10);
        body.add_css_class("dialog-content");
        let input = gtk::Entry::new();
        input.set_max_length(4096);
        input.update_property(&[gtk::accessible::Property::Label("Commande SSH à vérifier")]);
        let note = gtk::Label::new(Some(
            "Commandes non mémorisées par RustRemote. Vérifiez la commande et chaque cible ; leur résultat reste dans les terminaux. Jusqu’à 8 sessions SSH authentifiées, hors tunnels.",
        ));
        note.set_wrap(true);
        body.append(&note);
        for (label, command) in [
            ("Macro : système", "uname -a"),
            ("Macro : espace disque", "df -h"),
            ("Macro : interfaces réseau", "ip -br address"),
        ] {
            let button = gtk::Button::with_label(label);
            let entry = input.clone();
            button.connect_clicked(move |_| entry.set_text(command));
            body.append(&button);
        }
        body.append(&input);
        let mut sessions = self
            .0
            .terminals
            .borrow()
            .values()
            .filter(|s| s.can_dispatch())
            .cloned()
            .collect::<Vec<_>>();
        sessions.sort_by_key(|s| s.target());
        if !multiple {
            sessions.retain(|s| self.0.tabs.upgrade().is_some_and(|t| s.selected(&t)));
        }
        let choices = sessions
            .into_iter()
            .map(|session| {
                let button = gtk::ToggleButton::with_label(&session.target());
                button.set_active(!multiple);
                body.append(&button);
                (button, session)
            })
            .collect::<Vec<_>>();
        let error = gtk::Label::new(None);
        error.set_wrap(true);
        body.append(&error);
        let verify = gtk::Button::with_label("Vérifier les cibles et la commande");
        let controller = self.clone();
        verify.connect_clicked(move |_| {
            let selected = choices
                .iter()
                .filter(|(b, _)| b.is_active())
                .map(|(_, s)| s.clone())
                .collect::<Vec<_>>();
            let command = input.text().to_string();
            if let Err(message) = crate::tools::command(&command, selected.len()) {
                error.set_text(&message);
                return;
            }
            if controller.0.offline.get() {
                error.set_text("Exécution SSH bloquée hors ligne");
                return;
            }
            let preview = controller.window("Confirmer les commandes SSH", 600);
            let content = gtk::Box::new(gtk::Orientation::Vertical, 12);
            content.add_css_class("dialog-content");
            let label = gtk::Label::new(Some(&format!(
                "Commande :\n{command}\n\nCibles :\n{}",
                selected
                    .iter()
                    .map(|s| s.target())
                    .collect::<Vec<_>>()
                    .join("\n")
            )));
            label.set_wrap(true);
            label.set_selectable(true);
            content.append(&label);
            let confirm =
                gtk::Button::with_label(&format!("Exécuter sur {} session(s) SSH", selected.len()));
            let c = controller.clone();
            let w = preview.downgrade();
            let error = error.clone();
            confirm.connect_clicked(move |button| {
                if c.0.offline.get() || selected.iter().any(|s| !s.can_dispatch()) {
                    error.set_text("Cible fermée ou mode hors ligne : aucune commande envoyée");
                    return;
                }
                button.set_sensitive(false);
                for session in &selected {
                    if let Err(message) = session.send_command(&command) {
                        error.set_text(&message);
                        return;
                    }
                }
                c.0.status.set_text(&format!(
                    "Commande transmise à {} session(s) SSH · consultez les terminaux",
                    selected.len()
                ));
                if let Some(w) = w.upgrade() {
                    w.close();
                }
            });
            content.append(&confirm);
            let cancel = gtk::Button::with_label("Annuler les commandes SSH");
            let w = preview.downgrade();
            cancel.connect_clicked(move |_| {
                if let Some(w) = w.upgrade() {
                    w.close();
                }
            });
            content.append(&cancel);
            preview.set_child(Some(&content));
            preview.present();
        });
        body.append(&verify);
        let close = gtk::Button::with_label("Fermer les outils de commandes");
        let w = window.downgrade();
        close.connect_clicked(move |_| {
            if let Some(w) = w.upgrade() {
                w.close();
            }
        });
        body.append(&close);
        window.set_child(Some(
            &gtk::ScrolledWindow::builder()
                .child(&body)
                .min_content_height(260)
                .vexpand(true)
                .build(),
        ));
        window.present();
    }

    pub fn tunnels(&self) {
        let window = self.window("Tunnels SSH", 620);
        let body = gtk::Box::new(gtk::Orientation::Vertical, 10);
        body.add_css_class("dialog-content");
        let note = gtk::Label::new(Some(
            "Tunnels directs local, distant ou SOCKS. Écoute demandée sur 127.0.0.1 uniquement, ports ≥ 1024. Agent, clé et mot de passe du profil ; la configuration OpenSSH est ignorée pour éviter des redirections implicites. Fermeture de l’onglet = arrêt du tunnel.",
        ));
        note.set_wrap(true);
        body.append(&note);
        for session in self
            .0
            .terminals
            .borrow()
            .values()
            .filter(|s| s.is_tunnel() && s.active())
        {
            let button = gtk::Button::with_label(&format!("Arrêter {}", session.target()));
            let s = session.clone();
            button.connect_clicked(move |b| {
                s.disconnect();
                b.set_sensitive(false);
            });
            body.append(&button);
        }
        let profiles = self
            .0
            .profiles
            .borrow()
            .iter()
            .filter(|p| p.kind() == crate::profiles::SessionKind::Ssh && !p.simulation)
            .cloned()
            .collect::<Vec<_>>();
        let names = profiles.iter().map(|p| p.name.as_str()).collect::<Vec<_>>();
        let profile = gtk::DropDown::from_strings(&names);
        profile.update_property(&[gtk::accessible::Property::Label("Profil SSH du tunnel")]);
        body.append(&profile);
        let kind = gtk::DropDown::from_strings(&[
            "Local — poste vers serveur",
            "Distant — serveur vers poste",
            "SOCKS — proxy local",
        ]);
        kind.update_property(&[gtk::accessible::Property::Label("Type de tunnel SSH")]);
        body.append(&kind);
        fn field(body: &gtk::Box, name: &str, value: &str) -> gtk::Entry {
            let e = gtk::Entry::new();
            e.set_text(value);
            e.set_placeholder_text(Some(name));
            e.update_property(&[gtk::accessible::Property::Label(name)]);
            body.append(&e);
            e
        }
        let listen = field(&body, "Port d’écoute du tunnel", "1080");
        let host = field(&body, "Hôte destination du tunnel", "127.0.0.1");
        let port = field(&body, "Port destination du tunnel", "22");
        let error = gtk::Label::new(None);
        error.set_wrap(true);
        body.append(&error);
        let start = gtk::Button::with_label("Vérifier le tunnel SSH");
        let c = self.clone();
        let settings_window = window.downgrade();
        start.connect_clicked(move |_| {
            let Some(profile)=profiles.get(profile.selected() as usize).cloned() else { error.set_text("Enregistrez un profil SSH non simulé");return; };
            let forward=crate::tools::Forward { kind:kind.selected(),listen:listen.text().parse().unwrap_or(0),host:host.text().trim().into(),port:port.text().parse().unwrap_or(0) };
            if let Err(message)=forward.arguments() { error.set_text(&message);return; }
            if c.0.offline.get() { error.set_text("Connexion de tunnel bloquée hors ligne");return; }
            let confirmation=c.window("Confirmer le tunnel SSH",540);
            let content=gtk::Box::new(gtk::Orientation::Vertical,12);content.add_css_class("dialog-content");
            let label=gtk::Label::new(Some(&format!("Serveur SSH : {}@{}:{}\nÉcoute loopback demandée : 127.0.0.1:{}\nType : {}\nDestination (sauf SOCKS) : {}:{}\n\nOuvrir cette connexion ?",profile.username,profile.host,profile.port,forward.listen,["Local","Distant","SOCKS"][forward.kind as usize],forward.host,forward.port)));label.set_wrap(true);content.append(&label);
            let confirm=gtk::Button::with_label("Autoriser et ouvrir le tunnel");let controller=c.clone();let w=confirmation.downgrade();let error=error.clone();let settings_window=settings_window.clone();
            confirm.connect_clicked(move |button| {
                if controller.0.offline.get() { error.set_text("Connexion bloquée hors ligne");return; }
                button.set_sensitive(false);let c=controller.clone();let w=w.clone();let e=error.clone();let f=forward.clone();let p=profile.clone();let settings_window=settings_window.clone();
                controller.with_password(p.clone(),None,move |secret| {
                    let Some(w)=w.upgrade().filter(|w| w.is_visible()) else { return; };
                    let secret=match secret { Ok(v)=>v,Err(message)=>{ e.set_text(&message);w.close();return; } };
                    if c.0.offline.get() { w.close();return; }
                    let mut p=p;p.name=format!("Tunnel {} · {}",f.listen,p.name.chars().take(60).collect::<String>());
                    if let Some(tabs)=c.0.tabs.upgrade() {
                        if let Some(s)=c.0.terminals.borrow().get(&p.name).filter(|s| s.active()) { s.present(&tabs);w.close();return; }
                        match crate::terminal::open_tunnel(&tabs,&p,&c.0.status,&c.0.preferences.borrow(),secret,&f) {
                            Ok(s)=>{c.0.terminals.borrow_mut().retain(|_,s| s.active());c.0.terminals.borrow_mut().insert(p.name,s.clone());s.present(&tabs);c.sync_view_actions();w.close();if let Some(config)=settings_window.upgrade(){config.close();}},Err(message)=>{e.set_text(&message);w.close();}
                        }
                    }
                });
            });content.append(&confirm);
            let cancel=gtk::Button::with_label("Annuler le tunnel");let w=confirmation.downgrade();cancel.connect_clicked(move |_| { if let Some(w)=w.upgrade(){w.close();} });content.append(&cancel);
            confirmation.set_child(Some(&content));confirmation.present();
        });
        body.append(&start);
        let close = gtk::Button::with_label("Fermer les tunnels");
        let w = window.downgrade();
        close.connect_clicked(move |_| {
            if let Some(w) = w.upgrade() {
                w.close();
            }
        });
        body.append(&close);
        window.set_child(Some(
            &gtk::ScrolledWindow::builder()
                .child(&body)
                .min_content_height(260)
                .vexpand(true)
                .build(),
        ));
        window.present();
    }
}
