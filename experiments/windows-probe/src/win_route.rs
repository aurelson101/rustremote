//! Direct SFTP transport boundary. No implicit OpenSSH config or executable proxy.
use rustremote_core::profiles::{ProtocolOptions, SessionProfile};
use std::{
    net::{TcpStream, ToSocketAddrs},
    os::windows::io::{AsRawSocket, RawSocket},
    path::PathBuf,
    time::Duration,
};
pub struct Route {
    pub profile: SessionProfile,
    pub key_host: String,
    pub key_port: u16,
    pub identities: Vec<PathBuf>,
}
pub struct Stream(TcpStream);
impl AsRawSocket for Stream {
    fn as_raw_socket(&self) -> RawSocket {
        self.0.as_raw_socket()
    }
}
impl Stream {
    pub fn try_clone(&self) -> std::io::Result<Self> {
        self.0.try_clone().map(Self)
    }
    pub fn shutdown(&self) {
        let _ = self.0.shutdown(std::net::Shutdown::Both);
    }
}
pub struct Proxy;
impl Route {
    pub fn resolve(profile: &SessionProfile) -> Result<Self, String> {
        crate::ssh_launch::arguments(profile)?;
        let ProtocolOptions::Ssh { identity_file, .. } = &profile.options else {
            return Err("SFTP requires SSH".into());
        };
        Ok(Self {
            profile: profile.clone(),
            key_host: profile.host.clone(),
            key_port: profile.port,
            identities: identity_file.iter().map(PathBuf::from).collect(),
        })
    }
    pub fn connect(&self) -> Result<(Stream, Option<Proxy>), String> {
        let addresses = (&*self.profile.host, self.profile.port)
            .to_socket_addrs()
            .map_err(|_| "SFTP host resolution failed")?;
        for address in addresses.take(4) {
            if let Ok(stream) = TcpStream::connect_timeout(&address, Duration::from_secs(5)) {
                stream
                    .set_read_timeout(Some(Duration::from_secs(5)))
                    .map_err(|_| "SFTP socket options failed")?;
                stream
                    .set_write_timeout(Some(Duration::from_secs(5)))
                    .map_err(|_| "SFTP socket options failed")?;
                return Ok((Stream(stream), None));
            }
        }
        Err("SFTP connection refused or timed out".into())
    }
}
