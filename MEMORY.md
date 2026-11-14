# MEMORY.md — Mémoire de projet partagée Claude Code / Codex

> **Source de vérité commune**, suivie dans Git et mise à jour après chaque tâche ou relais. Ce document est distinct de la mémoire automatique propre à chaque agent.
> **Interdit :** secrets, jetons, clés privées, identifiants d'accès, commandes contenant des mots de passe, IP sensibles, journaux bruts confidentiels.

## 1. État de référence

- **Mise à jour :** 2026-11-14 — 0.8.0 découverte réseau et menus de dossiers en validation ; 0.7.0 publiée.
- **Dernier agent :** Codex ; Apple ARD via TigerVNC DH/SSH, recherche SFTP et tests Apple locaux.
- **Phase :** P1 — terminal/SFTP natifs, MVP encore incomplet.
- **État confirmé :** terminal SSH/série intégré VTE, SFTP natif ssh2/libssh2 ; RDP/VNC externes ; profils/import/export, dossiers et paramètres persistants.
- **Code existant :** package Rust/GTK4, ui/dialogs/profiles/store/simulation/icons ; scripts AppImage, GTK et laboratoires réseau/PTY.
- **Plateforme cible initiale :** Ubuntu Linux / GNOME. Debian ensuite.
- **Dernière vérification réelle :** 45 tests Rust et contrôles Cargo réussis ; AppImage 0.8.0 et parcours complet GUI/discovery/SSH/SFTP/Wayland PASS. Voir `docs/VALIDATION-0.8.0.md` et `docs/validation/0.8.0.json`.

## 2. Objectif immuable

Créer **RustRemote**, application de bureau Linux centrée sur des sessions SSH/terminal/SFTP, puis RDP, VNC, tunnels, coffre OS, macros, SSH bastion, X11 forwarding. Architecture modulaire en Rust, interface moderne anglaise par défaut et française configurable, inspirée de la maquette. Projet original et non fork de MobaXterm.

## 3. Décisions connues

| ID | Décision | Statut | Raison |
|---|---|---|---|
| D-001 | Nouveau projet Rust indépendant, pas un fork MobaXterm. | Validé | Licence propriétaire du programme d'origine. |
| D-002 | Ubuntu/GNOME prioritaire, Debian ensuite. | Choix de cadrage à confirmer | Environnement Linux visé. |
| D-003 | GTK4 direct pour le premier PoC ; Relm4 reste optionnel. | Validé par prototype | gtk4 0.10.3 (MIT), GTK système 4.22.4 ; limiter les dépendances. |
| D-004 | OpenSSH/VTE pour terminal, ssh2/libssh2 pour SFTP séparé. | Implémenté 0.4 | PTY natif et navigateur testés ; Russh reste option future. |
| D-005 | IronRDP pour future intégration RDP. | Candidat | Bibliothèques Rust modulaires RDP. |
| D-006 | Secrets dans trousseau OS ; pas de fichier credentials suivi. | Exigence | Éviter fuites et duplication de secrets. |
| D-007 | VKC Raritan traité comme intégration spécifique, pas assimilé à VNC. | À investiguer | Produits et protocoles variables. |
| D-008 | Un seul agent travaille en écriture à la fois. | Validé | Éviter les conflits et pertes de contexte. |

## 4. Travaux réalisés (faits vérifiés uniquement)

| Date | Agent | Tâches | Fichiers | Vérification |
|---|---|---|---|---|
| 2026-11-07 | Préparation | Création du pack documentaire | README, instructions agents, docs, prompts, assets | Présence des fichiers et archive ZIP validées ; Rust non compilé |

## 5. Tâches à faire (ordre prioritaire)

- [x] **P0-01** Vérifier l'environnement Ubuntu et les prérequis Rust/GTK ; consigner versions/commandes.
- [x] **P0-02** Initialiser Git et le Cargo workspace minimal, avec licence choisie et `README` cohérent.
- [x] **P0-03** Créer une fenêtre GTK4/Relm4 native avec arborescence de sessions, onglets et panneau SFTP fictif ; lancer et vérifier.
- [x] **P0-04** Définir les structs `SessionProfile`, `SessionKind`, `SessionState` et modèles de stockage sans secrets ; tests.
- [x] **P1-01** Authentification SSH agent + contrôle d'empreinte d'hôte + ouverture PTY ; tests.
- [x] **P1-02** Terminal réellement interactif (séquences ANSI, resizing, copier/coller contrôlé) ; tests.
- [ ] **P1-03** SFTP asynchrone, progression/transferts, protections contre l'écrasement et les traversées de chemin.
- [ ] **P1-04** Sessions persistantes + import/export sans secrets + ergonomie et clavier.
- [ ] **P2-01** RDP via PoC IronRDP ; certificats validés.
- [ ] **P2-02** VNC via moteur évalué et documenté.
- [ ] **P2-03** Tunnels, bastion, X11, macros et exécutions multiples avec confirmations appropriées.
- [ ] **P3-01** Intégration VKC/KVM après identification du modèle Raritan et du protocole.
- [ ] **P3-02** Packaging .deb/AppImage, CI, SBOM, audits et tests de publication.

**Détail et critères :** voir `docs/ROADMAP.md` (statut synchronisé à chaque session).

## 6. Dernière session et preuves

### Codex — 2026-11-08, incrément non publié et audit des demandes (état courant)

- Demande finale : vérifier **toutes** les demandes avant nouvelle AppImage. Résultat :
  couverture partielle ; **aucune AppImage nouvelle créée**, version Cargo toujours
  0.4.1. Ne pas reconstruire sur l’ancien nom : incrémenter la version au packaging.
  Rapport : `docs/AUDIT-DEMANDES-2026-11-08.md` ; anciennes images conservées.
- Champs mot de passe dans les profils SSH/RDP, domaine AD/realm commun ; UPN et
  DOMAINE\utilisateur SSH validés. Stockage durable facultatif par `keyring 4.2.0`
  (Secret Service), références UUID seules dans JSON/age ; exports sans références,
  import neutralisé. Cache séparé par cible/utilisateur/domaine, purge au verrouillage.
- Pont `rustremote-askpass` indépendant, socket privé, UID/exécutable/PPid du pair
  vérifiés, secret hors argv/env/logs ; prompts sensibles explicites. SFTP vérifie
  known_hosts avant authentification par mot de passe. Secret de session libéré à la
  déconnexion, mémoire zeroize ; pas de promesse de purge absolue de GTK/OS.
- FreeRDP lancé directement avec mot de passe sur stdin, suivi par pidfd, déconnexion,
  fermeture à la sortie de RustRemote et code d’échec observable. Coffre et paramètres
  sont deux fenêtres différentes. Aucun contournement known_hosts ou TLS ajouté.
- Dossiers : bouton de barre latérale, création depuis l’éditeur, clic droit
  renommage/suppression uniquement à vide. Affichage : panneaux, barres, plein écran,
  zoom/fermeture d’onglet ; limites d’action sur l’onglet courant.
