//! Transactional profile mutations; the UI never stores passwords in JSON.
use rustremote_core::profiles::{SessionProfile, Workspace};
use std::collections::HashSet;
use zeroize::Zeroizing;

pub trait Backend: Send + Sync {
    fn load(&self) -> Result<(Workspace, bool), String>;
    fn save(&self, data: &Workspace, encrypted: bool) -> Result<(), String>;
    fn credential_write(&self, profile: &SessionProfile, secret: &str) -> Result<String, String>;
    fn credential_remove(&self, reference: &str) -> Result<(), String>;
}

pub struct SecretChange {
    pub index: usize,
    pub value: Option<Zeroizing<String>>,
    pub forget: bool,
}
pub struct Saved {
    pub data: Workspace,
    pub warning: Option<String>,
}

pub fn save(
    backend: &dyn Backend,
    before: &Workspace,
    mut data: Workspace,
    encrypted: bool,
    secret: Option<SecretChange>,
) -> Result<Saved, String> {
    before.validate()?;
    data.validate()?;
    // A reference may only survive if it already belonged to this exact identity.
    for profile in &mut data.profiles {
        if let Some(reference) = &profile.credential_ref {
            let Ok(scope) = profile.credential_scope() else {
                profile.credential_ref = None;
                continue;
            };
            if !before.profiles.iter().any(|old| {
                old.credential_ref.as_ref() == Some(reference)
                    && old.credential_scope().ok().as_ref() == Some(&scope)
            }) {
                profile.credential_ref = None;
            }
        }
    }
    let mut created = None;
    if let Some(change) = secret {
        let profile = data
            .profiles
            .get_mut(change.index)
            .ok_or("Profile no longer exists")?;
        if change.forget {
            profile.credential_ref = None;
        }
        if let Some(value) = change.value {
            let reference = backend.credential_write(profile, &value)?;
            profile.credential_ref = Some(reference.clone());
            created = Some(reference);
        }
    }
    if let Err(error) = backend.save(&data, encrypted) {
        if let Some(reference) = created
            && backend.credential_remove(&reference).is_err()
        {
            return Err(format!("{error}; new vault entry could not be rolled back"));
        }
        return Err(error);
    }
    let keep: HashSet<_> = data
        .profiles
        .iter()
        .filter_map(|p| p.credential_ref.as_deref())
        .collect();
    let old: HashSet<_> = before
        .profiles
        .iter()
        .filter_map(|p| p.credential_ref.as_deref())
        .collect();
    let cleanup_failed = old.difference(&keep).fold(false, |failed, reference| {
        backend.credential_remove(reference).is_err() || failed
    });
    Ok(Saved {
        data,
        warning: cleanup_failed
            .then(|| "Profiles saved; an obsolete vault entry could not be removed".into()),
    })
}

#[cfg(windows)]
pub struct Native(pub crate::win_storage::Store);
#[cfg(windows)]
impl Backend for Native {
    fn load(&self) -> Result<(Workspace, bool), String> {
        self.0.load()
    }
    fn save(&self, data: &Workspace, encrypted: bool) -> Result<(), String> {
        self.0.save(data, encrypted)
    }
    fn credential_write(&self, profile: &SessionProfile, secret: &str) -> Result<String, String> {
        crate::win_credentials::write(profile, secret)
    }
    fn credential_remove(&self, reference: &str) -> Result<(), String> {
        crate::win_credentials::remove(reference)
    }
}

pub fn open() -> Result<std::sync::Arc<dyn Backend>, String> {
    #[cfg(windows)]
    {
        Ok(std::sync::Arc::new(Native(
            crate::win_storage::Store::open()?,
        )))
    }
    #[cfg(not(windows))]
    {
        Err("The Windows profile manager requires Windows".into())
    }
}

