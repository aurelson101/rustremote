use super::*;
impl Controller {
    pub fn diagnostics(&self) {
        let window = self.window("Outils réseau — Ping / Traceroute", 560);
        let body = gtk::Box::new(gtk::Orientation::Vertical, 12);
        body.add_css_class("dialog-content");
        let host = gtk::Entry::new();
        host.set_text("127.0.0.1");
        host.update_property(&[gtk::accessible::Property::Label(&crate::i18n::text(
            "Cible du diagnostic",
        ))]);
        body.append(&host);
        let kind = crate::i18n::dropdown(&["Ping (4 requêtes)", "Traceroute (12 sauts max)"]);
        body.append(&kind);
        let simulated = gtk::ToggleButton::with_label("Simulation locale — aucun paquet réseau");
        simulated.set_active(true);
        body.append(&simulated);
        let output = gtk::TextView::new();
        output.set_editable(false);
        output.set_monospace(true);
        output.update_property(&[gtk::accessible::Property::Label(&crate::i18n::text(
            "Résultat du diagnostic",
        ))]);
        body.append(
            &gtk::ScrolledWindow::builder()
                .child(&output)
                .min_content_height(220)
                .build(),
        );
        let run = gtk::Button::with_label("Exécuter le diagnostic sur cette cible");
        body.append(&run);
        let w = window.clone();
        let controller = self.clone();
        run.connect_clicked(move |button|{
            let target=host.text().trim().to_string();
            let profile=SessionProfile{name:"Diagnostic".into(),host:target.clone(),port:22,username:"diagnostic".into(),credential_ref:None,favorite:false,folder:String::new(),platform:crate::profiles::Platform::Unknown,simulation:false,options:ProtocolOptions::Ssh{identity_file:None,agent:false,known_hosts_file:None,config_file:None}};
            if let Err(e)=profile.validate(){output.buffer().set_text(&e);return;}
            let enabled=if kind.selected()==0{controller.0.preferences.borrow().ping_enabled}else{controller.0.preferences.borrow().traceroute_enabled};
            if !enabled{output.buffer().set_text("Outil désactivé dans les paramètres");return;}
            if simulated.is_active(){output.buffer().set_text("SIMULATION LOCALE\n4 réponses fictives, aucune latence mesurée.\nRoute fictive : poste → passerelle simulée → serveur IA simulé.\nAucun paquet réseau envoyé.");return;}
            if controller.0.offline.get() {output.buffer().set_text("Diagnostic réel bloqué par --offline");return;}
            output.buffer().set_text("Diagnostic en cours — limite 20 secondes…");
            button.set_sensitive(false);w.set_deletable(false);let b=button.clone();let w=w.clone();let out=output.clone();let ping=kind.selected()==0;
            io(move ||{
                let binary=if ping{"/usr/bin/ping"}else{"/usr/bin/traceroute"};let args=if ping{vec!["-n","-c","4","-W","2"]}else{vec!["-n","-m","12","-q","1","-w","1"]};
                let mut child=std::process::Command::new(binary).args(args).arg("--").arg(target).env_remove("LD_LIBRARY_PATH").env_remove("LD_PRELOAD").stdout(std::process::Stdio::piped()).stderr(std::process::Stdio::piped()).spawn().map_err(|_|"Outil absent : installez iputils-ping ou traceroute")?;
                let deadline=std::time::Instant::now()+std::time::Duration::from_secs(20);
                loop {if child.try_wait().map_err(|_|"Suivi impossible")?.is_some(){break;}
                if std::time::Instant::now()>deadline{let _=child.kill();let _=child.wait();return Err("Diagnostic arrêté après 20 secondes".into());}std::thread::sleep(std::time::Duration::from_millis(100));}
                let result=child.wait_with_output().map_err(|_|"Lecture du résultat impossible")?;
                Ok(format!("Code de sortie : {}\n{}\n{}",result.status,String::from_utf8_lossy(&result.stdout),String::from_utf8_lossy(&result.stderr)))
            },move |result|{b.set_sensitive(true);w.set_deletable(true);out.buffer().set_text(&result.unwrap_or_else(|e|e));});
        });
        let close = gtk::Button::with_label("Fermer le diagnostic");
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
