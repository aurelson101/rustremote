# Validation RustRemote 0.4.0 — 2026-11-07

Version de test Ubuntu 26.04.1 x86_64, GTK4 4.22.4, GNOME/Wayland et X11.
Aucune connexion à une infrastructure réelle ; laboratoires 127.0.0.1 et PTY privés.
Les fonctionnalités suivantes ne constituent pas une équivalence complète à MobaXterm.

## Changements livrés

- Terminal SSH et série intégré VTE, OpenSSH/picocom ; contrôle known_hosts,
  authentification clé/agent, PTY, resize, ANSI/UTF-8, copie et collage contrôlé.
- Navigateur SFTP réel ssh2/libssh2, couplé à la fermeture du terminal : listing,
  création de dossiers, éditeur UTF-8, transferts fichiers/dossiers dans les deux sens.
  Refus d’écrasement, symlinks refusés, sauvegarde explicite avec contrôle de conflit.
- Panneau droit normal relié au navigateur SFTP réel ; listing fictif limité au
  laboratoire/hors ligne. Mode normal au démarrage, accueil pour dossiers et connexions Linux/Windows,
  sortie du laboratoire temporaire vers profils persistants. --offline reste bloquant.
- Paramètres fonctionnels et persistants : police, historique borné, fond noir,
  activation de Ping/Traceroute. Écritures privées et atomiques hors thread GTK.

## Vérifications

Commandes Cargo : `cargo fmt --all -- --check`, `cargo check --workspace`,
`cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace`.
Résultat : réussies, **23 tests**, aucun échec, aucun avertissement Clippy.
Build : `./scripts/build-appimage.sh`, version, SHA256 et version en mode
`APPIMAGE_EXTRACT_AND_RUN=1` vérifiés.

Le parcours `scripts/test-ssh-sftp-local.py --gui IMAGE` vérifie dans l’AppImage
l’authentification clé et agent, commandes UTF-8/ANSI, consentement multiligne,
resize PTY depuis la fenêtre, presse-papiers bidirectionnel, édition/mkdir et copies
récursives bidirectionnelles SFTP, collision et conflit d’édition, symlink refusé,
déconnexion et refus d’hôte modifié (OpenSSH et navigateur SFTP).
Le parcours ciblé SSH/SFTP de la dernière AppImage a terminé avec **code 0** :
tous les contrôles SSH/SFTP ci-dessus réussissent, série explicitement exclue.
Les services du laboratoire ont été arrêtés. Synthèse : summary.json.
Le résultat final, les journaux et les captures sont conservés dans
`dist/validation-0.4.0-38n_04hi/` ; les résultats JSON bruts sont également dans dist.

`smoke-profiles.py IMAGE --settings-demo-only` et sa variante `--forced-offline`
valident sortie/annulation démo, absence de copie des exemples, création de dossiers
et profils Linux/Windows, confirmation de connexion annulée ou interdiction hors ligne,
préférences persistantes 0600 et désactivation des diagnostics.

Laboratoires RDP et VNC/série : réussis. RDP/NLA avec certificat épinglé, mauvais
certificat/mot de passe refusés pour les causes attendues, pixels et application
sur bureau virtuel isolé. VNC authentifié par tunnel SSH, mauvais secret refusé,
pixels vérifiés ; série 115200/8N1 bidirectionnelle sur PTY, sortie propre.
Aucun bureau personnel partagé. Services des laboratoires arrêtés par les scripts.

## Défauts trouvés et corrections

- VTE 0.84 : `set_pty(None)` pendant déconnexion provoquait un SIGSEGV natif
  (backtrace GDB). Appel retiré ; HUP par pidfd et événement child-exited assurent
  la fermeture sans signaler un PID réattribué. Descripteur pidfd libéré immédiatement.
- Copie VTE parfois ancienne dans le test : copie explicite de la sélection bornée.
  PRIMARY/middle-click et Shift+Insert passent par le même contrôle de collage.
- Statut horizontal élargissant excessivement le terminal : ligne distincte.
- Automatisation : bus AT-SPI isolé par runtime privé, sélection du terminal visible,
  démontage du portail FUSE doc propre au test avant nettoyage. Les avertissements
  des portails GNOME/PipeWire/AT-SPI et DRI3 sous Xvfb ne prouvent pas un défaut réseau.
- RDP Shadow journalise des erreurs PEM/AUDIN et les refus de tests négatifs ;
  client authentifié et application : zéro erreur relevée. Voir rdp-local-result.json.

Plusieurs passages multi-onglets ont obtenu un texte vide ou ancien via AT-SPI/VTE.
Une capture d’échec montre pourtant correctement le refus OpenSSH de clé d’hôte.
Après un changement d’onglet, le test lit désormais la sortie via les vrais boutons
Tout sélectionner/Copier et le presse-papiers privé, plutôt que ce buffer d’accessibilité. L’accessibilité VTE lors
des changements d’onglet reste à qualifier ; ne pas prétendre une correction GTK.

