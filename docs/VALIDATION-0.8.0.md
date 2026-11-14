# Validation RustRemote 0.8.0 — 2026-11-14

## Fonctionnement livré

Le clic droit sur un dossier propose Apple et VNC, en conservant le dossier
sélectionné. Pour Apple, laisser la passerelle vide reprend l’hôte et le compte
Mac du formulaire ; les validations de tunnel vers le même Mac restent actives.

Outils → Découvrir les équipements : simulation initiale, recherche TCP 22/3389/
5900/445, IPv4 RFC1918 ou loopback explicite, CIDR /24 maximum, huit workers par
scan. Budget 30s, arrêt après la sonde courte en cours ; connexion 180ms, annonce
RFB 250ms. Aucun DNS ni accès privilégié. L’annonce ARD est lue sans sélectionner
un mode d’authentification. Les indications OS restent des hypothèses de service.
Ni un port SSH ni SMB/RDP ne prouve à lui seul le système du poste.

Le scan réel exige confirmation de la plage ; modifier celle-ci efface l’accord.
--offline le bloque. Public, link-local et IPv6 sont refusés. Annulation disponible
et déclenchée quand la fenêtre est fermée. Aucun réseau réel n’a été scanné ici.

Clic droit d’un résultat, ou bouton Ajouter comme connexion : choix SSH, Linux/SSH,
Windows/RDP, Apple/ARD ou VNC, puis éditeur existant prérempli. Compte utilisateur,
domaine, clés/mot de passe et dossier sont gérés dans cet éditeur. Dossier existant
ou nouveau chemin hiérarchique possible. Enregistrer ne lance aucune connexion.
Les résultats simulés conservent simulation=true, même si la case de simulation
est décochée après leur affichage.

## Vérifications

- Cargo fmt/check/Clippy avec -D warnings/test : réussis, 45 tests (44 +1 helper).
- Tests de plages privées, limites CIDR, refus public/link-local/IPv6 ; socket TCP
  réel mais exclusivement loopback sur port éphémère, annulation avant sonde.
- Serveur RFB factice loopback : type30 détecté ; serveur vérifie qu’aucun choix
  d’authentification n’est envoyé avant fermeture du client.
- Harnais graphique : menu Apple du dossier, passerelle implicite validée ; refus
  du scan sans consentement, refus public, accord réinitialisé après changement
  de plage ; trois équipements fictifs, clic droit Apple/Windows, bouton Linux,
  noms/comptes/domaine AD et dossiers existants/nouveaux contrôlés dans le JSON.
- Parcours SSH/SFTP de l’AppImage finale : fichiers/dossiers dans les deux sens,
  édition/relecture, DND, filtres, menus et langue anglaise/française rejoués.
- Build portable Debian 12 x86_64, démarrage X11 sur ce userspace ; ABI glibc 2.36,
  GTK4.8.3/VTE0.70.6 conservée.

Les résultats et empreintes sont dans [validation/0.8.0.json](validation/0.8.0.json).
Captures : [découverte simulée](screenshots/network-discovery.png) ; hôtes RFC5737,
aucun compte réel ni secret.

## Erreurs de test et limites

Premier harnais arrêté sur une CheckButton dépourvue d’action AT-SPI exploitable
sous GTK4.8 ; corrigé par clic physique sur le Xvfb privé, avec vérification du
refus réel ensuite. La revue de capture a révélé un bouton ajouté dynamiquement
encore en français ; traduit avant le dernier build.

Les diagnostics de focus GTK4.8 précédents restent ouverts, sans promesse zéro bug.
Pas de qualification d’identification OS exhaustive, IPv6, réseaux publics, Mac
réel, Windows/AD réel, imprimantes/audio/CLIPRDR ni d’endurance dans cet incrément.
Le test du domaine vérifie son enregistrement, pas une authentification AD.
Un équipement sans ces ports ouverts peut ne pas apparaître. La fermeture de
port ou un pare-feu ne prouve pas l’absence d’équipement. Les profils VNC/Apple
continuent d’utiliser des clients externes et les limites SFTP 0.7 sont inchangées.

## Rejouer

```bash
cargo test --workspace discovery::tests
RUSTREMOTE_ENGLISH_GUI=1 RUSTREMOTE_DISCOVERY_ONLY=1 RUSTREMOTE_SSH_SFTP_ONLY=1 python3 scripts/test-ssh-sftp-local.py --gui dist/RustRemote-0.8.0-preview-x86_64.AppImage
RUSTREMOTE_ENGLISH_GUI=1 RUSTREMOTE_SSH_SFTP_ONLY=1 python3 scripts/test-ssh-sftp-local.py --gui dist/RustRemote-0.8.0-preview-x86_64.AppImage
```

Les laboratoires exigent les outils de test existants (Xvfb/Xauth/XTest, PyGObject,
Paramiko/Pillow et clients locaux) et restent éphémères sans sudo.

Artefact validé — SHA256 :

```text
6499c7effff769a214bb832a551fb8b1345d277c902d6c24d5cfa5894c34bede
```

[Préversion v0.8.0 publiée](https://github.com/aurelson101/rustremote/releases/tag/v0.8.0)
le 2026-11-14 sur le commit `7b6492f390b18f1832841abcfec20826c622065e`.
Les quatre assets ont été retéléchargés et comparés aux locaux ; SHA256 vérifié.
Anciennes releases et visibilité publique existante conservées.
