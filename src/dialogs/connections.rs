use super::*;
use crate::profiles::SshTunnel;

pub(super) struct ConnectionsForm {
    pub vnc: gtk::Box,
    pub serial: gtk::Box,
    tunnel: gtk::ToggleButton,
    gateway: gtk::Entry,
    gateway_port: gtk::Entry,
    gateway_user: gtk::Entry,
    view_only: gtk::ToggleButton,
    device: gtk::Entry,
    baud: gtk::Entry,
    bits: gtk::Entry,
    parity: gtk::Entry,
    stop: gtk::Entry,
    flow: gtk::Entry,
}
impl ConnectionsForm {
    pub fn new(original: Option<&SessionProfile>) -> Self {
        fn field(body: &gtk::Box, title: &str, value: &str) -> gtk::Entry {
            let row = gtk::Box::new(gtk::Orientation::Horizontal, 8);
            let label = gtk::Label::new(Some(title));
            label.set_xalign(0.0);
            row.append(&label);
            let entry = gtk::Entry::new();
            entry.set_hexpand(true);
            entry.set_text(value);
            entry.set_max_length(256);
            entry.update_property(&[gtk::accessible::Property::Label(&crate::i18n::text(title))]);
            row.append(&entry);
            body.append(&row);
            entry
        }
        let vnc = gtk::Box::new(gtk::Orientation::Vertical, 8);
        let tunnel = gtk::ToggleButton::with_label("Sécuriser VNC par un tunnel SSH");
        tunnel.set_active(true);
        vnc.append(&tunnel);
        let gateway_box = gtk::Box::new(gtk::Orientation::Vertical, 8);
        let gateway = field(&gateway_box, "Passerelle SSH", "");
        let gateway_port = field(&gateway_box, "Port de la passerelle SSH", "22");
        let gateway_user = field(&gateway_box, "Utilisateur de la passerelle SSH", "");
        vnc.append(&gateway_box);
        let b = gateway_box.clone();
        tunnel.connect_toggled(move |t| b.set_sensitive(t.is_active()));
        let note = gtk::Label::new(Some(
            "Tunnel : SSH Agent et ~/.ssh/config ; empreinte hôte à vérifier. L’hôte VNC est vu depuis la passerelle. Sans tunnel : certificat X509 signé requis. Aucun mot de passe enregistré.",
        ));
        note.set_wrap(true);
        vnc.append(&note);
        let apple = gtk::Label::new(Some(
            "macOS : choisir macOS / Apple, VNC et le même Mac comme passerelle SSH. Activez Partage d’écran et Connexion à distance sur le Mac. TigerVNC 1.14+ demande le compte macOS et son mot de passe dans sa fenêtre ; aucun repli VNC ni mot de passe exporté.",
        ));
        apple.set_wrap(true);
        vnc.append(&apple);
        let view_only = gtk::ToggleButton::with_label("VNC en lecture seule");
        vnc.append(&view_only);
        let serial = gtk::Box::new(gtk::Orientation::Vertical, 8);
        let device = field(&serial, "Périphérique série", "/dev/ttyUSB0");
        let baud = field(&serial, "Débit série", "115200");
        let bits = field(&serial, "Bits de données", "8");
        let parity = field(&serial, "Parité (n/e/o)", "n");
        let stop = field(&serial, "Bits d’arrêt", "1");
        let flow = field(&serial, "Contrôle de flux (n/h/s)", "n");
        let note = gtk::Label::new(Some(
            "picocom : quitter avec Ctrl+A puis Ctrl+X. Les permissions du port restent celles du système ; aucun sudo automatique.",
        ));
        note.set_wrap(true);
        serial.append(&note);
        let result = Self {
            vnc,
            serial,
            tunnel,
            gateway,
            gateway_port,
            gateway_user,
            view_only,
            device,
            baud,
            bits,
            parity,
            stop,
            flow,
        };
        if let Some(profile) = original {
            match &profile.options {
                ProtocolOptions::Vnc { tunnel, view_only } => {
                    result.tunnel.set_active(tunnel.is_some());
                    result.view_only.set_active(*view_only);
                    if let Some(tunnel) = tunnel {
                        result.gateway.set_text(&tunnel.host);
                        result.gateway_port.set_text(&tunnel.port.to_string());
                        result.gateway_user.set_text(&tunnel.username);
                    }
                }
                ProtocolOptions::Serial {
                    device,
                    baud,
                    data_bits,
                    parity,
                    stop_bits,
                    flow,
                } => {
                    for (entry, value) in [
                        (&result.device, device.clone()),
                        (&result.baud, baud.to_string()),
                        (&result.bits, data_bits.to_string()),
                        (&result.parity, parity.clone()),
                        (&result.stop, stop_bits.to_string()),
                        (&result.flow, flow.clone()),
                    ] {
                        entry.set_text(&value);
                    }
                }
                _ => {}
            }
        }
        result
    }
    pub fn options(&self, protocol: u32) -> ProtocolOptions {
        if protocol == 2 {
            ProtocolOptions::Vnc {
                tunnel: self.tunnel.is_active().then(|| SshTunnel {
                    host: self.gateway.text().trim().into(),
                    port: self.gateway_port.text().parse().unwrap_or(0),
                    username: self.gateway_user.text().trim().into(),
                }),
                view_only: self.view_only.is_active(),
            }
        } else {
            ProtocolOptions::Serial {
                device: self.device.text().trim().into(),
                baud: self.baud.text().parse().unwrap_or(0),
                data_bits: self.bits.text().parse().unwrap_or(0),
                parity: self.parity.text().into(),
                stop_bits: self.stop.text().parse().unwrap_or(0),
                flow: self.flow.text().into(),
            }
        }
    }
}
