# Validation RustRemote 0.3.0 — 2026-11-07

L'objectif reste un client Linux indépendant. Cette version n'est pas équivalente à
l'ensemble des fonctionnalités de MobaXterm. Tests sur Ubuntu 26.04.1, GNOME Shell 50.1,
GTK4 4.22.4, OpenSSH 10.2p1, FreeRDP 3.32.1, traceroute 2.1.6.

## Rejeu complet disponible — 2026-11-07

- Cargo format/check/Clippy/tests : réussis, 20 tests sans échec. Parcours GTK X11
  et Wayland, laboratoires SSH/SFTP, RDP, VNC via SSH, série PTY et outils loopback : réussis.
- Tests renforcés : liste complète des actions déclarées, entrées VNC/série du menu
  Sessions, création sans chiffrement et verrouillage/réouverture sans phrase secrète.
  Le test de démarrage peut être rejoué seul avec `--startup-only` après le chemin AppImage.
- Refus RDP : les journaux confirment explicitement « Host key verification failed »,
  certificat non approuvé et ERRCONNECT_LOGON_FAILURE ; un simple code non nul ne suffit
  plus au test. Les traces négatives sont conservées avec leurs causes, sans secrets.
- Défaut visuel corrigé : un profil RDP sans type de machine utilisait l'icône SSH.
  L'icône RDP est désormais utilisée dans ce cas ; le choix explicite Linux reste respecté.
  AppImage reconstruite ; contrôle final X11 du démarrage, des menus et de cette icône réussi.
  Empreinte, version et exécution sans FUSE vérifiées.
- Mesures courtes : X11 178,09 Mio (+4,43), 17 fd (+0), 21 threads (-4) ; Wayland
  163,98 Mio (+3,18), 98 fd (-12), 10 threads (-2). CPU repos mesuré : 0 % sur 1 s.
  Aucun traceback, panic Rust ou critique GTK dans les journaux finaux de ce rejeu.
  Les erreurs attendues et messages amont Shadow PEM/AUDIN restent documentés.
- Résultats, captures et journaux privés : `dist/revalidation-0.3.0-_rzp_wzw/`,
  synthèse `summary.json`. SHA256 de l'AppImage :
  `c7cb23fff6611b923119603e8a315331e63ccda821345ac539b398a8e39c9514`.

Cette batterie ne représente pas une couverture logicielle de 100 %. Les scripts réseau
testent les clients système indépendamment des lanceurs graphiques en mode hors ligne.
Les connexions réelles depuis l'UI de bout en bout, moteurs intégrés, Windows/KDE,
clipboard RDP réel, périphériques physiques et charge prolongée restent hors couverture.
Les mesures et comptes ci-dessous décrivent le premier passage, conservé pour historique.

## Capacités constatées

| Domaine | État et limites |
|---|---|
| Menus et profils | Formulaires SSH/RDP, favoris, édition et menus contextuels ; déconnexion de simulations. Déconnexion externe non suivie. |
| Dossiers | Dossiers de sessions persistants, dont Linux ; renommage/suppression non livrés. |
| Import / export | JSON RustRemote versionné ou ancien tableau, prévisualisation, refus des doublons. Export consenti en clair sans écrasement ni secrets. Import MobaXterm non livré. |
| Chiffrement | Magasin age facultatif, déverrouillage et verrou manuel ; aucun mot de passe distant stocké. |
| SSH | Lanceur OpenSSH externe ; moteur/PTY/terminal intégré non livré. |
| RDP / Windows | Configuration et lanceur FreeRDP externe, NLA/certificat stricts ; aucun Windows réel testé. |
| Options RDP | Taille automatique/fixe/smart sizing, multi-écrans, audio, microphone, imprimantes, clipboard/fichiers, empreinte SHA256 ; périphériques physiques non testés. |
| VNC | Lanceur TigerVNC ; tunnel SSH optionnel activé par défaut à la création, passerelle/port/utilisateur. Sans tunnel : X509Vnc et CA système. Agent/config OpenSSH pour le tunnel ; pas de clé copiée. |
| Série / switch | Lanceur picocom, périphérique/débit/bits/parité/arrêt/flux ; switch simulé en lecture seule et test de liaison PTY, sans switch physique. |
| SFTP | Éditeur et transferts dans un laboratoire temporaire uniquement ; protocole/transferts réels non livrés. |
| Presse-papiers texte | Simulation locale via GDK ; ne prouve pas un échange distant SSH/RDP. |
| Dossiers/fichiers | Copie récursive dans les deux sens dans le laboratoire ; presse-papiers de fichiers RDP réel non testé. |
| Outils réseau | Ping/Traceroute paramétrés, simulation par défaut ; accès réel explicitement déclenché hors --offline seulement. |
| Tunnels généraux / bastion SSH intégré / X11 / macros / Multi-Exec | Non livrés ; seul le tunnel dédié au lanceur VNC est disponible. |
| Coffre OS, migrations de mode, sauvegardes | Non livrés ; chiffrement des métadonnées disponible. |
| Distribution | AppImage locale x86_64 ; signature, AppStream, .deb, CI, SBOM et portabilité générale non validés. |