- SFTP : renommage sans écrasement, permissions 000–777, suppression explicite avec
  préanalyse bornée 128 entrées/8 niveaux ; annulation possible, suppression non atomique.
  Lots/multisélection et sélecteurs de fichiers/dossiers locaux ajoutés. Plafonds de
  transfert inchangés : 16 Mio/128 entrées/8 niveaux/30 s, édition UTF-8 1 Mio.
- Fichiers : Cargo.toml/lock ; src/{credentials,askpass_wire,ssh_auth,external}.rs ;
  src/bin/rustremote-askpass.rs ; src/{main,ui,profiles,store,terminal,sftp,sftp_ui,dialogs}.rs ;
  src/dialogs/{credentials,rdp,exchange,diagnostics}.rs ; scripts/build-appimage.sh ;
  scripts/{smoke-profiles,smoke-connected,test-ssh-sftp-local,test-rdp-local}.py ;
  documentation d’audit, architecture, sécurité, mémoire et roadmap.
- Vérifiés : parcours complet profils/chiffrement/import/export/simulations ; sortie
  démo, Linux/Windows/paramètres/outils ; SSH/SFTP par mot de passe temporaire puis
  coffre après redémarrage ; RDP profil+domaine+secret+certificat+pixels, déconnexion
  et arrêt du client à la fermeture de l’application. Les laboratoires détruisent
  leurs seules fixtures temporaires ; aucun trousseau personnel ni serveur réel utilisé.
- Erreurs corrigées : domaine omis de la clé du cache ; Déconnecter RDP inactif ;
  état Affichage calculé avant mappage via visibilité effective (utiliser propriété
  `visible`) ; événements de dossier capturés sur l’Expander. Tests : coordonnées
  AT-SPI SCREEN nulles sous Xvfb, utiliser WINDOW moins origine du frame ; noms de
  labels/contrôles identiques et nœuds obsolètes. Menu VTE capturé sur le parent avant
  les contrôleurs internes ; clic droit réel et téléchargement mixte fichier/dossier
  validés dans le dernier parcours complet.
- Dernier parcours complet : clic droit Copier/Coller, zoom, multi-sélection SFTP,
  clés/agent, sécurité et série PASS ; `docs/validation/2026-11-08.json`. Quatre bascules
  Affichage et gestion des dossiers par souris PASS. Le dernier test Wayland échoue
  au démarrage (« Protection des profils » reste ouvert), plein écran non atteint.
  Ne pas déclarer la prise en charge Wayland complète ni masquer cet échec.
- GitHub : utilisateur demande publication du projet complet dans le dépôt privé
  `aurelson101/rustremote` (initialement vide). Licence explicitement choisie : usage
  professionnel autorisé, revente interdite ; licence personnalisée dans LICENSE,
  tiers séparés dans NOTICE, métadonnées Cargo et packaging ajustés. README réécrit
  avec deux captures réelles de laboratoire dans docs/screenshots. Projet complet publié
  sur `main` : `604a029040ec5fa9d6d6d9aeb0300bd33cd2abe1`, visibilité privée conservée.
- Publication GitHub : 86 fichiers de projet, sans dist/target/.tools ni secret détecté,
  via le connecteur GitHub (CLI non authentifiée). Arbre Git distant identique à
  l’index local ; historique local synchronisé aux objets Git vérifiés par SHA,
  branche main et origin configurés. Aucun force-push ni changement de visibilité.
  Licence personnalisée d’usage professionnel, revente interdite ; README et captures
  réelles publiés. `cargo check --workspace` après métadonnées/licence : PASS.
- Incomplets : CLIPRDR réel texte/fichiers/dossiers RDP (Shadow 3.32 non compatible),
  AD/Windows/périphériques physiques, KDE et tests prolongés, progression/reprise et
  bastions SFTP ; tunnels autonomes/macros/Multi-Exec désactivés. Pas d’équivalence
  MobaXterm/FileZilla revendiquée. Audit des dépendances vulnérabilités non exécuté.

### Codex — 2026-11-08, 0.4.1 (historique)


- Défauts applicatifs : changement de chemin SFTP laissait l’ancienne sélection,
  susceptible d’ouvrir/télécharger sous le mauvais chemin ; lignes/sélection invalidées
  sur changement et rafraîchissement. Coller/Ouvrir SFTP/Déconnecter restaient actifs
  après fin de session : bindings input-enabled/sensitive + vérifications avant/après
  lecture asynchrone. État série n’annonce plus de sécurité SSH, mais une liaison locale
  non chiffrée. Aperçu du texte capturé ajouté à la confirmation de collage multiligne.
- Fichiers : src/sftp_ui.rs, src/terminal.rs, src/ui.rs (version À propos issue de Cargo,
  texte Outils corrigé), Cargo.toml/lock 0.4.1, scripts/build-appimage.sh,
  scripts/smoke-connected.py, README, docs/ROADMAP et VALIDATION-0.4.1.
- Le faux échec série recevait 536 octets : le test recopiait le texte du terminal en
  observant sa réponse et influençait son propre clipboard. La série est maintenant
  validée via termios du PTY privé, commandes reçues/réponses écrites et capture
  inspectée. Résultat : 29 octets, show version/show interfaces, deux réponses affichées.
  AT-SPI VTE reste parfois obsolète après bascule ; recours copie privée dans les seules
  phases SSH sans collage concurrent. Événements GLib traités et aperçu vérifié exactement.
- Construction : HTTP 504 sur runtime GitHub, même après retries. Cache explicite et
  SHA256 officiel 617aa77314d57c8b8e4fae9c3f0b35af7b30d5529cb69e98f64f92510ed98a46
  ajouté. Cache initial récupéré du runtime de l’AppImage existante, digest MD5 retiré
  de sa copie extraite ; résultat identique au hash officiel vérifié. Original préservé.
- Vérifications finales : fmt/check/Clippy -D warnings et 23 tests PASS ; bash -n et
  py_compile PASS ; build et version/checksum/mode sans FUSE PASS. Parcours GUI complet
  de l’AppImage (sans exclusion série) code 0 : protections chemins SFTP, édition/copies,
  clé/agent, hôte modifié refusé, désactivation des contrôles, PTY/clipboard et série.
  Aucune infrastructure réelle, aucun équipement physique ni bureau personnel utilisé.
- Avertissement GTK focus-out durant automatisation des dialogues : non fatal, reste
  à qualifier ; pas de promesse zéro bug/fuite. RDP/Windows/KDE non rejoués à cette étape.
- Preuves : dist/validation-0.4.1-PtVjDW/summary.json et docs/VALIDATION-0.4.1.md.
- Prochain incrément : P1-03 progression/reprise et bastions SFTP ; avertissement avant
  abandon d’éditions, puis P1-04 ergonomie. RDP/VNC intégrés, coffre OS, macros et
  distribution signée restent ouverts. Ne pas refaire l’ancien contournement série
  par copie répétée du terminal. Aucun commit/push effectué.


