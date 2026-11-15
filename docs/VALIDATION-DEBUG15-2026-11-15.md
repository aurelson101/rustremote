# Linux / Windows — 15 contrôles, 10 améliorations

Date : 15 novembre 2026. Laboratoires exclusivement ; aucune infrastructure réelle
ni aucun compte système distant créé. Windows reste un prototype de terminal,
sans port complet SSH/SFTP/RDP ni MSI. Windows 11 n'est pas certifié.

## Dix améliorations

| ID | Plateforme | Changement |
|---|---|---|
| L1 | Linux | Finalisation de l'édition SFTP atomique : conservation des permissions ordinaires, sans rétablir les bits privilégiés. |
| L2 | Linux | Nettoyage du temporaire après échec d'écriture ; un échec d'ouverture exclusive ne supprime jamais un nom existant. |
| L3 | Linux | Une seule mutation de métadonnées de file admise à la fois par navigateur SFTP ; retour explicite si occupée et reprise de la pompe après achèvement. |
| L4 | Linux | Sélecteur GTK actionné par son action accessible dans le bon dialogue ; boutons et recherches limités au bon scope, priorité aux éléments visibles. |
| L5 | Linux | Compteur réel du listing SFTP, description accessible et contraste lisible. |
| W1 | Windows | Redimensionnement refusé après demande d'arrêt, même si la file de commandes dispose d'une place libre. |
| W2 | Windows | Lecture des modes clavier/collage sans copie de l'écran ; collage refusé si le mode n'est pas disponible. |
| W3 | Windows | Diagnostic ConPTY et terminal interactif mutuellement exclusifs dans une même fenêtre ; restauration du diagnostic après déconnexion. |
| W4 | Windows | Presse-papiers vide ou illisible signalé, sans échec silencieux. Les refus I/O du presse-papiers ne sont pas injectés dans cette campagne. |
| W5 | Windows | Boutons réorganisables, titre enveloppant et défilement de la fenêtre ; limites des boutons contrôlées dans une fenêtre de 540 px. |

## Quinze points de contrôle

Ces points sont des scénarios distincts, pas quinze répétitions du même test.
Le parcours GUI SSH/SFTP complémentaire passe aussi, avec le calage Xvfb décrit ci-dessous.

| # | Contrôle | Résultat et preuve |
|---|---|---|
| 01 | Dossiers imbriqués, déplacement de profil et glisser-déposer | PASS dans le rejeu GTK Linux : création, déplacement contextuel, dossiers/profils glissés. |
| 02 | Mauvaise identité SSH et clé hôte modifiée | PASS, fixture OpenSSH/Paramiko sur 127.0.0.1. |
| 03 | SSH UTF-8, PTY et redimensionnement | PASS, fixture loopback et ouverture du terminal applicatif. |
| 04 | Transfert SFTP de fichiers et dossiers | PASS bidirectionnel par transport réel et GUI : fichiers/dossiers multiples, glisser-déposer, cinq cycles de navigation, édition et reprise manuelle de file. |
| 05 | Nombre d'entrées du listing accessible | PASS, assertion d'un nombre réel non nul et description anglaise. |
| 06 | Édition par le client Rust : conservation de 0755 | PASS, test Rust ignoré par défaut exécuté avec la fixture locale. |
| 07 | Échec SFTP injecté : original intact et temporaire supprimé | PASS, même test Rust contre le vrai protocole SFTP. |
| 08 | File persistante, chiffrement et protection de la destination | PASS, tests `transfers` du workspace. |
| 09 | Focus GTK avant désactivation | PASS, test GTK ignoré exécuté explicitement sous Xvfb avec fatal-warnings. |
| 10 | Windows : entrées bornées et resize après arrêt | PASS, test sur file libérée après arrêt. |
| 11 | Windows : modes clavier et mutex occupé | PASS, modes application/bracketed-paste et refus sans blocage. |
| 12 | ANSI, Unicode fragmenté, texte échappé, OSC long | PASS, tests du modèle sous Linux et Server 2022. |
| 13 | Touches de contrôle et curseur application | PASS, tests du codeur de touches natifs. |
| 14 | ConPTY live, resize, exit0 et annulation | PASS, Windows Server 2022 natif. |
| 15 | GUI Windows : IM commit, affichage, fenêtre étroite, exclusion diagnostic et déconnexion | PASS, Windows Server 2022 natif. |