## Vérifications

- `cargo fmt --all -- --check`, `cargo check --workspace`, Clippy `-D warnings` : réussis.
- `cargo test --workspace` : 20 tests réussis. Validation/sérialisation, migration,
  import/fusion, simulation sans commande réseau, politiques clipboard RDP, chiffrement,
  altération, permissions, verrou interprocessus, limites, symlinks, traversal,
  édition/copie de dossiers dans les deux sens et collisions sont couverts.
- AppImage 0.3.0 compilée ; résultats graphiques et mesures consignés ci-dessous.
  Checksum, `--version` et `APPIMAGE_EXTRACT_AND_RUN=1 ... --version` réussis.
  Après la correction du libellé du gestionnaire série, contrôle ciblé de l'image finale
  réussi : démo éphémère, menus/onglets, boutons VNC/série, gestionnaire, ouverture puis
  annulation du profil et capture. Journal : `dist/validation/final-image.log`.
- Ping/Traceroute : versions interrogées ; FreeRDP accepte les paramètres de clipboard
  via `/help`. Ping 4/4 et traceroute 1 saut réussissent aussi sur 127.0.0.1.

### Laboratoires réseau autorisés et isolés

- `python3 scripts/test-ssh-sftp-local.py` : OpenSSH UTF-8, refus de mauvaise clé et
  de changement de clé hôte, PTY/resize, OpenSSH SFTP upload/download récursifs,
  édition/relecture ; symlinks/traversal refusés par le serveur de test Paramiko.
  Résultat : `dist/ssh-sftp-local-result.json`.
- `python3 scripts/test-rdp-local.py` : deux affichages privés, NLA authentifiée,
  certificat SHA256 épinglé, refus de mauvaise empreinte et mauvais mot de passe,
  interface RustRemote dans le bureau virtuel et pixels distants vérifiés.
  Résultat/capture : `dist/rdp-local-result.json`, `dist/rdp-local.png`.
  Le backend Shadow ne fournit pas CLIPRDR : clipboard distant réel non validé.
- `python3 scripts/test-vnc-serial-local.py` : TigerVNC limité à loopback, authentifié
  via passerelle SSH éphémère, image distante vérifiée, authentification RFB invalide
  refusée ; picocom 115200 8N1 échange les commandes/réponses du faux switch sur PTY
  et sort proprement. Résultat/capture : `dist/vnc-serial-local-result.json`, `dist/vnc-local.png`.

Ces tests utilisent les clients système, sans compte SSH système, shell personnel,
bureau personnel, port série physique ou serveur IA. Ils ne prouvent pas l'intégration
des futurs moteurs Rust. Les services, clés et fichiers privés de laboratoire sont éphémères.

