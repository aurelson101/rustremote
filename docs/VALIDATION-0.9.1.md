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

La [préversion0.9.1](https://github.com/aurelson101/rustremote/releases/tag/v0.9.1)
est publiée sur `db99bb1ba943b3f244f8c31c3aafebd706d8254f`. Les quatre assets ont été retéléchargés et comparés aux
locaux, puis les six anciennes releases v0.5.0–v0.9.0 ont été supprimées à la demande
de l'utilisateur. Métadonnées et22assets sont sauvegardés et vérifiés dans l'archive
locale dist/release-backups/2026-11-14-before-0.9.1, hors Git. Les six tags historiques
sont conservés à l'identique. Les17anciens artefacts locaux ont été déplacés dans
l'archive, sans suppression des copies de rollback. Seule0.9.1 reste dans les releases
GitHub. Le rapport HTML personnel reste local et n'est pas un asset de release.

SHA256 de l'AppImage : `e2642a459140ad0326117f50aee5d6b85e767834008fa085145abd101b18a29b`.
