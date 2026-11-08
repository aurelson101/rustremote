# Validation 0.5.0 — 8 novembre 2026

Aperçu Ubuntu 26.04 x86_64, Rust/GTK4. Les essais réseau utilisent uniquement des
laboratoires éphémères sur loopback ; aucun serveur de production contacté.
Cette version ne revendique pas une équivalence complète avec MobaXterm/FileZilla.

## SFTP réel, distinct de la démonstration

Le navigateur utilise ssh2/libssh2 et le sous-système SFTP d’un serveur SSH local
Paramiko. Les validations comparent les octets des fichiers réellement présents
sur le disque du serveur et ceux du client ; elles ne se limitent pas aux libellés UI.

Couverture : clé, agent et mot de passe ; listing, navigation, édition UTF-8 et
relecture distante ; détection de modification concurrente ; création de dossiers,
renommage, permissions, suppression confirmée ; envoi/téléchargement de fichiers et
dossiers récursifs, sélection multiple et sélecteurs GTK ; refus d’écrasement et de
liens ; progression par octets, annulation, préservation du téléchargement partiel,
reprise de fichiers dans les deux sens avec comparaison préalable du préfixe.

Le transport SFTP reste distinct du terminal. HostName, HostKeyAlias, fichiers de
confiance/identité et ProxyJump sont résolus par OpenSSH. Le bastion exige une clé
ou un agent et une empreinte déjà validée ; ProxyCommand personnalisé est refusé.

Limites : 16 Mio/128 entrées/8 niveaux/30 secondes par opération, édition UTF-8
1 Mio ; aucune file persistante ni reprise récursive des dossiers. Un envoi interrompu
peut laisser une cible partielle. Les suppressions distantes sont définitives.

## Corrections et outils

- Sélecteurs locaux GTK utilisables sans dépendre d’un portail FileChooser absent.
- Annulation et libération du transport SSH sur worker ; progression atomique,
  consultation UI à 100 ms pendant une opération, aucun polling permanent au repos.
- Tunnels OpenSSH local/distant/SOCKS, écoute explicitement loopback, arrêt suivi.
- Macros prédéfinies et Multi-Exec après aperçu/confirmation, 1 à 8 sessions SSH.
  Le contrôle privé OpenSSH n’autorise l’envoi qu’après authentification ; les
  tunnels et terminaux attendant une clé d’hôte/mot de passe ne sont pas des cibles.
- Harnais Wayland corrigé : événements AT-SPI pompés et actions sur éléments visibles.

## Vérifications reproductibles

```bash
cargo fmt --all -- --check
cargo check --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
./scripts/build-appimage.sh
python3 scripts/test-ssh-sftp-local.py --gui dist/RustRemote-0.5.0-preview-x86_64.AppImage
RUSTREMOTE_TUNNELS_ONLY=1 RUSTREMOTE_SSH_SFTP_ONLY=1 python3 scripts/test-ssh-sftp-local.py --gui dist/RustRemote-0.5.0-preview-x86_64.AppImage
RUSTREMOTE_TEST_PASSWORDS=1 RUSTREMOTE_PASSWORD_ONLY=1 RUSTREMOTE_SSH_SFTP_ONLY=1 python3 scripts/test-ssh-sftp-local.py --gui dist/RustRemote-0.5.0-preview-x86_64.AppImage
python3 scripts/test-rdp-local.py --app dist/RustRemote-0.5.0-preview-x86_64.AppImage
python3 scripts/test-rdp-clipboard-local.py
python3 scripts/test-vnc-serial-local.py
RUSTREMOTE_TEST_BACKEND=wayland python3 scripts/smoke-profiles.py dist/RustRemote-0.5.0-preview-x86_64.AppImage --startup-only --display-checks
```

Contrôles Cargo réussis : **33 tests applicatifs + 1 test askpass**, zéro échec.
AppImage : version normale et `APPIMAGE_EXTRACT_AND_RUN=1`, SHA256 vérifiés.
Artefact : `RustRemote-0.5.0-preview-x86_64.AppImage`, 41 081 336 octets.
SHA256 : `03151f5520f30a29f9d52b8d120ebe291ddbee0d69337108feb9609acef80570`.
RDP depuis le profil : domaine/mot de passe, certificat épinglé, refus certificat et
mot de passe incorrects, pixels, déconnexion et fermeture du client à la sortie : PASS.
VNC authentifié via tunnel et série bidirectionnelle sur pseudo-terminal : PASS.
Tunnels SSH natifs local/distant/SOCKS : rejeu ciblé PASS, écho bidirectionnel et
fermeture effective du port à chaque déconnexion. Macros et Multi-Exec : aperçu,
annulation sans envoi, puis commande reçue par deux sessions réelles : PASS.
Résultats structurés : [rapport 0.5.0](validation/0.5.0.json) et
[CLIPRDR](validation/rdp-clipboard-0.5.0.json).
Wayland natif sur l’AppImage : ouverture sans chiffrement, plein écran confirmé par
le compositeur puis restauration : PASS.

CLIPRDR réel : GNOME Remote Desktop/GNOME Shell headless dans des namespaces
user/net/mount/pid privés, PipeWire et bureaux distincts, FreeRDP avec NLA et
certificat épinglé. Texte dans les deux sens ; un/plusieurs fichiers et un/plusieurs
dossiers dans chaque sens : **10 cas PASS**. Les chemins reçus diffèrent des sources,
passent par le montage FUSE et leurs contenus sont comparés récursivement.
Le laboratoire Shadow utilisé pour les profils ne fournit pas CLIPRDR : les deux
laboratoires sont complémentaires, pas interchangeables.

## Erreurs observées et limites de qualification

Un relais du laboratoire bastion coupait les sessions après 5 s/8 Kio : limite
corrigée en délai d’inactivité de 60 s et 16 Mio pour rejouer SSH/SFTP réellement.
Les anciens échecs Wayland provenaient d’éléments AT-SPI obsolètes/hors écran.
Le portail FileChooser absent bloquait les sélecteurs natifs : remplacement GTK.
Le parcours SSH/SFTP complet a ensuite échoué dans le harnais au choix du type de
tunnel : GTK expose le texte sélectionné comme nom accessible du DropDown, et non
son Label. Correction du sélecteur du harnais puis rejeu ciblé ; les étapes SFTP,
bastion et Multi-Exec antérieures étaient réussies sur l’AppImage. Le parcours
global initial n’est donc pas déclaré PASS ; les rapports conservent ce résultat.
Les erreurs des tests RDP négatifs sont attendues ; les rapports les distinguent
des échecs de connexion positive. Des avertissements GTK focus-out, AT-SPI et rendu
logiciel Xvfb restent observables ; ils ne sont pas une garantie d’absence de bug.

Mesure courte du parcours Wayland complet sur binaire de développement : RSS
209,5 Mio, variation +3,46 Mio après 10 dialogues/3 laboratoires, CPU inactif 0 %,
descripteurs −12, threads −2. Ce n’est pas un test d’endurance ni une preuve
universelle d’absence de fuite mémoire.

Non qualifiés : Windows/AD/LDAP réels, audio/imprimantes/multimoniteur physiques,
switch série physique, KDE, autres distributions. IronRDP/VNC intégrés, SQLite,
verrouillage automatique du coffre, X11 forwarding, CI/.deb et signature restent
ouverts dans la roadmap. Publication en préversion, dépôt privé conservé.
