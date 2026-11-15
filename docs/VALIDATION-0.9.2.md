# Correctif focus GTK et priorisation Windows — 2026-11-15

## Cause et correction

Reproduction minimale sous GTK4.8.3 de l'AppImage : un GtkText actif dans un panneau
mis en insensitive conserve son callback de clignotement sans recevoir focus-out.
Le processus s'arrête avec exit133 sous G_DEBUG=fatal-warnings. Libérer le focus avant
la désactivation termine avec exit0. Aucune désactivation du clignotement ni aucun
filtre de journalisation n'est utilisé.

set_input_sensitive libère le focus uniquement s'il appartient au sous-arbre à
désactiver. Appliqué aux enregistrements de profils et paramètres, mot de passe de
connexion, navigateur SFTP, chemin d'import, découverte, écrans RDP et passerelle
VNC. Aucun changement d'authentification, de données ou de protocoles.

Le test GTK dédié vérifie la perte de focus avant désactivation, attend plus d'un
cycle de clignotement, vérifie que l'entrée hors du panneau conserve son focus et
que le panneau peut être réactivé. Ignoré par défaut car il exige un affichage ;
exécuté explicitement sous Xvfb/D-Bus privés avec les bibliothèques GTK4.8.3 de
l'AppImage et G_DEBUG=fatal-warnings : PASS.

## Vérifications

fmt/check/Clippy -D warnings et55tests normaux PASS, plus1régression GTK exécutée
explicitement. Build AppImage portable Debian12 et startup PASS. L'image0.9.2 est
locale ; aucune release GitHub remplacée ni publiée pour cette étape.

Le premier parcours GUI global fatal-warnings passe sans GtkText dans RustRemote,
mais faisait aussi avorter les portails de test GNOME. Le harnais dispose désormais
de RUSTREMOTE_GTK_FATAL_WARNINGS=1 pour rendre les warnings fatals uniquement dans
RustRemote, sans modifier la politique des services du laboratoire. Les warnings
applicatifs ne sont pas masqués. Rejeu final avec ce réglage : PASS. Aucun GtkText focus-out ni warning/critical GTK
RustRemote. SSH/SFTP loopback, dossiers, DND, transferts multiples, édition, navigation
souris, file persistante, limites et redémarrage français passent. Les avertissements
externes du portail/du rendu logiciel ne sont pas assimilés aux messages applicatifs.

```bash
cargo fmt --all -- --check
cargo check --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
# Ce test utilise le GTK système ; pour le GTK packagé, lancer le même exécutable
# de tests sous les bibliothèques extraites, après création de Xvfb/D-Bus privés.
GTK_THEME=Adwaita GSK_RENDERER=cairo G_DEBUG=fatal-warnings xvfb-run -a dbus-run-session -- cargo test --bin rustremote dialogs::focus_tests::disabling_input_delivers_focus_out_and_preserves_other_input -- --ignored --exact --test-threads=1
GTK_THEME=Adwaita RUSTREMOTE_GTK_FATAL_WARNINGS=1 RUSTREMOTE_ENGLISH_GUI=1 RUSTREMOTE_SSH_SFTP_ONLY=1 python3 scripts/test-ssh-sftp-local.py --gui dist/RustRemote-0.9.2-preview-x86_64.AppImage
```

## Windows

Ordre, dépendances, critères d'acceptation et références primaires figurent dans
ROADMAP.md. Premier risque : remplacer le terminal VTE/PTY/pidfd actuel par une
intégration Windows prouvée, sans réécrire l'interface Linux. Le PoC GTK4/ConPTY
précède les frontières OS, le coffre, SSH/SFTP, RDP, les tests UI Windows et MSIX.
Aucune compilation Windows ni qualification matérielle/AD/LAN/endurance effectuée.
