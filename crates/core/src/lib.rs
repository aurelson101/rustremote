//! Shared profile schema and logical session tree. No GUI, OS I/O or credentials backend.
#[cfg(all(feature = "linux-adapters", target_os = "linux"))]
mod linux;
pub mod profiles;
pub mod session_tree;
#[cfg(all(feature = "linux-adapters", target_os = "linux"))]
pub use linux::freerdp_binary;

pub fn valid_credential_reference(value: &str) -> bool {
    value.len() == 36
        && value.bytes().enumerate().all(|(i, b)| {
            if [8, 13, 18, 23].contains(&i) {
                b == b'-'
            } else {
                b.is_ascii_hexdigit()
            }
        })
}

#[cfg(test)]
mod portable_tests {
    use super::{profiles::*, *};

    fn profile() -> SessionProfile {
        serde_json::from_value(serde_json::json!({
            "name":"Lab", "host":"localhost", "port":22, "username":"lab",
            "favorite":false, "options":{"protocol":"Ssh","identity_file":null,"agent":true}
        }))
        .unwrap()
    }

    #[test]
    fn legacy_profiles_and_portable_export_preserve_schema_without_credentials() {
        let mut profile = profile();
        profile.credential_ref = Some("e0a306ec-71ee-44d0-bc89-dcf7f657135a".into());
        let data = Workspace::decode(&serde_json::to_vec(&vec![profile]).unwrap()).unwrap();
        let exported = data.export_json().unwrap();
        assert!(!String::from_utf8_lossy(&exported).contains("credential_ref"));
        let decoded = Workspace::decode(&exported).unwrap();
        assert_eq!(decoded.profiles[0].name, "Lab");
        assert!(decoded.profiles[0].credential_ref.is_none());
    }

    #[test]
    fn import_is_bounded_and_rejects_unknown_secret_fields() {
        assert!(Workspace::decode(&vec![b' '; MAX_PROFILE_BYTES + 1]).is_err());
        let mut value = serde_json::to_value(profile()).unwrap();
        value["password"] = "not-a-credential".into();
        assert!(Workspace::decode(&serde_json::to_vec(&vec![value]).unwrap()).is_err());
        assert!(Workspace::decode(br#"{"version":999,"folders":[],"profiles":[]}"#).is_err());
    }

    #[test]
    fn logical_folders_and_identity_are_independent_of_the_os() {
        let mut data = Workspace::default();
        data.add_folder("Windows/Production").unwrap();
        data.add_folder("Archive").unwrap();
        data.move_item("folder:Windows", "Archive").unwrap();
        assert!(data.folders.contains(&"Archive/Windows/Production".into()));
        assert!(data.add_folder("../escape").is_err());
        assert_eq!(ssh_identity("lab", "ad.example").unwrap(), "lab@ad.example");
        assert!(ssh_identity("lab", "bad\nrealm").is_err());
        assert!(valid_credential_reference(
            "e0a306ec-71ee-44d0-bc89-dcf7f657135a"
        ));
        assert!(!valid_credential_reference("../secret"));
    }

    #[test]
    fn ssh_paths_follow_native_path_semantics() {
        let mut profile = profile();
        let key = if cfg!(windows) {
            r"C:\Users\Lab\clé privée"
        } else {
            "/tmp/clé privée"
        };
        profile.options = ProtocolOptions::Ssh {
            identity_file: Some(key.into()),
            agent: false,
            known_hosts_file: Some(
                if cfg!(windows) {
                    r"C:\Users\Lab\known_hosts"
                } else {
                    "/tmp/known_hosts"
                }
                .into(),
            ),
            config_file: None,
        };
        profile.validate().unwrap();
        profile.validate_native_paths().unwrap();
        if let ProtocolOptions::Ssh { identity_file, .. } = &mut profile.options {
            *identity_file = Some("relative-key".into());
        }
        assert!(profile.validate().is_err());
        let foreign = if cfg!(windows) {
            "/tmp/portable-key"
        } else {
            r"C:\Users\Lab\portable-key"
        };
        if let ProtocolOptions::Ssh { identity_file, .. } = &mut profile.options {
            *identity_file = Some(foreign.into());
        }
        profile.validate().unwrap(); // Imports preserve metadata for another OS.
        assert!(profile.validate_native_paths().is_err());
    }
}
