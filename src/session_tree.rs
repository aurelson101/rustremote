//! Session folders are logical paths, never local filesystem operations.
use crate::profiles::{Workspace, validate_folder};
pub fn contains(parent: &str, path: &str) -> bool {
    path == parent
        || path
            .strip_prefix(parent)
            .is_some_and(|s| s.starts_with('/'))
}
impl Workspace {
    pub fn add_folder(&mut self, path: &str) -> Result<(), String> {
        validate_folder(path)?;
        if self.folders.iter().any(|f| f == path) {
            return Err("Ce dossier existe déjà".into());
        }
        let mut next = self.clone();
        let mut current = String::new();
        for part in path.split('/') {
            if !current.is_empty() {
                current.push('/');
            }
            current.push_str(part);
            if !next.folders.contains(&current) {
                next.folders.push(current.clone());
            }
        }
        next.validate()?;
        *self = next;
        Ok(())
    }
    pub fn rename_folder(&mut self, old: &str, new: &str) -> Result<(), String> {
        validate_folder(new)?;
        if !self.folders.iter().any(|f| f == old) {
            return Err("Dossier introuvable".into());
        }
        if old == new {
            return Ok(());
        }
        if contains(old, new) {
            return Err("Un dossier ne peut pas être déplacé dans lui-même".into());
        }
        if let Some((parent, _)) = new.rsplit_once('/')
            && !self.folders.iter().any(|f| f == parent)
        {
            return Err("Dossier parent introuvable".into());
        }
        let mut next = self.clone();
        for f in &mut next.folders {
            if contains(old, f) {
                *f = format!("{new}{}", &f[old.len()..]);
            }
        }
        for p in &mut next.profiles {
            if contains(old, &p.folder) {
                p.folder = format!("{new}{}", &p.folder[old.len()..]);
            }
        }
        next.validate()?;
        *self = next;
        Ok(())
    }
    pub fn move_item(&mut self, item: &str, target: &str) -> Result<(), String> {
        if !target.is_empty() && !self.folders.iter().any(|f| f == target) {
            return Err("Dossier destination introuvable".into());
        }
        if let Some(old) = item.strip_prefix("folder:") {
            let leaf = old.rsplit('/').next().ok_or("Dossier invalide")?;
            self.rename_folder(
                old,
                &if target.is_empty() {
                    leaf.to_string()
                } else {
                    format!("{target}/{leaf}")
                },
            )
        } else if let Some(name) = item.strip_prefix("profile:") {
            let p = self
                .profiles
                .iter_mut()
                .find(|p| p.name == name)
                .ok_or("Connexion introuvable")?;
            p.folder = target.into();
            Ok(())
        } else {
            Err("Déplacement non pris en charge".into())
        }
    }
    pub fn remove_empty_folder(&mut self, folder: &str) -> Result<(), String> {
        if self.profiles.iter().any(|p| contains(folder, &p.folder))
            || self
                .folders
                .iter()
                .any(|f| f != folder && contains(folder, f))
        {
            return Err("Le dossier contient des connexions ou sous-dossiers".into());
        }
        self.folders.retain(|f| f != folder);
        Ok(())
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn moving_nested_sessions_preserves_credentials_and_siblings() {
        let mut w =
            Workspace::decode(include_bytes!("../assets/simulations/serveur-ia.json")).unwrap();
        w.add_folder("Linux/DB").unwrap();
        w.add_folder("Linux2").unwrap();
        w.add_folder("Archive").unwrap();
        let name = w.profiles[0].name.clone();
        w.move_item(&format!("profile:{name}"), "Linux/DB").unwrap();
        let before = w.profiles[0].clone();
        w.move_item("folder:Linux", "Archive").unwrap();
        assert_eq!(w.profiles[0].folder, "Archive/Linux/DB");
        assert_eq!(w.profiles[0].host, before.host);
        assert_eq!(w.profiles[0].credential_ref, before.credential_ref);
        assert!(w.folders.iter().any(|f| f == "Linux2"));
        assert!(w.remove_empty_folder("Archive/Linux").is_err());
    }
    #[test]
    fn hierarchy_moves_are_transactional_and_reject_cycles_collisions() {
        let mut w = Workspace::default();
        w.add_folder("Linux/Production/DB").unwrap();
        w.add_folder("Archive").unwrap();
        assert_eq!(w.folders.len(), 4);
        let before = w.clone();
        assert!(w.move_item("folder:Linux", "Linux/Production").is_err());
        assert_eq!(w, before);
        w.move_item("folder:Linux/Production", "Archive").unwrap();
        assert!(w.folders.contains(&"Archive/Production/DB".into()));
        assert!(w.remove_empty_folder("Archive").is_err());
        w.add_folder("Linux/Production").unwrap();
        let before = w.clone();
        assert!(w.move_item("folder:Archive/Production", "Linux").is_err());
        assert_eq!(w, before);
        assert_eq!(
            Workspace::decode(&serde_json::to_vec(&w).unwrap()).unwrap(),
            w
        );
        for name in ["Linux/../DB", "/Linux", "Linux//DB", "Linux/", "Linux/ DB"] {
            assert!(validate_folder(name).is_err());
        }
    }
}
