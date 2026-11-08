# RustRemote

Client de sessions distantes pour Linux, écrit en **Rust** avec une interface
**GTK4 en français** : terminaux SSH multi-onglets, navigateur SFTP, profils et
lancement de connexions RDP, VNC et série.

**En développement — Ubuntu 26.04 x86_64 est la plateforme actuellement testée.**
RustRemote est un projet indépendant, inspiré des usages des outils de gestion de
sessions. Il ne contient pas de code propriétaire de MobaXterm ou de FileZilla.

[État des demandes](docs/AUDIT-DEMANDES-2026-11-08.md) ·
[Feuille de route](docs/ROADMAP.md) · [Sécurité](docs/SECURITY.md) · [Licence](LICENSE)

## Aperçu

### Terminal SSH intégré

![Terminal SSH multi-onglets de RustRemote](docs/screenshots/terminal-ssh.png)

Connexion réelle à un serveur SSH de laboratoire local : PTY, commandes et réponses
UTF-8, sélection et presse-papiers. Aucun serveur de production n'est présenté.

### Navigateur SFTP

![Navigateur SFTP et éditeur de fichier distant](docs/screenshots/navigateur-sftp.png)

Liste et éditeur de fichiers d'un laboratoire SFTP. Les fichiers et dossiers de test
servent à vérifier les transferts, les collisions et le refus des liens symboliques.

## Fonctionnalités présentes dans les sources

- **SSH** : terminaux VTE/OpenSSH dans des onglets, clé existante, SSH Agent ou mot
  de passe du profil, vérification des clés d'hôte, redimensionnement, zoom et
  copier/coller contrôlé, y compris au clic droit.
- **SFTP** : connexion native ssh2/libssh2, listing, édition UTF-8, création de dossiers,
  renommage, permissions Unix, suppression confirmée, transferts récursifs et sélection
  multiple ; protection contre l'écrasement et détection des conflits d'édition.
- **Profils** : dossiers Linux/Windows, favoris, recherche, édition, menus contextuels,
  import/export JSON RustRemote ; chiffrement age facultatif au premier lancement.
- **Identifiants** : mot de passe dans le formulaire de chaque profil SSH/RDP ;
  sauvegarde facultative dans le coffre du système, référence seule dans les profils.
  Domaine AD/realm configurable. Coffre et Paramètres sont deux fenêtres distinctes.
- **RDP** : lancement FreeRDP avec domaine et mot de passe sur pipe, certificat contrôlé,
  NLA, déconnexion suivie ; options de taille, écrans, son, microphone, imprimantes
  et presse-papiers avec consentements distincts.
- **VNC et série** : lanceur TigerVNC avec option de tunnel SSH ; terminal picocom
  intégré pour les équipements série.
- **Interface et outils** : thème sombre, icônes embarquées, panneaux et barres
  masquables, plein écran, paramètres persistants, Ping/Traceroute avec simulation.

## Limites actuelles

Le projet n'est pas une réimplémentation complète de MobaXterm ou de FileZilla.
RDP/VNC utilisent des fenêtres externes ; tunnels autonomes, macros et Multi-Exec
restent désactivés. Le SFTP ouvre un transport distinct et n'applique pas les alias,
ProxyJump ou options OpenSSH ; progression détaillée et reprise sont à développer.

Les transferts SFTP sont bornés à **16 Mio, 128 entrées, 8 niveaux et 30 secondes** ;
l'éditeur accepte du texte UTF-8 jusqu'à 1 Mio. Un envoi interrompu peut laisser une
cible partielle ; les destinations existantes sont refusées par les transferts.
La suppression distante est définitive, confirmée et non atomique.

Les options RDP de presse-papiers sont configurées, mais **la copie réelle de texte,
fichiers et dossiers via CLIPRDR n'est pas validée** par le laboratoire Shadow actuel.
Windows/AD réels, son, imprimantes, double écran, matériel série et KDE restent à
qualifier. Le dernier rejeu automatisé Wayland de cet incrément a échoué pendant
le parcours de démarrage ; le fonctionnement X11 ne vaut pas validation Wayland.

