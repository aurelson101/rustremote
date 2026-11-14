# Validation RustRemote 0.9.1 — 2026-11-14

Incrément ciblé de distribution et notices ; aucune réécriture des protocoles.

- GPL-3.0-only, NOTICE et PRIVACY.md présents dans l'AppImage ; octets comparés aux
  fichiers du dépôt après extraction. Manifeste des dépendances : GPL-3.0-only.
- À propos : copyright, licence, absence de garantie et confidentialité affichés,
  traductions anglaises ajoutées. Test graphique anglais ajouté.
- Correction packaging : le build isolé ne copiait pas PRIVACY.md. Le premier build
  a échoué sans ce fichier ; la liste des entrées du builder est corrigée et le
  build portable Debian12/startup final passe.
- fmt/check/Clippy -D warnings et55tests Rust PASS ; syntaxe Bash/Python vérifiée.
- Le rapport HTML demandé reste local, exclu de Git et de l'AppImage. 19 champs,
  affichage 1280/390px sans débordement horizontal, état copie succès/échec avec
  presse-papiers simulé et impression sans texte tronqué vérifiés dans Brave
  headless isolé. Aucune ressource distante intégrée, aucun formulaire soumis.
  La première vérification a détecté une hauteur d'impression insuffisante ; corrigée.

Le rejeu graphique final anglais sous G_DEBUG=fatal-criticals passe : About
GPL/confidentialité, SSH/SFTP natifs loopback, navigation souris répétée, envoi et
réception de fichiers/dossiers multiples, DND, édition/readback, file persistante
fermée/rouverte, limites sauvegardées et français après redémarrage. L'avertissement nonfatal
GtkText focus-out reste présent et n'est pas masqué. Aucun serveur IA, réseau LAN,
Windows/AD/macOS ou périphérique réel n'est testé ici ; aucune garantie générale
contre les bugs, les fuites mémoire ou les détections antivirus.

AppImage x86_64, glibc >=2.36 ; GTK4.8.3 et VTE0.70.6. Cette base ne qualifie pas tous
les Linux. Pas de signature SignPath ni de livrable Windows.

Les six anciennes releases v0.5.0–v0.9.0 ont été téléchargées et vérifiées dans une
archive locale avant suppression demandée. Leur retrait ne sera effectué qu'après
publication et comparaison des quatre assets de la nouvelle release. Les anciens
tags Git seront conservés. Publication et retrait : en préparation.

SHA256 de l'AppImage : `e2642a459140ad0326117f50aee5d6b85e767834008fa085145abd101b18a29b`.
