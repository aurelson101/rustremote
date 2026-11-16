//! Current-user askpass bridge. Only credential references, never passwords, enter argv/env.
use rustremote_core::profiles::{ProtocolOptions, SessionProfile};
use std::{collections::BTreeMap, ffi::OsString, os::windows::ffi::OsStrExt, path::PathBuf};
pub const PROFILE_ENV: &str = "RUSTREMOTE_WINDOWS_ASKPASS_PROFILE";

pub fn system_directory() -> Result<PathBuf, String> {
    use std::os::windows::ffi::OsStringExt;
    let mut value = vec![0u16; 32768];
    let length = unsafe {
        windows_sys::Win32::System::SystemInformation::GetSystemDirectoryW(
            value.as_mut_ptr(),
            value.len() as u32,
        )
    } as usize;
    if length == 0 || length >= value.len() {
        return Err("Windows system directory is unavailable".into());
    }
    Ok(PathBuf::from(OsString::from_wide(&value[..length])))
}

pub fn client_path() -> Result<PathBuf, String> {
    let bundled = std::env::current_exe()
        .map_err(|_| "Application path unavailable")?
        .parent()
        .ok_or("Application directory unavailable")?
        .join("OpenSSH/ssh.exe");
    if bundled.is_file() {
        return Ok(bundled);
    }
    let system = system_directory()?.join("OpenSSH/ssh.exe");
    if !system.is_file() {
        return Err("Install Windows OpenSSH Client or the RustRemote bundled runtime".into());
    }
    Ok(system)
}

pub fn environment(profile: Option<&SessionProfile>) -> Result<Vec<u16>, String> {
    let mut values = BTreeMap::<String, OsString>::new();
    // All names are ASCII, so uppercase ordering is the Windows ordinal ordering.
    for name in [
        "PATH",
        "PATHEXT",
        "TEMP",
        "TMP",
        "USERPROFILE",
        "LOCALAPPDATA",
        "APPDATA",
        "COMSPEC",
    ] {
        if let Some(value) = std::env::var_os(name) {
            values.insert(name.into(), value);
        }
    }
    let system = system_directory()?;
    let root = system.parent().ok_or("Windows root unavailable")?;
    values.insert("SYSTEMROOT".into(), root.as_os_str().into());
    values.insert("WINDIR".into(), root.as_os_str().into());
    if let Some(profile) = profile {
        if matches!(&profile.options, ProtocolOptions::Ssh { agent: true, .. })
            && let Some(socket) = std::env::var_os("SSH_AUTH_SOCK")
        {
            values.insert("SSH_AUTH_SOCK".into(), socket);
        }
        if profile.credential_ref.is_some() {
            let secret = crate::win_credentials::read(profile)?
                .ok_or("Saved SSH credential is missing; edit this connection")?;
            if secret.len() > 1000 {
                return Err("SSH passwords/passphrases are limited to 1000 UTF-8 bytes".into());
            }
            values.insert(
                PROFILE_ENV.into(),
                serde_json::to_string(profile)
                    .map_err(|_| "Cannot bind SSH credential")?
                    .into(),
            );
            values.insert(
                "SSH_ASKPASS".into(),
                std::env::current_exe()
                    .map_err(|_| "SSH helper unavailable")?
                    .into_os_string(),
            );
            values.insert("SSH_ASKPASS_REQUIRE".into(), "force".into());
        } else {
            values.insert("SSH_ASKPASS_REQUIRE".into(), "never".into());
        }
    }
    let mut block = Vec::new();
    for (name, value) in values {
        let value: Vec<u16> = value.encode_wide().collect();
        if value.contains(&0) {
            return Err("Invalid Windows environment value".into());
        }
        block.extend(name.encode_utf16());
        block.push(b'=' as u16);
        block.extend(value);
        block.push(0);
    }
    block.push(0);
    if block.len() > 32767 {
        return Err("Windows terminal environment exceeds its budget".into());
    }
    Ok(block)
}

