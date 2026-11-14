# Validation RustRemote 0.9.0 — 2026-11-14

## Corrections et ajouts

- Navigation SFTP : clic simple dossier local/distant ; Ctrl/Shift conservent la
  sélection multiple. Le ListBox reçoit l'activation souris/clavier ; les ouvertures
  sont différées pour éviter de retirer une ligne pendant son événement. Le chemin
  distant est canonicalisé par SFTP realpath (plus d'accumulation `/../..`).
  [GTK ListBoxRow::activate](https://docs.gtk.org/gtk4/signal.ListBoxRow.activate.html)
  est un signal de raccourci ; le signal du ListBox couvre aussi la souris.
- File persistante privée par profil : 256 transferts maximum, écritures atomiques
  et verrou exclusif hors UI, aucune reprise automatique après réouverture. Les
  jobs Running retrouvés deviennent Interrupted. Pause, démarrage, réessai et
  nettoyage des métadonnées terminées ; aucun effacement de fichiers par ce bouton.
  Chiffrement age selon le magasin actif, sans mot de passe dans la file ; chemins
  bornés et identité hôte/port/compte vérifiée avant reprise.
- Paramètres : limites 1–1024 Mio, 1–4096 entrées, 1–32 niveaux, 5–3600 secondes.
  Défaut 16 Mio/128/8/30s ; appliquées au prochain job. Listing et éditeur gardent
  leurs bornes spécifiques. Paramètres défilants pour éviter le pied de fenêtre coupé.
- Diagnostic GTK critique reproduit sous GDB avec GTK4.8 : RdpForm::new appelait
  Notebook::set_current_page avant GtkRoot. Appel déplacé au premier map.
- IPv6 : loopback et ULA /120–/128 explicites (256 adresses maximum), refus public,
  link-local sans interface et grandes plages. Noms inverses optionnels via
  `/usr/bin/getent hosts`, NSS hôte DNS/mDNS, jamais utilisés pour la connexion.
  32 équipements maximum, 800 ms par nom dans le budget global30s ; sortie1Kio,
  nom ASCII validé, annulation et kill/wait du sous-processus, aucun shell.
  [NSS systemd](https://www.freedesktop.org/software/systemd/man/latest/nss-resolve.html)
  décrit la résolution selon les services configurés. Pas d'inventaire DNS-SD.

## Tests effectués

- Cargo fmt/check/Clippy `-D warnings` : PASS. Tests : 54 applicatifs +1 askpass.
- File : redémarrage, récupération des interrompus, exclusivité, refus d'une autre
  cible, chiffrement réel sans chemins lisibles, chemins relatifs/liens symboliques
  refusés et limite256 transactionnelle. Paramètres : roundtrip privé avec limites
  non standard ; limites octets/entrées invalides refusées.
- IPv6 : vraie sonde TCP loopback `::1` ; ULA et refus de plages. Résolution inverse
  localhost, annulation avant requête et noms malformés/adresse différente refusés.
- Build portable Debian12 rootless et startup : PASS. Minimum glibc2.36 x86_64,
  GTK4.8.3/VTE0.70.6. Aucune prétention de compatibilité avec tous les Linux.
- GUI SSH/SFTP loopback : envoi/réception graphique de fichiers/dossiers multiples,
  DND, édition/readback, filtres et sélection. Cinq navigations aller/retour par
  clic simple sur chacun des deux panneaux : PASS. File mise en pause, enregistrée,
  navigateur fermé/rouvert, absence de reprise implicite puis démarrage : PASS.
- Startup sous G_DEBUG=fatal-criticals : PASS. Wayland startup et plein écran /
  restauration : PASS avec le même mode bloquant pour les criticals.
- GUI final sur l’AppImage reconstruite, G_DEBUG=fatal-criticals : PASS. Les quatre
  limites saisies sont sauvegardées à 32/256/12/60 et la langue française est
  appliquée après redémarrage. Aucun critical GTK dans ce rejeu ; les warnings
  nonfatals restent visibles. Résultats structurés : [validation/0.9.0.json](validation/0.9.0.json).

Commandes principales :

```bash
cargo fmt --all -- --check
cargo check --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
python3 scripts/build-portable-appimage.py --validate
G_DEBUG=fatal-criticals RUSTREMOTE_ENGLISH_GUI=1 RUSTREMOTE_SSH_SFTP_ONLY=1 python3 scripts/test-ssh-sftp-local.py --gui dist/RustRemote-0.9.0-preview-x86_64.AppImage
RUSTREMOTE_TEST_BACKEND=wayland G_DEBUG=fatal-criticals python3 scripts/smoke-profiles.py dist/RustRemote-0.9.0-preview-x86_64.AppImage --startup-only --display-checks
```

## Limites conservées

L'avertissement nonfatal GtkText focus-out persiste sous GTK4.8 ; GDB fatal-warnings
le situe dans blink_cb, sans frame applicative. Non masqué et distinct du critical
corrigé. Des avertissements de thème hôte peuvent apparaître sous GTK ancien.
Le blocage précis de la capture utilisateur n'avait pas de processus actif à
inspecter ; les corrections de navigation et le rejeu ne prouvent pas l'absence de
tout blocage possible sous d'autres conditions réseau.

Pas de connexion serveur IA, scan LAN, mDNS réel/Windows/AD/macOS, périphériques,
KDE/Xfce/Cinnamon ou endurance mémoire qualifiés ici. mDNS dépend du NSS configuré
sur l'hôte. Les noms restent indicatifs. Renommer un profil ou changer le
chiffrement ne migre pas ses anciennes files ; les métadonnées anciennes restent
privées sur disque. Reprise récursive partielle des dossiers encore manuelle,
destination existante jamais écrasée automatiquement. Aucun audit global de sécurité
ni garantie d'absence de fuite n'est revendiqué.

[Préversion 0.9.0 publiée](https://github.com/aurelson101/rustremote/releases/tag/v0.9.0) sur `ed643426f7f86f701ff4bc1c52d6fea73ece8f27`.
Les quatre assets AppImage/SHA256/ABI/paquets ont été retéléchargés et comparés aux
fichiers locaux avant publication ; SHA256 vérifié. Anciennes releases conservées.

SHA256 de l’AppImage validée : `ecc4576883aca2613acd70584a494e3d068659cf920d8d15de2cb7894121bcd8`.
