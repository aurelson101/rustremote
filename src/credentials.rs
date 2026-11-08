//! Secrets liés aux profils ; aucune sérialisation du mot de passe.
use std::sync::Mutex;
pub type Secret = zeroize::Zeroizing<String>;
static ACCESS: Mutex<()> = Mutex::new(());
const SERVICE: &str = "com.rustremote.credentials";
pub fn valid_reference(value: &str) -> bool {
    value.len() == 36
        && value.bytes().enumerate().all(|(i, b)| {
            if [8, 13, 18, 23].contains(&i) {
                b == b'-'
            } else {
                b.is_ascii_hexdigit()
            }
        })
}
fn entry(reference: &str) -> Result<keyring::Entry, String> {
    if !valid_reference(reference) {
        return Err("Référence de coffre invalide".into());
    }
    keyring::Entry::new(SERVICE, reference)
        .map_err(|_| "Le coffre système est indisponible ou verrouillé".into())
}
pub fn read(reference: &str) -> Result<Option<Secret>, String> {
    let _access = ACCESS.lock().map_err(|_| "Coffre interrompu")?;
    match entry(reference)?.get_password() {
        Ok(value) => Ok(Some(Secret::new(value))),
        Err(keyring::Error::NoEntry) => Ok(None),
        Err(_) => {
            Err("Impossible de lire le mot de passe : déverrouillez le coffre système".into())
        }
    }
}
pub fn write(reference: &str, value: &str) -> Result<(), String> {
    validate_password(value)?;
    let _access = ACCESS.lock().map_err(|_| "Coffre interrompu")?;
    entry(reference)?
        .set_password(value)
        .map_err(|_| "Le mot de passe n’a pas pu être enregistré dans le coffre système".into())
}
pub fn remove(reference: &str) -> Result<(), String> {
    let _access = ACCESS.lock().map_err(|_| "Coffre interrompu")?;
    match entry(reference)?.delete_credential() {
        Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
        Err(_) => Err("Le coffre n’a pas pu effacer le mot de passe".into()),
    }
}
pub fn validate_password(value: &str) -> Result<(), String> {
    if value.is_empty() || value.len() > 4096 || value.contains(['\0', '\n', '\r']) {
        Err("Mot de passe requis, limité à 4096 octets sans retour à la ligne".into())
    } else {
        Ok(())
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rejects_invalid_references_and_line_injection() {
        assert!(valid_reference("e0a306ec-71ee-44d0-bc89-dcf7f657135a"));
        for value in ["", "../account", "a\nprofile"] {
            assert!(!valid_reference(value));
        }
        assert!(validate_password("fixture-only").is_ok());
        assert!(validate_password("fixture\nnext").is_err());
    }
}