### Codex — 2026-11-07, 0.4.0 (historique)

- Ajouts applicatifs : terminal.rs, clipboard.rs, sftp.rs, sftp_ui.rs, preferences.rs,
  dialogs/settings.rs ; adaptations ui/main/dialogs/profiles/store/simulation/diagnostics.
  Dépendances VTE 0.9.0, ssh2 0.9.7, base64 0.21.7 ; notices incluses dans AppImage.
- Terminal intégré OpenSSH et picocom, PTY/resize/ANSI/UTF-8, copier/coller borné,
  HUP par pidfd ; connexion SFTP distincte par navigateur, lié au cycle de vie du terminal.
  SFTP strict known_hosts avant auth, clé ou agent, refus d’hôte modifié ; aucun secret
  utilisateur copié. Clés protégées via agent + fichier public .pub uniquement.
- Navigateur : création de dossiers, éditeur UTF-8 1 Mio avec contrôle de conflit,
  transferts récursifs 16 Mio/128 entrées/8 niveaux/30 s sans écrasement implicite,
  annulation et refus des symlinks. Restent reprise/progression détaillée, aliases/
  ProxyJump/config OpenSSH dans SFTP et transport partagé (P1-03 PARTIAL).
- Accueil normal sans faux onglets ; sortie du mode démo vers magasin personnel avec
  avertissement de non-copie des exemples ; --offline reste contraignant. Paramètres
  non secrets atomiques 0600 : police, historique borné, fond et outils Ping/Traceroute.
- Erreurs corrigées : SIGSEGV natif VTE lors de set_pty(None), retiré au profit du
  cycle child-exited/HUP ; presse-papiers VTE parfois ancien, copie de sélection explicite ;
  barre du terminal trop large, statut placé sur sa ligne. Tests : AT-SPI isolé par runtime
  privé, terminal visible sélectionné, montage FUSE doc de test démonté avant nettoyage.
- Dépendances dev VTE/OpenSSL/libssh2 installées via authentification système locale.
  En-têtes de secours .tools/native-dev seulement si pkg-config système indisponible.
- Attention rétrocompatibilité : anciens profils lisibles en 0.4 ; après réenregistrement,
  nouveaux champs SSH non acceptés par 0.3. Sauvegarde privée avant mise à niveau recommandée.
- Vérification : 23 tests Rust, fmt/check/Clippy sans avertissement ; rapport précis et
  résultats GUI/laboratoires dans docs/VALIDATION-0.4.0.md. Aucune infrastructure réelle.
- Dernières finitions : panneau droit normal relié au SFTP réel ; listing fictif limité
  au laboratoire/hors ligne. Fenêtre de profil (dont RDP) : formulaire défilant, actions
  fixes/repliables, libellés longs repliables ; test réel 600×480/480×520/800×680 réussi.
  terminal.grab_focus à l’ouverture. Ajouts ciblés dialogs.rs/dialogs/rdp.rs/terminal.rs
  et smoke-profiles.py --responsive-only ; erreur Rust E0382 du label déplacé corrigée
  par clone de la référence pour le pied de fenêtre.
- Test AT-SPI : plusieurs lectures vides/anciennes du terminal après changement
  d’onglet ; capture d’échec montre le refus de clé SSH correctement affiché. Le script
  smoke-connected lit maintenant via les boutons de copie réels et le clipboard privé ;
  qualification accessibilité VTE encore nécessaire. Nettoyage privé doc/gvfs renforcé
  contre montage disparu entre vérification et démontage ; GIO_USE_VFS=local.
- Parcours Wayland complet et paramètres/démo : code 0 ; X11 fonctionnel complet,
  mais premier nettoyage du portail FUSE échoué (cause et correction dans rapport).
- Série intégrée après plusieurs onglets SSH : échange de commandes encore en échec
  dans le dernier parcours complet ; scénario isolé et client direct réussis. Cause non
  établie. Le script complet conserve ce contrôle ; mode ciblé explicite
  RUSTREMOTE_SSH_SFTP_ONLY=1 pour vérifier seulement SSH/SFTP.
- Résultat final : parcours SSH/SFTP ciblé de la dernière AppImage code 0, tous
  contrôles clé/agent/hôte/PTY/collage/transferts/édition/refus réussis ; services arrêtés.
  AppImage 39 725 560 octets, SHA256
  61661b6ca3cdcf66f646db7f9dd746a46cd74801ba7431bb4b81b26f4c25be27.
  Synthèse vérifiable : dist/validation-0.4.0-38n_04hi/summary.json ; documentation
  docs/VALIDATION-0.4.0.md. Aucun commit/push ni accès réel effectué.
- Prochaine tâche : isoler ce défaut série multi-onglets puis finaliser P1-03 (progression/reprise + bastions) ; moteurs RDP/VNC
  intégrés et trousseau OS restent à faire. Ne pas déclarer une parité complète MobaXterm.


### Codex — 2026-11-07, nouveau rejeu demandé « tout à 100 % »

- Rejeu local uniquement : Cargo fmt/check/Clippy -D warnings/tests réussis (20 tests),
  GTK X11/Wayland, SSH/SFTP, RDP, VNC par SSH, série PTY, ping/traceroute loopback réussis.
- Revue parallèle en lecture seule des tests ; un seul agent a écrit. Deux faiblesses
  renforcées dans scripts/test-rdp-local.py et smoke-profiles.py : cause exacte des refus
  RDP exigée, actions et menus VNC/série couverts, création sans chiffrement + permissions
  et réouverture sans phrase vérifiées. `--startup-only` permet le rejeu ciblé ;
  `--icon-capture` capture le profil RDP de contrôle sur X11.
- Défaut trouvé sur la capture : icône SSH pour profil RDP/plateforme inconnue. Correction
  minimale dans src/dialogs.rs, icône RDP par défaut ; Linux explicitement choisi conservé.
  AppImage reconstruite et test ciblé final X11 réussi, capture inspectée. Pas de nouvelle
  connexion à une infrastructure réelle ni modification de profils utilisateur.
- Nouveaux logs/captures/synthèse : dist/revalidation-0.3.0-_rzp_wzw/summary.json.
  Zéro traceback/panic/critique GTK dans les journaux finaux ; erreurs négatives et
  messages Shadow PEM/AUDIN identifiés, conservés. Le premier contrôle SHA lancé depuis
  la racine cherchait le fichier au mauvais endroit ; contrôle depuis dist ensuite réussi.
- Mesures courtes X11 : 178,09 Mio (+4,43), fd 17 (+0), threads 21 (-4) ; Wayland :
  163,98 Mio (+3,18), fd 98 (-12), threads 10 (-2) ; CPU repos 0 %/1 s. Pas de garantie
  d'absence de fuite sur longue durée. Checksum/version/mode sans FUSE réussis.
