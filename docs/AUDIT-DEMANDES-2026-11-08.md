# Contrôle des demandes avant nouvelle AppImage — 2026-11-08

**Décision : publication différée. Toutes les demandes ne sont pas terminées.**
L’AppImage 0.4.1 existante est conservée ; elle ne contient pas les changements
non publiés décrits ici. Aucun essai ne contacte ia.aurelson.com ou un serveur réel.

## Couverture des demandes

| Demande | État actuel et limite |
|---|---|
| Interface sombre française, icônes Linux/Windows/dossiers | Présente ; appréciation esthétique à confirmer, aucune équivalence visuelle complète revendiquée. |
| Quitter la démonstration, créer ses connexions et dossiers | Parcours GTK réel réussi ; profils Linux/Windows persistants, chiffrement initial facultatif, paramètres persistants. |
| Modifier une connexion et menus contextuels | Éditeur complet présent ; domaine, mot de passe, clés, protocoles et dossier. Clics droits de dossiers et de terminal vérifiés sur le bureau privé. |
| SSH interactif multi-onglets, clé et agent | Laboratoire loopback réussi : PTY, ANSI/UTF-8, resize, clé/agent, refus d’une clé d’hôte modifiée. |
| Mot de passe SSH dans le profil | Réussi en terminal et SFTP ; mémoire temporaire ou sauvegarde facultative dans le trousseau OS, référence seule dans le profil. |
| Domaine AD/realm SSH et RDP | Champs et validation présents ; UPN ou DOMAINE\\utilisateur SSH, domaine RDP séparé. Tests de construction ; authentification RDP locale avec domaine réussie. AD/LDAP réel non testé. |
| Mot de passe RDP dans le profil | Connexion depuis l’interface au bureau RDP isolé réussie, secret transmis par pipe, déconnexion suivie. |
| Copier/coller texte SSH bidirectionnel | Parcours réel réussi avec consentement multiligne ; clic droit souris Copier/Coller vérifié avec échanges SSH réels. |
| Texte/fichiers/dossiers bidirectionnels RDP, sélection multiple | Options distinctes opt-in présentes et testées dans les arguments ; **transferts réels non validés** : FreeRDP Shadow 3.32 ne fournit pas CLIPRDR. Une simulation de copie ne prouve pas le canal RDP. |
| RDP autosize, smart sizing, son, imprimantes, double écran | Options présentes et relues ; son/impression/multimoniteur physiques non validés. RDP reste une fenêtre FreeRDP externe. |
| SFTP éditable et transférable | Réussi : listing, texte UTF-8, enregistrement avec contrôle de conflit, création de dossiers, envoi/téléchargement récursif, annulation, refus d’écrasement et de liens. |
| Commandes SFTP inspirées des usages FileZilla | Renommage sans écrasement, permissions Unix, suppression confirmée bornée testés ; téléchargement mixte fichier+dossier à sélection multiple testé ; sélecteurs locaux ajoutés mais pas encore rejoués. Pas de file de transferts persistante, progression détaillée ou reprise. |
| SFTP par bastion/configuration OpenSSH | **Non disponible** : connexion libssh2 directe distincte du terminal. La fenêtre le signale. |
| Coffre distinct des paramètres | Corrigé ; fenêtre propre aux identifiants, référence par profil, lecture depuis un coffre de laboratoire après redémarrage réussie. Aucun secret dans le JSON/export. |
| Menu Affichage | Panneaux Sessions/SFTP, barres d’outils/état, plein écran, zoom et fermeture de l’onglet ajoutés ; quatre bascules et zoom testés ; plein écran non validé par le dernier essai Wayland. |
| Import/export de profils et exemple Serveur IA | Parcours GTK réussi avec simulation IA sans réseau, refus de doublon ; tests Rust : références de coffre exclues des exports et neutralisées à l’import. |
| Ping et traceroute | Outils configurables présents ; simulations et désactivation par paramètres testées. Aucun diagnostic vers infrastructure réelle. |
| VNC avec tunnel SSH | Configuration et lanceur existants ; laboratoire local de la 0.4.1 documenté. Moteur VNC intégré absent. |
| Série/switch | Terminal picocom et laboratoire PTY réussis ; adaptateur et switch physiques non testés. |
| Tunnels autonomes, port forwarding, macros, Multi-Exec | **Non implémentés** ; boutons désactivés avec indication « À venir ». |
| GNOME/Wayland/KDE, RAM/CPU, absence de fuite/blocage | Mesures ciblées seulement : parcours X11, cycles de fenêtres et compteurs ; dernier démarrage Wayland automatisé en échec, KDE non testé ; aucune garantie d’absence absolue de fuite ni certification KDE/Wayland complète. |
| AppImage nouvelle version | **Non créée à cette étape**, car couverture encore partielle. Préparer une version de test en conservant explicitement ces limites après les validations manquantes. |

