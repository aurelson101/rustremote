//! Native loopback qualification; no production host or system account.
use crate::sftp::{self, Budget, Operation, Output};
use rustremote_core::profiles::{ProtocolOptions, SessionProfile};
use std::{
    io::Read,
    path::PathBuf,
    sync::{Arc, atomic::AtomicBool},
};
use zeroize::{Zeroize, Zeroizing};
fn budget() -> Budget {
    Budget::new(Arc::new(AtomicBool::new(false)))
}
pub fn run() -> Result<String, String> {
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
        .map_err(|_| "Lab stdin unavailable")?;
    if bytes.len() > 32768 {
        return Err("Lab input exceeds budget".into());
    }
    let input: Input = serde_json::from_slice(&bytes).map_err(|_| "Invalid SFTP lab input")?;
    let mut profile = input.profile.clone();
    if !profile
        .host
        .parse::<std::net::IpAddr>()
        .is_ok_and(|ip| ip.is_loopback())
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
        return Err("Only explicit loopback SFTP fixtures are allowed".into());
    }
    let backend = crate::profile_service::laboratory()?;
    profile.credential_ref = Some(backend.credential_write(&profile, &input.secret)?);
    let secret = crate::win_credentials::read(&profile)?.ok_or("Lab vault entry unavailable")?;
    let connection = sftp::Connection::connect(&profile, Some(&secret));
    if input.expect_reject {
        return if connection.is_err() {
            Ok("WINDOWS_SFTP_REJECT_OK".into())
        } else {
            Err("Rejected SFTP scenario authenticated".into())
        };
    }
    let connection = connection?;
    let temporary = tempfile::tempdir().map_err(|_| "SFTP local fixture unavailable")?;
    let root = temporary.path();
    let upload = root.join("dossier été");
    std::fs::create_dir(&upload).map_err(|_| "Fixture mkdir failed")?;
    let original = b"SFTP UTF8 binary\0\xff\xfe".to_vec();
    std::fs::write(upload.join("fichier é.bin"), &original).map_err(|_| "Fixture write failed")?;
    let remote = PathBuf::from("/dossier été");
    connection.run(
        Operation::Upload {
            local: upload,
            remote: remote.clone(),
        },
        &mut budget(),
    )?;
    let list = connection.run(Operation::List(remote.clone()), &mut budget())?;
    if !matches!(list,Output::Entries{entries,..} if entries.iter().any(|e|e.name=="fichier é.bin"&&e.regular))
    {
        return Err("Remote Unicode listing mismatch".into());
    }
    connection.run(
        Operation::Permissions {
            path: PathBuf::from("/dossier été/fichier é.bin"),
            mode: 0o755,
        },
        &mut budget(),
    )?;
    let downloaded = root.join("retour été");
    connection.run(
        Operation::Download {
            remote: remote.clone(),
            local: downloaded.clone(),
        },
        &mut budget(),
    )?;
    if std::fs::read(downloaded.join("fichier é.bin")).map_err(|_| "Readback unavailable")?
        != original
    {
        return Err("Binary roundtrip mismatch".into());
    }
    if connection
        .run(
            Operation::Download {
                remote: remote.clone(),
                local: downloaded,
            },
            &mut budget(),
        )
        .is_ok()
    {
        return Err("Download replaced an existing directory".into());
    }
    let file = PathBuf::from("/dossier été/fichier é.bin");
    let edited = b"edited via native SFTP".to_vec();
    connection.run(
        Operation::Save {
            path: file.clone(),
            original: original.clone(),
            text: edited.clone(),
        },
        &mut budget(),
    )?;
    if !matches!(connection.run(Operation::Read(file.clone()),&mut budget())?,Output::Text{bytes,..} if bytes==edited)
    {
        return Err("Remote edit mismatch".into());
    }
    if connection
        .run(
            Operation::Save {
                path: file,
                original,
                text: b"conflict".to_vec(),
            },
            &mut budget(),
        )
        .is_ok()
    {
        return Err("Concurrent edit was overwritten".into());
    }
    if connection
        .run(Operation::Read("/blocked-link".into()), &mut budget())
        .is_ok()
    {
        return Err("Remote symlink was accepted".into());
    }
    let renamed = PathBuf::from("/renommé été");
    connection.run(
        Operation::Rename {
            from: remote,
            to: renamed.clone(),
        },
        &mut budget(),
    )?;
    let cancelled = Arc::new(AtomicBool::new(true));
    if connection
        .run(Operation::List("/".into()), &mut Budget::new(cancelled))
        .is_ok()
    {
        return Err("Cancelled operation ran".into());
    }
    let from = root.join("source");
    let to = root.join("destination");
    std::fs::write(&from, b"source").map_err(|_| "Fixture unavailable")?;
    std::fs::write(&to, b"keep").map_err(|_| "Fixture unavailable")?;
    if crate::sftp_platform::publish(&from, &to).is_ok()
        || std::fs::read(&to).map_err(|_| "Collision fixture unavailable")? != b"keep"
    {
        return Err("Local collision protection failed".into());
    }
    for name in ["NUL", "CON", "data:stream", "trailing."] {
        if crate::sftp_platform::validate_local(&root.join(name)).is_ok() {
            return Err("Windows device/stream/alias accepted".into());
        }
    }
    connection.run(Operation::Delete(renamed), &mut budget())?;
    connection.abort();
    Ok("WINDOWS_SFTP_OK: binary files/folders, Unicode paths, listing, edit/conflict, rename/delete, cancellation, collisions, link/device rejection".into())
}