pub fn import(path: &std::path::Path) -> Result<Workspace, String> {
    use std::io::Read;
    if !std::fs::symlink_metadata(path)
        .map_err(|_| "Cannot inspect imported file")?
        .file_type()
        .is_file()
    {
        return Err("Import requires a regular JSON file".into());
    }
    let mut options = std::fs::OpenOptions::new();
    options.read(true);
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        options.custom_flags(windows_sys::Win32::Storage::FileSystem::FILE_FLAG_OPEN_REPARSE_POINT);
    }
    let file = options
        .open(path)
        .map_err(|_| "Cannot open imported profiles")?;
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        let metadata = file
            .metadata()
            .map_err(|_| "Cannot inspect imported profiles")?;
        if metadata.file_attributes()
            & windows_sys::Win32::Storage::FileSystem::FILE_ATTRIBUTE_REPARSE_POINT
            != 0
        {
            return Err("Reparse imports are refused".into());
        }
    }
    let mut bytes = Vec::new();
    file.take((rustremote_core::profiles::MAX_PROFILE_BYTES + 1) as u64)
        .read_to_end(&mut bytes)
        .map_err(|_| "Cannot read imported profiles")?;
    let mut workspace = Workspace::decode(&bytes)?;
    for profile in &mut workspace.profiles {
        profile.credential_ref = None;
    }
    Ok(workspace)
}

pub fn export(path: &std::path::Path, workspace: &Workspace) -> Result<(), String> {
    use std::io::Write;
    let bytes = workspace.export_json()?;
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(|_| "Export requires a new writable file; existing files are preserved")?;
    file.write_all(&bytes)
        .and_then(|_| file.sync_all())
        .map_err(|_| "Export failed; the new file may be incomplete".into())
}

