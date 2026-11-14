# Validation RustRemote 0.7.0 — 2026-11-14

Incrément Apple / SFTP développé indépendamment. Pas de code MGRemote importé.
La connexion compte macOS utilise le client système TigerVNC 1.14+ avec DH/type30.
Le profil impose SSH vers le même Mac, cible VNC 127.0.0.1 vue depuis ce Mac et
refuse le repli VncAuth/None. La vérification known_hosts SSH reste active. Le
compte et son mot de passe sont demandés par le client externe, sans stockage
VNC dans RustRemote. SRP et mode Apple haute performance non pris en charge.

## Tests et portée

- `cargo fmt --all -- --check`, `cargo check --workspace`,
  `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace` :
  réussis, 42 tests (41 application +1 helper). Modèle Apple : tunnel obligatoire,
  même hôte, sélection DH sans downgrade, JSON et simulation sans réseau.
- `python3 scripts/test-apple-local.py` : vrai TigerVNC dans un Xvfb privé, serveur
  RFB simulant la bannière Apple 003.889 et l’authentification compte type30. Secret
  généré pour le test, authentification et pixels positifs ; refus de mauvais mot
  de passe, serveur n’offrant pas ARD et longueur de clé invalide. Services arrêtés.
- `python3 scripts/test-vnc-serial-local.py` : VNC réel via tunnel SSH loopback,
  pixels, refus d’authentification et série PTY 115200 bidirectionnelle réussis.
- Rejeu de l’AppImage : preset Apple enregistré en simulation, menus/dossiers/DND,
  SFTP multisélection fichiers/dossiers dans les deux sens, DND local, édition et
  relecture distante, filtre SFTP, anglais par défaut et français après redémarrage.
- Le filtre SFTP vide les sélections lors d’un changement utilisateur. Le rendu
  après refresh applique le filtre sans vider le panneau opposé ; un test GUI
  vérifie que rafraîchir le local conserve la sélection distante.
- AppImage construite dans Debian 12 ; démarrage X11 sur ce userspace réussi.
  ABI x86_64/glibc≥2.36, GTK4.8.3/VTE0.70.6. Sources et pilotes graphiques inchangés.
  Le build final a écrit son marqueur de succès et l’artefact vérifié ; la session
  d’orchestration a ensuite été terminée avec 143, sans erreur de compilation dans
  le journal. Les replays suivants portent sur ce fichier et son SHA256.
- Démarrage Wayland et plein écran/restauration rejoués sur Ubuntu 26.04/GNOME.

Les résultats détaillés et le SHA256 figurent dans [validation/0.7.0.json](validation/0.7.0.json).

## Limites et erreurs réellement observées

**Pas de Mac physique ni de VM macOS testé.** Le transport du simulateur ARD est
loopback uniquement ; le tunnel SSH est validé dans un laboratoire VNC séparé.
Ce n’est pas une preuve de bout en bout sur macOS. Le système de fichiers du
simulateur SFTP est Linux ; aucun comportement APFS particulier n’est certifié.
Les noms de fichiers sont conservés, seule la recherche est insensible à la casse.

Le client TigerVNC peut quitter avec code 0 après refus d’authentification : le test
vérifie le refus côté protocole et non un code de sortie arbitraire. Le premier
essai a échoué sur le cookie X11 de l’automatisation, corrigé par XAUTHORITY privé.

Un premier harnais de la compilation native GTK de l’hôte s’est arrêté sur le
bouton Move après interaction au clavier avec un sélecteur. Cause non établie ;
le parcours AppImage GTK4.8 a passé cette étape. Ne pas qualifier tous les builds
natifs. Les warnings/criticals de focus GTK4.8 déjà documentés en 0.6 persistent.
L’avertissement PIL getdata concerne le harnais ; aucune assertion protocole n’est
ignorée pour faire réussir le laboratoire Apple. Pas de promesse zéro bug.

RDP, CLIPRDR, Windows/AD et périphériques physiques n’ont pas été requalifiés pour
cet incrément. VNC/Apple restent des fenêtres externes avec fermeture dans leur
client. Le cycle de vie VNC n’est pas suivi comme celui du client RDP.
SFTP reste borné à 16 Mio/128 entrées/8 niveaux/30 s ; éditeur UTF-8 ≤1 Mio ; pas de file
persistante ni reprise récursive de dossiers. KDE/Xfce/Cinnamon, ARM/musl/glibc<2.36
restent hors qualification.

## Sources de protocole et configuration

- [Apple : accès VNC / partage d’écran](https://support.apple.com/en-mz/guide/remote-desktop/apdbed09830/mac).
- [TigerVNC : implémentation de référence DH](https://github.com/TigerVNC/tigervnc/blob/v1.15.0/common/rfb/CSecurityDH.cxx), consultée pour le format du protocole ; aucune copie de code intégrée.

## Rejouer les vérifications

```bash
python3 scripts/test-apple-local.py
python3 scripts/test-vnc-serial-local.py
RUSTREMOTE_ENGLISH_GUI=1 RUSTREMOTE_SSH_SFTP_ONLY=1 python3 scripts/test-ssh-sftp-local.py --gui dist/RustRemote-0.7.0-preview-x86_64.AppImage
RUSTREMOTE_TEST_BACKEND=wayland python3 scripts/smoke-profiles.py dist/RustRemote-0.7.0-preview-x86_64.AppImage --startup-only --display-checks
```

Artefact validé — SHA256 :

```text
683bc554341cbd1ead6b2989a6752123f000441546710895e8b907a3f5f3c6a5
```

Publication GitHub en préparation.
