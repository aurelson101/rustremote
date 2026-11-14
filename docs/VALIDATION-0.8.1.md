# Validation RustRemote 0.8.1 — 2026-11-14

Vérification ciblée de la découverte et de son cycle de vie, sans audit global ni
scan d’un réseau réel. Aucun autre serveur n’a été contacté pour cette étape.

## Défauts corrigés

- La limite huit workers était par fenêtre : plusieurs scans pouvaient se cumuler.
  Un permis global refuse maintenant un second scan réel ; libération RAII à la
  fin du job. Le simulateur, sans réseau, n’occupe pas ce permis.
- Le statut précédent confondait fin normale, annulation et expiration. Rapport
  structuré, états distincts et nombre d’hôtes entièrement traités/total ; résultats
  partiels explicitement signalés.
- Changer la cible ou la simulation laissait les anciennes lignes visibles. Ces
  changements effacent les résultats et le consentement, annulent l’ancien job et
  incrémentent une génération. Son callback tardif ne peut plus repeupler la vue.
  Le champ IPv4 est borné à 64 caractères.

## Tests

`cargo fmt --all -- --check`, `cargo check --workspace`, Clippy avec -D warnings
et tests workspace réussis : 49 tests (48 +1 helper). Nouveaux cas : concurrence
refusée et permis libéré, budget nul avant tout réseau, bannière RFB incomplète
avec délai borné, annonce de 255 types refusée. Les sockets de tests sont uniquement
loopback sur ports éphémères. Aucun identifiant ni choix d’authentification envoyé.

AppImage portable construite dans Debian 12 ; démarrage X11 sur ce userspace réussi.
Le rejeu graphique ciblé crée les profils Apple/Windows/Linux depuis les fixtures,
vérifie les refus public/sans consentement et l’effacement après changement de mode
ou de cible, puis relance la découverte. Le parent exécute aussi ses contrôles
protocolaires SSH/SFTP loopback ; ce mode ne prétend pas tester le terminal GUI.
Wayland : démarrage et plein écran/restauration vérifiés séparément.
Résultats détaillés : [validation/0.8.1.json](validation/0.8.1.json).

```bash
cargo test --workspace discovery::tests
RUSTREMOTE_ENGLISH_GUI=1 RUSTREMOTE_DISCOVERY_ONLY=1 RUSTREMOTE_SSH_SFTP_ONLY=1 python3 scripts/test-ssh-sftp-local.py --gui dist/RustRemote-0.8.1-preview-x86_64.AppImage
RUSTREMOTE_TEST_BACKEND=wayland python3 scripts/smoke-profiles.py dist/RustRemote-0.8.1-preview-x86_64.AppImage --startup-only --display-checks
```

## Limites et suites proposées

Les avertissements/criticals de focus GTK4.8 persistent : correction encore ouverte.
Pas de certification OS, réseau réel, Mac/Windows/AD réel, périphériques, CLIPRDR,
KDE/Xfce/Cinnamon ni endurance mémoire. Les plafonds de la découverte et du SFTP 0.8
restent inchangés. Le rejeu GUI complet SSH/SFTP 0.8 est historique ; ici le GUI
ciblé porte sur les éléments modifiés. Les compteurs de progression sont finaux,
pas encore rafraîchis en direct pendant la sonde.

Priorités proposées, non livrées : corriger les diagnostics GTK avec reproduction
ciblée ; file SFTP persistante et limites réglables ; noms DNS/mDNS facultatifs et
IPv6 sous plages explicitement autorisées. Aucune absence générale de faille ou de
fuite n’est revendiquée par cette passe.

SHA256 de l’artefact validé :

```text
d0d5f763874f2093c9646284edd060122da5328bb317c25ea54105239f71146c
```

[Préversion 0.8.1 publiée](https://github.com/aurelson101/rustremote/releases/tag/v0.8.1)
sur `ed08e9e420a75d13ad30bd31e0f422d06d87ca9d`, le 2026-11-14.
Les quatre assets ont été retéléchargés et comparés ; SHA256 vérifié.
Les anciennes releases et la visibilité existante sont préservées.