- SHA256 finale : c7cb23fff6611b923119603e8a315331e63ccda821345ac539b398a8e39c9514.
  Fichiers documentaires mis à jour : cette mémoire, ROADMAP et VALIDATION-0.3.0.
- Limites confirmées : scripts réseau = clients système, pas connexions UI réelles
  de bout en bout. Moteurs intégrés non livrés ; Windows/KDE, clipboard RDP et matériel
  physique non testés. Prochaine tâche P1-01 inchangée : moteur SSH/PTY intégré et suivi
  de connexion sur le laboratoire autorisé ; ne pas transformer ces tests en statut DONE.

### Codex — 2026-11-07, 0.3.0 : simulations, ressources et connexions VNC/série

- Travail local autorisé, aucun accès à ia.aurelson.com. L'utilisateur a explicitement
  autorisé RDP loopback isolé puis demandé SSH/SFTP, VNC et faux switch série.
- Fichiers applicatifs : profiles/store/dialogs/simulation/ui/style/main, nouveaux
  dialogs/{exchange,diagnostics,rdp,connections}.rs et icons.rs ; build.rs et SVG originaux
  assets/icons, assets/simulations/serveur-ia.json. Scripts smoke-profiles/capture-preview,
  test-{ssh-sftp,rdp,vnc-serial}-local.py, paquetage atomique et notices.
- Dossiers, import JSON versionné/prévisualisé, export consenti sans écrasement, anciennes
  données migrées, verrou interprocessus et une écriture active ; données utilisateur préservées.
- Laboratoire IA : édition, dossiers, copies récursives bidirectionnelles bornées, clipboard
  texte borné/annulable ; aucune commande réseau. `--demo` utilise des profils éphémères,
  `--offline` bloque clients/diagnostics et refuse la réutilisation d'une autre instance.
- Options RDP : tailles fixe/dynamique/smart, multi-écrans, audio/micro/imprimantes,
  clipboard texte/fichiers et pin SHA256. Paramètres matériels non testés sur Windows.
- VNC : TigerVNC externe avec tunnel SSH activé par défaut, passerelle/port/utilisateur
  validés, agent/config OpenSSH, host-key checking ; hors tunnel X509Vnc/CA système.
  Tunnel via mécanisme natif TigerVNC sleep 20, non suivi dans l'UI ; clipboard désactivé.
- Série : picocom configurable (périphérique/débit/bits/parité/arrêt/flux), icône switch,
  simulation Rust en lecture seule (show version/interfaces/running-config/help), pas de sudo.
- I/O/crypto hors GTK, widgets désactivés pendant sauvegarde ; limites 2 Mio import,
  1 Mio édition/copie, 128 entrées/8 niveaux ; clipboard 64 Kio/2 s ; diagnostics 20 s.
  Références faibles pour fenêtres/listes et destruction du contenu à la fermeture.
- Erreurs corrigées : classe CSS vide GTK ; actions GtkCheckButton inaccessibles remplacées
  par ToggleButton ; course save/connect ; déconnexion de fenêtres de simulation ; références
  fortes cycliques. Le test initial a lancé une fixture .invalid avant le blocage offline :
  ce cas est documenté, aucun accès réel IA. Le terminal Ptyxis est désormais standalone.
- Tests locaux réels : OpenSSH clé valide/mauvaise clé/changement d'hôte, UTF-8 et PTY/resize ;
  SFTP fichiers/dossiers bidirectionnels, édition/relecture, symlinks/traversal refusés ;
  RDP NLA/pin valide, mauvais pin/mot de passe refusés et bureau virtuel affiché ;
  VNC à travers tunnel SSH avec rendu, réponse RFB invalide refusée ; série PTY 115200 8N1
  bidirectionnelle et sortie propre. Tous les services sont arrêtés à la fin.
- Outils Ping/Traceroute testés sur 127.0.0.1 : 4/4 réponses et 1 saut ; simulation et
  blocage offline vérifiés via UI. Aucun outil lancé sur le serveur IA.
- Cargo fmt/check/Clippy -D warnings/tests : réussis, 20 tests. AppImage 0.3.0 compilée
  dans dist avec checksum. Capture et résultats JSON de tests dans dist (non suivis).
  Dernière image : checksum, --version et extraction sans FUSE réussis ; après la
  correction du libellé du gestionnaire série, test GUI ciblé de l'image finale réussi
  (démo éphémère, menus/onglets, VNC/série, ouverture/annulation de profil, capture).
  Journaux privés dans dist/validation et revue automatisée dans review.json :
  aucun traceback, panic Rust ou critique GTK dans les essais finaux conservés.
- X11 : parcours chiffré/profils/menus/import-export/lab/clipboard/VNC/série/switch réussi ;
  177,96 Mio RSS, +4,45 Mio après 10 dialogues+3 labos, CPU repos 0 %/1 s,
  17 fd (+0), 23 threads (-2).
  Mesures courtes, pas une garantie d'absence de fuite. Wayland : parcours complet réussi,
  menus et profils VNC/série/switch inclus ; 160,89 Mio RSS (+3,18), CPU repos 0 %/1 s,
  98 fd (-12), 10 threads (-2). Ne pas inventer une validation KDE.
- Logs : erreurs négatives attendues, Shadow PEM/AUDIN amont conservées dans résultat RDP ;
  recherche KDC fictif supprimée du test SAM (NTLM seulement, NLA maintenue), 0 ERROR du
  client RDP authentifié et 0 critique GTK dans le bureau virtuel. Portails/AT-SPI privés
  bavards ; lancer les suites graphiques séquentiellement (conflit a11y sinon).
- OSV 173 crates Linux : deux avis de non-maintenance transitifs d'age (basic-toml,
  proc-macro-error2), pas de preuve de deux failles exploitables ; pas d'audit natif global.
- Suite : P1-01 moteur SSH/PTY intégré et suivi de cycle de vie sur ce labo autorisé,
  puis SFTP réseau intégré. RDP Windows réel, KDE, périphériques audio/impression/multimon,
  port série physique, VNC natif, coffre OS, auto-lock, SQLite et tunnels généraux restent ouverts.

### Codex — 2026-11-07, 0.2.0 : profils, menus et apparence

- Fichiers : Cargo.toml/lock, src/main.rs/ui.rs/style.css, nouveaux profiles.rs/store.rs/dialogs.rs,
  scripts/build-appimage.sh/smoke-ui.py et nouveaux smoke-profiles.py/capture-preview.py,
  notices packaging/third-party, README, docs ARCHITECTURE/SECURITY/ROADMAP et cette mémoire.
- P0-04 livrée : modèles typés sans mots de passe ; validation des hôtes/ports/utilisateurs,
  sérialisation et refus des champs inconnus ; arguments séparés sans shell.
