use super::*;
use crate::profiles::RdpSettings;

pub(super) struct RdpForm {
    pub widget: gtk::Notebook,
    domain: gtk::Entry,
    width: gtk::Entry,
    height: gtk::Entry,
    monitors: gtk::Entry,
    pin: gtk::Entry,
    size_mode: gtk::DropDown,
    audio: gtk::DropDown,
    fullscreen: gtk::ToggleButton,
    multi: gtk::ToggleButton,
    microphone: gtk::ToggleButton,
    printers: gtk::ToggleButton,
    clipboard: gtk::ToggleButton,
    files: gtk::ToggleButton,
}
impl RdpForm {
    pub fn new(original: Option<&SessionProfile>, domain: &gtk::Entry) -> Self {
        fn field(grid: &gtk::Grid, row: i32, name: &str, value: &str) -> gtk::Entry {
            let label = gtk::Label::new(Some(name));
            label.set_xalign(0.0);
            label.set_wrap(true);
            label.set_max_width_chars(22);
            grid.attach(&label, 0, row, 1, 1);
            let entry = gtk::Entry::new();
            entry.set_width_chars(12);
            entry.set_text(value);
            entry.set_hexpand(true);
            entry.set_max_length(256);
            entry.update_property(&[gtk::accessible::Property::Label(name)]);
            grid.attach(&entry, 1, row, 1, 1);
            entry
        }
        fn grid() -> gtk::Grid {
            let g = gtk::Grid::new();
            g.set_row_spacing(10);
            g.set_column_spacing(12);
            g.set_margin_top(12);
            g.set_margin_bottom(12);
            g
        }
        let widget = gtk::Notebook::new();
        widget.set_scrollable(true);
        let display = grid();
        let width = field(&display, 0, "Largeur", "1280");
        let height = field(&display, 1, "Hauteur", "720");
        let size_mode = crate::i18n::dropdown(&[
            "Taille fixe",
            "Automatique (résolution dynamique)",
            "Adapter l’image (smart sizing)",
        ]);
        size_mode.set_selected(1);
        size_mode.update_property(&[gtk::accessible::Property::Label(&crate::i18n::text(
            "Taille RDP",
        ))]);
        display.attach(&size_mode, 0, 2, 2, 1);
        let fullscreen = gtk::ToggleButton::with_label("Plein écran");
        display.attach(&fullscreen, 0, 3, 2, 1);
        let multi = gtk::ToggleButton::with_label("Multi-écrans (plein écran)");
        display.attach(&multi, 0, 4, 2, 1);
        let monitors = field(&display, 5, "Écrans (vide = tous)", "");
        monitors.set_placeholder_text(Some("0,1 pour deux écrans"));
        monitors.set_sensitive(false);
        let m = monitors.clone();
        multi.connect_toggled(move |b| m.set_sensitive(b.is_active()));
        widget.append_page(&display, Some(&gtk::Label::new(Some("Affichage RDP"))));
        let devices = gtk::Box::new(gtk::Orientation::Vertical, 10);
        devices.set_margin_top(12);
        devices.append(&gtk::Label::new(Some("Sortie audio :")));
        let audio = crate::i18n::dropdown(&["Sur ce poste", "Sur le serveur", "Désactivée"]);
        audio.set_selected(2);
        audio.update_property(&[gtk::accessible::Property::Label(&crate::i18n::text(
            "Audio RDP",
        ))]);
        devices.append(&audio);
        let microphone = gtk::ToggleButton::with_label("Partager le microphone local (opt-in)");
        devices.append(&microphone);
        let printers = gtk::ToggleButton::with_label("Rediriger les imprimantes locales (opt-in)");
        devices.append(&printers);
        let clipboard = gtk::ToggleButton::with_label("Partager le presse-papiers (opt-in)");
        devices.append(&clipboard);
        let files = gtk::ToggleButton::with_label("Copier fichiers/dossiers RDP (opt-in)");
        files.set_sensitive(false);
        devices.append(&files);
        let f = files.clone();
        clipboard.connect_toggled(move |b| {
            f.set_sensitive(b.is_active());
            if !b.is_active() {
                f.set_active(false);
            }
        });
        widget.append_page(&devices, Some(&gtk::Label::new(Some("Périphériques RDP"))));
        let security = grid();
        let domain = domain.clone();
        let pin = field(&security, 0, "Empreinte SHA256 du certificat", "");
        pin.set_placeholder_text(Some("Facultatif : 64 caractères hexadécimaux"));
        let note = gtk::Label::new(Some(
            "NLA requis. Sans empreinte, le certificat doit être validé par le système. Vérifiez une empreinte par un canal indépendant avant de l’enregistrer.",
        ));
        note.set_wrap(true);
        security.attach(&note, 0, 1, 2, 1);
        widget.append_page(&security, Some(&gtk::Label::new(Some("Sécurité RDP"))));
        widget.set_current_page(Some(2));
        let result = Self {
            widget,
            domain,
            width,
            height,
            monitors,
            pin,
            size_mode,
            audio,
            fullscreen,
            multi,
            microphone,
            printers,
            clipboard,
            files,
        };
        if let Some(SessionProfile {
            options:
                ProtocolOptions::Rdp {
                    domain,
                    width,
                    height,
                    fullscreen,
                    clipboard,
                    clipboard_files,
                    settings,
                },
            ..
        }) = original
        {
            result.domain.set_text(domain);
            result.width.set_text(&width.to_string());
            result.height.set_text(&height.to_string());
            result.fullscreen.set_active(*fullscreen);
            result.clipboard.set_active(*clipboard);
            result.files.set_active(*clipboard_files);
            result.size_mode.set_selected(settings.size_mode.into());
            result.multi.set_active(settings.multi_monitor);
            result.monitors.set_text(
                &settings
                    .monitors
                    .iter()
                    .map(u16::to_string)
                    .collect::<Vec<_>>()
                    .join(","),
            );
            result.audio.set_selected(settings.audio_mode.into());
            result.microphone.set_active(settings.microphone);
            result.printers.set_active(settings.printers);
            if let Some(pin) = &settings.certificate_sha256 {
                result.pin.set_text(pin);
            }
        }
        result
    }
    pub fn options(&self) -> Result<ProtocolOptions, String> {
        let monitors = if self.multi.is_active() && !self.monitors.text().trim().is_empty() {
            self.monitors
                .text()
                .split(',')
                .map(|v| {
                    v.trim()
                        .parse::<u16>()
                        .map_err(|_| "Écrans invalides : utilisez par exemple 0,1.".to_string())
                })
                .collect::<Result<Vec<_>, _>>()?
        } else {
            Vec::new()
        };
        let pin: String = self
            .pin
            .text()
            .chars()
            .filter(|c| *c != ':' && !c.is_whitespace())
            .collect();
        let settings = RdpSettings {
            size_mode: self.size_mode.selected() as u8,
            multi_monitor: self.multi.is_active(),
            monitors,
            audio_mode: self.audio.selected() as u8,
            microphone: self.microphone.is_active(),
            printers: self.printers.is_active(),
            certificate_sha256: if pin.is_empty() { None } else { Some(pin) },
        };
        settings.validate()?;
        Ok(ProtocolOptions::Rdp {
            domain: self.domain.text().to_string(),
            width: self.width.text().parse().unwrap_or(0),
            height: self.height.text().parse().unwrap_or(0),
            fullscreen: self.fullscreen.is_active(),
            clipboard: self.clipboard.is_active(),
            clipboard_files: self.files.is_active(),
            settings,
        })
    }
}
