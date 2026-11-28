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
#[cfg(target_os = "linux")]
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

#[cfg(target_os = "macos")]
pub fn peer(stream: &UnixStream) -> io::Result<(u32, i32)> {
    use std::os::fd::AsRawFd;
    let fd = stream.as_raw_fd();
    let (mut uid, mut gid, mut pid) = (0, 0, 0i32);
    let mut size = std::mem::size_of_val(&pid) as libc::socklen_t;
    // Both credentials come from the connected socket, never from the message.
    unsafe {
        if libc::getpeereid(fd, &mut uid, &mut gid) != 0
            || libc::getsockopt(
                fd,
                libc::SOL_LOCAL,
                libc::LOCAL_PEERPID,
                (&mut pid as *mut i32).cast(),
                &mut size,
            ) != 0
        {
            return Err(io::Error::last_os_error());
        }
    }
    if pid <= 0 || size as usize != std::mem::size_of_val(&pid) {
        return Err(io::ErrorKind::InvalidData.into());
    }
    Ok((uid, pid))
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn framing_and_peer_are_bounded() {
        use std::os::unix::net::UnixListener;
        let directory = tempfile::tempdir_in("/tmp").unwrap();
        let path = directory.path().join("sock");
        let listener = UnixListener::bind(&path).unwrap();
        let mut a = UnixStream::connect(&path).unwrap();
        let (mut b, _) = listener.accept().unwrap();
        send(&mut a, b"fixture").unwrap();
        assert_eq!(receive(&mut b).unwrap(), b"fixture");
        assert!(send(&mut a, &vec![0; MAX_PACKET + 1]).is_err());
        assert_eq!(
            peer(&b).unwrap(),
            (unsafe { libc::geteuid() }, std::process::id() as i32)
        );
    }
}
