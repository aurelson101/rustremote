# Validation RustRemote 0.6.0 — 2026-11-09

Préversion, application indépendante en Rust/GTK4. Les résultats concernent
l’AppImage x86_64 et des laboratoires locaux exclusivement. Aucun serveur réel,
compte système ou bureau personnel n’a été utilisé pour les connexions.
Les preuves structurées figurent dans [validation/0.6.0.json](validation/0.6.0.json).

## Construction et portabilité

Construction rootless dans Debian 12 amd64 : couche OCI et Rust 1.93.0 épinglés
par SHA256, crates Cargo verrouillées/vendored, APT signé dans le rootfs privé.
Aucune installation sur l’hôte, aucun sudo ni démon Docker. Les versions des
paquets de construction sont jointes à la release ; les paquets APT ne constituent
pas un snapshot figé, donc la reproductibilité binaire complète n’est pas promise.

L’audit ELF de toutes les bibliothèques embarquées fixe **glibc ≥2.36**, GTK 4.8.3
et VTE 0.70.6. GTK/VTE, SVG, IBus/Fcitx sont inclus. Les pilotes graphiques, C++ et
Wayland restent ceux du système cible ; les chemins GTK/GIO/LD de l’AppImage sont
retirés avant le lancement des clients externes. Le chargeur SVG et les modules
compatibles ont corrigé respectivement les icônes absentes et les symboles IBus
incompatibles. Voir [GTK runtime](https://docs.gtk.org/gtk4/running.html).

Vérifié : démarrage X11 dans le véritable userspace Debian 12 ; parcours sur
Ubuntu 26.04.1/GNOME, X11 privé et Wayland. KDE, Xfce, Cinnamon/Mint et les autres
distributions ne sont pas installés ni certifiés par ces essais. L’image cible
x86_64/glibc≥2.36 : Ubuntu 22.04/glibc2.35, ARM et musl demandent un autre build.
Les clients système OpenSSH/FreeRDP3/TigerVNC/picocom restent des prérequis selon
le protocole. Sans FUSE, utiliser `APPIMAGE_EXTRACT_AND_RUN=1`.

## Contrôles réalisés

- `cargo fmt --all -- --check`, `cargo check --workspace`, Clippy avec
  `-D warnings`, `cargo test --workspace` : réussis, **39 tests** (38 + 1 askpass).
  Catalogue de traduction revérifié après le complément final des libellés.
- `python3 scripts/build-portable-appimage.py --validate` : AppImage construite,
  démarrage sans chiffrement, permissions privées, verrouillage et réouverture.
- Anglais par défaut sans variable de langue ; français sauvegardé dans Paramètres
  puis confirmé après redémarrage. Les noms utilisateurs et contenus distants
  restent dans leur langue d’origine. Captures README issues de ce laboratoire.
- Sous-dossiers, déplacement par menu, DND réel d’un dossier avec descendants et
  d’un profil ; données persistantes contrôlées. Cycles/collisions testés en Rust.
- Affichage : déplier/replier, rechercher, concentration/restauration ; outils
  système et empreinte de clé publique. Pas de commandes arbitraires ni de clé
  privée lue par cet outil.
- SSH/SFTP réel sur loopback : clés/agent, terminal UTF-8/PTY/resize, refus de clé
  invalide et d’empreinte hôte modifiée. Refus de traversée et de liens symboliques.
- Navigateur SFTP : envoi de plusieurs fichiers et dossiers, DND physique d’un
  fichier local, édition distante puis relecture, téléchargement multisélection
  fichiers/dossiers ; contenu comparé octet par octet.
- RDP/NLA réel : bureau Xvfb isolé sur 127.0.0.1, certificat épinglé, refus de mauvais
  certificat/mot de passe, pixels distants, profil domaine/mot de passe protégé,
  connexion, déconnexion/reconnexion et arrêt du client à la fermeture de l’app.
- Wayland : démarrage, formulaires de connexion et plein écran/restauration
  confirmés par le compositeur.

Commandes de rejeu (depuis le dépôt, outils de laboratoire installés) :

```bash
RUSTREMOTE_ENGLISH_GUI=1 RUSTREMOTE_SSH_SFTP_ONLY=1 python3 scripts/test-ssh-sftp-local.py --gui dist/RustRemote-0.6.0-preview-x86_64.AppImage
python3 scripts/test-rdp-local.py --app dist/RustRemote-0.6.0-preview-x86_64.AppImage
RUSTREMOTE_TEST_BACKEND=wayland python3 scripts/smoke-profiles.py dist/RustRemote-0.6.0-preview-x86_64.AppImage --startup-only --display-checks
```

## Erreurs observées et limites ouvertes

GTK 4.8 émet encore `gtk_root_get_focus: GTK_IS_ROOT` et des avertissements
`GtkText - did not receive a focus-out event` lors des fermetures de dialogues.
Les parcours terminent et le processus reste fonctionnel, mais ces diagnostics
restent à corriger. Le harnais RDP les compte explicitement ; il continue de
refuser tout autre critical/panic. Aucun symbole IBus indéfini sur le dernier
parcours. Les messages FreeRDP de refus/certificat, d’arrêt volontaire et certaines
limitations Shadow restent dans le rapport assaini : zéro message d’erreur n’est
pas revendiqué. Les essais initiaux ont échoué sur des différences AT-SPI GTK4.8
(visibilité, délégation, objets périmés, popover sous le client RDP) ; le harnais a
été adapté avant le rejeu final. Le cache par nœud d’AT-SPI causait un crash du
harnais dans Debian ; il a été retiré, puis le démarrage a réussi.

CLIPRDR n’est pas fourni par ce serveur Shadow ; le test bidirectionnel réel de
la version 0.5.0 sous GNOME Remote Desktop est historique, pas un nouveau test de
cette image. Audio, imprimantes, multimoniteur et Windows/AD ne sont pas qualifiés.
VNC, série et import/export conservent leurs validations antérieures et tests
unitaires ; pas de nouveau test physique de ces protocoles dans cet incrément.
Pas d’essai d’endurance établissant absence de fuite mémoire.

SFTP reste borné à 16 Mio / 128 entrées / 8 niveaux / 30 s ; éditeur UTF-8 ≤1 Mio. Reprise des
fichiers vérifiée, pas de reprise récursive des dossiers ni file persistante.
Stockage SQLite, moteur RDP intégré, CI/signature/.deb et autres architectures
restent ouverts. Ce n’est pas une parité complète MobaXterm/FileZilla.

## Artefact validé

`RustRemote-0.6.0-preview-x86_64.AppImage` — SHA256 :

```text
3009d663af6bc29e93fb6009adf485fa43804f35ff2908ca4466d5cc8cb8a428
```

Publication GitHub en préparation ; le statut final figure dans le rapport JSON.
