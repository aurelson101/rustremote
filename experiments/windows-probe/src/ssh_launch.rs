//! OpenSSH Windows launch policy. No shell, implicit config execution or trust bypass.
use rustremote_core::profiles::{ProtocolOptions, SessionProfile};

#[derive(Clone)]
pub enum Target {
    Local,
    Ssh(Box<SessionProfile>),
}

pub fn arguments(profile: &SessionProfile) -> Result<Vec<String>, String> {
    profile.validate()?;
    profile.validate_native_paths()?;
    if profile.simulation {
        return Err("Simulation profiles never contact a server".into());
    }
    let ProtocolOptions::Ssh {
        identity_file,
        agent,
        known_hosts_file,
        config_file,
    } = &profile.options
    else {
        return Err("Choose an SSH profile".into());
    };
    if config_file.is_some() {
        return Err("Custom OpenSSH configuration is not supported on Windows yet; use explicit host/key/known_hosts settings".into());
    }
    let mut args: Vec<String> = [
        "-F",
        "none",
        "-tt",
        "-oStrictHostKeyChecking=yes",
        "-oUpdateHostKeys=no",
        "-oPermitLocalCommand=no",
        "-oProxyCommand=none",
        "-oProxyJump=none",
        "-oForwardAgent=no",
        "-oForwardX11=no",
        "-oClearAllForwardings=yes",
        "-oKbdInteractiveAuthentication=no",
        "-oConnectTimeout=10",
        "-oServerAliveInterval=15",
        "-oServerAliveCountMax=3",
        "-oNumberOfPasswordPrompts=1",
    ]
    .into_iter()
    .map(str::to_owned)
    .collect();
    args.push(
        if identity_file.is_some() {
            "-oPreferredAuthentications=publickey"
        } else {
            "-oPreferredAuthentications=publickey,password"
        }
        .into(),
    );
    if !agent {
        args.push("-oIdentityAgent=none".into());
    }
    if let Some(path) = identity_file {
        safe_path(path)?;
        args.extend(["-oIdentitiesOnly=yes".into(), "-i".into(), path.clone()]);
    }
    if let Some(path) = known_hosts_file {
        safe_path(path)?;
        // OpenSSH parses the -o value as a configuration line too. Preserve spaces there.
        args.push(format!(
            "-oUserKnownHostsFile=\"{}\"",
            path.replace('\\', "/")
        ));
    }
    args.extend([
        "-p".into(),
        profile.port.to_string(),
        "-l".into(),
        profile.username.clone(),
        "--".into(),
        profile.host.clone(),
    ]);
    Ok(args)
}
fn safe_path(path: &str) -> Result<(), String> {
    if path
        .chars()
        .any(|c| c.is_control() || matches!(c, '"' | '<' | '>' | '|' | '?' | '*'))
        || path.starts_with("\\\\")
    {
        return Err(
            "SSH files require a local absolute path without Windows special characters".into(),
        );
    }
    Ok(())
}
/// The Microsoft C runtime quoting algorithm, including quotes and trailing slashes.
#[cfg_attr(not(windows), allow(dead_code))]
pub fn quote(argument: &str) -> Result<String, String> {
    if argument.contains('\0') {
        return Err("NUL in process argument".into());
    }
    let mut quoted = String::from("\"");
    let mut slashes = 0;
    for character in argument.chars() {
        if character == '\\' {
            slashes += 1;
            continue;
        }
        if character == '"' {
            quoted.extend(std::iter::repeat_n('\\', slashes * 2 + 1));
            quoted.push('"');
        } else {
            quoted.extend(std::iter::repeat_n('\\', slashes));
            quoted.push(character);
        }
        slashes = 0;
    }
    quoted.extend(std::iter::repeat_n('\\', slashes * 2));
    quoted.push('"');
    Ok(quoted)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn profile() -> SessionProfile {
        serde_json::from_value(serde_json::json!({"name":"Apple lab","host":"127.0.0.1","port":2222,"username":"lab@realm.example","platform":"MacOs","favorite":false,"options":{"protocol":"Ssh","identity_file":null,"agent":false}})).unwrap()
    }
    #[test]
    fn apple_ssh_keeps_identity_separate_and_refuses_implicit_shell_config() {
        let mut p = profile();
        let args = arguments(&p).unwrap();
        assert!(args.contains(&"-oStrictHostKeyChecking=yes".into()));
        assert!(args.contains(&"-oForwardAgent=no".into()));
        assert_eq!(
            &args[args.len() - 5..],
            &["2222", "-l", "lab@realm.example", "--", "127.0.0.1"]
        );
        if let ProtocolOptions::Ssh { config_file, .. } = &mut p.options {
            *config_file = Some(
                if cfg!(windows) {
                    r"C:\Users\Lab\config"
                } else {
                    "/tmp/config"
                }
                .into(),
            );
        }
        assert!(arguments(&p).is_err());
        p.simulation = true;
        assert!(arguments(&p).is_err());
    }
    #[test]
    fn quote_preserves_spaces_quotes_trailing_slashes_and_blocks_nul() {
        assert_eq!(quote("").unwrap(), "\"\"");
        assert_eq!(quote(r"C:\Lab Space\").unwrap(), "\"C:\\Lab Space\\\\\"");
        assert_eq!(quote("a\"b").unwrap(), "\"a\\\"b\"");
        assert!(quote("a\0b").is_err());
        assert!(safe_path(r"\\server\key").is_err());
        assert!(safe_path("C:\\key\"bad").is_err());
    }
}
