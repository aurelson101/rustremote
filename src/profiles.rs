//! Compatibility imports for the Linux application; models are shared with Windows.
pub use rustremote_core::freerdp_binary;
pub use rustremote_core::profiles::*;

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    pub fn example() -> SessionProfile {
        serde_json::from_value(serde_json::json!({
            "name":"Test local", "host":"example.invalid", "port":22,
            "username":"demo", "favorite":true,
            "options":{"protocol":"Ssh", "identity_file":null, "agent":true}
        }))
        .unwrap()
    }
}