Le dépôt contient un incrément **non publié après la 0.4.1**. L'ancienne AppImage
0.4.1 ne contient pas les derniers ajouts ; aucune nouvelle image n'est annoncée
comme complète. Voir [l'audit avant AppImage](docs/AUDIT-DEMANDES-2026-11-08.md).

## Compiler et lancer

Prérequis : Rust/Cargo avec prise en charge de l'édition 2024, compilateur C,
`pkg-config`, en-têtes GTK4/VTE GTK4/OpenSSL/libssh2, Python 3 et curl pour le packaging.
Clients système selon le protocole : `openssh-client`, `freerdp3-x11`,
`tigervnc-viewer`, `picocom`. Ping/Traceroute : `iputils-ping` et `traceroute`.
Un service compatible Secret Service est nécessaire pour mémoriser les mots de passe.

```bash
git clone https://github.com/aurelson101/rustremote.git
cd rustremote
cargo build --locked --workspace --bins
# Démonstration temporaire :
cargo run --locked -- --demo
# Profils personnels, sans autoriser les connexions réseau :
cargo run --locked -- --offline
```

Le build produit aussi `rustremote-askpass` : conserver ce helper à côté du binaire
principal. Au premier démarrage, choisir de chiffrer les métadonnées ou de continuer
sans chiffrement. Les profils sont conservés dans `$XDG_CONFIG_HOME/rustremote`, ou
`~/.config/rustremote`, avec permissions privées. Les mots de passe sauvegardés vont
au coffre OS, jamais au JSON/export ; la phrase secrète age n'est pas récupérable.

Pour les connexions réelles, lancer sans `--offline`, puis confirmer la cible dans
l'application. **Quitter la démonstration** ouvre les profils personnels sans copier
les exemples. `--offline` reste contraignant après cette sortie : c'est un contrôle
applicatif, pas une sandbox réseau du système.

## Vérifications

```bash
cargo fmt --all -- --check
cargo check --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
python3 scripts/smoke-profiles.py target/debug/rustremote
python3 scripts/test-ssh-sftp-local.py --gui target/debug/rustremote
python3 scripts/test-rdp-local.py --app target/debug/rustremote
```

Les laboratoires nécessitent leurs outils supplémentaires (Paramiko, PyGObject,
AT-SPI, Xvfb, xauth, xclip, FreeRDP Shadow, Pillow…). Ils utilisent des fixtures
éphémères et des bureaux privés. Ne pas remplacer leurs cibles locales par une
infrastructure réelle sans autorisation. Résultats réels et commandes spécialisées :
[audit du 8 novembre 2026](docs/AUDIT-DEMANDES-2026-11-08.md).

Construction AppImage : `./scripts/build-appimage.sh` après incrément de version,
validation et conservation de l'ancien artefact. Le script embarque les bibliothèques
ELF hors glibc, leurs licences et le helper d'authentification ; la portabilité vers
d'autres distributions n'est pas encore validée.

## Licence

**Usage personnel et professionnel autorisé, revente interdite.**
Les prestations distinctes de support, formation ou intégration peuvent être facturées ;
la copie du logiciel et son droit d'utilisation doivent rester gratuits.

Les éléments originaux sont soumis à la licence personnalisée
[LicenseRef-RustRemote-No-Resale-1.0](LICENSE). Le code est disponible, mais cette
restriction de revente en fait un projet **source disponible**, pas une licence open
source au sens de [l'Open Source Initiative](https://opensource.org/osd).
Les [composants tiers](NOTICE) conservent leurs propres licences et droits.

## Contribuer

Consulter [AGENTS.md](AGENTS.md), [MEMORY.md](MEMORY.md) et
[docs/ROADMAP.md](docs/ROADMAP.md) avant toute modification. Préférer les changements
ciblés, les tests locaux et les rapports qui distinguent ce qui est testé des limites.
Ne publier aucun secret, clé privée, profil personnel ou capture d'infrastructure réelle.