- Menus et barre SSH/RDP/Paramètres branchés ; formulaires création/édition, favoris,
  clé SSH choisie par sélecteur natif, agent ; options RDP domaine/résolution/clipboard/plein écran.
- JSON privé ou chiffré au premier lancement : age 0.11.5 (MIT ou Apache-2.0), SecretString,
  0700/0600, écritures atomiques, refus de symlinks, limite 2 Mio ; crypto/I/O hors thread GTK.
  Choix JSON/age minimal pour cette étape ; SQLite reste à faire. Verrouillage manuel disponible.
- Clients externes OpenSSH/FreeRDP via Ptyxis (xterm en fallback), confirmation de la cible,
  aucune clé/mot de passe en arguments, SSH host-key checking, RDP NLA et cert:deny.
  LD_LIBRARY_PATH AppImage retiré pour lancer les clients système.
- FreeRDP installé avec authentification système (freerdp3-x11 3.32.1) ; ses options ont été
  parsées avec /help sans connexion (code 0). Aucun serveur réel contacté/testé.
- Finition UI arrondie : panneaux/onglets/boutons/champs ; disposition trois panneaux conservée.
- Corrections/optimisations : actions de menu factices supprimées, recherche des profils réels,
  sauvegarde non refermable pendant l'écriture, annulation explicite de connexion, suppression
  d'une copie complète des profils pendant l'I/O, notices limitées aux dépendances Linux.
- cargo fmt --all -- --check, cargo check --workspace, cargo clippy --workspace --all-targets -- -D warnings,
  cargo test --workspace : réussis ; 8 tests (sérialisation/validation/injection/options sûres,
  permissions/no-clobber, chiffrement/altération/mauvais mot de passe, taille/format/symlinks).
- AppImage finale 0.2.0 reconstruite, checksum vérifié et version exécutée, y compris en mode APPIMAGE_EXTRACT_AND_RUN. `python3 scripts/smoke-profiles.py dist/RustRemote-0.2.0-preview-x86_64.AppImage --capture` : code 0.
- Test GUI isolé : validation d'un formulaire vide, sauvegarde SSH/RDP chiffrée, refus d'une mauvaise phrase, verrouillage/réouverture, options RDP conservées, annulation de connexion, gestionnaire/Paramètres/Outils/À propos, actions exportées, chemin Fichier → Nouveau profil et ouverture/fermeture d'onglet. Aucun client distant lancé ; aucun profil utilisateur modifié. Capture finale inspectée dans `dist/RustRemote-preview.png`.
- Défaut d'accessibilité GTK localisé et corrigé : titres de menu sans action et entrées GtkModelButton sans nom exporté. Menus rendus par MenuButton/Popover et boutons GTK nommés, actions existantes conservées. Les tentatives de pilotage clavier/souris ont été retirées du test ; pas de dépendance XTest ajoutée.
- Les délais du pilote AT-SPI sont maintenant bornés en temps réel ; avertissements de cache AT-SPI GNOME observés, sans échec du test final. Statut trompeur « aucune connexion active » retiré puisque les clients externes ne sont pas suivis.
- Vérification locale `ssh -G -F /dev/null ...` : host-key checking ask, agent forwarding no, local command no, clear all forwardings yes. FreeRDP /help accepte les options, sans réseau. Contrôle des connexions réelles toujours non effectué.
- Erreurs corrigées : ambiguïté Read/Write::by_ref ; Clippy if imbriqués ; notices manquantes dans
  certaines crates récupérées à leur commit amont (ou texte standard déclaré et README). Notices et
  provenance conservées dans packaging/third-party ; aucun contournement du contrôle de licences.
- Le test AT-SPI initial confondait labels et champs ; sélecteurs EditableText corrigés.
  gapplication list-actions exige un desktop installé : remplacé par org.gtk.Actions.DescribeAll.
- Limites : aucune équivalence complète MobaXterm ; terminaux intégrés toujours fictifs,
  VNC/SFTP/tunnels/macros/Multi-Exec/coffre de mots de passe non implémentés. Migration de chiffrement,
  auto-lock, sauvegardes et suivi des clients externes à faire. Avertissement AppStream inchangé.

### Codex — 2026-11-07, correction visuelle à la demande de l'utilisateur

