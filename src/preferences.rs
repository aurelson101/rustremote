use serde::{Deserialize, Serialize};
use std::{
    fs,
    io::{Read, Write},
    os::unix::fs::{OpenOptionsExt, PermissionsExt},
    path::Path,
};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Preferences {
    pub version: u8,
    pub sftp: crate::sftp::Limits,
    pub language: String,
    pub font_size: u8,
    pub scrollback: u32,
    pub black_terminal: bool,
    pub ping_enabled: bool,
    pub traceroute_enabled: bool,
}
impl Default for Preferences {
    fn default() -> Self {
        Self {
            version: 1,
            sftp: Default::default(),
            language: "en".into(),
            font_size: 11,
            scrollback: 5000,
            black_terminal: false,
            ping_enabled: true,
            traceroute_enabled: true,
        }
    }
}
impl Preferences {
    pub fn validate(&self) -> Result<(), String> {
        self.sftp.validate()?;
        if !matches!(self.language.as_str(), "en" | "fr")
            || self.version != 1
            || !(8..=24).contains(&self.font_size)
            || !(100..=20000).contains(&self.scrollback)
        {
            Err(
                "Paramètres invalides : police 8–24, historique 100–20 000 lignes, version 1"
                    .into(),
            )
        } else {
            Ok(())
        }
    }
    pub fn load(path: &Path) -> Result<Self, String> {
        let file = match fs::OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
            .open(path)
        {
            Ok(file) => file,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Self::default()),
            Err(_) => return Err("Paramètres illisibles ; aucun fichier n’a été remplacé".into()),
        };
        if !file
            .metadata()
            .map_err(|_| "Lecture des paramètres impossible")?
            .is_file()
        {
            return Err("Les paramètres doivent être un fichier régulier".into());
        }
        let mut bytes = Vec::new();
        file.take(16385)
            .read_to_end(&mut bytes)
            .map_err(|_| "Lecture des paramètres impossible")?;
        if bytes.len() > 16384 {
            return Err("Fichier de paramètres limité à 16 Kio".into());
        }
        let data: Self = serde_json::from_slice(&bytes)
            .map_err(|_| "Format des paramètres non pris en charge")?;
        data.validate()?;
        Ok(data)
    }
    pub fn save(&self, path: &Path) -> Result<(), String> {
        self.validate()?;
        if let Ok(meta) = fs::symlink_metadata(path)
            && !meta.is_file()
        {
            return Err("Destination des paramètres non régulière : enregistrement refusé".into());
        }
        let parent = path.parent().ok_or("Dossier de paramètres invalide")?;
        fs::create_dir_all(parent).map_err(|_| "Création du dossier des paramètres impossible")?;
        fs::set_permissions(parent, fs::Permissions::from_mode(0o700))
            .map_err(|_| "Permissions des paramètres impossibles")?;
        let bytes = serde_json::to_vec_pretty(self).map_err(|_| "Sérialisation impossible")?;
        let mut file = tempfile::NamedTempFile::new_in(parent)
            .map_err(|_| "Préparation des paramètres impossible")?;
        file.write_all(&bytes)
            .and_then(|_| file.as_file().sync_all())
            .map_err(|_| "Écriture des paramètres impossible")?;
        file.persist(path)
            .map_err(|_| "Publication atomique des paramètres impossible")?;
        Ok(())
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn language_defaults_and_legacy_settings_are_compatible() {
        let old: Preferences = serde_json::from_str(r#"{"version":1,"font_size":11}"#).unwrap();
        assert_eq!(old.language, "en");
        let fr = Preferences {
            language: "fr".into(),
            ..old
        };
        fr.validate().unwrap();
        let bytes = serde_json::to_vec(&fr).unwrap();
        assert_eq!(serde_json::from_slice::<Preferences>(&bytes).unwrap(), fr);
        assert!(
            Preferences {
                language: "invalid".into(),
                ..fr
            }
            .validate()
            .is_err()
        );
    }
    #[test]
    fn private_roundtrip_and_reject_unsafe_settings() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("config/settings.json");
        let preferences = Preferences {
            font_size: 14,
            scrollback: 1000,
            sftp: crate::sftp::Limits {
                mib: 32,
                entries: 256,
                depth: 12,
                seconds: 60,
            },
            ..Preferences::default()
        };
        preferences.save(&path).unwrap();
        assert_eq!(Preferences::load(&path).unwrap(), preferences);
        assert_eq!(
            fs::metadata(&path).unwrap().permissions().mode() & 0o777,
            0o600
        );
        assert_eq!(
            fs::metadata(path.parent().unwrap())
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o700
        );
        fs::write(&path, b"{\"password\":\"rejected\"}").unwrap();
        assert!(Preferences::load(&path).is_err());
        assert!(
            Preferences {
                scrollback: u32::MAX,
                ..preferences
            }
            .validate()
            .is_err()
        );
        let link = root.path().join("link");
        std::os::unix::fs::symlink(&path, &link).unwrap();
        assert!(Preferences::load(&link).is_err());
        assert!(Preferences::default().save(&link).is_err());
    }
}
