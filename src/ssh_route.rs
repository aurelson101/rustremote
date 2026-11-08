//! Résolution HostName/ProxyJump par OpenSSH, transport SFTP sans shell local.
use crate::profiles::{ProtocolOptions, SessionProfile};
use std::{
    io::Read,
    net::{TcpStream, ToSocketAddrs},
    os::fd::{AsRawFd, RawFd},
    os::unix::net::UnixStream,
    path::PathBuf,
    process::{Child, Command, Stdio},
    time::{Duration, Instant},
};

pub struct Route {
    pub profile: SessionProfile,
    pub key_host: String,
    pub key_port: u16,
    pub identities: Vec<PathBuf>,
    jumps: Vec<String>,
    config: Option<String>,
}
pub enum Stream {
    Tcp(TcpStream),
    Proxy(UnixStream),
}
impl AsRawFd for Stream {
    fn as_raw_fd(&self) -> RawFd {
        match self {
            Self::Tcp(s) => s.as_raw_fd(),
            Self::Proxy(s) => s.as_raw_fd(),
        }
    }
}
impl Stream {
    pub fn try_clone(&self) -> std::io::Result<Self> {
        match self {
            Self::Tcp(s) => s.try_clone().map(Self::Tcp),
            Self::Proxy(s) => s.try_clone().map(Self::Proxy),
        }
    }
    pub fn shutdown(&self) {
        match self {
            Self::Tcp(s) => {
                let _ = s.shutdown(std::net::Shutdown::Both);
            }
            Self::Proxy(s) => {
                let _ = s.shutdown(std::net::Shutdown::Both);
            }
        }
    }
}
pub struct Proxy(Child);
// Connection est libérée sur un worker, jamais en attente sur le thread GTK.
impl Drop for Proxy {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}
fn command() -> Command {
    let mut command = Command::new("/usr/bin/ssh");
    for name in [
        "LD_LIBRARY_PATH",
        "LD_PRELOAD",
        "SSH_ASKPASS",
        "SSH_ASKPASS_REQUIRE",
        "RUSTREMOTE_ASKPASS_SOCKET",
    ] {
        command.env_remove(name);
    }
    command.stderr(Stdio::null());
    command
}
fn expanded(value: &str) -> PathBuf {
    value
        .strip_prefix("~/")
        .and_then(|v| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(v)))
        .unwrap_or_else(|| PathBuf::from(value))
}
impl Route {
    pub fn resolve(profile: &SessionProfile) -> Result<Self, String> {
        let (_, args) = profile.command()?;
        let output = tempfile::tempfile().map_err(|_| "Fichier temporaire SSH indisponible")?;
        let mut child = command()
            .arg("-G")
            .args(args)
            .stdout(output.try_clone().map_err(|_| "Sortie SSH indisponible")?)
            .spawn()
            .map_err(|_| "Résolution OpenSSH indisponible")?;
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            if let Some(status) = child.try_wait().map_err(|_| "Suivi OpenSSH interrompu")? {
                if !status.success() {
                    return Err("Configuration OpenSSH invalide".into());
                }
                break;
            }
            if Instant::now() > deadline
                || output.metadata().map_err(|_| "Sortie SSH illisible")?.len() > 65536
            {
                let _ = child.kill();
                let _ = child.wait();
                return Err("Résolution OpenSSH limitée à 5 s et 64 Kio".into());
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        use std::io::{Seek, SeekFrom};
        let mut output = output;
        output
            .seek(SeekFrom::Start(0))
            .map_err(|_| "Sortie SSH illisible")?;
        let mut text = String::new();
        output
            .take(65537)
            .read_to_string(&mut text)
            .map_err(|_| "Sortie SSH invalide")?;
        if text.len() > 65536 {
            return Err("Configuration SSH trop volumineuse".into());
        }
        Self::parse(profile, &text)
    }
    fn parse(original: &SessionProfile, text: &str) -> Result<Self, String> {
        let mut profile = original.clone();
        let mut key_host = None;
        let mut identities = Vec::new();
        let mut jump = String::new();
        let mut proxy_command = false;
        let ProtocolOptions::Ssh {
            config_file,
            identity_file,
            agent,
            known_hosts_file,
        } = &mut profile.options
        else {
            return Err("Profil SSH requis".into());
        };
        let config = config_file.clone();
        let explicit_identity = identity_file.is_some();
        for line in text.lines() {
            let Some((name, value)) = line.split_once(' ') else {
                continue;
            };
            let value = value.trim();
            match name {
                "hostname" => profile.host = value.into(),
                "hostkeyalias" if value != "none" => key_host = Some(value.to_string()),
                "proxyjump" if value != "none" => jump = value.into(),
                "proxycommand" if value != "none" => proxy_command = true,
                "identityagent" if value == "none" => *agent = false,
                "identityfile" if !explicit_identity && value != "none" && identities.len() < 4 => {
                    let path = expanded(value);
                    if path.is_file() {
                        identities.push(path);
                    }
                }
                "userknownhostsfile" if known_hosts_file.is_none() => {
                    let full = expanded(value);
                    let selected = if full.is_file() {
                        Some(full)
                    } else {
                        value.split_whitespace().map(expanded).find(|p| p.is_file())
                    };
                    if let Some(path) = selected {
                        *known_hosts_file = Some(path.to_string_lossy().into());
                    }
                }
                _ => {}
            }
        }
        if let Some(path) = identity_file {
            identities = vec![PathBuf::from(path.as_str())];
        }
        let jumps = jump
            .split(',')
            .filter(|s| !s.is_empty())
            .map(str::to_string)
            .collect::<Vec<_>>();
        if jumps.len() > 4 || proxy_command {
            return Err("SFTP : ProxyCommand personnalisé non pris en charge ; utilisez ProxyJump (4 sauts maximum)".into());
        }
        for spec in &jumps {
            jump_parts(spec)?;
        }
        profile.validate()?;
        let key_port = if key_host.is_some() { 22 } else { profile.port };
        let key_host = key_host.unwrap_or_else(|| profile.host.clone());
        if key_host.is_empty() || key_host.len() > 253 || key_host.chars().any(char::is_control) {
            return Err("HostKeyAlias invalide".into());
        }
        Ok(Self {
            profile,
            key_host,
            key_port,
            identities,
            jumps,
            config,
        })
    }
    pub fn connect(&self) -> Result<(Stream, Option<Proxy>), String> {
        if let Some(last) = self.jumps.last() {
            let (host, user, port) = jump_parts(last)?;
            let (socket, child_socket) =
                UnixStream::pair().map_err(|_| "Transport du bastion indisponible")?;
            socket
                .set_read_timeout(Some(Duration::from_secs(15)))
                .map_err(|_| "Délai du bastion indisponible")?;
            socket
                .set_write_timeout(Some(Duration::from_secs(5)))
                .map_err(|_| "Délai du bastion indisponible")?;
            let mut command = command();
            command.args([
                "-T",
                "-o",
                "BatchMode=yes",
                "-o",
                "StrictHostKeyChecking=yes",
                "-o",
                "ForwardAgent=no",
                "-o",
                "PermitLocalCommand=no",
                "-o",
                "ClearAllForwardings=yes",
                "-o",
                "ControlMaster=no",
                "-o",
                "ControlPath=none",
                "-o",
                "ConnectTimeout=10",
            ]);
            // -W implique ClearAllForwardings ; il reste compatible avec le stdio forwarding.
            if let Some(path) = &self.config {
                command.args(["-F", path]);
            }
            if self.jumps.len() > 1 {
                command.args(["-J", &self.jumps[..self.jumps.len() - 1].join(",")]);
            }
            if let Some(user) = user {
                command.args(["-l", &user]);
            }
            if let Some(port) = port {
                command.args(["-p", &port.to_string()]);
            }
            let target = if self.profile.host.contains(':') {
                format!("[{}]:{}", self.profile.host, self.profile.port)
            } else {
                format!("{}:{}", self.profile.host, self.profile.port)
            };
            command.args(["-W", &target, "--", &host]);
            let input: std::os::fd::OwnedFd = child_socket
                .try_clone()
                .map_err(|_| "Transport bastion interrompu")?
                .into();
            let output: std::os::fd::OwnedFd = child_socket.into();
            let child = command
                .stdin(Stdio::from(input))
                .stdout(Stdio::from(output))
                .spawn()
                .map_err(|_| "Client du bastion indisponible")?;
            return Ok((Stream::Proxy(socket), Some(Proxy(child))));
        }
        let addresses = (self.profile.host.as_str(), self.profile.port)
            .to_socket_addrs()
            .map_err(|_| "Résolution du serveur impossible")?;
        let deadline = Instant::now() + Duration::from_secs(15);
        for address in addresses.take(4) {
            let Some(left) = deadline.checked_duration_since(Instant::now()) else {
                break;
            };
            if let Ok(socket) =
                TcpStream::connect_timeout(&address, left.min(Duration::from_secs(5)))
            {
                socket
                    .set_read_timeout(Some(Duration::from_secs(5)))
                    .map_err(|_| "Délai SFTP indisponible")?;
                socket
                    .set_write_timeout(Some(Duration::from_secs(5)))
                    .map_err(|_| "Délai SFTP indisponible")?;
                return Ok((Stream::Tcp(socket), None));
            }
        }
        Err("Connexion SFTP impossible (limite 15 secondes)".into())
    }
}
fn jump_parts(value: &str) -> Result<(String, Option<String>, Option<u16>), String> {
    if value.is_empty()
        || value.len() > 600
        || value.chars().any(|c| c.is_whitespace() || c.is_control())
    {
        return Err("Bastion SSH invalide".into());
    }
    let (user, host) = value
        .rsplit_once('@')
        .map_or((None, value), |(u, h)| (Some(u.to_string()), h));
    let (host, port) = if let Some(rest) = host.strip_prefix('[') {
        let (host, tail) = rest
            .split_once(']')
            .ok_or("Adresse IPv6 du bastion invalide")?;
        let port = if tail.is_empty() {
            None
        } else {
            Some(
                tail.strip_prefix(':')
                    .ok_or("Port bastion invalide")?
                    .parse::<u16>()
                    .map_err(|_| "Port bastion invalide")?,
            )
        };
        if host.parse::<std::net::Ipv6Addr>().is_err() {
            return Err("IPv6 du bastion invalide".into());
        }
        (host.to_string(), port)
    } else if let Some((host, port)) = host.rsplit_once(':') {
        (
            host.into(),
            Some(port.parse::<u16>().map_err(|_| "Port bastion invalide")?),
        )
    } else {
        (host.into(), None)
    };
    if port == Some(0)
        || host.is_empty()
        || host.starts_with('-')
        || !host
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"._-:".contains(&b))
        || user.as_ref().is_some_and(|u| {
            u.is_empty()
                || u.starts_with('-')
                || !u
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b"._-@\\".contains(&b))
        })
    {
        return Err("Bastion SSH invalide".into());
    }
    Ok((host, user, port))
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn aliases_keep_strict_host_identity_and_custom_commands_are_rejected() {
        let original = crate::profiles::Workspace::decode(include_bytes!(
            "../assets/simulations/serveur-ia.json"
        ))
        .unwrap()
        .profiles
        .remove(0);
        let route = Route::parse(&original,"hostname 127.0.0.1\nhostkeyalias trusted-alias\nproxyjump lab@[::1]:2222\nidentityagent none\n").unwrap();
        assert_eq!(route.profile.host, "127.0.0.1");
        assert_eq!(route.key_host, "trusted-alias");
        assert_eq!(route.key_port, 22);
        assert!(matches!(
            route.profile.options,
            ProtocolOptions::Ssh { agent: false, .. }
        ));
        assert!(
            Route::parse(
                &original,
                "hostname 127.0.0.1\nproxycommand arbitrary command\n"
            )
            .is_err()
        );
    }
    #[test]
    fn jump_arguments_reject_options_and_preserve_ipv6_user_port() {
        assert_eq!(
            jump_parts("lab@[::1]:2222").unwrap(),
            ("::1".into(), Some("lab".into()), Some(2222))
        );
        for spec in [
            "-oProxyCommand=bad",
            "host:0",
            "host:abc",
            "a b",
            "u@-oX",
            "[bad]:22",
        ] {
            assert!(jump_parts(spec).is_err());
        }
    }
}