- Le premier aperçu avait trop d'espacements et un aspect tableau de bord ; la priorité utilisateur est la disposition compacte de `assets/maquette-rustremote.png`.
- Fichiers modifiés : `src/ui.rs`, `src/style.css`, `scripts/smoke-ui.py`, `MEMORY.md`, `docs/ROADMAP.md`.
- Menus Fichier/Sessions/Outils/Affichage/Aide et barre d'outils à icônes ; dossiers dépliables/recherche à gauche, onglets et aperçu terminal noir au centre, listing SFTP fictif et onglets de transferts à droite. Barre de statut compacte, séparateurs redimensionnables, onglet actif bleu.
- Ouverture d'un aperçu par Nouvelle session, SSH, session d'exemple ou Ctrl+T ; fermeture d'onglet conservée, menus Fichier/Quitter et Ctrl+Q câblés. Boutons des fonctionnalités absentes désactivés. Aucun protocole, secret ou hôte réel ajouté.
- L'affichage terminal est un TextView non éditable avec couleurs et contenu de démonstration original, pas un PTY. Le listing SFTP est fictif, explicitement indiqué dans l'UI.
- `cargo fmt --all -- --check`, `cargo check --workspace`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace` : réussis ; 0 test unitaire.
- `./scripts/build-appimage.sh` : réussi ; AppImage remplacé au même chemin `dist/RustRemote-0.1.0-preview-x86_64.AppImage`. Checksum associé vérifié ; avertissement AppStream connu inchangé.
- Nouvelle AppImage lancée sous X11/Xwayland ; `python3 scripts/smoke-ui.py` réussi. Capture actualisée `dist/RustRemote-preview.png`, rendu inspecté.
- Tests UI adaptés : appel explicite `Atspi.Text.get_text` pour éviter un conflit d'API Python ; comptage relatif des onglets pour fonctionner avec plusieurs aperçus déjà ouverts. Les premiers essais du test ont échoué sur ces sélecteurs, pas sur la compilation ; essai final réussi.
- Prochaine étape fonctionnelle inchangée : P0-04. Ne pas remplacer cette disposition par une interface à cartes sans demande utilisateur.

### Codex — 2026-11-07, AppImage demandé par l'utilisateur

- L'installation via `pkexec /usr/bin/apt-get install -y rustc cargo rustfmt rust-clippy pkg-config libgtk-4-dev` a réussi (code 0), authentification dans la fenêtre système, aucun secret dans les commandes.
- Versions exécutées : rustc 1.93.1, cargo 1.93.1, GTK4 4.22.4.
- Git initialisé localement, aucun commit/push ni dépôt distant.
- UI GTK4 directe (Relm4 non nécessaire pour ce premier prototype), crate gtk4 0.10.3 épinglée, notices MIT des bindings vérifiées par les métadonnées et fichiers téléchargés. GTK système sous LGPL ; notices natives incluses au paquetage.
- Fichiers créés : `.gitignore`, `Cargo.toml`, `Cargo.lock`, `src/{main.rs,ui.rs,style.css}`, `packaging/{rustremote.desktop,rustremote.svg}`, `scripts/{build-appimage.sh,smoke-ui.py}`. README et documents de reprise actualisés.
- Interface française sombre, trois panneaux redimensionnables, recherche dans les exemples et onglets ouvrables/fermables. Tous les protocoles et le coffre restent absents ; aucune connexion réseau de l'application.
- `cargo fmt --all -- --check`, `cargo check --workspace`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace` réussis. Ce dernier exécute 0 test unitaire ; test d'interface séparé prévu.
- Script de paquetage : appimagetool officiel, empreinte SHA256 vérifiée depuis l'API GitHub ; copie des dépendances ELF hors glibc avec notices de licence des paquets et crates. Cible de test : Ubuntu 26.04 x86_64 uniquement ; pas de promesse de portabilité générale, pas de release publique.
- Licence du code applicatif toujours à décider avant publication ; `publish = false`.
- AppImage créé : `dist/RustRemote-0.1.0-preview-x86_64.AppImage` (~23 Mo), exécutable. Empreinte dans le fichier `.sha256` associé, vérifiée avec `sha256sum --check`.
- `./scripts/build-appimage.sh` : code 0 après correction du mode de l'AppDir (mktemp crée 0700, corrigé en 0755) et utilisation d'une icône PNG (SVG source conservé). Premier essai rejeté par appimagetool sur permissions, aucun contournement activé.
- Lancement normal FUSE et `APPIMAGE_EXTRACT_AND_RUN=1 ... --version` : code 0, version 0.1.0.
- Lancement GUI depuis l'AppImage avec `GDK_BACKEND=x11` : fenêtre native affichée, test `python3 scripts/smoke-ui.py` réussi (trois panneaux, création/fermeture d'onglet, accueil). Capture `dist/RustRemote-preview.png` inspectée visuellement.
- Premier test UI : échec du sélecteur AT-SPI (bouton « Previous tab » au lieu de fermeture), corrigé en ciblant le label de l'onglet ; nouvelle exécution réussie. La capture Pillow du bureau Xwayland échouait ; capture ciblée de la fenêtre via XGetImage réussie.
- `bash -n scripts/build-appimage.sh` réussi. Sources applicatives, scripts et Cargo.lock ajoutés à l'index Git ; aucun commit.
- Avertissement restant : métadonnées AppStream absentes du paquet de test. P3-02 reste partielle (pas de .deb, CI, SBOM ou validation de distribution générale).
- Image PNG source ajoutée : `packaging/rustremote.png`. Les livrables et outils téléchargés sont ignorés par Git, le script permet leur reconstruction.
- Erreur de vérification corrigée : AT-SPI utilise `get_child_count()` (pas `child_count`).

### Historique — P0-01 terminée

- Fichiers modifiés : `MEMORY.md`, `docs/ROADMAP.md` uniquement.
- Maquette inspectée ; documentation d'architecture, produit et sécurité lue.
- `git status --short` : échec car aucun dépôt Git ; aucune modification existante écrasée.
- `uname -srmo`, `/etc/os-release` : Ubuntu 26.04.1 LTS, Linux 7.0.0-43-generic x86_64.
- `command -v`, `dpkg-query`, chemins standards : Rust/Cargo, Rustfmt, Clippy, pkg-config et GTK4-dev absents ; Git et compilateur C présents.
- `apt-cache policy` : candidats Rust/Cargo 1.93.1ubuntu1, pkg-config 2.5.1-4, GTK4 4.22.4+ds-0ubuntu0.1 (versions non installées).
- `apt-get --simulate install rustc cargo rustfmt rust-clippy pkg-config libgtk-4-dev` : code 0, résolution réussie ; aucune installation réelle.
- `sudo -n true` : authentification interactive requise. Installation à effectuer dans un terminal utilisateur ; aucun secret enregistré dans les fichiers ni transmis à une commande.
- Cargo fmt/check/clippy/test : non exécutés, outils et workspace absents.
- Décision : respecter le prérequis de la section 7 ; ne pas créer de squelette non testable avant installation. Aucune dépendance applicative ni licence choisie.

### Limites actuelles

Voir `docs/VALIDATION-0.5.0.md` : terminal SSH/série intégré et SFTP natif réels ;
RDP/VNC externes. SFTP borné (16 Mio, 128 entrées, profondeur 8, 30 s), reprise de
fichiers seulement, pas de file persistante. ProxyCommand personnalisé refusé.
Windows/AD, matériel physique, KDE et endurance non qualifiés. SQLite, IronRDP,
X11 forwarding et verrouillage automatique du coffre restent ouverts.

## 7. PROCHAINE ACTION EXACTE POUR LE NOUVEL AGENT

Partir de la 0.5.0 et de `docs/VALIDATION-0.5.0.md`, ne pas réécrire les moteurs
fonctionnels. Priorité SFTP suivante : file de transferts et limites configurables
avec budgets sûrs ; ensuite stockage SQLite/ergonomie P1-04. Réseau autorisé :
laboratoires locaux seulement ; aucun accès à ia.aurelson.com. La publication sur
GitHub et le remplacement des anciennes AppImages sont autorisés par l’utilisateur ;
conserver le dépôt privé et la licence autorisant l’usage professionnel sans revente.

## 8. Protocole de mise à jour (obligatoire)

À la fin **de chaque tranche** :
1. Remplacer « Dernier agent », date, phase et prochaine action exacte.
2. Mettre à jour tâches `[x]` **uniquement après vérification des critères** de `docs/ROADMAP.md`.
3. Décrire les fichiers touchés, les commandes de test réellement lancées et les résultats (réussite/échec/non lancé).
4. Enregistrer les décisions durables dans la table, en indiquant toute dépendance/licence retenue et sa justification.
5. En cas d'échec, écrire la cause et la commande de reproduction **sans données sensibles**.
6. Réduire les anciens détails à une synthèse utile ; conserver les décisions et blocages, pas les logs volumineux.
7. Ne jamais effacer le travail d'un autre agent ni inventer son historique. Faire un relais uniquement après sauvegarde cohérente des fichiers.

## Relais 0.5.0 — 2026-11-08

Codex : `sftp.rs`/`sftp_ui.rs` (progression, annulation, reprise vérifiée, sélecteurs
GTK), `ssh_route.rs` (HostName/ProxyJump strict), `tools.rs`, `dialogs/automation.rs`,
`terminal.rs`/`ui.rs` (tunnels, macros/Multi-Exec, contrôle privé après authentification),
version Cargo et harnais SSH/Wayland/CLIPRDR. Aucune dépendance applicative ajoutée.

