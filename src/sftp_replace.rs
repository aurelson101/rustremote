//! Bounded OpenSSH atomic replacement over a separate channel of the same verified session.
//! ssh2::Sftp::rename sends SSH_FXP_RENAME v3, which cannot request POSIX replacement.
use std::{
    io::{Read, Write},
    path::Path,
};
const EXTENSION: &[u8] = b"posix-rename@openssh.com";
const INVALID: &str = "Invalid or unsupported SFTP atomic-rename response";
fn number(input: &mut &[u8]) -> Result<u32, String> {
    let bytes = input.get(..4).ok_or(INVALID)?;
    let value = u32::from_be_bytes(bytes.try_into().map_err(|_| INVALID)?);
    *input = &input[4..];
    Ok(value)
}
fn string<'a>(input: &mut &'a [u8]) -> Result<&'a [u8], String> {
    let length = number(input)? as usize;
    let value = input.get(..length).ok_or(INVALID)?;
    *input = &input[length..];
    Ok(value)
}
fn version(packet: &[u8]) -> Result<(), String> {
    let mut input = packet;
    if input.first() != Some(&2) {
        return Err(INVALID.into());
    }
    input = &input[1..];
    if number(&mut input)? != 3 {
        return Err(INVALID.into());
    }
    let mut supported = false;
    while !input.is_empty() {
        let name = string(&mut input)?;
        let value = string(&mut input)?;
        supported |= name == EXTENSION && value == b"1";
    }
    if !supported {
        return Err(
            "Server does not support atomic SFTP replacement; original file preserved".into(),
        );
    }
    Ok(())
}
fn status(packet: &[u8]) -> Result<(), String> {
    let mut input = packet;
    if input.first() != Some(&101) {
        return Err(INVALID.into());
    }
    input = &input[1..];
    if number(&mut input)? != 1 {
        return Err(INVALID.into());
    }
    let code = number(&mut input)?;
    let _message = string(&mut input)?;
    let _language = string(&mut input)?;
    if !input.is_empty() {
        return Err(INVALID.into());
    }
    if code != 0 {
        return Err(format!("Atomic SFTP replacement refused (status {code})"));
    }
    Ok(())
}
fn read(channel: &mut impl Read) -> Result<Vec<u8>, String> {
    let mut length = [0; 4];
    channel.read_exact(&mut length).map_err(|_| INVALID)?;
    let length = u32::from_be_bytes(length) as usize;
    if !(5..=65536).contains(&length) {
        return Err("SFTP packet exceeds its budget".into());
    }
    let mut packet = vec![0; length];
    channel.read_exact(&mut packet).map_err(|_| INVALID)?;
    Ok(packet)
}
fn write(channel: &mut impl Write, packet: &[u8]) -> Result<(), String> {
    channel
        .write_all(&(packet.len() as u32).to_be_bytes())
        .and_then(|_| channel.write_all(packet))
        .map_err(|_| "Cannot send atomic SFTP request".into())
}
fn append(packet: &mut Vec<u8>, value: &[u8]) {
    packet.extend_from_slice(&(value.len() as u32).to_be_bytes());
    packet.extend_from_slice(value);
}
fn remote(path: &Path) -> Result<String, String> {
    let text = path.to_str().ok_or("Unicode remote path required")?;
    if text.is_empty() || text.len() > 4096 || text.chars().any(char::is_control) {
        return Err("Invalid remote replacement path".into());
    }
    #[cfg(windows)]
    return Ok(text.replace('\\', "/"));
    #[cfg(not(windows))]
    Ok(text.into())
}
pub fn replace(
    session: &ssh2::Session,
    from: &Path,
    to: &Path,
    mut check: impl FnMut() -> Result<(), String>,
) -> Result<(), String> {
    let from = remote(from)?;
    let to = remote(to)?;
    check()?;
    let mut channel = session
        .channel_session()
        .map_err(|_| "Cannot open SFTP replacement channel")?;
    channel
        .subsystem("sftp")
        .map_err(|_| "Cannot start SFTP replacement subsystem")?;
    let result = (|| {
        write(&mut channel, &[1, 0, 0, 0, 3])?;
        version(&read(&mut channel)?)?;
        check()?;
        let mut request = vec![200, 0, 0, 0, 1];
        append(&mut request, EXTENSION);
        append(&mut request, from.as_bytes());
        append(&mut request, to.as_bytes());
        write(&mut channel, &request)?;
        status(&read(&mut channel)?)
    })();
    let _ = channel.send_eof();
    let _ = channel.close();
    result
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn requires_supported_version_and_bounded_well_formed_packets() {
        let mut packet = vec![2, 0, 0, 0, 3];
        append(&mut packet, EXTENSION);
        append(&mut packet, b"1");
        assert!(version(&packet).is_ok());
        for length in 0..packet.len() {
            assert!(version(&packet[..length]).is_err());
        }
        *packet.last_mut().unwrap() = b'2';
        assert!(version(&packet).is_err());
        packet.push(0);
        assert!(version(&packet).is_err());
        assert!(read(&mut &u32::MAX.to_be_bytes()[..]).is_err());
        assert!(read(&mut &[0, 0, 0, 5, 2][..]).is_err());
    }
    #[test]
    fn validates_request_id_and_status_without_logging_server_text() {
        let mut packet = vec![101, 0, 0, 0, 1, 0, 0, 0, 0];
        append(&mut packet, b"");
        append(&mut packet, b"");
        assert!(status(&packet).is_ok());
        for length in 0..packet.len() {
            assert!(status(&packet[..length]).is_err());
        }
        packet[4] = 2;
        assert!(status(&packet).is_err());
        packet[4] = 1;
        packet[8] = 4;
        assert!(status(&packet).is_err());
    }
}
