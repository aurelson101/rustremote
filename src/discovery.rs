//! Bounded, opt-in LAN TCP discovery. Optional system reverse resolution; no authentication or remote commands.
use std::{
    io::{Read, Write},
    net::{IpAddr, Ipv4Addr, SocketAddr, TcpStream},
    sync::{
        Mutex,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
    time::{Duration, Instant},
};
#[derive(Clone, Debug)]
pub struct Scope {
    addresses: Vec<IpAddr>,
}
impl Scope {
    pub fn parse(value: &str) -> Result<Self, String> {
        let (ip, prefix) = value.trim().split_once('/').map_or(
            (value.trim(), if value.contains(':') { "128" } else { "32" }),
            |p| p,
        );
        if let Ok(IpAddr::V6(ip)) = ip.parse::<IpAddr>() {
            let prefix = prefix.parse::<u32>().map_err(|_| "Préfixe CIDR invalide")?;
            if !(120..=128).contains(&prefix) {
                return Err("IPv6 : /120 à /128 requis (256 adresses maximum)".into());
            }
            let size = 1u128 << (128 - prefix);
            let base = u128::from(ip) & !(size - 1);
            let addresses = (0..size)
                .map(|i| IpAddr::V6((base + i).into()))
                .collect::<Vec<_>>();
            if addresses.iter().any(|ip| match ip {
                IpAddr::V6(v) => !v.is_loopback() && (v.segments()[0] & 0xfe00 != 0xfc00),
                _ => true,
            }) {
                return Err(
                    "IPv6 : loopback ou ULA uniquement ; lien local sans interface refusé".into(),
                );
            }
            return Ok(Self { addresses });
        }
        let ip = ip
            .parse::<Ipv4Addr>()
            .map_err(|_| "Adresse IPv4 ou CIDR privé requis")?;
        let prefix = prefix.parse::<u32>().map_err(|_| "Préfixe CIDR invalide")?;
        if !(24..=32).contains(&prefix) {
            return Err("Limite : un réseau /24 maximum (254 hôtes)".into());
        }
        let size = 1u32 << (32 - prefix);
        let base = u32::from(ip) & !(size - 1);
        let addresses = (0..size)
            .filter(|i| prefix >= 31 || (*i > 0 && *i < size - 1))
            .map(|i| Ipv4Addr::from(base + i))
            .collect::<Vec<_>>();
        if addresses
            .iter()
            .any(|ip| !ip.is_private() && !ip.is_loopback())
        {
            return Err("Seules les adresses privées et loopback sont autorisées".into());
        }
        Ok(Self {
            addresses: addresses.into_iter().map(IpAddr::V4).collect(),
        })
    }
    pub fn description(&self) -> String {
        format!(
            "{} — {} · {}",
            self.addresses[0],
            self.addresses[self.addresses.len() - 1],
            self.addresses.len()
        )
    }
}
#[derive(Clone, Debug)]
pub struct Device {
    pub address: IpAddr,
    pub ports: Vec<u16>,
    pub apple_ard: bool,
    pub name: Option<String>,
}
impl Device {
    pub fn hint(&self) -> &'static str {
        if self.apple_ard {
            "Apple ARD annoncé · système à confirmer"
        } else if self.ports.contains(&3389) {
            "RDP · Windows possible"
        } else if self.ports.contains(&5900) {
            "VNC · macOS/Linux possible"
        } else if self.ports.contains(&445) {
            "SMB · Windows/Linux possible"
        } else {
            "SSH · système non identifié"
        }
    }
}
fn read_before(stream: &mut TcpStream, buffer: &mut [u8], deadline: Instant) -> bool {
    let mut filled = 0;
    while filled < buffer.len() {
        let left = deadline.saturating_duration_since(Instant::now());
        if left.is_zero() || stream.set_read_timeout(Some(left)).is_err() {
            return false;
        }
        match stream.read(&mut buffer[filled..]) {
            Ok(0) | Err(_) => return false,
            Ok(n) => filled += n,
        }
    }
    true
}
fn advertises_ard(stream: &mut TcpStream) -> bool {
    let deadline = Instant::now() + Duration::from_millis(250);
    let _ = stream.set_write_timeout(Some(Duration::from_millis(50)));
    let mut greeting = [0; 12];
    if !read_before(stream, &mut greeting, deadline) || !greeting.starts_with(b"RFB 003.") {
        return false;
    }
    if stream.write_all(b"RFB 003.008\n").is_err() {
        return false;
    }
    let mut count = [0];
    if !read_before(stream, &mut count, deadline) || count[0] == 0 || count[0] > 32 {
        return false;
    }
    let mut types = vec![0; count[0] as usize];
    read_before(stream, &mut types, deadline) && types.contains(&30)
}
pub fn simulated() -> Vec<Device> {
    [
        (10, vec![3389, 445], false),
        (11, vec![22, 5900], true),
        (12, vec![22], false),
    ]
    .into_iter()
    .map(|(last, ports, apple_ard)| Device {
        address: Ipv4Addr::new(192, 0, 2, last).into(),
        ports,
        apple_ard,
        name: None,
    })
    .collect()
}
#[derive(Debug, PartialEq)]
pub enum ScanStatus {
    Simulated,
    Complete,
    Cancelled,
    TimedOut,
}
pub struct Report {
    pub devices: Vec<Device>,
    pub checked: usize,
    pub total: usize,
    pub status: ScanStatus,
}
impl Report {
    pub fn simulation() -> Self {
        let devices = simulated();
        Self {
            checked: devices.len(),
            total: devices.len(),
            devices,
            status: ScanStatus::Simulated,
        }
    }
    pub fn label(&self) -> &'static str {
        match self.status {
            ScanStatus::Simulated => "Simulation locale — aucun paquet réseau",
            ScanStatus::Complete => "Scan terminé",
            ScanStatus::Cancelled => "Scan annulé · résultats partiels",
            ScanStatus::TimedOut => "Délai atteint · résultats partiels",
        }
    }
}
static SCAN_ACTIVE: AtomicBool = AtomicBool::new(false);
struct ScanPermit;
impl ScanPermit {
    fn acquire() -> Result<Self, String> {
        SCAN_ACTIVE
            .compare_exchange(false, true, Ordering::Acquire, Ordering::Relaxed)
            .map(|_| Self)
            .map_err(|_| "Un scan est déjà en cours ; attendez ou annulez-le.".into())
    }
}
impl Drop for ScanPermit {
    fn drop(&mut self) {
        SCAN_ACTIVE.store(false, Ordering::Release);
    }
}
pub fn scan(scope: Scope, cancel: &AtomicBool, names: bool) -> Result<Report, String> {
    let _permit = ScanPermit::acquire()?;
    let deadline = Instant::now() + Duration::from_secs(30);
    let mut report = scan_ports(
        scope,
        &[22, 3389, 5900, 445],
        cancel,
        Duration::from_secs(30),
    );
    if names {
        for device in report.devices.iter_mut().take(32) {
            if cancel.load(Ordering::Relaxed) || Instant::now() >= deadline {
                break;
            }
            device.name = reverse_name(device.address, cancel, deadline);
        }
    }
    if cancel.load(Ordering::Relaxed) {
        report.status = ScanStatus::Cancelled;
    }
    Ok(report)
}
// NSS follows the host's configured DNS/mDNS policy. Names are untrusted display
// hints only; connection profiles always retain the explicitly scanned address.
fn reverse_name(address: IpAddr, cancel: &AtomicBool, deadline: Instant) -> Option<String> {
    use std::{
        os::fd::AsRawFd,
        process::{Command, Stdio},
    };
    if cancel.load(Ordering::Relaxed) || Instant::now() >= deadline {
        return None;
    }
    let mut command = Command::new("/usr/bin/getent");
    command
        .args(["hosts", &address.to_string()])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    for variable in [
        "LD_LIBRARY_PATH",
        "LD_PRELOAD",
        "GIO_MODULE_DIR",
        "GTK_PATH",
    ] {
        command.env_remove(variable);
    }
    let mut child = command.spawn().ok()?;
    let mut output = child.stdout.take()?;
    let fd = output.as_raw_fd();
    if unsafe { libc::fcntl(fd, libc::F_SETFL, libc::O_NONBLOCK) } < 0 {
        let _ = child.kill();
        let _ = child.wait();
        return None;
    }
    let deadline = deadline.min(Instant::now() + Duration::from_millis(800));
    let mut bytes = Vec::new();
    let mut successful = false;
    loop {
        let mut buffer = [0u8; 512];
        match output.read(&mut buffer) {
            Ok(n) => bytes.extend_from_slice(&buffer[..n]),
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {}
            Err(_) => break,
        }
        if bytes.len() > 1024 || cancel.load(Ordering::Relaxed) || Instant::now() >= deadline {
            break;
        }
        match child.try_wait() {
            Ok(Some(status)) => {
                // Drain the small final pipe tail after the process has exited.
                let _ = output
                    .take(1025 - bytes.len() as u64)
                    .read_to_end(&mut bytes);
                successful = status.success();
                break;
            }
            Ok(None) => std::thread::sleep(Duration::from_millis(10)),
            Err(_) => break,
        }
    }
    let _ = child.kill();
    let _ = child.wait();
    if !successful || bytes.len() > 1024 {
        return None;
    }
    parse_name(address, &bytes)
}
fn parse_name(address: IpAddr, bytes: &[u8]) -> Option<String> {
    let text = std::str::from_utf8(bytes).ok()?;
    let mut fields = text.split_whitespace();
    if fields.next()?.parse::<IpAddr>().ok()? != address {
        return None;
    }
    let name = fields.next()?;
    (name.len() <= 253
        && !name.is_empty()
        && name
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || c == b'-' || c == b'.'))
    .then(|| name.to_owned())
}

