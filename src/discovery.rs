//! Bounded, opt-in LAN TCP discovery. No DNS, authentication or remote commands.
use std::{
    io::{Read, Write},
    net::{Ipv4Addr, SocketAddr, TcpStream},
    sync::{
        Mutex,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
    time::{Duration, Instant},
};
#[derive(Clone, Debug)]
pub struct Scope {
    addresses: Vec<Ipv4Addr>,
}
impl Scope {
    pub fn parse(value: &str) -> Result<Self, String> {
        let (ip, prefix) = value
            .trim()
            .split_once('/')
            .map_or((value.trim(), "32"), |p| p);
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
        Ok(Self { addresses })
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
    pub address: Ipv4Addr,
    pub ports: Vec<u16>,
    pub apple_ard: bool,
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
        address: Ipv4Addr::new(192, 0, 2, last),
        ports,
        apple_ard,
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
pub fn scan(scope: Scope, cancel: &AtomicBool) -> Result<Report, String> {
    let _permit = ScanPermit::acquire()?;
    Ok(scan_ports(
        scope,
        &[22, 3389, 5900, 445],
        cancel,
        Duration::from_secs(30),
    ))
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
            vec![Ipv4Addr::LOCALHOST]
        );
        for bad in [
            "8.8.8.8",
            "192.168.1.0/23",
            "localhost",
            "169.254.169.254",
            "::1",
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
        assert!(scan(Scope::parse("127.0.0.1").unwrap(), &AtomicBool::new(false)).is_err());
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