Cargo fmt/check/Clippy -D warnings et tests réussis : 33 + 1 askpass. AppImage
0.5.0 construite et contrôlée par version/FUSE et extraction sans FUSE/SHA256.
SSH/SFTP de l’AppImage : vrais fichiers locaux/distants vérifiés, édition, conflit,
permissions, dossiers, lots, reprise/annulation, clés/agent, bastion/alias et refus
bastion inconnu réussis. Mot de passe/coffre après redémarrage et série PTY native
réussis sur l’image ; RDP profil/domaine/certificat/pixels/arrêt et Wayland natif
avec plein écran réussis. CLIPRDR GNOME/FreeRDP : texte et fichiers/dossiers dans
les deux sens, 10 cas réels, chemins FUSE et octets comparés.

Erreurs rencontrées : portail GTK FileChooser absent (dialogue GTK substitué),
cache AT-SPI Wayland obsolète (pompage événements/actions visibles), relais bastion
du laboratoire coupé après 5 s/8 Kio (budget adapté). Parcours combiné terminé en
échec de harnais après les étapes SFTP réussies : mauvais repérage/choix GTK
DropDown de tunnel ; conservé comme échec, rejeux ciblés séparés. Ne pas présenter
le premier parcours combiné comme intégralement réussi. Voir validation 0.5.0.

Rejeu ciblé des tunnels sur AppImage réussi après ouverture accessible du menu puis
sélection clavier vérifiée : trois modes avec écho bidirectionnel et ports fermés
à la déconnexion. Résultats dans `docs/validation/0.5.0.json`. Les tests du harnais
peuvent être limités par RUSTREMOTE_TUNNELS_ONLY / RUSTREMOTE_SERIAL_ONLY /
RUSTREMOTE_AUTOMATION_ONLY ; timeout global porté à 600 s pour le parcours enrichi.

Publication finale : sources poussées, commit applicatif
`4e64054bcd2119ddea5be7a458c5be50939987b7`, préversion GitHub `v0.5.0` publiée avec
AppImage + SHA256 : https://github.com/aurelson101/rustremote/releases/tag/v0.5.0 .
Le fichier retéléchargé de GitHub est identique (SHA256 `03151f5520f30a29f9d52b8d120ebe291ddbee0d69337108feb9609acef80570`).
Anciennes images locales 0.1.0 à 0.4.1 et leurs checksums supprimés après vérification
de la nouvelle ; aucune ancienne release/AppImage GitHub n’existait. Outil de build
`.tools/appimagetool` conservé. Dépôt toujours privé. Création de release initialement
refusée avec SHA court, corrigée en utilisant le SHA complet ; aucun token exposé.
Le dernier harnais avec focus explicite du dialogue a également passé les trois
modes de tunnel sur l’AppImage. Les changements après le commit applicatif sont
uniquement les traces de publication/validation dans la documentation.

## Incrément 0.6.0 — développé le 2026-11-09, publié le 2026-11-10

Anglais par défaut, français dans Paramètres après redémarrage (`i18n.rs`, catalogue
`assets/locales/en.tsv`, préférences migrées par défaut en anglais). Traduction des
contrôles et messages, jamais du contenu terminal/éditeur ni des noms utilisateur.
Les panneaux créés après démarrage et les expandeurs sont traités explicitement.

SFTP : vue locale/distante, multisélection, boutons toujours accessibles, double-clic,
DND local vers distant, menus contextuels, éditeur avec bouton Enregistrer visible,
rafraîchissement après mutation. Listing local borné 2048 entrées, un seul worker
avec coalescence des rafraîchissements. Budget protocole 16 Mio/128/8/30 s inchangé.
Progression préservée pendant le rafraîchissement automatique après un transfert.

Dossiers logiques hiérarchiques : `session_tree.rs`, `dialogs/tree.rs`, `exchange.rs`.
Chemins de 8 niveaux, déplacement atomique du sous-arbre et des connexions, refus
cycles/collisions. DND GTK et clic droit Déplacer ; zone racine distincte afin que
le conteneur parent ne vole pas les dépôts. Aucun déplacement de fichiers système.
Affichage : déplier/replier, rechercher, concentration/restauration. Outils : infos
système, empreinte de clé publique et ouverture du dossier des profils.

Distribution : build dans Debian 12 amd64 épinglé + Rust 1.93.0 vérifié SHA256,
`packaging/portable-build.lock.json` et `scripts/build-portable-appimage.py`.
Rootless unshare/fakeroot, APT uniquement dans `.tools/portable`, aucun sudo ni
service Docker. Prérequis hôte : Cargo, Python 3.11+, user namespaces. GTK ≥4.8,
VTE ≥0.70 ; menu contextuel GTK existant conservé, appel VTE ≥0.78 superflu retiré.
Bibliothèques graphiques/C++/Wayland de l’hôte conservées pour éviter les conflits
avec Mesa récent. Chargeur SVG et modules IBus/Fcitx compatibles embarqués ; chemins
GTK/GIO retirés lors du lancement des clients système. FreeRDP `xfreerdp` accepté
si une sonde bornée confirme la version 3, sinon refus avant connexion.

39 tests Rust et check/Clippy réussis. AppImage construite et démarrage X11 dans
Debian 12 réussi, ABI glibc minimale 2.36 auditée. Parcours anglais sur l’AppImage :
dossiers/DND/clic droit, outils, transferts SFTP fichiers+dossiers dans les deux
sens et édition réelle, préférence française après redémarrage PASS. Captures
anglaises inspectées et copiées dans docs/screenshots (cinq vues, laboratoire uniquement).
Les modules IBus/Fcitx compatibles éliminent les symboles indéfinis du parcours.
Wayland/plein écran/restauration et RDP local NLA depuis le profil (domaine,
mot de passe, certificat, pixels, déconnexion/reconnexion/arrêt) réussis sur l’image.
Le harnais compte explicitement le critical GTK connu et refuse les autres ; il
ne prétend pas avoir des logs sans erreurs. Focus GTK 4.8 toujours à corriger.

Erreurs de harnais résolues : GTK4.8 sans SHOWING (VISIBLE + géométrie), actions
sur parent du label, lecture Text avec longueur explicite, focus FreeRDP masquant
le popover (réactiver la fenêtre privée). clear_cache sur chaque nœud AT-SPI causait
un crash du harnais dans Debian ; retiré puis démarrage rejoué. Pour la capture
SFTP attendre les lignes distantes après refresh, car le statut conserve le résultat
du lot (il ne revient pas systématiquement à « Listing authentifié »).

Rejeu final après complément du catalogue : Debian 12 startup, Ubuntu X11 anglais/FR
avec SFTP/DND, RDP NLA et Wayland/plein écran tous PASS sur le même artefact.
SHA256 et limites conservés dans docs/validation/0.6.0.json.

