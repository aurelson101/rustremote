//! Pont OpenSSH : le secret circule exclusivement sur socket privé puis stdout capturé par ssh.
#[path = "../askpass_wire.rs"]
mod wire;
use std::{io::Write, os::unix::net::UnixStream, time::Duration};
fn run() -> Result<(), Box<dyn std::error::Error>> {
    let path = std::env::var_os("RUSTREMOTE_ASKPASS_SOCKET").ok_or("socket absent")?;
    let mut stream = UnixStream::connect(path)?;
    if wire::peer(&stream)?.0 != unsafe { libc::geteuid() } {
        return Err("pair invalide".into());
    }
    stream.set_read_timeout(Some(Duration::from_secs(95)))?;
    stream.set_write_timeout(Some(Duration::from_secs(2)))?;
    let mode = std::env::var("SSH_ASKPASS_PROMPT").unwrap_or_default();
    if mode == "none" {
        return Err("sans interaction".into());
    }
    let prompt = std::env::args().nth(1).unwrap_or_default();
    let mut request = vec![u8::from(mode == "confirm")];
    request.extend_from_slice(prompt.as_bytes());
    wire::send(&mut stream, &request)?;
    let response = zeroize::Zeroizing::new(wire::receive(&mut stream)?);
    if response.first() != Some(&1) {
        return Err("annulé".into());
    }
    std::io::stdout().write_all(&response[1..])?;
    std::io::stdout().write_all(b"\n")?;
    Ok(())
}
fn main() -> std::process::ExitCode {
    if run().is_ok() {
        std::process::ExitCode::SUCCESS
    } else {
        std::process::ExitCode::FAILURE
    }
}
