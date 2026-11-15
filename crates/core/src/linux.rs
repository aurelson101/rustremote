//! Linux-only launch adapters; never compiled into the Windows application.
use crate::profiles::*;

pub fn freerdp_binary() -> &'static str {
    if std::path::Path::new("/usr/bin/xfreerdp3").is_file() {
        "/usr/bin/xfreerdp3"
    } else if std::path::Path::new("/usr/bin/xfreerdp").is_file() {
        "/usr/bin/xfreerdp"
    } else {
        "/usr/bin/xfreerdp3"
    }
}

impl SessionProfile {
    pub fn vnc_tunnel_command(&self) -> Result<Option<String>, String> {
        self.validate()?;
        self.validate_native_paths()?;
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
    pub fn command(&self) -> Result<(&'static str, Vec<String>), String> {
        self.validate()?;
        self.validate_native_paths()?;
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