Publication du 2026-11-10 : sources et cinq captures anglaises sur main, tag v0.6.0
sur 86615985090071be9a0d44227d5cb6f342d361b2, préversion publiée dans le dépôt privé
https://github.com/aurelson101/rustremote/releases/tag/v0.6.0 . AppImage, SHA256,
ABI.json et build-packages.txt retéléchargés et comparés aux quatre fichiers locaux.
SHA256 AppImage : 3009d663af6bc29e93fb6009adf485fa43804f35ff2908ca4466d5cc8cb8a428.
v0.5.0 reste le retour arrière ; aucun tag historique effacé. Le commit de relais
suivant ne change que les statuts de publication dans la documentation.

Prochaine tâche prioritaire : P1-04 reste PARTIAL (SQLite), mais corriger d’abord les
diagnostics de focus GTK 4.8 avec un test ciblé de fermeture/réouverture avant tout
nouveau changement fonctionnel. Qualifier ensuite KDE/Xfce/Cinnamon dans des bureaux
complets et étudier des builds distincts pour ARM/musl/glibc<2.36 si demandés.
Aucune qualification universelle Linux, Windows/AD ou d’endurance n’est acquise.

## Incrément 0.7.0 — 2026-11-14

Reprise propre sur main 34ba1f1, aucune modification préexistante. Inspection du
comportement Apple actuel de MGRemote (README/code), aucun code MGRemote importé.
TigerVNC installé 1.15 propose DH/type30. Plateforme MacOs, preset Apple, SVG original,
validation du compte <64 octets et tunnel SSH obligatoire vers le même Mac. Destination
VNC loopback vue du Mac ; StrictHostKeyChecking=ask, pas de repli VncAuth/None.
TigerVNC demande lui-même compte/mot de passe ; pas de stockage VNC ni d’injection
sur la ligne de commande. Authentification SRP/haute performance non intégrée.

Fichiers : profiles.rs, dialogs.rs/connections.rs, ui.rs, icône/manifest/catalogue ;
sftp_ui.rs/local.rs pour recherche, affichage cachés et tri. Changer le filtre vide
la sélection des deux panneaux ; appliquer un filtre lors d’un rafraîchissement ne
doit pas vider la sélection de l’autre panneau (corrigé avant le build final).

Test indépendant scripts/test-apple-local.py : client TigerVNC réel, simulateur RFB
Apple003.889/type30, identifiants éphémères, bureau X11 privé et framebuffer. Mauvais
mot de passe, downgrade sansARD et clé trop longue refusés. Le transport de ce seul
simulateur est loopback ; le tunnel SSH est rejoué séparément via le labo VNC existant.
Mac physique/VM macOS non testé ; ne pas présenter ces résultats comme un Mac certifié.
TigerVNC peut quitter avec code0 après refus : test fondé sur refus protocole et non
sur code de sortie. XAUTHORITY privé requis également pour les appels Xlib/PIL.

42 tests Rust et fmt/check/Clippy réussis. Premier harnais GUI sur GTK hôte arrêté
au bouton Move (sélecteur/focus) ; le parcours de l’AppImage GTK4.8 réussit, y compris
Apple, SFTP et français après redémarrage. Warnings/criticals de focus GTK4.8 toujours
présents et documentés, pas de promesse zéro bug. Build portable Debian12/ABI2.36
conservé. Dernier rejeu final AppImage SSH/SFTP/Apple et Wayland PASS ; captures Apple/SFTP inspectées et versionnées, résultats dans docs/validation/0.7.0.json. Publication effectuée et vérifiée ci-dessous.

GitHub constaté PUBLIC le 2026-11-14 via gh repo view (contrairement à la visibilité
privée historique) ; aucun changement de visibilité effectué. Préserver ce réglage.

Publication 0.7.0 effectuée le 2026-11-14 : tag sur
`a132fbb72e2f8df8fe4a42b30815cef1a1e3f7f5`, préversion disponible :
https://github.com/aurelson101/rustremote/releases/tag/v0.7.0 . Quatre assets
AppImage/SHA256/ABI/versions de paquets retéléchargés puis comparés aux locaux.
SHA256 : 683bc554341cbd1ead6b2989a6752123f000441546710895e8b907a3f5f3c6a5.
Dépôt public conservé, anciennes releases préservées. Le commit suivant n’actualise
que les documents de publication et le lien officiel Apple du README.

Prochaine tâche : qualifier un Mac réel de bout en bout (tunnel/compte/écran et SFTP
APFS) uniquement avec autorisation de sa cible ; sinon poursuivre les laboratoires.
Corriger ensuite les diagnostics de focus GTK et examiner le sélecteur Move sous
GTK natif de l’hôte. P1-04/SQLite et P2-02 moteur intégré restent ouverts. Ne pas
annoncer de parité ARD haute performance ni de stockage du mot de passe VNC.

## Incrément 0.8.0 — 2026-11-14

Reprise propre sur e43af17. Demande : Apple dans le clic droit des dossiers et
outil de découverte Windows/Apple/Linux avec ajout de connexions/dossiers/comptes.
`exchange.rs` ajoute Apple (preset4) et VNC au menu existant. L’éditeur conserve
le dossier demandé ; passerelle Apple laissée vide reprend host/user du formulaire.
`editor_prefilled` réutilise le même chemin de validation/coffre/persistance pour les
résultats réseau, sans connexion automatique.

Nouveaux `discovery.rs` et `dialogs/discovery.rs`, action/menu Outils. Simulation
par défaut ; scan réel explicitement autorisé, RFC1918/loopback IPv4 /24 max,
ports22/3389/5900/445, huit workers, 30s + fin I/O courte, annulable. Lecture RFB
limitée pour type30, sans choisir d’authentification. OS indicatif : ne pas assimiler
SSH à Linux ou SMB/RDP à une preuve Windows. IPv6/public/link-local refusés. Aucun
réseau réel scanné pendant le développement : sockets factices loopback en tests.

45 tests Rust et fmt/check/Clippy réussis ; AppImage 0.8 construite, ABI glibc 2.36 et
startup Debian 12 passés. Harnais GUI étendu pour menu Apple de dossier, contrôles
scope/consentement et création de trois profils depuis les résultats simulés avec
comptes et dossiers existants/nouveaux. Les CheckButton GTK4.8 n’offrent pas toujours
une action AT-SPI cliquable ; test corrigé par clic physique dans le Xvfb privé.
Mode RUSTREMOTE_DISCOVERY_ONLY=1 pour rejeu ciblé, le parent rapporte discovery_gui
au lieu d’affirmer que le terminal GUI a été testé dans ce mode.

Parcours ciblé et complet de l’artefact final PASS ; bouton dynamique traduit après
revue de capture, simulation reste hors ligne après bascule du contrôle. Capture
inspectée, README/rapport prêts. Prochaine action : publier et vérifier les assets.
Ne pas scanner un réseau réel pour qualifier le produit sans cible autorisée.
