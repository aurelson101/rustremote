use serde::{Deserialize, Serialize};

pub fn freerdp_binary() -> &'static str {
    if std::path::Path::new("/usr/bin/xfreerdp3").is_file() {
        "/usr/bin/xfreerdp3"
    } else if std::path::Path::new("/usr/bin/xfreerdp").is_file() {
        "/usr/bin/xfreerdp"
    } else {
        "/usr/bin/xfreerdp3"
    }
}

pub fn ssh_identity(user: &str, domain: &str) -> Result<String, String> {
    let user = user.trim();
    let domain = domain.trim();
    if user.is_empty() {
        return Err("Identifiant SSH requis".into());
    }
    if domain.is_empty() {
        return Ok(user.to_string());
    }
    if user.contains(['@', '\\']) {
        return Err("Pour SSH, indiquez soit l’identifiant complet, soit l’utilisateur et le domaine séparés".into());
    }
    if domain.len() > 253
        || domain.starts_with('-')
        || !domain
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"._-".contains(&b))
    {
        return Err("Domaine SSH invalide".into());
    }
    Ok(format!("{user}@{domain}"))
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SessionProfile {
    pub name: String,
    pub host: String,
    pub port: u16,
    pub username: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub credential_ref: Option<String>,
    pub favorite: bool,
    #[serde(default)]
    pub folder: String,
    #[serde(default)]
    pub platform: Platform,
    #[serde(default)]
    pub simulation: bool,
    pub options: ProtocolOptions,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "protocol", deny_unknown_fields)]