Le parcours complet termine encore en échec sur l’échange de commandes avec le switch
série intégré après les onglets SSH, malgré les étapes SSH/SFTP réussies. Cause à
isoler ; le scénario série seul et le client picocom direct ont réussi. Cette limite
n’est pas déclarée corrigée. Le parcours ciblé SSH/SFTP s’exécute avec :
`RUSTREMOTE_SSH_SFTP_ONLY=1 python3 scripts/test-ssh-sftp-local.py --gui IMAGE`.
Le mode normal du script conserve le test série ; il n’est pas silencieusement retiré.

### Fenêtre de connexion RDP adaptable

Le formulaire n’impose plus 480 pixels de hauteur minimale : contenu défilant,
pied de fenêtre séparé avec erreurs et actions, boutons pouvant revenir à la ligne,
libellés longs repliables et onglets RDP défilables. Le terminal prend le focus à
l’ouverture de son onglet.
`smoke-profiles.py IMAGE --responsive-only` : réussi aux dimensions réelles
600×480, 480×520 et 800×680 ; géométrie non nulle des trois boutons vérifiée à
l’intérieur de la fenêtre. Captures inspectées, notamment rdp-responsive-480x520.png.

### Parcours graphiques et mesures courtes

X11 : assertions du parcours complet et création sans chiffrement réussies ; le
nettoyage initial a échoué sur les montages FUSE du portail GNOME. Le script utilise
maintenant GIO_USE_VFS=local, démonte uniquement ses propres doc/gvfs et tolère leur
disparition concurrente. Paramètres/démo et Wayland ont ensuite terminé avec code 0.
Le parcours complet Wayland précède la dernière finition du pied de formulaire,
contrôlée ensuite par le test dimensionnel X11 et la création de profils dans la
dernière AppImage. Wayland : parcours complet réussi (profils/chiffrement/import/export, dossiers,
menus/actions, simulations et diagnostics, création sans chiffrement).
Mesures sur les simulations, pas sur une charge distante prolongée : X11 186,43 Mio
(+6,07), 17 fd (+0), 23 threads (-2), CPU 1 % sur 1 s ; Wayland 170,72 Mio (+5,08),
98 fd (-12), 10 threads (-2), CPU 0 % sur 1 s. Pas une preuve d’absence de fuite.

## Limites et maintenance

SFTP utilise une connexion séparée directe : pas de reprise des alias, ProxyJump,
ProxyCommand ou fichier de configuration OpenSSH. Authentification par clé non
protégée ou agent ; clé protégée explicite via agent et fichier public .pub.
Listing 1024 entrées ; éditeur UTF-8 1 Mio ; copie 16 Mio, 128 entrées, profondeur 8,
30 secondes ; timeout transport 5 s. Une résolution DNS système peut durer davantage.
Pas de progression détaillée, reprise ou file de transfert persistante. Une interruption
peut laisser une cible distante partielle. Les protections ne garantissent pas l’absence
de toute course avec un serveur ou acteur local/distant malveillant.

RDP/VNC restent des clients externes ; pas de moteur IronRDP/VNC intégré. Shadow
ne fournit pas CLIPRDR : presse-papiers RDP texte/fichiers, audio, imprimantes,
multimoniteur Windows, KDE et matériel série physique non validés. Aucune preuve
d’absence absolue de fuite mémoire ou de blocage sur longue durée.

Scan OSV de 184 dépendances Rust Linux (osv.json) : deux avis de maintenance
hérités d’age, basic-toml RUSTSEC-2026-0374 et proc-macro-error2 RUSTSEC-2026-0173.
Ce relevé ne couvre pas les bibliothèques natives ni les clients système et ne prouve
pas l’absence de vulnérabilité. Les notices des dépendances sont incluses au packaging.

AppImage de test, non signée : portabilité hors Ubuntu 26.04, AppStream, CI/SBOM,
SQLite, coffre OS, macros, tunnels généraux et reprise SFTP restent à finaliser.
Avant mise à niveau : sauvegarde privée du magasin recommandée ; après sauvegarde
avec nouveaux champs SSH, la 0.3 ne sait plus relire ces profils.

## Livrable

`dist/RustRemote-0.4.0-preview-x86_64.AppImage` (39 725 560 octets).
SHA256 : `61661b6ca3cdcf66f646db7f9dd746a46cd74801ba7431bb4b81b26f4c25be27`.
Version et empreinte vérifiées, y compris démarrage de --version sans FUSE.
Lancer sans --demo ni --offline pour créer des connexions persistantes ; fermer
l’ancienne instance avant d’ouvrir cette image. Les anciennes AppImage sont conservées.
