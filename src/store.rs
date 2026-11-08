#[cfg(test)]
use crate::profiles::SessionProfile;
use crate::profiles::Workspace;
use age::secrecy::SecretString;
use std::io::{Read, Write};
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
use std::path::{Path, PathBuf};

pub const MAX_SIZE: u64 = 2 * 1024 * 1024;
#[derive(Clone)]
pub struct Store {
    pub path: PathBuf,
    password: Option<SecretString>,
}

impl Store {
    pub fn directory() -> Result<PathBuf, String> {
        let base = std::env::var_os("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .filter(|p| p.is_absolute())
            .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))
            .ok_or("Répertoire de configuration introuvable")?;
        Ok(base.join("rustremote"))
    }
    pub fn new(path: PathBuf, password: Option<SecretString>) -> Self {
        Self { path, password }
    }
    pub fn encrypted(&self) -> bool {
        self.password.is_some()
    }
    pub fn load(&self) -> Result<Workspace, String> {
        let mut file = std::fs::OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
            .open(&self.path)
            .map_err(|_| "Impossible d’ouvrir les profils (liens symboliques refusés).")?;
        if !file
            .metadata()
            .map_err(|_| "Métadonnées illisibles")?
            .is_file()
        {
            return Err("Fichier de profils invalide".into());
        }
        let mut bytes = Vec::new();
        Read::by_ref(&mut file)
            .take(MAX_SIZE + 1)
            .read_to_end(&mut bytes)
            .map_err(|_| "Lecture impossible")?;
        if bytes.len() as u64 > MAX_SIZE {
            return Err("Fichier de profils trop volumineux".into());
        }
        let data = if let Some(password) = &self.password {
            let decryptor =
                age::Decryptor::new(&bytes[..]).map_err(|_| "Fichier chiffré invalide")?;
            let mut identity = age::scrypt::Identity::new(password.clone());
            identity.set_max_work_factor(20);
            let mut reader = decryptor
                .decrypt(std::iter::once(&identity as &dyn age::Identity))
                .map_err(|_| "Phrase secrète incorrecte ou fichier endommagé")?;
            let mut plain = Vec::new();
            reader
                .by_ref()
                .take(MAX_SIZE + 1)
                .read_to_end(&mut plain)
                .map_err(|_| "Authentification du fichier impossible")?;
            plain
        } else {
            bytes
        };
        Workspace::decode(&data)
    }
    pub fn save(&self, workspace: &Workspace, create: bool) -> Result<(), String> {
        workspace.validate()?;
        let dir = self.path.parent().ok_or("Chemin invalide")?;
        std::fs::create_dir_all(dir).map_err(|_| "Création du dossier impossible")?;
        if std::fs::symlink_metadata(dir)
            .map_err(|_| "Dossier inaccessible")?
            .file_type()
            .is_symlink()
        {
            return Err("Dossier symbolique refusé".into());
        }
        std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o700))
            .map_err(|_| "Permissions du dossier impossibles")?;
        let plain = serde_json::to_vec(workspace).map_err(|_| "Sérialisation impossible")?;
        if plain.len() as u64 > MAX_SIZE {
            return Err("Trop de profils".into());
        }
        let mut tmp =
            tempfile::NamedTempFile::new_in(dir).map_err(|_| "Fichier temporaire impossible")?;
        if let Some(password) = &self.password {
            let encryptor = age::Encryptor::with_user_passphrase(password.clone());
            let mut writer = encryptor
                .wrap_output(tmp.as_file_mut())
                .map_err(|_| "Chiffrement impossible")?;
            writer
                .write_all(&plain)
                .map_err(|_| "Écriture impossible")?;
            writer
                .finish()
                .map_err(|_| "Finalisation du chiffrement impossible")?;
        } else {
            tmp.write_all(&plain).map_err(|_| "Écriture impossible")?;
        }
        tmp.as_file()
            .sync_all()
            .map_err(|_| "Synchronisation impossible")?;
        if create {
            tmp.persist_noclobber(&self.path)
                .map_err(|_| "Le fichier existe déjà ou ne peut être créé")?;
        } else {
            tmp.persist(&self.path)
                .map_err(|_| "Enregistrement impossible")?;
        }
        std::fs::File::open(dir)
            .and_then(|f| f.sync_all())
            .map_err(|_| "Synchronisation du dossier impossible")?;
        Ok(())
    }
}