pub fn askpass() -> std::process::ExitCode {
    use std::io::Write;
    let result = (|| -> Result<(), String> {
        let raw = std::env::var(PROFILE_ENV).map_err(|_| "Missing SSH credential binding")?;
        if raw.len() > 16384 {
            return Err("SSH credential binding too large".into());
        }
        let profile: SessionProfile =
            serde_json::from_str(&raw).map_err(|_| "Invalid SSH credential binding")?;
        crate::ssh_launch::arguments(&profile)?;
        if std::env::var("SSH_ASKPASS_PROMPT").is_ok() {
            return Err("Host trust and agent confirmations require explicit user approval".into());
        }
        let prompt = std::env::args()
            .nth(1)
            .ok_or("Missing SSH authentication prompt")?
            .to_ascii_lowercase();
        let key = matches!(
            &profile.options,
            ProtocolOptions::Ssh {
                identity_file: Some(_),
                ..
            }
        );
        if (key && !prompt.contains("passphrase")) || (!key && !prompt.contains("password:")) {
            return Err("Unsupported SSH authentication prompt".into());
        }
        let secret = crate::win_credentials::read(&profile)?.ok_or("SSH credential unavailable")?;
        if secret.is_empty()
            || secret.len() > 1000
            || secret.chars().any(|c| matches!(c, '\0' | '\r' | '\n'))
        {
            return Err("Invalid SSH credential length or framing".into());
        }
        let mut output = std::io::stdout().lock();
        output
            .write_all(secret.as_bytes())
            .and_then(|_| output.write_all(b"\n"))
            .and_then(|_| output.flush())
            .map_err(|_| "SSH authentication pipe closed")?;
        Ok(())
    })();
    match result {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(_) => {
            eprintln!("SSH authentication helper refused the request");
            std::process::ExitCode::FAILURE
        }
    }
}

/// Loopback-only test input arrives on stdin, not in files, environment or arguments.
pub fn self_test() -> Result<String, String> {
    use std::io::Read;
    use zeroize::{Zeroize, Zeroizing};
    #[derive(serde::Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Input {
        profile: SessionProfile,
        secret: String,
        expect_reject: bool,
    }
    impl Drop for Input {
        fn drop(&mut self) {
            self.secret.zeroize();
        }
    }
    let mut bytes = Zeroizing::new(Vec::new());
    std::io::stdin()
        .lock()
        .take(32769)
        .read_to_end(&mut bytes)
        .map_err(|_| "SSH lab input unavailable")?;
    if bytes.len() > 32768 {
        return Err("SSH lab input exceeds its budget".into());
    }
    let input: Input = serde_json::from_slice(&bytes).map_err(|_| "Invalid SSH lab input")?;
    let mut profile = input.profile.clone();
    let loopback = profile
        .host
        .parse::<std::net::IpAddr>()
        .is_ok_and(|ip| ip.is_loopback());
    if !loopback
        || profile.credential_ref.is_some()
        || !matches!(
            &profile.options,
            ProtocolOptions::Ssh {
                agent: false,
                known_hosts_file: Some(_),
                config_file: None,
                ..
            }
        )
    {
        return Err("SSH lab accepts loopback, explicit trust files, no agent/config and no existing credential".into());
    }
    client_path()?;
    let reference = crate::win_credentials::write(&profile, &input.secret)?;
    struct Cleanup(String);
    impl Drop for Cleanup {
        fn drop(&mut self) {
            let _ = crate::win_credentials::remove(&self.0);
        }
    }
    let _cleanup = Cleanup(reference.clone());
    profile.credential_ref = Some(reference);
    let session =
        crate::live::Session::start_target(crate::ssh_launch::Target::Ssh(Box::new(profile)))?;
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(15);
    let mut sent = false;
    while std::time::Instant::now() < deadline {
        if let Some(snapshot) = session.snapshot(0) {
            let text = snapshot.screen.contents();
            if text.contains("RUSTREMOTE_SSH_READY") && !sent {
                if input.expect_reject {
                    return Err("SSH rejected scenario unexpectedly authenticated".into());
                }
                session.resize(92, 28)?;
                session.input("probe-é\r".as_bytes().to_vec())?;
                sent = true;
            }
            if text.contains("RUSTREMOTE_SSH_OK_été") {
                if input.expect_reject {
                    return Err("Rejected SSH scenario produced a success marker".into());
                }
                session.close();
                let stopped = std::time::Instant::now() + std::time::Duration::from_secs(5);
                while std::time::Instant::now() < stopped {
                    if session.snapshot(0).is_some_and(|state| !state.running) {
                        return Ok("WINDOWS_SSH_OK: real loopback SSH, stored credential askpass, UTF-8 terminal, resize and private job cancellation".into());
                    }
                    std::thread::sleep(std::time::Duration::from_millis(25));
                }
                return Err("SSH cancellation failed to complete".into());
            }
            if !snapshot.running {
                if input.expect_reject && snapshot.status == "SSH terminal exited (255)" {
                    return Ok("WINDOWS_SSH_REJECT_OK: unauthenticated or host-trust failure closed the session".into());
                }
                return Err(format!("SSH lab stopped: {}", snapshot.status));
            }
        }
        std::thread::sleep(std::time::Duration::from_millis(25));
    }
    session.close();
    Err("SSH loopback qualification timed out".into())
}
