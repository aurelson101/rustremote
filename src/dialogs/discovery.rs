use super::*;
use crate::{
    discovery::{Device, Scope},
    profiles::{Platform, SshTunnel},
};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
impl Controller {
    fn discovered_editor(&self, device: &Device, kind: u32, simulated: bool) {
        let host = device.address.to_string();
        let (platform, port, options) = match kind {
            1 => (
                Platform::Windows,
                3389,
                ProtocolOptions::Rdp {
                    domain: String::new(),
                    width: 1280,
                    height: 720,
                    fullscreen: false,
                    clipboard: false,
                    clipboard_files: false,
                    settings: Default::default(),
                },
            ),
            2 => (
                Platform::MacOs,
                5900,
                ProtocolOptions::Vnc {
                    tunnel: Some(SshTunnel {
                        host: host.clone(),
                        port: 22,
                        username: String::new(),
                    }),
                    view_only: false,
                },
            ),
            3 => (
                Platform::Unknown,
                5900,
                ProtocolOptions::Vnc {
                    tunnel: Some(SshTunnel {
                        host: host.clone(),
                        port: 22,
                        username: String::new(),
                    }),
                    view_only: false,
                },
            ),
            _ => (
                if kind == 4 {
                    Platform::Linux
                } else {
                    Platform::Unknown
                },
                22,
                ProtocolOptions::Ssh {
                    identity_file: None,
                    agent: true,
                    known_hosts_file: None,
                    config_file: None,
                },
            ),
        };
        let seed = SessionProfile {
            name: format!(
                "{host} · {}",
                if kind == 1 {
                    "RDP"
                } else if kind == 2 {
                    "Apple"
                } else if kind == 3 {
                    "VNC"
                } else {
                    "SSH"
                }
            ),
            host,
            port,
            username: String::new(),
            credential_ref: None,
            favorite: false,
            folder: String::new(),
            platform,
            simulation: simulated,
            options,
        };
        self.editor_prefilled(None, 0, None, Some(seed));
    }
    pub fn discovery(&self) {
        let window = self.window("Découverte réseau", 820);
        window.set_default_height(650);
        let body = gtk::Box::new(gtk::Orientation::Vertical, 10);
        body.add_css_class("dialog-content");
        let info = gtk::Label::new(Some(
            "TCP 22/3389/5900/445 · IPv4 privé /24 maximum · 8 workers · 30 s. Aucune authentification. Le système reste à confirmer.",
        ));
        info.set_wrap(true);
        body.append(&info);
        let scope = gtk::Entry::new();
        scope.set_text("127.0.0.1");
        scope.set_max_length(64);
        scope.update_property(&[gtk::accessible::Property::Label(&crate::i18n::text(
            "Plage IPv4 autorisée",
        ))]);
        body.append(&scope);
        let simulated = gtk::CheckButton::with_label("Simulation locale — aucun paquet réseau");
        simulated.set_active(true);
        body.append(&simulated);
        let consent =
            gtk::CheckButton::with_label("Je suis autorisé à analyser cette plage réseau");
        body.append(&consent);
        let status = gtk::Label::new(None);
        status.set_wrap(true);
        body.append(&status);
        let run = gtk::Button::with_label("Rechercher les équipements");
        body.append(&run);
        let list = gtk::ListBox::new();
        list.set_selection_mode(gtk::SelectionMode::None);
        list.update_property(&[gtk::accessible::Property::Label(&crate::i18n::text(
            "Équipements détectés",
        ))]);
        body.append(
            &gtk::ScrolledWindow::builder()
                .child(&list)
                .min_content_height(220)
                .vexpand(true)
                .build(),
        );
        let cancel = gtk::Button::with_label("Annuler le scan");
        cancel.set_sensitive(false);
        body.append(&cancel);
        let stop = Arc::new(AtomicBool::new(false));
        let generation = Rc::new(Cell::new(0u64));
        let flag = stop.clone();
        cancel.connect_clicked(move |_| {
            flag.store(true, Ordering::Relaxed);
        });
        let flag = stop.clone();
        window.connect_hide(move |_| {
            flag.store(true, Ordering::Relaxed);
        });
        for field in [
            scope.clone().upcast::<gtk::Widget>(),
            simulated.clone().upcast(),
        ] {
            let consent = consent.clone();
            let list = list.clone();
            let status = status.clone();
            let generation = generation.clone();
            let stop = stop.clone();
            let reset = move || {
                generation.set(generation.get().wrapping_add(1));
                stop.store(true, Ordering::Relaxed);
                consent.set_active(false);
                while let Some(row) = list.first_child() {
                    list.remove(&row);
                }
                status.set_text("Paramètres modifiés · relancez la découverte");
            };
            if let Some(entry) = field.downcast_ref::<gtk::Entry>() {
                entry.connect_changed(move |_| reset());
            } else if let Some(toggle) = field.downcast_ref::<gtk::CheckButton>() {
                toggle.connect_toggled(move |_| reset());
            }
        }
        let c = self.clone();
        let weak_window = window.downgrade();
        run.connect_clicked(move |button| {
            let fake=simulated.is_active();
            let parsed=match Scope::parse(&scope.text()) {Ok(s)=>s,Err(e)=>{status.set_text(&e);return;}};
            if !fake && (!consent.is_active() || c.0.offline.get()) {
                status.set_text("Scan réel bloqué : autorisation de la plage requise et mode hors ligne désactivé.");return;
            }
            while let Some(row)=list.first_child(){list.remove(&row);}
            status.set_text(&format!("{} · {}",crate::i18n::text("Plage"),parsed.description()));
            button.set_sensitive(false);scope.set_sensitive(false);simulated.set_sensitive(false);consent.set_sensitive(false);cancel.set_sensitive(true);
            stop.store(false,Ordering::Relaxed);
            let version=generation.get();let generation=generation.clone();
            let flag=stop.clone();let controller=c.clone();let list=list.clone();let status=status.clone();let run=button.clone();let cancel=cancel.clone();let scope=scope.clone();let simulated=simulated.clone();let consent=consent.clone();let weak_window=weak_window.clone();
            io(move ||if fake {Ok(crate::discovery::Report::simulation())}else{crate::discovery::scan(parsed,&flag)},move |result| {
                if weak_window.upgrade().is_none_or(|w|!w.is_visible()){return;}
                run.set_sensitive(true);scope.set_sensitive(true);simulated.set_sensitive(true);consent.set_sensitive(true);cancel.set_sensitive(false);
                if generation.get()!=version {return;}
                match result {
                    Err(e)=>status.set_text(&e),
                    Ok(report)=> {
                        status.set_text(&format!("{} · {}/{} · {} · {}",crate::i18n::text(report.label()),report.checked,report.total,report.devices.len(),crate::i18n::text("Clic droit pour créer un profil ; système à confirmer")));
                        for device in report.devices {
                            let row=gtk::Box::new(gtk::Orientation::Horizontal,8);
                            let label=gtk::Label::new(Some(&format!("{} · {:?} · {}",device.address,device.ports,crate::i18n::text(device.hint()))));
                            label.set_wrap(true);label.set_xalign(0.0);label.set_hexpand(true);label.add_css_class("user-content");row.append(&label);
                            let add=gtk::Button::with_label(&crate::i18n::text("Ajouter comme connexion…"));row.append(&add);
                            let controller=controller.clone();let anchor=row.downgrade();
                            let menu=std::rc::Rc::new(move || {
                                let Some(anchor)=anchor.upgrade() else{return;};
                                let popover=gtk::Popover::new();popover.set_parent(&anchor);
                                let choices=gtk::Box::new(gtk::Orientation::Vertical,4);
                                for (title,kind) in [("Ajouter en SSH…",0),("Ajouter en Linux / SSH…",4),("Ajouter en Windows / RDP…",1),("Ajouter en Apple / ARD…",2),("Ajouter en VNC…",3)] {
                                    let action=gtk::Button::with_label(title);let c=controller.clone();let device=device.clone();let popup=popover.downgrade();
                                    action.connect_clicked(move |_| {if let Some(p)=popup.upgrade(){p.popdown();}c.discovered_editor(&device,kind,fake);});choices.append(&action);
                                }
                                popover.set_child(Some(&choices));popover.connect_closed(crate::dialogs::defer_unparent);crate::i18n::translate(popover.upcast_ref());popover.popup();
                            });
                            let callback=menu.clone();add.connect_clicked(move |_|callback());
                            let gesture=gtk::GestureClick::new();gesture.set_button(3);gesture.connect_pressed(move |g,_,_,_| {g.set_state(gtk::EventSequenceState::Claimed);menu();});row.add_controller(gesture);
                            list.append(&row);
                        }
                    }
                }
            });
        });
        let close = gtk::Button::with_label("Fermer");
        let w = window.downgrade();
        close.connect_clicked(move |_| {
            if let Some(w) = w.upgrade() {
                w.close();
            }
        });
        body.append(&close);
        window.set_child(Some(&body));
        crate::i18n::present(&window);
    }
}