pub fn existing(dir: &Path) -> Result<Option<bool>, String> {
    let encrypted = dir.join("profiles.age").symlink_metadata().is_ok();
    let plain = dir.join("profiles.json").symlink_metadata().is_ok();
    match (encrypted, plain) {
        (true, true) => Err(
            "Deux magasins détectés : résolution manuelle requise, aucun fichier écrasé.".into(),
        ),
        (true, false) => Ok(Some(true)),
        (false, true) => Ok(Some(false)),
        _ => Ok(None),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn plain_permissions_and_no_clobber() {
        let d = tempfile::tempdir().unwrap();
        let s = Store::new(d.path().join("profiles.json"), None);
        s.save(&Workspace::default(), true).unwrap();
        assert!(s.save(&Workspace::default(), true).is_err());
        assert!(s.load().unwrap().profiles.is_empty());
        assert_eq!(
            std::fs::metadata(&s.path).unwrap().permissions().mode() & 0o777,
            0o600
        );
    }
    #[test]
    fn encrypted_roundtrip_wrong_password_tampering() {
        let d = tempfile::tempdir().unwrap();
        let path = d.path().join("profiles.age");
        let s = Store::new(
            path.clone(),
            Some(SecretString::from("test-only-passphrase".to_string())),
        );
        let profile = SessionProfile {
            name: "Fixture".into(),
            host: "example.invalid".into(),
            port: 22,
            username: "demo".into(),
            credential_ref: None,
            favorite: false,
            folder: String::new(),
            platform: crate::profiles::Platform::Unknown,
            simulation: false,
            options: crate::profiles::ProtocolOptions::Ssh {
                identity_file: None,
                agent: true,
                known_hosts_file: None,
                config_file: None,
            },
        };
        s.save(
            &Workspace {
                profiles: vec![profile.clone()],
                ..Workspace::default()
            },
            true,
        )
        .unwrap();
        assert_eq!(s.load().unwrap().profiles, vec![profile]);
        let ciphertext = std::fs::read(&path).unwrap();
        assert!(!String::from_utf8_lossy(&ciphertext).contains("example.invalid"));
        assert!(
            Store::new(
                path.clone(),
                Some(SecretString::from("incorrect-test-value".to_string()))
            )
            .load()
            .is_err()
        );
        let mut data = std::fs::read(&path).unwrap();
        let last = data.len() - 1;
        data[last] ^= 1;
        std::fs::write(path, data).unwrap();
        assert!(s.load().is_err());
    }
    #[test]
    fn rejects_invalid_and_oversized_data() {
        let d = tempfile::tempdir().unwrap();
        let path = d.path().join("profiles.json");
        std::fs::write(&path, b"not-json").unwrap();
        let store = Store::new(path.clone(), None);
        assert!(store.load().is_err());
        let file = std::fs::File::create(path).unwrap();
        file.set_len(MAX_SIZE + 1).unwrap();
        assert!(store.load().is_err());
    }
    #[test]
    fn rejects_symlink_and_conflicting_stores() {
        let d = tempfile::tempdir().unwrap();
        std::fs::write(d.path().join("target"), b"[]").unwrap();
        let path = d.path().join("profiles.json");
        std::os::unix::fs::symlink(d.path().join("target"), &path).unwrap();
        assert!(Store::new(path, None).load().is_err());
        std::fs::write(d.path().join("profiles.age"), b"invalid").unwrap();
        assert!(existing(d.path()).is_err());
    }
}

/// Verrou de session partagé par tous les magasins d'un même dossier.
pub fn lock_directory(dir: &Path) -> Result<std::fs::File, String> {
    std::fs::create_dir_all(dir).map_err(|_| "Dossier de profils inaccessible")?;
    if std::fs::symlink_metadata(dir)
        .map_err(|_| "Dossier inaccessible")?
        .file_type()
        .is_symlink()
    {
        return Err("Dossier symbolique refusé".into());
    }
    std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o700))
        .map_err(|_| "Permissions privées impossibles")?;
    let file = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .mode(0o600)
        .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
        .open(dir.join(".profiles.lock"))
        .map_err(|_| "Verrou de profils inaccessible")?;
    if !file.metadata().map_err(|_| "Verrou illisible")?.is_file() {
        return Err("Verrou de type invalide".into());
    }
    file.try_lock()
        .map_err(|_| "Une autre instance utilise ces profils. Fermez-la avant de continuer.")?;
    Ok(file)
}

