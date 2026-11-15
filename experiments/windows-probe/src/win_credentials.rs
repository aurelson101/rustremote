//! Passwords stay in the current user's Credential Manager, bound to an account.
use rustremote_core::{profiles::*, valid_credential_reference};
use std::ptr::null_mut;
use windows_sys::Win32::{Foundation::ERROR_NOT_FOUND, Security::Credentials::*};
use zeroize::{Zeroize, Zeroizing};

#[derive(serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct Envelope {
    version: u8,
    scope: String,
    secret: String,
}
impl Drop for Envelope {
    fn drop(&mut self) {
        self.secret.zeroize();
    }
}

fn target(reference: &str) -> Result<Vec<u16>, String> {
    if !valid_credential_reference(reference) {
        return Err("Invalid credential reference".into());
    }
    crate::win_storage::wide(format!("com.rustremote.credentials:{reference}"))
}

pub fn write(profile: &SessionProfile, secret: &str) -> Result<String, String> {
    if secret.is_empty()
        || secret.len() > 2048
        || secret.chars().any(|c| matches!(c, '\0' | '\n' | '\r'))
    {
        return Err("Password/passphrase must contain 1–2048 bytes and no line breaks".into());
    }
    let reference = crate::win_storage::new_reference()?;
    let envelope = Envelope {
        version: 1,
        scope: profile.credential_scope()?,
        secret: secret.into(),
    };
    let mut payload =
        Zeroizing::new(serde_json::to_vec(&envelope).map_err(|_| "Cannot encode credential")?);
    if payload.len() > CRED_MAX_CREDENTIAL_BLOB_SIZE as usize {
        return Err("Credential exceeds Windows storage budget".into());
    }
    let target = target(&reference)?;
    let credential = CREDENTIALW {
        Type: CRED_TYPE_GENERIC,
        TargetName: target.as_ptr().cast_mut(),
        CredentialBlobSize: payload.len() as u32,
        CredentialBlob: payload.as_mut_ptr(),
        Persist: CRED_PERSIST_LOCAL_MACHINE,
        ..Default::default()
    };
    if unsafe { CredWriteW(&credential, 0) } == 0 {
        return Err("Windows Credential Manager is unavailable for this logon session".into());
    }
    Ok(reference)
}

pub fn read(profile: &SessionProfile) -> Result<Option<Zeroizing<String>>, String> {
    let Some(reference) = &profile.credential_ref else {
        return Ok(None);
    };
    let target = target(reference)?;
    let mut pointer = null_mut();
    if unsafe { CredReadW(target.as_ptr(), CRED_TYPE_GENERIC, 0, &mut pointer) } == 0 {
        return if std::io::Error::last_os_error().raw_os_error() == Some(ERROR_NOT_FOUND as i32) {
            Ok(None)
        } else {
            Err("Windows Credential Manager could not read this credential".into())
        };
    }
    struct Credential(*mut CREDENTIALW);
    impl Drop for Credential {
        fn drop(&mut self) {
            unsafe {
                let data = &*self.0;
                if !data.CredentialBlob.is_null()
                    && data.CredentialBlobSize <= CRED_MAX_CREDENTIAL_BLOB_SIZE
                {
                    std::slice::from_raw_parts_mut(
                        data.CredentialBlob,
                        data.CredentialBlobSize as usize,
                    )
                    .zeroize();
                }
                CredFree(self.0.cast());
            }
        }
    }
    if pointer.is_null() {
        return Err("Windows returned an invalid credential".into());
    }
    let _allocation = Credential(pointer);
    let data = unsafe { &*pointer };
    if data.CredentialBlob.is_null() || data.CredentialBlobSize > CRED_MAX_CREDENTIAL_BLOB_SIZE {
        return Err("Credential exceeds Windows storage budget".into());
    }
    let bytes = unsafe {
        std::slice::from_raw_parts(data.CredentialBlob, data.CredentialBlobSize as usize)
    };
    let mut envelope: Envelope =
        serde_json::from_slice(bytes).map_err(|_| "Unsupported credential format")?;
    if envelope.version != 1 || envelope.scope != profile.credential_scope()? {
        return Err(
            "Saved credential belongs to another host, user, domain or SSH configuration".into(),
        );
    }
    Ok(Some(Zeroizing::new(std::mem::take(&mut envelope.secret))))
}

