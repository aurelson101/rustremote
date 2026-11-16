//! Explicit mstsc configuration. Unsupported security controls are refused, never weakened.
use rustremote_core::profiles::{ProtocolOptions, SessionProfile};
pub fn configuration(profile: &SessionProfile) -> Result<String, String> {
    profile.validate()?;
    if profile.simulation {
        return Err("Simulation profiles never contact a server".into());
    }
    let ProtocolOptions::Rdp {
        domain,
        width,
        height,
        fullscreen,
        clipboard,
        clipboard_files,
        settings,
    } = &profile.options
    else {
        return Err("Choose an RDP profile".into());
    };
    if settings.certificate_sha256.is_some() {
        return Err(
            "mstsc cannot enforce this profile's SHA256 pin; use a client supporting pinning"
                .into(),
        );
    }
    if *clipboard && !*clipboard_files {
        return Err("mstsc clipboard includes file transfer: explicitly allow clipboard files in RDP options, or disable clipboard".into());
    }
    let host = if profile.host.contains(':') {
        format!("[{}]", profile.host)
    } else {
        profile.host.clone()
    };
    let mut result = format!(
        "full address:s:{host}:{}\r\nusername:s:{}\r\ndomain:s:{domain}\r\n",
        profile.port, profile.username
    );
    // Certificate failures refuse the connection; CredSSP/NLA is enabled. No secret in this file.
    result.push_str("authentication level:i:1\r\nenablecredsspsupport:i:1\r\nprompt for credentials:i:1\r\npromptcredentialonce:i:0\r\nadministrative session:i:0\r\nremoteapplicationmode:i:0\r\ngatewayusagemethod:i:0\r\n");
    result.push_str(&format!("screen mode id:i:{}\r\ndesktopwidth:i:{width}\r\ndesktopheight:i:{height}\r\ndynamic resolution:i:{}\r\nsmart sizing:i:{}\r\nuse multimon:i:{}\r\naudiomode:i:{}\r\naudiocapturemode:i:{}\r\nredirectprinters:i:{}\r\nredirectclipboard:i:{}\r\n",if *fullscreen{2}else{1},u8::from(settings.size_mode==1),u8::from(settings.size_mode==2),u8::from(settings.multi_monitor),settings.audio_mode,u8::from(settings.microphone),u8::from(settings.printers),u8::from(*clipboard)));
    if !settings.monitors.is_empty() {
        result.push_str(&format!(
            "selectedmonitors:s:{}\r\n",
            settings
                .monitors
                .iter()
                .map(u16::to_string)
                .collect::<Vec<_>>()
                .join(",")
        ));
    }
    result.push_str("drivestoredirect:s:\r\nredirectdrives:i:0\r\nredirectcomports:i:0\r\nredirectsmartcards:i:0\r\nredirectwebauthn:i:0\r\nusbdevicestoredirect:s:\r\ndevicestoredirect:s:\r\ncamerastoredirect:s:\r\n");
    Ok(result)
}
#[cfg(windows)]
pub fn launch(profile: SessionProfile, stop: std::sync::mpsc::Receiver<()>) -> Result<(), String> {
    use std::{os::windows::process::CommandExt, time::Duration};
    let content = configuration(&profile)?;
    if !matches!(stop.try_recv(), Err(std::sync::mpsc::TryRecvError::Empty)) {
        return Ok(());
    }
    let client = client()?;
    let protected = protected_from_profile(&profile)?;
    let (_temporary, path) = prepare(&content, protected.as_deref().map(Vec::as_slice))?;
    drop(protected);
    if !matches!(stop.try_recv(), Err(std::sync::mpsc::TryRecvError::Empty)) {
        return Ok(());
    }
    let mut child = std::process::Command::new(client)
        .arg(&path)
        .creation_flags(0x08000000)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .map_err(|_| "Cannot start Windows Remote Desktop")?;
    // mstsc may hand off to another instance: retain the file until the user closes our controller.
    while matches!(
        stop.recv_timeout(Duration::from_millis(250)),
        Err(std::sync::mpsc::RecvTimeoutError::Timeout)
    ) {
        let _ = child.try_wait();
    }
    if child
        .try_wait()
        .map_err(|_| "Cannot inspect owned mstsc process")?
        .is_none()
    {
        let _ = child.kill();
        let _ = child.wait();
    }
    Ok(())
}
#[cfg(windows)]
fn client() -> Result<std::path::PathBuf, String> {
    let mut directory = [0u16; 32768];
    let length = unsafe {
        windows_sys::Win32::System::SystemInformation::GetSystemDirectoryW(
            directory.as_mut_ptr(),
            directory.len() as u32,
        )
    } as usize;
    if length == 0 || length >= directory.len() {
        return Err("Windows system directory unavailable".into());
    }
    use std::os::windows::ffi::OsStringExt;
    let client = std::path::PathBuf::from(std::ffi::OsString::from_wide(&directory[..length]))
        .join("mstsc.exe");
    if !client.is_file() {
        return Err("Remote Desktop Connection (mstsc.exe) is unavailable".into());
    }
    Ok(client)
}
#[cfg(windows)]
fn protected_from_profile(
    profile: &SessionProfile,
) -> Result<Option<zeroize::Zeroizing<Vec<u8>>>, String> {
    let secret = crate::win_credentials::read(profile)?;
    if profile.credential_ref.is_some() && secret.is_none() {
        return Err("The RDP profile password is missing from Windows Credential Manager".into());
    }
    secret
        .as_ref()
        .map(|value| protect_password(value))
        .transpose()
}
#[cfg(windows)]
fn protect_password(secret: &str) -> Result<zeroize::Zeroizing<Vec<u8>>, String> {
    if secret.is_empty() || secret.len() > 2048 || secret.contains(['\0', '\r', '\n']) {
        return Err(
            "RDP password must contain 1–2048 UTF-8 bytes and no NUL or line breaks".into(),
        );
    }
    let bytes = zeroize::Zeroizing::new(
        secret
            .encode_utf16()
            .flat_map(u16::to_le_bytes)
            .collect::<Vec<_>>(),
    );
    crate::win_storage::crypt(&bytes, true)
}
#[cfg(windows)]
fn prepare(
    content: &str,
    protected: Option<&[u8]>,
) -> Result<(tempfile::TempDir, std::path::PathBuf), String> {
    use std::io::Write;
    let mut content = zeroize::Zeroizing::new(content.to_owned());
    if let Some(bytes) = protected {
        if bytes.is_empty() || bytes.len() > 16384 {
            return Err("Protected RDP password exceeds its budget".into());
        }
        use std::fmt::Write;
        *content = content.replace(
            "prompt for credentials:i:1\r\n",
            "prompt for credentials:i:0\r\n",
        );
        content.push_str("password 51:b:");
        for byte in bytes {
            write!(content, "{byte:02X}").map_err(|_| "Cannot encode protected RDP password")?;
        }
        content.push_str("\r\n");
    }
    let temporary = tempfile::tempdir().map_err(|_| "Cannot prepare RDP session")?;
    crate::sftp_platform::validate_local(temporary.path())
        .map_err(|_| "Unsafe RDP temporary folder")?;
    let private = temporary.path().join("session");
    crate::win_storage::private_directory(&private)?;
    let path = private.join("connection.rdp");
    let mut file = crate::win_storage::private_file(&path)?;
    file.write_all(&[0xff, 0xfe])
        .and_then(|_| {
            let bytes = content
                .encode_utf16()
                .flat_map(u16::to_le_bytes)
                .collect::<Vec<_>>();
            file.write_all(&bytes)
        })
        .and_then(|_| file.sync_all())
        .map_err(|_| "Cannot write private RDP configuration")?;
    drop(file);
    Ok((temporary, path))
}
#[cfg(windows)]
pub fn self_test() -> Result<String, String> {
    let _client = client()?;
    let profile:SessionProfile=serde_json::from_value(serde_json::json!({"name":"RDP offline lab","host":"::1","port":3389,"username":"lab","favorite":false,"options":{"protocol":"Rdp","domain":"LAB","width":1280,"height":720,"fullscreen":false,"clipboard":false,"clipboard_files":false}})).map_err(|_|"Invalid RDP fixture")?;
    let content = configuration(&profile)?;
    let (temporary, path) = prepare(&content, None)?;
    let file =
        crate::sftp_platform::read(&path, false).map_err(|_| "Cannot read private RDP fixture")?;
    crate::win_storage::validate_key_acl(&file)?;
    drop(file);
    let bytes = std::fs::read(&path).map_err(|_| "RDP fixture readback failed")?;
    let expected = std::iter::once(0xfeffu16)
        .chain(content.encode_utf16())
        .flat_map(u16::to_le_bytes)
        .collect::<Vec<_>>();
    if bytes != expected {
        return Err("RDP UTF16 configuration mismatch".into());
    }
    temporary
        .close()
        .map_err(|_| "RDP fixture cleanup failed")?;
    if path.exists() {
        return Err("RDP private fixture retained".into());
    }
    password_fixture(profile)?;
    Ok("WINDOWS_RDP_CONFIG_OK: system client found; private ACL/UTF16 config, profile-scoped vault password, current-user DPAPI roundtrip/tamper rejection, plaintext absence and cleanup; no process launched, no network or RDP session tested".into())
}
#[cfg(windows)]
fn password_fixture(mut profile: SessionProfile) -> Result<(), String> {
    use zeroize::Zeroizing;
    if protected_from_profile(&profile)?.is_some() {
        return Err("Unexpected RDP fixture credential".into());
    }
    let backend = crate::profile_service::laboratory()?;
    let secret = Zeroizing::new(format!("{}é漢🔒", crate::win_storage::new_reference()?));
    profile.credential_ref = Some(backend.credential_write(&profile, &secret)?);
    let protected = protected_from_profile(&profile)?.ok_or("RDP fixture vault lookup failed")?;
    let content = configuration(&profile)?;
    let (temporary, path) = prepare(&content, Some(&protected))?;
    let file =
        crate::sftp_platform::read(&path, false).map_err(|_| "RDP password fixture unavailable")?;
    crate::win_storage::validate_key_acl(&file)?;
    drop(file);
    let bytes = Zeroizing::new(std::fs::read(&path).map_err(|_| "RDP fixture readback failed")?);
    let raw = Zeroizing::new(
        secret
            .encode_utf16()
            .flat_map(u16::to_le_bytes)
            .collect::<Vec<_>>(),
    );
    if bytes
        .windows(raw.len())
        .any(|chunk| chunk == raw.as_slice())
    {
        return Err("Plaintext password written to RDP file".into());
    }
    let units = bytes
        .chunks_exact(2)
        .map(|b| u16::from_le_bytes([b[0], b[1]]))
        .collect::<Vec<_>>();
    let document = Zeroizing::new(String::from_utf16(&units).map_err(|_| "Invalid RDP UTF16")?);
    let cipher = document
        .lines()
        .find_map(|line| line.strip_prefix("password 51:b:"))
        .ok_or("Protected RDP password missing")?;
    if cipher.len() % 2 != 0
        || cipher.len() > 32768
        || document.contains(profile.credential_ref.as_ref().unwrap())
        || !document.contains("prompt for credentials:i:0")
    {
        return Err("Invalid protected RDP launch file".into());
    }
    let mut decoded = Zeroizing::new(Vec::new());
    for offset in (0..cipher.len()).step_by(2) {
        decoded.push(
            u8::from_str_radix(&cipher[offset..offset + 2], 16)
                .map_err(|_| "Invalid RDP protected encoding")?,
        );
    }
    let clear = crate::win_storage::crypt(&decoded, false)?;
    if clear.as_slice() != raw.as_slice() {
        return Err("RDP password Unicode roundtrip failed".into());
    }
    if let Some(last) = decoded.last_mut() {
        *last ^= 1;
    }
    if crate::win_storage::crypt(&decoded, false).is_ok() {
        return Err("Tampered RDP password was accepted".into());
    }
    for kind in 0..3 {
        let mut changed = profile.clone();
        match kind {
            0 => changed.username = "another-account".into(),
            1 => changed.port = 3390,
            _ => {
                if let ProtocolOptions::Rdp { domain, .. } = &mut changed.options {
                    *domain = "OTHER".into();
                }
            }
        }
        if protected_from_profile(&changed).is_ok() {
            return Err("RDP password escaped its profile scope".into());
        }
    }
    profile.credential_ref = Some(crate::win_storage::new_reference()?);
    if protected_from_profile(&profile).is_ok() {
        return Err("Missing RDP profile credential did not fail closed".into());
    }
    if protect_password("").is_ok() || protect_password("bad\0value").is_ok() {
        return Err("Invalid RDP password accepted".into());
    }
    temporary
        .close()
        .map_err(|_| "RDP password fixture cleanup failed")?;
    if path.exists() {
        return Err("Protected RDP launch file retained".into());
    }
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    fn profile() -> SessionProfile {
        serde_json::from_value(serde_json::json!({"name":"RDP lab","host":"::1","port":3389,"username":"user@realm.example","favorite":false,"options":{"protocol":"Rdp","domain":"LAB","width":1280,"height":720,"fullscreen":false,"clipboard":false,"clipboard_files":false}})).unwrap()
    }
    #[test]
    fn configuration_is_strict_and_does_not_expose_credentials() {
        let mut p = profile();
        let text = configuration(&p).unwrap();
        assert!(text.contains("full address:s:[::1]:3389\r\n"));
        assert!(text.contains("authentication level:i:1\r\n"));
        assert!(text.contains("redirectclipboard:i:0"));
        assert!(!text.contains("password"));
        assert!(!text.contains("TERMSRV"));
        p.username = "user\r\nauthentication level:i:0".into();
        assert!(configuration(&p).is_err());
    }
    #[test]
    fn options_require_explicit_redirection_and_supported_trust() {
        let mut p = profile();
        if let ProtocolOptions::Rdp {
            clipboard,
            clipboard_files,
            settings,
            ..
        } = &mut p.options
        {
            *clipboard = true;
            assert!(!*clipboard_files);
            settings.multi_monitor = true;
            settings.monitors = vec![0, 1];
            settings.printers = true;
            settings.audio_mode = 0;
        }
        assert!(configuration(&p).is_err());
        if let ProtocolOptions::Rdp {
            clipboard_files, ..
        } = &mut p.options
        {
            *clipboard_files = true;
        }
        let text = configuration(&p).unwrap();
        assert!(text.contains("selectedmonitors:s:0,1"));
        assert!(text.contains("redirectprinters:i:1"));
        assert!(text.contains("audiomode:i:0"));
        if let ProtocolOptions::Rdp { settings, .. } = &mut p.options {
            settings.certificate_sha256 = Some("a".repeat(64));
        }
        assert!(configuration(&p).is_err());
        p.simulation = true;
        assert!(configuration(&p).is_err());
    }
}