Preuve Windows : [run 43700777708](https://github.com/aurelson101/rustremote/actions/runs/43700777708),
source `a79985eb385dead5af1706dc5d816d125b31674f`, huit tests unitaires et parcours
ConPTY/GTK réussis. Rapports locaux : `dist/windows-probe-validation/43700777708/`.

## Commandes et limites

```sh
cargo fmt --all -- --check
cargo check --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
GDK_BACKEND=x11 GTK_THEME=Adwaita GSK_RENDERER=cairo xvfb-run -a dbus-run-session -- env G_DEBUG=fatal-warnings cargo test --workspace dialogs::focus_tests::disabling_input_delivers_focus_out_and_preserves_other_input -- --ignored --exact
python3 scripts/test-ssh-sftp-local.py --gui target/debug/rustremote --rust-editor-only
cargo fmt --manifest-path experiments/windows-probe/Cargo.toml -- --check
CARGO_TARGET_DIR=target cargo clippy --locked --offline --manifest-path experiments/windows-probe/Cargo.toml --all-targets -- -D warnings
CARGO_TARGET_DIR=target cargo test --locked --offline --manifest-path experiments/windows-probe/Cargo.toml
```

Résultats automatiques Linux : 54 tests principaux +1 askpass PASS, deux tests
ignorés par défaut exécutés séparément avec succès (focus et SFTP). Prototype :
huit tests PASS localement et huit PASS natifs (mêmes cas, pas seize cas uniques).

Journaux locaux : `/tmp/rustremote-debug15-{linux-tests,probe-tests,editor,focus,gui}.log`.
Le test Rust SFTP exige la fixture éphémère : profil limité à 127.0.0.1 et racine de
laboratoire dans le dossier temporaire ; aucune connexion de production.

Le parcours GUI SSH/SFTP passe intégralement avec la commande suivante. Le pilote
AT-SPI de cet hôte exposait des coordonnées différentes des pixels rendus : le
calage explicite de40px concerne uniquement ce laboratoire Xvfb (valeur par défaut0,
borne±128), pas l'application. Aucune modification des gestes produit n'a été gardée.
Le helper actionne les sélecteurs nativement et ne remplace plus un bouton valide
par un label ne possédant que l'action de copie (cause du StopIteration observé).

```sh
GDK_BACKEND=x11 GTK_THEME=Adwaita RUSTREMOTE_TEST_SFTP_POINTER_OFFSET=40 RUSTREMOTE_GTK_FATAL_WARNINGS=1 RUSTREMOTE_ENGLISH_GUI=1 RUSTREMOTE_SSH_SFTP_ONLY=1 python3 scripts/test-ssh-sftp-local.py --gui target/debug/rustremote
```

Navigation physique locale/distante répétée cinq fois, filtre/sélection,
glisser-déposer, upload/download, permissions0755, édition, reprise explicite de file,
échec de sauvegarde sans altération de l'original, limites SFTP et passage au français
après redémarrage : PASS. Le log est `/tmp/rustremote-debug15-gui-calibrated.log`.
Le flag SSH/SFTP exclut explicitement la découverte Apple : son menu contextuel
n'est pas requalifié ici. Les échecs exploratoires antérieurs ne sont pas masqués,
mais ne constituent plus le résultat final de ce parcours calibré.

Propriétaire/ACL/xattrs lors du remplacement SFTP, endurance RAM/CPU, bureaux
KDE/Xfce/Cinnamon, Windows 11, presse-papiers défaillant et périphériques restent
à qualifier. Aucune nouvelle AppImage ou release n'est publiée dans cette campagne.