pub fn remove(reference: &str) -> Result<(), String> {
    let target = target(reference)?;
    if unsafe { CredDeleteW(target.as_ptr(), CRED_TYPE_GENERIC, 0) } == 0
        && std::io::Error::last_os_error().raw_os_error() != Some(ERROR_NOT_FOUND as i32)
    {
        return Err("Windows could not delete the saved credential".into());
    }
    Ok(())
}

pub fn self_test() -> Result<String, String> {
    let mut profile: SessionProfile = serde_json::from_value(serde_json::json!({
        "name":"Credential lab", "host":"localhost", "port":3389, "username":"lab",
        "favorite":false, "simulation":true,
        "options":{"protocol":"Rdp", "domain":"LAB", "width":1280,"height":720,
            "fullscreen":false,"clipboard":true,"clipboard_files":false}
    }))
    .map_err(|_| "Invalid credential lab")?;
    let secret = Zeroizing::new(crate::win_storage::new_reference()?);
    let reference = write(&profile, &secret)?;
    struct Cleanup(String);
    impl Drop for Cleanup {
        fn drop(&mut self) {
            let _ = remove(&self.0);
        }
    }
    let _cleanup = Cleanup(reference.clone());
    profile.credential_ref = Some(reference.clone());
    let temp = tempfile::tempdir().map_err(|_| "Credential profile lab unavailable")?;
    let store = crate::win_storage::Store::at(temp.path().join("Credential profiles"))?;
    let workspace = Workspace {
        profiles: vec![profile.clone()],
        ..Default::default()
    };
    for encrypted in [false, true] {
        store.save(&workspace, encrypted)?;
        let (loaded, mode) = store.load()?;
        if loaded.profiles != workspace.profiles || mode != encrypted {
            return Err("Credential profile persistence mismatch".into());
        }
        if read(&loaded.profiles[0])?.as_deref().map(|s| s.as_str()) != Some(secret.as_str()) {
            return Err("Persisted profile lost its credential binding".into());
        }
        let bytes = std::fs::read(temp.path().join("Credential profiles/profiles.rstore"))
            .map_err(|_| "Cannot inspect credential profile lab")?;
        if bytes
            .windows(secret.len())
            .any(|part| part == secret.as_bytes())
        {
            return Err("Profile store leaked a plaintext password".into());
        }
    }
    if read(&profile)?.as_deref().map(|s| s.as_str()) != Some(secret.as_str()) {
        return Err("Credential binding roundtrip failed".into());
    }
    let mut changed = profile.clone();
    changed.username = "another-user".into();
    if read(&changed).is_ok() {
        return Err("Credential leaked to a different account".into());
    }
    changed = profile.clone();
    changed.host = "another.example.invalid".into();
    if read(&changed).is_ok() {
        return Err("Credential leaked to a different host".into());
    }
    changed = profile.clone();
    if let ProtocolOptions::Rdp { domain, .. } = &mut changed.options {
        *domain = "OTHER".into();
    }
    if read(&changed).is_ok() {
        return Err("Credential leaked to a different AD domain".into());
    }
    let export = Workspace {
        profiles: vec![profile],
        ..Default::default()
    }
    .export_json()?;
    if String::from_utf8_lossy(&export).contains("credential_ref") {
        return Err("Export leaked a credential reference".into());
    }
    remove(&reference)?;
    Ok("WINDOWS_CREDENTIALS_OK: current-user vault, account/host/domain binding, plain/DPAPI profile integration, secret-free export, cleanup".into())
}