pub enum ProtocolOptions {
    Vnc {
        #[serde(default)]
        tunnel: Option<SshTunnel>,
        #[serde(default)]
        view_only: bool,
    },
    Serial {
        device: String,
        baud: u32,
        data_bits: u8,
        parity: String,
        stop_bits: u8,
        flow: String,
    },
    Ssh {
        identity_file: Option<String>,
        agent: bool,
        #[serde(default)]
        known_hosts_file: Option<String>,
        #[serde(default)]
        config_file: Option<String>,
    },
    Rdp {
        domain: String,
        width: u16,
        height: u16,
        fullscreen: bool,
        clipboard: bool,
        #[serde(default)]
        clipboard_files: bool,
        #[serde(default)]
        settings: RdpSettings,
    },
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SshTunnel {
    pub host: String,
    pub port: u16,
    pub username: String,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub enum Platform {
    #[default]
    Unknown,
    Linux,
    Windows,
    MacOs,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct RdpSettings {
    pub size_mode: u8,
    pub multi_monitor: bool,
    pub monitors: Vec<u16>,
    pub audio_mode: u8,
    pub microphone: bool,
    pub printers: bool,
    pub certificate_sha256: Option<String>,
}
impl Default for RdpSettings {
    fn default() -> Self {
        Self {
            size_mode: 1,
            multi_monitor: false,
            monitors: Vec::new(),
            audio_mode: 2,
            microphone: false,
            printers: false,
            certificate_sha256: None,
        }
    }
}
impl RdpSettings {
    pub fn validate(&self) -> Result<(), String> {
        if self.size_mode > 2
            || self.audio_mode > 2
            || self.monitors.len() > 16
            || self.monitors.iter().any(|m| *m > 15)
            || self
                .monitors
                .iter()
                .collect::<std::collections::HashSet<_>>()
                .len()
                != self.monitors.len()
            || (!self.multi_monitor && !self.monitors.is_empty())
        {
            return Err("Options d’affichage/audio RDP invalides.".into());
        }
        if self
            .certificate_sha256
            .as_ref()
            .is_some_and(|p| p.len() != 64 || !p.bytes().all(|b| b.is_ascii_hexdigit()))
        {
            return Err(
                "L’empreinte de certificat doit contenir 64 chiffres hexadécimaux SHA256.".into(),
            );
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub enum SessionKind {
    Ssh,
    Rdp,
    Vnc,
    Serial,
}

impl SessionProfile {
    pub fn vnc_tunnel_command(&self) -> Result<Option<String>, String> {
        self.validate()?;
        Ok(match &self.options {
            ProtocolOptions::Vnc {
                tunnel: Some(tunnel),
                ..
            } => Some(format!(
                r#"/usr/bin/ssh -T -oStrictHostKeyChecking=ask -oForwardAgent=no -oPermitLocalCommand=no -oExitOnForwardFailure=yes -oConnectTimeout=15 -p {} -f -L "127.0.0.1:$L:$H:$R" "$G" sleep 20"#,
                tunnel.port
            )),
            _ => None,
        })
    }
    pub fn kind(&self) -> SessionKind {
        match self.options {
            ProtocolOptions::Ssh { .. } => SessionKind::Ssh,
            ProtocolOptions::Rdp { .. } => SessionKind::Rdp,
            ProtocolOptions::Vnc { .. } => SessionKind::Vnc,
            ProtocolOptions::Serial { .. } => SessionKind::Serial,
        }
    }

    pub fn validate(&self) -> Result<(), String> {
        if self.name.trim().is_empty()
            || self.name.len() > 100
            || self.name.chars().any(char::is_control)
        {
            return Err("Nom requis (100 caractères maximum, sans caractères de contrôle).".into());
        }
        if !self.folder.is_empty() {
            validate_folder(&self.folder)?;
        }
        if let ProtocolOptions::Serial {
            device,
            baud,
            data_bits,
            parity,
            stop_bits,
            flow,
        } = &self.options
        {
            if !(device.starts_with("/dev/ttyUSB")
                || device.starts_with("/dev/ttyACM")
                || device.starts_with("/dev/ttyS")
                || device.starts_with("/dev/serial/by-id/")
                || device.starts_with("/dev/pts/"))
                || device.split('/').any(|p| p == "..")
                || !device
                    .bytes()
                    .all(|c| c.is_ascii_alphanumeric() || b"/._-:".contains(&c))
                || ![9600, 19200, 38400, 57600, 115200, 230400].contains(baud)
                || ![7, 8].contains(data_bits)
                || ![1, 2].contains(stop_bits)
                || !["n", "e", "o"].contains(&parity.as_str())
                || !["n", "h", "s"].contains(&flow.as_str())
            {
                return Err("Port série ou paramètres invalides (périphérique /dev autorisé, débit standard, 7/8 bits, parité n/e/o, arrêt 1/2, flux n/h/s).".into());
            }
            return Ok(());
        }
        let host = &self.host;
        let valid_dns = !host.is_empty()
            && host.len() <= 253
            && host.split('.').all(|part| {
                !part.is_empty()
                    && part.len() <= 63
                    && !part.starts_with('-')
                    && !part.ends_with('-')
                    && part.bytes().all(|c| c.is_ascii_alphanumeric() || c == b'-')
            });
        if host.parse::<std::net::IpAddr>().is_err() && !valid_dns {
            return Err(
                "Hôte invalide : indiquez une IP ou un nom DNS, sans URL ni options.".into(),
            );
        }
        if self
            .credential_ref
            .as_ref()
            .is_some_and(|r| !crate::credentials::valid_reference(r))
        {
            return Err("Référence de coffre invalide".into());
        }
        if self.port == 0
            || (self.username.is_empty() && !matches!(self.options, ProtocolOptions::Vnc { .. }))
            || self.username.len() > 320
            || !self
                .username
                .bytes()
                .all(|c| c.is_ascii_alphanumeric() || b"._-@\\".contains(&c))
            || self.username.starts_with('-')
            || self.username.matches('\\').count() > 1
            || self.username.starts_with('\\')
            || self.username.ends_with('\\')
            || (self.username.contains('\\') && self.username.contains('@'))
        {
            return Err("Port (1–65535) et utilisateur valides requis.".into());
        }
        if self.platform == Platform::MacOs
            && let ProtocolOptions::Vnc { tunnel, .. } = &self.options
        {
            if self.username.is_empty() || self.username.len() >= 64 {
                return Err("Apple ARD : compte macOS de 1 à 63 octets requis.".into());
            }
            if tunnel
                .as_ref()
                .is_none_or(|t| !t.host.eq_ignore_ascii_case(&self.host))
            {
                return Err("Apple ARD : tunnel SSH vers le même Mac obligatoire.".into());
            }
        }
        match &self.options {
            ProtocolOptions::Vnc {
                tunnel: Some(tunnel),
                ..
            } => {
                let gateway = SessionProfile {
                    name: "Tunnel SSH".into(),
                    host: tunnel.host.clone(),
                    port: tunnel.port,
                    username: tunnel.username.clone(),
                    credential_ref: None,
                    favorite: false,
                    folder: String::new(),
                    platform: Platform::Unknown,
                    simulation: false,
                    options: ProtocolOptions::Ssh {
                        identity_file: None,
                        agent: true,
                        known_hosts_file: None,
                        config_file: None,
                    },
                };
                gateway.validate()?;
            }
            ProtocolOptions::Ssh {
                identity_file: Some(path),
                ..
            } => {
                if !std::path::Path::new(path).is_absolute() || path.chars().any(char::is_control) {
                    return Err(
                        "Le chemin de clé SSH doit être absolu et sans caractères de contrôle."
                            .into(),
                    );
                }
            }
            ProtocolOptions::Rdp {
                domain,
                width,
                height,
                clipboard,
                clipboard_files,
                settings,
                ..
            } => {
                settings.validate()?;
                if *clipboard_files && !*clipboard {
                    return Err("Activez le presse-papiers RDP avant les fichiers.".into());
                }
                if !domain
                    .bytes()
                    .all(|c| c.is_ascii_alphanumeric() || b"._-".contains(&c))
                    || !(640..=7680).contains(width)
                    || !(480..=4320).contains(height)
                {
                    return Err("Domaine ou résolution RDP invalide (640×480 à 7680×4320).".into());
                }
            }
            _ => {}
        }
        if let ProtocolOptions::Ssh {
            known_hosts_file: Some(path),
            ..
        } = &self.options
            && (!std::path::Path::new(path).is_absolute()
                || path.len() > 4096
                || path.chars().any(char::is_control)
                || path.contains(['%', '$']))
        {
            return Err("Fichier known_hosts invalide : chemin absolu, sans caractères de contrôle ni expansions %/$ requis.".into());
        }
        if let ProtocolOptions::Ssh {
            config_file: Some(path),
            ..
        } = &self.options
            && (!std::path::Path::new(path).is_absolute()
                || path.len() > 4096
                || path.chars().any(char::is_control))
        {
            return Err(
                "Configuration SSH : chemin absolu sans caractères de contrôle requis.".into(),
            );
        }
        Ok(())
    }

    pub fn command(&self) -> Result<(&'static str, Vec<String>), String> {
        self.validate()?;
        if self.simulation {
            return Err("Profil de simulation : toute connexion réseau est interdite.".into());
        }
        match &self.options {
            ProtocolOptions::Vnc { tunnel, view_only } => {
                let mut args = vec![
                    "-SecurityTypes".into(),
                    if self.platform == Platform::MacOs {
                        "DH".into()
                    } else if tunnel.is_some() {
                        "VncAuth".into()
                    } else {
                        "X509Vnc".into()
                    },
                    "-AcceptClipboard=0".into(),
                    "-SendClipboard=0".into(),
                ];
                if let Some(tunnel) = tunnel {
                    args.extend([
                        "-via".into(),
                        format!("{}@{}", tunnel.username, tunnel.host),
                    ]);
                } else {
                    args.extend([
                        "-X509CA".into(),
                        "/etc/ssl/certs/ca-certificates.crt".into(),
                    ]);
                }
                if *view_only {
                    args.push("-ViewOnly=1".into());
                }
                let host = if self.platform == Platform::MacOs {
                    "127.0.0.1".to_string()
                } else if self.host.contains(':') {
                    format!("[{}]", self.host)
                } else {
                    self.host.clone()
                };
                args.push(format!("{host}::{}", self.port));
                Ok(("/usr/bin/xtigervncviewer", args))
            }
            ProtocolOptions::Serial {
                device,
                baud,
                data_bits,
                parity,
                stop_bits,
                flow,
            } => Ok((
                "/usr/bin/picocom",
                vec![
                    "--baud".into(),
                    baud.to_string(),
                    "--databits".into(),
                    data_bits.to_string(),
                    "--parity".into(),
                    parity.clone(),
                    "--stopbits".into(),
                    stop_bits.to_string(),
                    "--flow".into(),
                    flow.clone(),
                    "--".into(),
                    device.clone(),
                ],
            )),
            ProtocolOptions::Ssh {
                identity_file,
                agent,
                known_hosts_file,
                config_file,
            } => {
                let mut args: Vec<String> = [
                    "-o",
                    "StrictHostKeyChecking=ask",
                    "-o",
                    "ForwardAgent=no",
                    "-o",
                    "PermitLocalCommand=no",
                    "-o",
                    "ClearAllForwardings=yes",
                    "-o",
                    "ConnectTimeout=15",
                    "-p",
                ]
                .iter()
                .map(|s| s.to_string())
                .collect();
                args.push(self.port.to_string());
                if let Some(path) = config_file {
                    args.extend(["-F".into(), path.clone()]);
                }
                if let Some(path) = known_hosts_file {
                    args.extend([
                        "-o".into(),
                        format!(
                            "UserKnownHostsFile=\"{}\"",
                            path.replace('\\', "\\\\").replace('"', "\\\"")
                        ),
                    ]);
                }
                if !agent {
                    args.extend(["-o".into(), "IdentityAgent=none".into()]);
                }
                if let Some(key) = identity_file {
                    args.extend([
                        "-o".into(),
                        "IdentitiesOnly=yes".into(),
                        "-i".into(),
                        key.clone(),
                    ]);
                }
                args.extend([
                    "-l".into(),
                    self.username.clone(),
                    "--".into(),
                    self.host.clone(),
                ]);
                Ok(("/usr/bin/ssh", args))
            }
            ProtocolOptions::Rdp {
                domain,
                width,
                height,
                fullscreen,
                clipboard,
                clipboard_files,
                settings,
            } => {
                let host = if self.host.contains(':') {
                    format!("[{}]", self.host)
                } else {
                    self.host.clone()
                };
                let mut args = vec![
                    format!("/v:{host}:{}", self.port),
                    format!("/u:{}", self.username),
                    "/sec:nla".into(),
                    settings
                        .certificate_sha256
                        .as_ref()
                        .map(|pin| format!("/cert:fingerprint:sha256:{pin}"))
                        .unwrap_or_else(|| "/cert:deny".into()),
                    "/from-stdin:force".into(),
                    format!("/size:{width}x{height}"),
                    if settings.size_mode == 1 {
                        "+dynamic-resolution".into()
                    } else {
                        "-dynamic-resolution".into()
                    },
                    format!("/audio-mode:{}", settings.audio_mode),
                    "-drives".into(),
                ];
                if !domain.is_empty() {
                    args.push(format!("/d:{domain}"));
                }
                args.push(
                    if *clipboard {
                        if *clipboard_files {
                            "/clipboard:direction-to:all,files-to:all"
                        } else {
                            "/clipboard:direction-to:all,files-to:off"
                        }
                    } else {
                        "-clipboard"
                    }
                    .into(),
                );
                if settings.size_mode == 2 {
                    args.push("/smart-sizing".into());
                }
                if settings.multi_monitor {
                    args.push("/multimon".into());
                    if !settings.monitors.is_empty() {
                        args.push(format!(
                            "/monitors:{}",
                            settings
                                .monitors
                                .iter()
                                .map(u16::to_string)
                                .collect::<Vec<_>>()
                                .join(",")
                        ));
                    }
                }
                if *fullscreen || settings.multi_monitor {
                    args.push("/f".into());
                }
                if settings.audio_mode == 0 {
                    args.push("/sound".into());
                }
                if settings.microphone {
                    args.push("/microphone".into());
                }
                if settings.printers {
                    args.push("/printer".into());
                }
                Ok((freerdp_binary(), args))
            }
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum SessionState {
    Disconnected,
    Launching,
}

#[cfg(test)]
mod tests {
    use super::*;
    pub fn example() -> SessionProfile {
        SessionProfile {
            name: "Test local".into(),
            host: "example.invalid".into(),
            port: 22,
            username: "demo".into(),
            credential_ref: None,
            favorite: true,
            folder: String::new(),
            platform: Platform::Unknown,
            simulation: false,
            options: ProtocolOptions::Ssh {
                identity_file: None,
                agent: true,
                known_hosts_file: None,
                config_file: None,
            },
        }
    }
    #[test]
    fn domain_identities_and_import_do_not_borrow_credentials() {
        assert_eq!(
            ssh_identity("alice", "lab.example").unwrap(),
            "alice@lab.example"
        );
        assert_eq!(ssh_identity("LAB\\alice", "").unwrap(), "LAB\\alice");
        for (user, domain) in [
            ("", "lab"),
            ("alice@lab", "other"),
            ("alice", "-bad"),
            ("alice", "bad\nrealm"),
        ] {
            assert!(ssh_identity(user, domain).is_err());
        }
        let mut profile = example();
        profile.credential_ref = Some("e0a306ec-71ee-44d0-bc89-dcf7f657135a".into());
        let data = Workspace {
            profiles: vec![profile],
            ..Workspace::default()
        };
        let imported = Workspace::default().merge(&data).unwrap();
        assert!(imported.profiles[0].credential_ref.is_none());
        assert!(data.profiles[0].credential_ref.is_some());
    }
    #[test]
    fn apple_ard_requires_same_mac_tunnel_and_never_downgrades() {
        let mut p = example();
        p.platform = Platform::MacOs;
        p.port = 5900;
        p.options = ProtocolOptions::Vnc {
            tunnel: None,
            view_only: false,
        };
        assert!(p.validate().is_err());
        p.options = ProtocolOptions::Vnc {
            tunnel: Some(SshTunnel {
                host: p.host.clone(),
                port: 22,
                username: "macadmin".into(),
            }),
            view_only: false,
        };
        let (_, args) = p.command().unwrap();
        assert_eq!(args[1], "DH");
        assert_eq!(args.last().unwrap(), "127.0.0.1::5900");
        assert!(!args.iter().any(|a| a.contains("VncAuth") || a == "None"));
        assert!(
            p.vnc_tunnel_command()
                .unwrap()
                .unwrap()
                .contains("StrictHostKeyChecking=ask")
        );
        let copy =
            serde_json::from_str::<SessionProfile>(&serde_json::to_string(&p).unwrap()).unwrap();
        assert_eq!(copy, p);
        if let ProtocolOptions::Vnc {
            tunnel: Some(t), ..
        } = &mut p.options
        {
            t.host = "other.invalid".into();
        }
        assert!(p.validate().is_err());
        p.simulation = true;
        assert!(p.command().is_err());
    }
    #[test]
    fn serialization_and_options() {
        let p = example();
        assert_eq!(p.kind(), SessionKind::Ssh);
        let text = serde_json::to_string(&p).unwrap();
        assert_eq!(serde_json::from_str::<SessionProfile>(&text).unwrap(), p);
        assert!(!text.contains("password"));
        let (_, args) = p.command().unwrap();
        assert!(args.contains(&"StrictHostKeyChecking=ask".into()));
        assert!(args.contains(&"ForwardAgent=no".into()));
        assert_eq!(args[args.len() - 2], "--");
    }
    #[test]
    fn ssh_trust_and_config_paths_are_validated() {
        let mut profile = example();
        profile.options = ProtocolOptions::Ssh {
            identity_file: None,
            agent: true,
            known_hosts_file: Some("/tmp/known hosts".into()),
            config_file: Some("/dev/null".into()),
        };
        let args = profile.command().unwrap().1;
        assert!(args.windows(2).any(|pair| pair == ["-F", "/dev/null"]));
        assert!(
            args.iter()
                .any(|value| value == "UserKnownHostsFile=\"/tmp/known hosts\"")
        );
        for path in ["relative", "/tmp/%h", "/tmp/${HOME}", "/tmp/x\n"] {
            if let ProtocolOptions::Ssh {
                known_hosts_file, ..
            } = &mut profile.options
            {
                *known_hosts_file = Some(path.into());
            }
            assert!(profile.validate().is_err());
        }
    }
    #[test]
    fn vnc_requires_x509_or_explicit_ssh_tunnel() {
        let mut profile = example();
        profile.port = 5900;
        profile.username.clear();
        profile.options = ProtocolOptions::Vnc {
            tunnel: None,
            view_only: true,
        };
        let (client, args) = profile.command().unwrap();
        assert_eq!(client, "/usr/bin/xtigervncviewer");
        assert!(args.iter().any(|s| s == "X509Vnc"));
        assert!(!args.iter().any(|s| s == "None"));
        profile.options = ProtocolOptions::Vnc {
            tunnel: Some(SshTunnel {
                host: "bastion.example.invalid".into(),
                port: 2222,
                username: "lab".into(),
            }),
            view_only: false,
        };
        let (_, args) = profile.command().unwrap();
        assert!(args.iter().any(|s| s == "lab@bastion.example.invalid"));
        let tunnel = profile.vnc_tunnel_command().unwrap().unwrap();
        assert!(
            tunnel.contains("-p 2222")
                && tunnel.contains("127.0.0.1:$L")
                && tunnel.contains("StrictHostKeyChecking=ask")
        );
        if let ProtocolOptions::Vnc {
            tunnel: Some(t), ..
        } = &mut profile.options
        {
            t.host = "host;id".into();
        }
        assert!(profile.command().is_err());
    }
    #[test]
    fn serial_device_options_and_simulation_are_validated() {
        let mut profile = example();
        profile.host.clear();
        profile.username.clear();
        profile.options = ProtocolOptions::Serial {
            device: "/dev/ttyUSB0".into(),
            baud: 115200,
            data_bits: 8,
            parity: "n".into(),
            stop_bits: 1,
            flow: "n".into(),
        };
        let (binary, args) = profile.command().unwrap();
        assert_eq!(binary, "/usr/bin/picocom");
        assert_eq!(args.last().unwrap(), "/dev/ttyUSB0");
        assert!(args.iter().any(|s| s == "115200"));
        for path in ["/etc/passwd", "/dev/pts/../ttyUSB0", "/dev/ttyUSB0;id"] {
            if let ProtocolOptions::Serial { device, .. } = &mut profile.options {
                *device = path.into();
            }
            assert!(profile.validate().is_err());
        }
        if let ProtocolOptions::Serial { device, baud, .. } = &mut profile.options {
            *device = "/dev/pts/123".into();
            *baud = 123;
        }
        assert!(profile.validate().is_err());
        if let ProtocolOptions::Serial { baud, .. } = &mut profile.options {
            *baud = 9600;
        }
        profile.simulation = true;
        assert!(profile.command().is_err());
        assert!(crate::simulation::switch_response("show version").contains("Simulator"));
        assert!(crate::simulation::switch_response("reload; rm").contains("lecture seule"));
    }
    #[test]
    fn rejects_secret_fields() {
        let mut value = serde_json::to_value(example()).unwrap();
        value["password"] = serde_json::json!("fixture-only");
        assert!(serde_json::from_value::<SessionProfile>(value).is_err());
    }
    #[test]
    fn rejects_option_injection_and_bad_values() {
        for host in [
            "-oProxyCommand=bad",
            "a;touch /tmp/x",
            "ssh://host",
            "host\nother",
            "a/b",
        ] {
            let mut p = example();
            p.host = host.into();
            assert!(p.command().is_err());
        }
        let mut p = example();
        p.port = 0;
        assert!(p.validate().is_err());
        p.port = 22;
        p.username = "-option".into();
        assert!(p.validate().is_err());
    }
    #[test]
    fn rdp_secure_defaults_and_ipv6() {
        let mut p = example();
        p.host = "::1".into();
        p.port = 3389;
        p.options = ProtocolOptions::Rdp {
            domain: String::new(),
            width: 1280,
            height: 720,
            fullscreen: false,
            clipboard: false,
            clipboard_files: false,
            settings: RdpSettings::default(),
        };
        let (_, args) = p.command().unwrap();
        assert!(args.contains(&"/v:[::1]:3389".into()));
        for safe in ["/cert:deny", "/sec:nla", "-clipboard"] {
            assert!(args.contains(&safe.into()));
        }
        assert!(!args.iter().any(|a| a.starts_with("/p:")));
    }
}

pub fn validate_folder(name: &str) -> Result<(), String> {
    if name.len() > 512
        || name.split('/').count() > 8
        || name.split('/').any(|part| {
            part.trim() != part
                || part.is_empty()
                || part.len() > 64
                || matches!(part, "." | "..")
                || part.chars().any(|c| c.is_control() || c == '\\')
        })
    {
        return Err("Dossier invalide : 8 niveaux maximum, noms de 1 à 64 caractères".into());
    }
    Ok(())
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Workspace {
    pub version: u32,
    pub folders: Vec<String>,
    pub profiles: Vec<SessionProfile>,
}
impl Default for Workspace {
    fn default() -> Self {
        Self {
            version: 1,
            folders: Vec::new(),
            profiles: Vec::new(),
        }
    }
}
impl Workspace {
    pub fn decode(bytes: &[u8]) -> Result<Self, String> {
        if bytes.len() > 2 * 1024 * 1024 {
            return Err("Import trop volumineux (2 Mio maximum).".into());
        }
        #[derive(Deserialize)]
        #[serde(untagged)]
        enum Input {
            Versioned(Workspace),
            Legacy(Vec<SessionProfile>),
        }
        let workspace = match serde_json::from_slice(bytes)
            .map_err(|_| "Format invalide : JSON RustRemote sans secrets attendu.")?
        {
            Input::Versioned(w) => w,
            Input::Legacy(profiles) => {
                let mut folders = Vec::new();
                for p in &profiles {
                    if !p.folder.is_empty() && !folders.contains(&p.folder) {
                        folders.push(p.folder.clone());
                    }
                }
                Self {
                    profiles,
                    folders,
                    ..Self::default()
                }
            }
        };
        workspace.validate()?;
        Ok(workspace)
    }
    pub fn validate(&self) -> Result<(), String> {
        if self.version != 1 || self.folders.len() > 128 || self.profiles.len() > 1024 {
            return Err("Version ou nombre de profils/dossiers non pris en charge.".into());
        }
        let mut names = std::collections::HashSet::new();
        for folder in &self.folders {
            validate_folder(folder)?;
            if let Some((parent, _)) = folder.rsplit_once('/')
                && !self.folders.iter().any(|f| f == parent)
            {
                return Err("Dossier parent absent".into());
            }
            if !names.insert(folder) {
                return Err("Dossier dupliqué.".into());
            }
        }
        names.clear();
        for profile in &self.profiles {
            profile.validate()?;
            if !names.insert(&profile.name) {
                return Err("Nom de profil dupliqué.".into());
            }
            if !profile.folder.is_empty() && !self.folders.contains(&profile.folder) {
                return Err("Un profil référence un dossier absent.".into());
            }
        }
        Ok(())
    }
    pub fn merge(&self, incoming: &Self) -> Result<Self, String> {
        incoming.validate()?;
        let mut result = self.clone();
        for folder in &incoming.folders {
            if !result.folders.contains(folder) {
                result.folders.push(folder.clone());
            }
        }
        result
            .profiles
            .extend(incoming.profiles.iter().cloned().map(|mut p| {
                p.credential_ref = None;
                p
            }));
        result.validate()?;
        Ok(result)
    }
}

#[cfg(test)]
mod workspace_tests {
    use super::*;
    #[test]
    fn migrates_legacy_and_retains_empty_folder() {
        let mut value = serde_json::to_value(super::tests::example()).unwrap();
        value.as_object_mut().unwrap().remove("folder");
        value.as_object_mut().unwrap().remove("simulation");
        let data = Workspace::decode(&serde_json::to_vec(&vec![value]).unwrap()).unwrap();
        assert_eq!(data.version, 1);
        assert!(!data.profiles[0].simulation);
        let empty = Workspace {
            folders: vec!["Linux".into()],
            ..Workspace::default()
        };
        assert_eq!(
            Workspace::decode(&serde_json::to_vec(&empty).unwrap()).unwrap(),
            empty
        );
    }
    #[test]
    fn sample_is_network_disabled_and_merges_without_overwrite() {
        let fixture =
            Workspace::decode(include_bytes!("../assets/simulations/serveur-ia.json")).unwrap();
        assert_eq!(fixture.profiles[0].host, "ia.aurelson.com");
        assert!(fixture.profiles[0].command().is_err());
        let start = Workspace {
            folders: vec!["Linux".into()],
            ..Workspace::default()
        };
        let merged = start.merge(&fixture).unwrap();
        assert_eq!(merged.folders.len(), 1);
        assert!(merged.merge(&fixture).is_err());
        assert!(start.profiles.is_empty());
    }
    #[test]
    fn rejects_versions_secret_fields_and_bad_folders() {
        for folder in ["../Linux", "/Linux", "..", "Linux\\etc", "Linux\n"] {
            assert!(validate_folder(folder).is_err());
        }
        assert!(Workspace::decode(br#"{"version":9,"folders":[],"profiles":[]}"#).is_err());
        assert!(
            Workspace::decode(br#"{"version":1,"folders":[],"profiles":[],"password":"fixture"}"#)
                .is_err()
        );
    }
    #[test]
    fn rdp_clipboard_files_require_opt_in() {
        let mut profile = super::tests::example();
        profile.options = ProtocolOptions::Rdp {
            domain: String::new(),
            width: 1280,
            height: 720,
            fullscreen: false,
            clipboard: true,
            clipboard_files: false,
            settings: RdpSettings::default(),
        };
        assert!(
            profile
                .command()
                .unwrap()
                .1
                .contains(&"/clipboard:direction-to:all,files-to:off".into())
        );
        if let ProtocolOptions::Rdp {
            clipboard_files, ..
        } = &mut profile.options
        {
            *clipboard_files = true;
        }
        assert!(
            profile
                .command()
                .unwrap()
                .1
                .contains(&"/clipboard:direction-to:all,files-to:all".into())
        );
        if let ProtocolOptions::Rdp { clipboard, .. } = &mut profile.options {
            *clipboard = false;
        }
        assert!(profile.validate().is_err());
    }
}

#[cfg(test)]
mod advanced_options_tests {
    use super::*;
    #[test]
    fn rdp_size_devices_monitors_and_pinning() {
        let mut p = super::tests::example();
        let settings = RdpSettings {
            size_mode: 2,
            multi_monitor: true,
            monitors: vec![0, 1],
            audio_mode: 0,
            microphone: true,
            printers: true,
            certificate_sha256: Some("a".repeat(64)),
        };
        p.options = ProtocolOptions::Rdp {
            domain: "LAB".into(),
            width: 1920,
            height: 1080,
            fullscreen: false,
            clipboard: false,
            clipboard_files: false,
            settings,
        };
        let args = p.command().unwrap().1;
        for item in [
            "-dynamic-resolution",
            "/smart-sizing",
            "/multimon",
            "/monitors:0,1",
            "/audio-mode:0",
            "/sound",
            "/microphone",
            "/printer",
            "/f",
        ] {
            assert!(args.contains(&item.to_string()), "{item}");
        }
        assert!(args.contains(&format!("/cert:fingerprint:sha256:{}", "a".repeat(64))));
        assert!(!args.contains(&"/cert:deny".into()));
        if let ProtocolOptions::Rdp { settings, .. } = &mut p.options {
            settings.certificate_sha256 = Some("bad/pin".into());
        }
        assert!(p.validate().is_err());
    }
    #[test]
    fn ssh_key_reference_is_one_argument_without_agent_forwarding() {
        let mut p = super::tests::example();
        p.options = ProtocolOptions::Ssh {
            identity_file: Some("/tmp/test key".into()),
            agent: false,
            known_hosts_file: None,
            config_file: None,
        };
        let args = p.command().unwrap().1;
        assert!(args.windows(2).any(|w| w == ["-i", "/tmp/test key"]));
        assert!(args.contains(&"IdentityAgent=none".into()));
        assert!(args.contains(&"IdentitiesOnly=yes".into()));
        p.options = ProtocolOptions::Ssh {
            identity_file: Some("relative-key".into()),
            agent: false,
            known_hosts_file: None,
            config_file: None,
        };
        assert!(p.validate().is_err());
    }
}
