use std::io::{self, Read, Write};
use std::os::unix::net::UnixStream;
pub const MAX_PACKET: usize = 8192;
pub fn send(stream: &mut UnixStream, data: &[u8]) -> io::Result<()> {
    if data.len() > MAX_PACKET {
        return Err(io::ErrorKind::InvalidData.into());
    }
    stream.write_all(&(data.len() as u32).to_be_bytes())?;
    stream.write_all(data)
}
pub fn receive(stream: &mut UnixStream) -> io::Result<Vec<u8>> {
    let mut length = [0; 4];
    stream.read_exact(&mut length)?;
    let length = u32::from_be_bytes(length) as usize;
    if length > MAX_PACKET {
        return Err(io::ErrorKind::InvalidData.into());
    }
    let mut bytes = vec![0; length];
    stream.read_exact(&mut bytes)?;
    Ok(bytes)
}
pub fn peer(stream: &UnixStream) -> io::Result<(u32, i32)> {
    use std::os::fd::AsRawFd;
    let mut cred = libc::ucred {
        pid: 0,
        uid: 0,
        gid: 0,
    };
    let mut size = std::mem::size_of_val(&cred) as libc::socklen_t;
    let rc = unsafe {
        libc::getsockopt(
            stream.as_raw_fd(),
            libc::SOL_SOCKET,
            libc::SO_PEERCRED,
            (&mut cred as *mut libc::ucred).cast(),
            &mut size,
        )
    };
    if rc != 0 {
        return Err(io::Error::last_os_error());
    }
    Ok((cred.uid, cred.pid))
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn framing_and_peer_are_bounded() {
        let (mut a, mut b) = UnixStream::pair().unwrap();
        send(&mut a, b"fixture").unwrap();
        assert_eq!(receive(&mut b).unwrap(), b"fixture");
        assert!(send(&mut a, &vec![0; MAX_PACKET + 1]).is_err());
        assert_eq!(peer(&a).unwrap().0, unsafe { libc::geteuid() });
    }
}
