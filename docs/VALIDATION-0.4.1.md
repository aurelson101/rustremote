# RustRemote 0.4.1 — corrections du 2026-11-08

## Défauts corrigés

- **Sélection SFTP périmée :** changer le chemin ou relancer un listing invalide
  les anciennes lignes et la sélection. Une ancienne sélection ne peut plus ouvrir
  ou télécharger un fichier sous le nouveau chemin saisi.
- **Actions après déconnexion :** Coller, Ouvrir SFTP et Déconnecter suivent l’état
  du terminal. Le lecteur de presse-papiers vérifie aussi l’état avant et après son
  attente asynchrone ; un terminal terminé ne crée plus de confirmation de collage.
- **Collage multiligne :** aperçu du texte réellement capturé, lecture seule et
  défilant, avant confirmation. Limites conservées : 64 Kio et 2 secondes.
- **État série trompeur :** le port local n’affiche plus une validation SSH ; sa
  liaison non chiffrée est indiquée explicitement.
- **Version et outils :** À propos utilise la version Cargo ; l’information Outils
  distingue les fonctions disponibles et les boutons encore désactivés.
- **Construction AppImage :** runtime explicite, cache et SHA256 fixé sur le digest
  de l’asset officiel. Plus de téléchargement implicite à chaque construction.

## Reproduction et méthode

Le test série recevait 536 octets correspondant au texte qu’il venait de recopier
pour observer le terminal, au lieu des deux commandes attendues. Cette observation
modifiait le presse-papiers pendant le scénario. Le scénario série utilise maintenant
l’état termios du PTY privé, les commandes réellement reçues, les réponses écrites et
une capture de l’onglet, sans recopier son écran. L’aperçu du collage est comparé
exactement à la donnée attendue avant confirmation.

Les anciennes lectures AT-SPI de texte VTE restent parfois obsolètes après changement
d’onglet. Le lecteur de test traite les événements GLib ; la copie privée reste un
recours dans les seules phases SSH où elle n’interfère pas avec un collage. Ce n’est
pas une correction de l’accessibilité VTE elle-même.

## Vérifications

- `cargo fmt --all -- --check`, `cargo check --workspace`,
  `cargo clippy --workspace --all-targets -- -D warnings`,
  `cargo test --workspace` : réussis, **23 tests**.
- Parcours réel : `python3 scripts/test-ssh-sftp-local.py --gui IMAGE`.
  Régressions ajoutées : sélection SFTP après changement de chemin, contrôles inactifs
  après déconnexion, contenu exact du collage et état série distinct de SSH.
- Aucun serveur réel ni équipement physique contacté : uniquement 127.0.0.1,
  agent/clés éphémères et pseudo-port série privé ; aucun bureau personnel utilisé.
- Synthèse et résultats réels : `dist/validation-0.4.1-PtVjDW/summary.json`.
  Parcours final complet : **code 0**, série comprise. Le switch reçoit exactement
  les deux commandes (29 octets), répond deux fois ; capture inspectée. Tous les
  services du laboratoire sont arrêtés. Journal : `native-full.log`.
- Le script peut encore journaliser un avertissement GTK de focus-out lors des
  manipulations de dialogues via AT-SPI. Il reste à qualifier ; aucune affirmation
  d’absence totale de défaut ou de fuite mémoire n’est faite.

## Incident de packaging

Le téléchargement automatique du runtime a échoué avec HTTP 504, y compris après
plusieurs tentatives. Le runtime a été récupéré de l’AppImage existante, en retirant
uniquement le digest MD5 embarqué de sa copie extraite, puis validé bit pour bit avec
le SHA256 publié par l’API officielle GitHub AppImage/type2-runtime :
`617aa77314d57c8b8e4fae9c3f0b35af7b30d5529cb69e98f64f92510ed98a46`.
Le cache `.tools/runtime-x86_64` est revérifié à chaque build ; aucun fichier AppImage
existant n’a été modifié par cette extraction. Un changement du runtime amont nécessite
une revue et une mise à jour explicite de son empreinte, jamais sa désactivation.

## Principaux manques confirmés

| Priorité | Reste à livrer |
|---|---|
| P1-03 | Progression/reprise SFTP, transport partagé et prise en charge des alias/bastions OpenSSH ; limites actuelles 16 Mio/128 entrées/8 niveaux/30 s. |
| P1-04 | Renommage/suppression de dossiers, avertissement avant abandon des éditions SFTP, SQLite et migrations ; profils actuellement en JSON privé/age. |
| P2-01/02 | RDP et VNC intégrés : les clients actuels restent externes. Clipboard/périphériques Windows réels non validés. |
| P2-03/04/05 | Tunnels généraux, macros, Multi-Exec et coffre Secret Service/keyring. |
| P3-02 | Publication signée, .deb, CI/SBOM, portabilité et audit actualisé des dépendances. |

Les essais matériels, KDE et Windows ne sont pas couverts par cette étape.

Livrable : `dist/RustRemote-0.4.1-preview-x86_64.AppImage` (39729656 octets).
SHA256 : `a43f5102104d530d05b51afe5aa0fed5e386816834e06c4673bdb5c3d0cc8c6d`. Les anciennes AppImage sont conservées.
