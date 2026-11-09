use super::*;
use crate::credentials::Secret;
use std::sync::Arc;
pub(super) struct Change {
    pub before: Option<String>,
    pub after: Option<String>,
    pub password: Option<Arc<Secret>>,
    pub remember: bool,
    pub old_cache: Option<String>,
    pub new_cache: String,
}
fn domain(profile: &SessionProfile) -> &str {
    match &profile.options {
        ProtocolOptions::Rdp { domain, .. } => domain,
        _ => "",
    }
}
pub(super) fn cache_key(profile: &SessionProfile) -> String {
    format!(
        "{:?}\0{}\0{}\0{}\0{}\0{}",
        profile.kind(),
        profile.name,
        profile.host,
        profile.port,
        profile.username,
        domain(profile)
    )
}
impl Controller {
    pub fn vault(&self) {
        if self.0.store.borrow().is_none() {
            self.startup();
            return;
        }
        let window = self.window("Coffre des identifiants", 600);
        let body = gtk::Box::new(gtk::Orientation::Vertical, 12);
        body.add_css_class("dialog-content");
        let note = gtk::Label::new(Some(
            "Les mots de passe sont liés aux profils et protégés par le coffre du système. Ils ne figurent ni dans les fichiers de profils ni dans les exports. Les paramètres d’affichage sont accessibles par Paramètres.",
        ));
        note.set_wrap(true);
        body.append(&note);
        if self.0.demo_root.borrow().is_some() {
            let note = gtk::Label::new(Some(
                "Démonstration : aucun mot de passe n’est enregistré durablement.",
            ));
            note.set_wrap(true);
            body.append(&note);
        }
        let list = gtk::Box::new(gtk::Orientation::Vertical, 6);
        for (index, profile) in self.0.profiles.borrow().iter().enumerate() {
            if !matches!(
                profile.options,
                ProtocolOptions::Ssh { .. } | ProtocolOptions::Rdp { .. }
            ) {
                continue;
            }
            let state = if profile.credential_ref.is_some() {
                "mot de passe enregistré"
            } else if self.0.passwords.borrow().contains_key(&cache_key(profile)) {
                "mot de passe temporaire"
            } else {
                "aucun mot de passe"
            };
            let row = gtk::Button::with_label(&format!("{} · {}", profile.name, state));
            let c = self.clone();
            let w = window.downgrade();
            row.connect_clicked(move |_| {
                if let Some(w) = w.upgrade() {
                    w.close();
                }
                c.editor(Some(index), false);
            });
            list.append(&row);
        }
        body.append(
            &gtk::ScrolledWindow::builder()
                .child(&list)
                .min_content_height(160)
                .max_content_height(400)
                .build(),
        );
        let lock = gtk::Button::with_label(
            "Verrouiller les profils et oublier les mots de passe temporaires",
        );
        let c = self.clone();
        let w = window.downgrade();
        lock.connect_clicked(move |_| {
            if let Some(w) = w.upgrade() {
                w.close();
            }
            c.lock();
        });
        body.append(&lock);
        let close = gtk::Button::with_label("Fermer le coffre");
        let w = window.downgrade();
        close.connect_clicked(move |_| {
            if let Some(w) = w.upgrade() {
                w.close();
            }
        });
        body.append(&close);
        window.set_child(Some(&body));
        crate::i18n::present(&window);
    }
    pub(super) fn with_password(
        &self,
        profile: SessionProfile,
        entered: Option<Arc<Secret>>,
        done: impl FnOnce(Result<Option<Arc<Secret>>, String>) + 'static,
    ) {
        let cached =
            entered.or_else(|| self.0.passwords.borrow().get(&cache_key(&profile)).cloned());
        if cached.is_some() || profile.credential_ref.is_none() {
            done(Ok(cached));
            return;
        }
        let reference = profile.credential_ref.unwrap_or_default();
        io(
            move || {
                crate::credentials::read(&reference).and_then(|secret| {
                    secret.map(Arc::new).ok_or_else(|| {
                        "Le mot de passe enregistré n’existe plus : modifiez le profil".into()
                    })
                })
            },
            move |result| done(result.map(Some)),
        );
    }
}
impl Controller {
    pub(super) fn credential_change(
        &self,
        original: Option<&SessionProfile>,
        profile: &mut SessionProfile,
        entered: &str,
        remember: bool,
    ) -> Result<Change, String> {
        let supported = matches!(
            profile.options,
            ProtocolOptions::Ssh { .. } | ProtocolOptions::Rdp { .. }
        );
        let before = original.and_then(|p| p.credential_ref.clone());
        let same_target = original.is_some_and(|p| {
            p.host == profile.host
                && p.port == profile.port
                && p.username == profile.username
                && p.kind() == profile.kind()
                && domain(p) == domain(profile)
        });
        if remember && self.0.demo_root.borrow().is_some() {
            return Err("Quittez la démonstration pour enregistrer un mot de passe".into());
        }
        if remember && before.is_some() && !same_target && entered.is_empty() {
            return Err("La cible ou l’identifiant a changé : ressaisissez le mot de passe ou désactivez son enregistrement".into());
        }
        let old_cache = original.map(cache_key);
        let password = if supported && !entered.is_empty() {
            crate::credentials::validate_password(entered)?;
            Some(Arc::new(Secret::new(entered.to_string())))
        } else if supported && same_target {
            old_cache
                .as_ref()
                .and_then(|key| self.0.passwords.borrow().get(key).cloned())
        } else {
            None
        };
        let after = if remember && supported {
            if !entered.is_empty() || before.is_none() && password.is_some() {
                Some(gtk::glib::uuid_string_random().to_string())
            } else {
                before.clone()
            }
        } else {
            None
        };
        if remember && (after.is_none() || after != before && password.is_none()) {
            return Err("Saisissez le mot de passe à enregistrer".into());
        }
        profile.credential_ref = after.clone();
        Ok(Change {
            before,
            after,
            password,
            remember,
            old_cache,
            new_cache: cache_key(profile),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rdp_domains_cannot_share_cached_passwords() {
        let mut profile =
            Workspace::decode(include_bytes!("../../assets/simulations/serveur-ia.json"))
                .unwrap()
                .profiles
                .remove(0);
        profile.options = serde_json::from_value(serde_json::json!({"protocol":"Rdp","domain":"ALPHA","width":1200,"height":800,"fullscreen":false,"clipboard":false,"clipboard_files":false})).unwrap();
        let first = cache_key(&profile);
        if let ProtocolOptions::Rdp { domain, .. } = &mut profile.options {
            *domain = "BETA".into();
        }
        assert_ne!(first, cache_key(&profile));
    }
}