/// Explicit GUI qualification only: never opens the user's production profile store.
pub fn laboratory() -> Result<ArcBackend, String> {
    let directory = tempfile::tempdir().map_err(|_| "Profile laboratory unavailable")?;
    #[cfg(windows)]
    let native = Native(crate::win_storage::Store::at(
        directory.path().join("Profiles"),
    )?);
    Ok(std::sync::Arc::new(Laboratory {
        #[cfg(windows)]
        native,
        references: std::sync::Mutex::new(Vec::new()),
        #[cfg(not(windows))]
        data: std::sync::Mutex::new((Workspace::default(), false)),
        _directory: directory,
    }))
}
type ArcBackend = std::sync::Arc<dyn Backend>;
struct Laboratory {
    #[cfg(windows)]
    native: Native,
    references: std::sync::Mutex<Vec<String>>,
    #[cfg(not(windows))]
    data: std::sync::Mutex<(Workspace, bool)>,
    _directory: tempfile::TempDir,
}
impl Drop for Laboratory {
    fn drop(&mut self) {
        #[cfg(windows)]
        if let Ok(references) = self.references.lock() {
            for reference in references.iter() {
                let _ = crate::win_credentials::remove(reference);
            }
        }
    }
}
impl Backend for Laboratory {
    fn load(&self) -> Result<(Workspace, bool), String> {
        #[cfg(windows)]
        {
            self.native.load()
        }
        #[cfg(not(windows))]
        {
            Ok(self.data.lock().map_err(|_| "Lab interrupted")?.clone())
        }
    }
    fn save(&self, data: &Workspace, encrypted: bool) -> Result<(), String> {
        #[cfg(windows)]
        {
            self.native.save(data, encrypted)
        }
        #[cfg(not(windows))]
        {
            *self.data.lock().map_err(|_| "Lab interrupted")? = (data.clone(), encrypted);
            Ok(())
        }
    }
    fn credential_write(&self, profile: &SessionProfile, secret: &str) -> Result<String, String> {
        #[cfg(windows)]
        let reference = self.native.credential_write(profile, secret)?;
        #[cfg(not(windows))]
        let reference = {
            let _ = (profile, secret);
            static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
            format!(
                "00000000-0000-4000-8000-{:012x}",
                NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
            )
        };
        self.references
            .lock()
            .map_err(|_| "Lab interrupted")?
            .push(reference.clone());
        Ok(reference)
    }
    fn credential_remove(&self, reference: &str) -> Result<(), String> {
        #[cfg(windows)]
        self.native.credential_remove(reference)?;
        self.references
            .lock()
            .map_err(|_| "Lab interrupted")?
            .retain(|r| r != reference);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{
        Mutex,
        atomic::{AtomicBool, Ordering},
    };
    #[derive(Default)]
    struct Fake {
        persisted: Mutex<Option<Workspace>>,
        references: Mutex<Vec<String>>,
        fail_save: AtomicBool,
        fail_remove: AtomicBool,
    }
    impl Backend for Fake {
        fn load(&self) -> Result<(Workspace, bool), String> {
            Ok((
                self.persisted.lock().unwrap().clone().unwrap_or_default(),
                false,
            ))
        }
        fn save(&self, data: &Workspace, _: bool) -> Result<(), String> {
            if self.fail_save.load(Ordering::Relaxed) {
                return Err("Injected write failure".into());
            }
            *self.persisted.lock().unwrap() = Some(data.clone());
            Ok(())
        }
        fn credential_write(&self, _: &SessionProfile, _: &str) -> Result<String, String> {
            let mut refs = self.references.lock().unwrap();
            let value = format!("00000000-0000-4000-8000-{:012x}", refs.len() + 1);
            refs.push(value.clone());
            Ok(value)
        }
        fn credential_remove(&self, reference: &str) -> Result<(), String> {
            if self.fail_remove.load(Ordering::Relaxed) {
                return Err("Injected vault failure".into());
            }
            self.references.lock().unwrap().retain(|r| r != reference);
            Ok(())
        }
    }
    fn data() -> Workspace {
        Workspace::decode(br#"[{"name":"Lab","host":"localhost","port":22,"username":"lab","favorite":false,"options":{"protocol":"Ssh","identity_file":null,"agent":true}}]"#).unwrap()
    }
    fn secret() -> SecretChange {
        SecretChange {
            index: 0,
            value: Some(Zeroizing::new(std::process::id().to_string())),
            forget: false,
        }
    }
    #[test]
    fn failed_profile_save_rolls_back_new_vault_entry_and_keeps_original() {
        let backend = Fake::default();
        let before = Workspace::default();
        let first = save(&backend, &before, data(), false, Some(secret()))
            .unwrap()
            .data;
        let old = first.profiles[0].credential_ref.clone().unwrap();
        backend.fail_save.store(true, Ordering::Relaxed);
        assert!(save(&backend, &first, first.clone(), true, Some(secret())).is_err());
        assert_eq!(*backend.references.lock().unwrap(), vec![old]);
        assert_eq!(backend.load().unwrap().0, first);
    }
    #[test]
    fn rename_keeps_secret_but_identity_change_and_delete_remove_it_after_save() {
        let backend = Fake::default();
        let first = save(
            &backend,
            &Workspace::default(),
            data(),
            false,
            Some(secret()),
        )
        .unwrap()
        .data;
        let mut renamed = first.clone();
        renamed.profiles[0].name = "Renamed".into();
        let second = save(&backend, &first, renamed, false, None).unwrap().data;
        assert_eq!(
            first.profiles[0].credential_ref,
            second.profiles[0].credential_ref
        );
        let mut changed = second.clone();
        changed.profiles[0].host = "127.0.0.2".into();
        let third = save(&backend, &second, changed, false, None).unwrap().data;
        assert!(third.profiles[0].credential_ref.is_none());
        assert!(backend.references.lock().unwrap().is_empty());
        let fourth = save(&backend, &third, third.clone(), false, Some(secret()))
            .unwrap()
            .data;
        let deleted = save(&backend, &fourth, Workspace::default(), false, None).unwrap();
        assert!(deleted.data.profiles.is_empty());
        assert!(backend.references.lock().unwrap().is_empty());
    }
    #[test]
    fn successful_save_with_failed_cleanup_remains_saved_and_reports_warning() {
        let backend = Fake::default();
        let first = save(
            &backend,
            &Workspace::default(),
            data(),
            false,
            Some(secret()),
        )
        .unwrap()
        .data;
        backend.fail_remove.store(true, Ordering::Relaxed);
        let saved = save(&backend, &first, Workspace::default(), false, None).unwrap();
        assert!(saved.warning.is_some());
        assert!(backend.load().unwrap().0.profiles.is_empty());
    }
    #[test]
    fn import_export_scrubs_references_and_preserves_existing_exports() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("profiles.json");
        let mut data = data();
        data.profiles[0].credential_ref = Some("00000000-0000-4000-8000-000000000001".into());
        export(&path, &data).unwrap();
        assert!(export(&path, &Workspace::default()).is_err());
        let imported = import(&path).unwrap();
        assert_eq!(imported.profiles.len(), 1);
        assert!(imported.profiles[0].credential_ref.is_none());
        std::fs::write(&path, serde_json::to_vec(&data).unwrap()).unwrap();
        assert!(import(&path).unwrap().profiles[0].credential_ref.is_none());
    }
}