#[cfg(test)]
mod lock_tests {
    use super::*;
    #[test]
    fn exclusive_lock_releases_and_rejects_symlink() {
        let temp = tempfile::tempdir().unwrap();
        let lock = lock_directory(temp.path()).unwrap();
        assert!(lock_directory(temp.path()).is_err());
        drop(lock);
        drop(lock_directory(temp.path()).unwrap());
        let alias = temp.path().join("alias");
        std::os::unix::fs::symlink(temp.path(), &alias).unwrap();
        assert!(lock_directory(&alias).is_err());
    }
}

pub fn export_json(path: &Path, data: &Workspace) -> Result<(), String> {
    data.validate()?;
    if !path.is_absolute() {
        return Err("Choisissez un chemin absolu pour l’export.".into());
    }
    let mut exported = data.clone();
    for profile in &mut exported.profiles {
        profile.credential_ref = None;
    }
    let bytes = serde_json::to_vec_pretty(&exported).map_err(|_| "Export impossible")?;
    if bytes.len() as u64 > MAX_SIZE {
        return Err("Export trop volumineux (2 Mio maximum).".into());
    }
    let mut temp = tempfile::NamedTempFile::new_in(path.parent().ok_or("Chemin invalide")?)
        .map_err(|_| "Destination inaccessible")?;
    temp.write_all(&bytes)
        .map_err(|_| "Écriture de l’export impossible")?;
    temp.as_file()
        .sync_all()
        .map_err(|_| "Synchronisation impossible")?;
    temp.persist_noclobber(path)
        .map_err(|_| "Destination existante ou export impossible : aucun remplacement effectué.")?;
    Ok(())
}

#[cfg(test)]
mod export_tests {
    use super::*;
    #[test]
    fn export_omits_credential_references() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("export.json");
        let mut data =
            Workspace::decode(include_bytes!("../assets/simulations/serveur-ia.json")).unwrap();
        let reference = "e0a306ec-71ee-44d0-bc89-dcf7f657135a";
        data.profiles[0].credential_ref = Some(reference.into());
        export_json(&path, &data).unwrap();
        let bytes = std::fs::read(path).unwrap();
        assert!(!String::from_utf8_lossy(&bytes).contains(reference));
        assert!(
            Workspace::decode(&bytes).unwrap().profiles[0]
                .credential_ref
                .is_none()
        );
        assert!(data.profiles[0].credential_ref.is_some());
    }
    #[test]
    fn export_reimport_and_no_overwrite() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("export.json");
        let data =
            Workspace::decode(include_bytes!("../assets/simulations/serveur-ia.json")).unwrap();
        export_json(&path, &data).unwrap();
        let content = std::fs::read(&path).unwrap();
        assert_eq!(Workspace::decode(&content).unwrap(), data);
        assert_eq!(
            std::fs::metadata(&path).unwrap().permissions().mode() & 0o777,
            0o600
        );
        assert!(export_json(&path, &Workspace::default()).is_err());
        assert_eq!(std::fs::read(path).unwrap(), content);
    }
}