fn scan_ports(scope: Scope, ports: &[u16], cancel: &AtomicBool, budget: Duration) -> Report {
    let index = AtomicUsize::new(0);
    let results = Mutex::new(Vec::new());
    let deadline = Instant::now() + budget;
    let checked = AtomicUsize::new(0);
    std::thread::scope(|threads| {
        for _ in 0..8.min(scope.addresses.len()) {
            threads.spawn(|| {
                loop {
                    if cancel.load(Ordering::Relaxed) || Instant::now() >= deadline {
                        break;
                    }
                    let i = index.fetch_add(1, Ordering::Relaxed);
                    let Some(address) = scope.addresses.get(i).copied() else {
                        break;
                    };
                    let mut device = Device {
                        address,
                        ports: Vec::new(),
                        apple_ard: false,
                        name: None,
                    };
                    let mut complete = true;
                    for port in ports {
                        if cancel.load(Ordering::Relaxed) || Instant::now() >= deadline {
                            complete = false;
                            break;
                        }
                        if let Ok(mut socket) = TcpStream::connect_timeout(
                            &SocketAddr::from((address, *port)),
                            Duration::from_millis(180),
                        ) {
                            device.ports.push(*port);
                            if *port == 5900 {
                                device.apple_ard = advertises_ard(&mut socket);
                            }
                        }
                    }
                    if complete {
                        checked.fetch_add(1, Ordering::Relaxed);
                    }
                    if !device.ports.is_empty() {
                        results.lock().expect("discovery result lock").push(device);
                    }
                }
            });
        }
    });
    let mut results = results.into_inner().expect("discovery results");
    results.sort_by_key(|d| d.address);
    let checked = checked.into_inner();
    let status = if cancel.load(Ordering::Relaxed) {
        ScanStatus::Cancelled
    } else if checked == scope.addresses.len() {
        ScanStatus::Complete
    } else {
        ScanStatus::TimedOut
    };
    Report {
        devices: results,
        checked,
        total: scope.addresses.len(),
        status,
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn scopes_are_private_bounded_and_explicit() {
        assert_eq!(
            Scope::parse("192.168.3.99/24").unwrap().addresses.len(),
            254
        );
        assert_eq!(
            Scope::parse("127.0.0.1").unwrap().addresses,
            vec![IpAddr::V4(Ipv4Addr::LOCALHOST)]
        );
        for bad in [
            "8.8.8.8",
            "192.168.1.0/23",
            "localhost",
            "169.254.169.254",
            "2001:4860::1",
            "fe80::1",
            "fd00::/64",
            "0.0.0.0",
            "192.168.1.1/33",
        ] {
            assert!(Scope::parse(bad).is_err(), "{bad}");
        }
    }
    #[test]
    fn loopback_probe_and_cancellation_do_not_authenticate() {
        let listener = std::net::TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
        let port = listener.local_addr().unwrap().port();
        let scope = Scope::parse("127.0.0.1").unwrap();
        let cancelled = scan_ports(
            scope.clone(),
            &[port],
            &AtomicBool::new(true),
            Duration::from_secs(1),
        );
        assert_eq!(cancelled.status, ScanStatus::Cancelled);
        assert_eq!(cancelled.checked, 0);
        assert!(cancelled.devices.is_empty());
        let result = scan_ports(
            scope,
            &[port],
            &AtomicBool::new(false),
            Duration::from_secs(1),
        );
        assert_eq!(result.status, ScanStatus::Complete);
        assert_eq!(result.checked, 1);
        assert_eq!(result.devices[0].ports, vec![port]);
        assert!(!result.devices[0].apple_ard);
    }
    #[test]
    fn reverse_names_are_bounded_validated_and_cancellable() {
        let ip = IpAddr::V4(Ipv4Addr::LOCALHOST);
        assert_eq!(
            parse_name(ip, b"127.0.0.1 localhost\n"),
            Some("localhost".into())
        );
        assert!(parse_name(ip, b"127.0.0.2 forged.local").is_none());
        assert!(parse_name(ip, b"127.0.0.1 <markup>").is_none());
        assert!(
            reverse_name(
                ip,
                &AtomicBool::new(true),
                Instant::now() + Duration::from_secs(1)
            )
            .is_none()
        );
        assert!(
            reverse_name(
                ip,
                &AtomicBool::new(false),
                Instant::now() + Duration::from_secs(1)
            )
            .is_some()
        );
    }
    #[test]
    fn ipv6_ula_scopes_and_loopback_transport() {
        assert_eq!(Scope::parse("fd12::/120").unwrap().addresses.len(), 256);
        assert_eq!(Scope::parse("::1").unwrap().addresses.len(), 1);
        let listener = std::net::TcpListener::bind((std::net::Ipv6Addr::LOCALHOST, 0)).unwrap();
        let report = scan_ports(
            Scope::parse("::1").unwrap(),
            &[listener.local_addr().unwrap().port()],
            &AtomicBool::new(false),
            Duration::from_secs(1),
        );
        assert_eq!(report.status, ScanStatus::Complete);
        assert_eq!(report.devices.len(), 1);
    }
    #[test]
    fn apple_advertisement_is_read_without_selecting_authentication() {
        let listener = std::net::TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
        let address = listener.local_addr().unwrap();
        let server = std::thread::spawn(move || {
            let (mut s, _) = listener.accept().unwrap();
            s.set_read_timeout(Some(Duration::from_secs(2))).unwrap();
            s.write_all(b"RFB 003.889\n").unwrap();
            let mut greeting = [0; 12];
            s.read_exact(&mut greeting).unwrap();
            assert_eq!(&greeting, b"RFB 003.008\n");
            s.write_all(&[2, 2, 30]).unwrap();
            let mut selected = [0];
            assert_eq!(s.read(&mut selected).unwrap(), 0);
        });
        let mut client = TcpStream::connect(address).unwrap();
        assert!(advertises_ard(&mut client));
        drop(client);
        server.join().unwrap();
    }
    #[test]
    fn simultaneous_scans_are_refused_and_permit_is_released() {
        let permit = ScanPermit::acquire().unwrap();
        assert!(
            scan(
                Scope::parse("127.0.0.1").unwrap(),
                &AtomicBool::new(false),
                false
            )
            .is_err()
        );
        drop(permit);
        assert!(ScanPermit::acquire().is_ok());
    }
    #[test]
    fn deadline_is_reported_without_claiming_a_complete_inventory() {
        let result = scan_ports(
            Scope::parse("127.0.0.1").unwrap(),
            &[22],
            &AtomicBool::new(false),
            Duration::ZERO,
        );
        assert_eq!(result.status, ScanStatus::TimedOut);
        assert_eq!(result.checked, 0);
        assert_eq!(result.total, 1);
        assert!(result.devices.is_empty());
    }
    #[test]
    fn incomplete_rfb_banner_cannot_hold_the_probe_open() {
        let listener = std::net::TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
        let address = listener.local_addr().unwrap();
        let server = std::thread::spawn(move || {
            let (mut socket, _) = listener.accept().unwrap();
            socket.write_all(b"RFB").unwrap();
            std::thread::sleep(Duration::from_millis(400));
        });
        let mut client = TcpStream::connect(address).unwrap();
        let started = Instant::now();
        assert!(!advertises_ard(&mut client));
        assert!(started.elapsed() < Duration::from_secs(1));
        server.join().unwrap();
    }
    #[test]
    fn excessive_rfb_security_count_is_refused() {
        let listener = std::net::TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
        let address = listener.local_addr().unwrap();
        let server = std::thread::spawn(move || {
            let (mut socket, _) = listener.accept().unwrap();
            socket
                .set_read_timeout(Some(Duration::from_secs(2)))
                .unwrap();
            socket.write_all(b"RFB 003.008\n").unwrap();
            let mut greeting = [0; 12];
            socket.read_exact(&mut greeting).unwrap();
            socket.write_all(&[255]).unwrap();
        });
        let mut client = TcpStream::connect(address).unwrap();
        assert!(!advertises_ard(&mut client));
        server.join().unwrap();
    }
}