Les options du tunnel et du certificat suivent la [documentation TigerVNC](https://tigervnc.org/doc/vncviewer.html).

## Sécurité et maintenance

OSV interrogé pour 173 dépendances Rust Linux du lockfile. Deux avis sont présents :

- [RUSTSEC-2026-0374 — basic-toml non maintenu](https://osv.dev/vulnerability/RUSTSEC-2026-0374).
- [RUSTSEC-2026-0173 — proc-macro-error2 non maintenu](https://osv.dev/vulnerability/RUSTSEC-2026-0173).

Ce sont des avis informatifs de maintenance, sans CVSS déclaré, transitifs de la chaîne
i18n utilisée par age. Ils demandent une mise à niveau compatible de cette chaîne ;
il ne s'agit pas de deux failles d'exploitation démontrées. Résultats bruts :
`dist/security-osv.json`. Référence de méthode : [API OSV querybatch](https://google.github.io/osv.dev/post-v1-querybatch/).
Ce scan ne certifie pas l'absence de failles, et ne couvre pas GTK natif, FreeRDP natif
ou l'OS. Aucune évaluation d'un serveur de production n'a été effectuée.

Un essai GUI a révélé une course entre sauvegarde et ouverture de profil, avec lancement
intempestif du client externe sur une adresse de fixture `.invalid`. Il a été corrigé
par l'attente de fin d'écriture, le verrou occupé et un mode hors ligne explicite.
Les tests finaux doivent utiliser `--offline`, même pour les profils fictifs.

Autres corrections : refus d'écrasement, verrou de magasin, lecture du clipboard bornée,
annulation des lectures lentes, nettoyage des widgets de fenêtres fermées et références
faibles dans les rafraîchissements/menus de laboratoire. Pas de preuve d'absence générale
de fuite mémoire ; les échantillons courts ci-dessous ne sont pas un benchmark.

### Lecture des journaux et erreurs constatées

- Erreur GTK `gtk_widget_add_css_class` sur une classe vide : corrigée dans `ui.rs`.
- Course sauvegarde/connexion : profil indisponible pendant l'écriture, formulaire
  désactivé et mode hors ligne bloquant les lancements réels.
- Les erreurs d'authentification des scénarios négatifs SSH/RDP/VNC sont attendues.
- FreeRDP cherchait un KDC pour le domaine fictif du laboratoire. Le test SAM utilise
  désormais explicitement NTLM avec NLA ; le client authentifié ne journalise plus
  d'ERROR. Cela ne modifie pas la négociation Kerberos des profils utilisateurs.
- Shadow 3.32 conserve des erreurs d'extraction de propriétés du PEM pourtant valide,
  et de canal AUDIN sans périphérique. Le rendu et NLA passent ; ces messages amont
  restent consignés dans `log_errors_redacted` du résultat RDP, sans les masquer.
- Les sessions D-Bus privées produisent des avertissements de portails/Secret Service
  et cache AT-SPI. Un essai simultané X11/Wayland a échoué à cause du bus d'accessibilité
  partagé ; les parcours doivent être exécutés séquentiellement.
- TigerVNC/Xvfb peut journaliser la recherche d'un numéro d'affichage libre et
  XOpenIM/Fontconfig ; le serveur SSH Paramiko de test ne négocie pas d'algorithme
  post-quantique. Ce laboratoire n'est pas un modèle de serveur de production.

## GNOME, Wayland, KDE et ressources

Parcours X11 réussi : profils chiffrés, import/export, clés SSH, options RDP,
menus/contextes, fichiers/dossiers et clipboard simulés, outils bloqués hors ligne.
Dernier parcours X11 avec VNC/série/switch : RSS 177,96 Mio (+4,45 Mio après 10 dialogues
et 3 laboratoires), CPU au repos 0 % sur une seconde, 17 descripteurs (+0), 23 threads
(-2). L'allocateur GTK garde
des caches ; cette variation n'établit ni fuite ni absence de fuite sur longue durée.
I/O/chiffrement hors thread GTK, écriture unique, clipboard borné et limité à 2 s,
diagnostics limités à 20 s ; aucune réécriture du moteur de threads n'a été nécessaire.
Le test automatisé surveille également l'accumulation des threads/descripteurs.
Parcours Wayland réussi également, avec menus Fichier/Sessions/Outils/Affichage/Aide,
configuration/réouverture du tunnel VNC et de la série, console switch et refus de
commande non prise en charge. Mesure : 160,89 Mio RSS (+3,18 Mio), CPU repos 0 %/1 s,
98 descripteurs (-12), 10 threads (-2). Les comptes diffèrent selon le backend GTK/GSK ;
ils ne sont pas comparables comme un benchmark de performances.
Résultats machine : `dist/runtime-x11.json` et `dist/runtime-wayland.json`.
Journaux conservés avec permissions privées dans `dist/validation/` : suites GTK,
SSH/SFTP, RDP, VNC/série, Cargo, compilation et outils. `review.json` résume la recherche
de tracebacks, critiques GTK et panics ; les erreurs attendues et amont restent classées
dans le résultat RDP. Une première vérification finale a cherché à tort un bouton
« Fermer » dans le gestionnaire ; le pilote a été corrigé pour utiliser son parcours
réel de sélection de profil puis annulation. Aucun dysfonctionnement applicatif déduit.
KDE/Plasma n'est pas disponible sur cette machine et reste non testé. Les portails
natifs de sélection de fichiers ne sont pas couverts par l'entrée manuelle de chemin.
L'isolation D-Bus peut produire des avertissements de portails GNOME ; elle ne reproduit
pas une session de bureau complète. Les profils utilisés par les tests sont temporaires
et les captures X11 sont filtrées par PID pour préserver les autres fenêtres.

## Priorités restantes

1. SSH/PTY/terminal intégré et cycle de connexion/déconnexion réel, sur labo autorisé.
2. SFTP réel avec progression, annulation, permissions et protections d'écrasement.
3. RDP natif ou intégration contrôlée ; tests Windows de texte/fichiers/dossiers dans
   les deux sens, certificats, NLA et déconnexions.
4. Coffre OS et verrouillage automatique ; sauvegardes et migration de chiffrement.
5. Matrice KDE/GNOME, Wayland/X11, HiDPI, clavier et portails ; audit natif et packaging.
