//! Validation des outils SSH : aucun shell local et écoute loopback uniquement.
#[derive(Clone)]
pub struct Forward {
    pub kind: u32,
    pub listen: u16,
    pub host: String,
    pub port: u16,
}
impl Forward {
    pub fn arguments(&self) -> Result<Vec<String>, String> {
        if self.kind > 2 || self.listen < 1024 {
            return Err("Type de tunnel invalide ou port d’écoute inférieur à 1024".into());
        }
        if self.kind != 2
            && (self.port == 0
                || self.host.is_empty()
                || self.host.len() > 253
                || self.host.starts_with('-')
                || !self
                    .host
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b".-_:".contains(&b)))
        {
            return Err("Destination du tunnel invalide".into());
        }
        let host = if self.host.contains(':') {
            format!("[{}]", self.host)
        } else {
            self.host.clone()
        };
        let endpoint = if self.kind == 2 {
            format!("127.0.0.1:{}", self.listen)
        } else {
            format!("127.0.0.1:{}:{host}:{}", self.listen, self.port)
        };
        Ok([
            "-N",
            "-T",
            "-F",
            "/dev/null",
            "-o",
            "ClearAllForwardings=no",
            "-o",
            "ExitOnForwardFailure=yes",
            "-o",
            "GatewayPorts=no",
            match self.kind {
                0 => "-L",
                1 => "-R",
                _ => "-D",
            },
            &endpoint,
        ]
        .into_iter()
        .map(String::from)
        .collect())
    }
}
pub fn command(value: &str, targets: usize) -> Result<(), String> {
    if targets == 0 || targets > 8 {
        return Err("Choisissez entre 1 et 8 sessions SSH actives".into());
    }
    if value.trim().is_empty() || value.len() > 4096 || value.chars().any(char::is_control) {
        return Err("Commande non vide sur une seule ligne, limitée à 4096 octets".into());
    }
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn forwards_are_loopback_only_and_destinations_cannot_inject_options() {
        let mut f = Forward {
            kind: 0,
            listen: 1080,
            host: "::1".into(),
            port: 22,
        };
        assert!(
            f.arguments()
                .unwrap()
                .contains(&"127.0.0.1:1080:[::1]:22".into())
        );
        for host in ["-oProxyCommand=bad", "127.0.0.1:22 -L0.0.0.0:1", "a\nb"] {
            f.host = host.into();
            assert!(f.arguments().is_err());
        }
        f.kind = 2;
        assert!(f.arguments().unwrap().contains(&"127.0.0.1:1080".into()));
        f.listen = 22;
        assert!(f.arguments().is_err());
    }
    #[test]
    fn command_dispatch_rejects_hidden_controls_and_unbounded_fanout() {
        assert!(command("uname -a", 1).is_ok());
        for input in ["", "echo a\necho b", "echo\0hidden", "\u{1b}[31m"] {
            assert!(command(input, 1).is_err());
        }
        assert!(command("uname", 0).is_err());
        assert!(command("uname", 9).is_err());
    }
}