## Vérifications de cet incrément

- `cargo fmt --all -- --check`, `cargo check --workspace`,
  `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace` :
  réussis, **28 tests applicatifs + 1 test du helper askpass**.
- `scripts/smoke-profiles.py target/debug/rustremote` : réussi ; profils chiffrés,
  clés, domaine RDP, dossiers/import/export, simulations et menus existants.
- `scripts/smoke-profiles.py target/debug/rustremote --settings-demo-only` : réussi ;
  sortie de démonstration, Linux/Windows, paramètres, outils désactivables.
- `scripts/test-ssh-sftp-local.py --gui target/debug/rustremote` : dernier parcours
  complet réussi : clic droit Copier/Coller, zoom, clé/agent, SFTP avec renommage,
  permissions, suppression annulée/confirmée, téléchargement mixte fichier+dossier,
  conflits, refus des liens et de l’écrasement ; série PTY : 29 octets et deux
  commandes/réponses attendues.
- `RUSTREMOTE_TEST_PASSWORDS=1 RUSTREMOTE_PASSWORD_ONLY=1 RUSTREMOTE_SSH_SFTP_ONLY=1
  python3 scripts/test-ssh-sftp-local.py --gui target/debug/rustremote` : réussi ;
  SSH/SFTP par mot de passe, coffre privé, redémarrage et absence de secret JSON.
- `python3 scripts/test-rdp-local.py --app target/debug/rustremote` : réussi ;
  NLA, mauvais certificat/mot de passe refusés, pixels distants, lancement et arrêt
  depuis le profil. Les erreurs des cas négatifs et de l’arrêt forcé restent
  distinguées des erreurs applicatives ; zéro erreur critique dans l’application
  affichée dans le bureau virtuel.
- Syntaxe Python des scripts modifiés et Bash du packaging vérifiée.
- Mesure ciblée du parcours X11 : RSS 211,8 Mio, variation +6,03 Mio après
  10 dialogues/3 laboratoires ; CPU au repos 0 % sur 1 s ; delta FD 0,
  delta threads −2. Ce test court ne prouve pas l’absence de fuite à long terme.

## Conditions restantes avant de déclarer les demandes terminées

1. Diagnostiquer le dernier parcours Wayland : échec au démarrage dans
   `plaintext_startup`, dialogue « Protection des profils » resté ouvert. Ce rejeu
   n’a pas atteint le plein écran ; ne pas réutiliser un ancien PASS Wayland comme
   validation de cet incrément. Rejouer aussi les sélecteurs locaux SFTP.
2. Tester CLIPRDR texte et fichiers/dossiers, simples et multiples dans chaque sens,
   avec un serveur local qui implémente ces canaux ; conserver la limite Windows réelle.
3. Livrer séparément progression/reprise/bastions SFTP et outils actuellement désactivés.
4. Pour une nouvelle AppImage de test, incrémenter la version, embarquer
   `rustremote-askpass`, conserver les anciens artefacts et rejouer les tests sur
   l’artefact construit. Aucun test du binaire de développement ne vaut test AppImage.


## Préparation du dépôt GitHub

Dépôt choisi par l’utilisateur : `aurelson101/rustremote`, privé et initialement vide.
Publication du projet complet explicitement demandée. README avec deux captures
réelles de laboratoire, licence personnalisée `LicenseRef-RustRemote-No-Resale-1.0` :
usage professionnel et prestations autorisés, revente de copies interdite.
Ce choix n’est pas une licence open source OSI. Les composants tiers gardent leurs
licences ; LICENSE/NOTICE sont prévus dans le prochain packaging.

Autres rejeux réussis : actions de dossiers par souris, quatre bascules Affichage,
Coffre distinct des Paramètres ; fermeture FreeRDP à la sortie de RustRemote.
Résultats structurés : `validation/2026-11-08.json`.
