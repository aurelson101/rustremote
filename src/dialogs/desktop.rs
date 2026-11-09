use super::*;
impl Controller {
    pub fn system_information(&self) {
        fn env(name: &str) -> String {
            std::env::var(name)
                .unwrap_or_else(|_| "—".into())
                .chars()
                .filter(|c| !c.is_control())
                .take(160)
                .collect()
        }
        let clients = [
            ("SSH", "/usr/bin/ssh"),
            ("RDP", crate::profiles::freerdp_binary()),
            ("VNC", "/usr/bin/vncviewer"),
            ("Serial", "/usr/bin/picocom"),
        ]
        .iter()
        .map(|(name, path)| {
            format!(
                "{name}: {}",
                if std::path::Path::new(path).is_file() {
                    "✓"
                } else {
                    "—"
                }
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
        self.information(&crate::i18n::text("Informations système"), &format!("RustRemote {}\nGTK {}.{}.{}\n\nDesktop: {}\nSession: {}\n\n{clients}\n\n{}",env!("CARGO_PKG_VERSION"),gtk::major_version(),gtk::minor_version(),gtk::micro_version(),env("XDG_CURRENT_DESKTOP"),env("XDG_SESSION_TYPE"),crate::i18n::text("Coffre : service Secret Service requis (GNOME Keyring ou KWallet compatible). Aucun service n’est installé automatiquement.")));
    }
    pub fn open_profile_directory(&self) {
        let path = match Store::directory() {
            Ok(p) => p,
            Err(e) => {
                self.information("Dossier des profils", &e);
                return;
            }
        };
        if !path.is_dir() {
            self.information("Dossier des profils", "Créez ou ouvrez d’abord vos profils");
            return;
        }
        let uri = gtk::gio::File::for_path(path).uri();
        let context = gtk::gio::AppLaunchContext::new();
        for name in [
            "LD_LIBRARY_PATH",
            "LD_PRELOAD",
            "GDK_PIXBUF_MODULE_FILE",
            "GIO_MODULE_DIR",
            "GTK_PATH",
        ] {
            context.unsetenv(name);
        }
        if gtk::gio::AppInfo::launch_default_for_uri(&uri, Some(&context)).is_err() {
            self.information(
                "Gestionnaire de fichiers",
                "Aucun gestionnaire de fichiers disponible",
            );
        }
    }
    pub fn public_key_fingerprint(&self) {
        let window = self.window("Empreinte de clé publique SSH", 560);
        let body = gtk::Box::new(gtk::Orientation::Vertical, 10);
        body.add_css_class("dialog-content");
        let path = gtk::Entry::new();
        path.set_placeholder_text(Some("Clé publique SSH"));
        path.set_tooltip_text(Some("Chemin absolu du fichier .pub"));
        path.update_property(&[gtk::accessible::Property::Label(&crate::i18n::text(
            "Clé publique SSH",
        ))]);
        body.append(&path);
        let output = gtk::Label::new(None);
        output.set_wrap(true);
        output.set_selectable(true);
        body.append(&output);
        let run = gtk::Button::with_label("Calculer l’empreinte SHA256");
        body.append(&run);
        run.connect_clicked(move |b| {
            let path = std::path::PathBuf::from(path.text().as_str());
            b.set_sensitive(false);
            let button = b.clone();
            let out = output.clone();
            io(
                move || {
                    use std::{
                        io::{Read, Write},
                        os::unix::fs::OpenOptionsExt,
                    };
                    if !path.is_absolute() || path.extension().is_none_or(|e| e != "pub") {
                        return Err("Un fichier de clé publique .pub est requis".into());
                    }
                    let file = std::fs::OpenOptions::new()
                        .read(true)
                        .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
                        .open(&path)
                        .map_err(|_| "Clé publique illisible")?;
                    if !file
                        .metadata()
                        .map_err(|_| "Métadonnées indisponibles")?
                        .is_file()
                    {
                        return Err("Fichier régulier requis".into());
                    }
                    let mut bytes = Vec::new();
                    file.take(16385)
                        .read_to_end(&mut bytes)
                        .map_err(|_| "Clé publique illisible")?;
                    if bytes.len() > 16384
                        || !(bytes.starts_with(b"ssh-")
                            || bytes.starts_with(b"ecdsa-")
                            || bytes.starts_with(b"sk-"))
                    {
                        return Err("Format de clé publique non pris en charge".into());
                    }
                    let mut temp = tempfile::NamedTempFile::new()
                        .map_err(|_| "Fichier temporaire indisponible")?;
                    temp.write_all(&bytes).map_err(|_| "Écriture impossible")?;
                    let result = std::process::Command::new("/usr/bin/ssh-keygen")
                        .args(["-l", "-E", "sha256", "-f"])
                        .arg(temp.path())
                        .env_remove("LD_LIBRARY_PATH")
                        .env_remove("LD_PRELOAD")
                        .output()
                        .map_err(|_| "ssh-keygen indisponible")?;
                    if !result.status.success() {
                        return Err("Clé publique invalide".into());
                    }
                    Ok(String::from_utf8_lossy(&result.stdout).into_owned())
                },
                move |result| {
                    button.set_sensitive(true);
                    out.set_text(&result.unwrap_or_else(|e| e));
                },
            );
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
