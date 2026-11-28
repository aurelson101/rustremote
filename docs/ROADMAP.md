# Roadmap pilotée par critères — RustRemote

**Règle :** le statut `DONE` exige résultat observable + commande de vérification enregistrée dans `MEMORY.md`. Si l'outil est indisponible, marquer `BLOCKED`, jamais `DONE`. Une seule tâche principale ouverte par agent.

**Déploiement local macOS Apple Silicon — DONE (2026-11-28).** Aperçu SSH/SFTP/RDP
0.9.2 installé dans `~/Applications/RustRemote.app` et fenêtre vérifiée.
Parcours graphique SSH en boucle locale validé après correction du lancement et de
la fermeture VTE. Rejeu GTK local automatisé et trousseau validé sur entrée
temporaire. RDP réel validé en boucle locale via FreeRDP SDL de Homebrew, avec
NLA, contrôle du certificat et déconnexion. Windows réel et redirections RDP
restent à qualifier. Verrou corrigé pour RUSTSEC-2026-0544.
Les fichiers SSH locaux des profils importés doivent être choisis dans l’éditeur.
VNC et série disponibles en simulation. Depuis le 2026-11-28 (non publié),
le paquet construit embarque GTK/VTE ; démarrage vérifié sans `/opt/homebrew`.
Il exige macOS ≥ 27.2. Seul FreeRDP vient encore de Homebrew. Apple/VNC ouvre
Partage d’écran via tunnel SSH strict (labo local validé, vrai Mac non testé).
Le ZIP 0.9.2 publié dépend encore de Homebrew.
Logo intégré à la barre de titre et au paquet macOS. Reconstruction locale avec
`python3 scripts/build-macos-app.py`.
Distribution GitHub via le tag [macos-v0.9.2-preview](https://github.com/aurelson101/rustremote/releases/tag/macos-v0.9.2-preview).

## Campagne 15 contrôles / 10 améliorations — 2026-11-15

Dix améliorations livrées dans le code (cinq Linux, cinq prototype Windows), avec
[15 points de contrôle et limites détaillés](VALIDATION-DEBUG15-2026-11-15.md).
Éditeur Rust SFTP testé réellement en loopback : permissions0755 conservées,
échec d’écriture injecté nettoyé et original protégé. Mutations de file admises
une à la fois ; compteur accessible du listing corrigé. Sélecteurs du pilote GTK
limités à leur dialogue et actionnés nativement.

Windows Server2022 : run43700777708 PASS, huit tests et parcours CLI/GUI incluant
une fenêtre540px, exclusion diagnostic/live et déconnexion. Modes clavier lus
sans clonage de l’écran, resize refusé après arrêt, erreurs clipboard affichées.

Rejeu GUI Linux SSH/SFTP PASS avec calage Xvfb explicite40px (pilote uniquement),
y compris navigation physique répétée, transferts/édition/file/échec injecté et
préférences/français après redémarrage. Aucun changement spéculatif de geste produit
conservé. Windows11 reste non qualifié ; WIN-02 à WIN-08 non achevés.
Pas de nouvelle AppImage/release dans cette campagne.

## Revue ciblée Linux / Windows — 2026-11-15

- Linux : édition SFTP corrigée pour conserver les permissions ordinaires (000–777) avant remplacement atomique ; refus si permissions indisponibles. Les bits privilégiés ne sont pas restaurés. Propriétaire, ACL et attributs étendus ne sont pas conservés par ce remplacement : qualification et stratégie dédiée restent TODO.
- Windows : validation ConPTY renforcée pour refuser un flux UTF-8 invalide, même contenant le marqueur attendu ; test unitaire local PASS. Revalidation native Server2022 PASS (run43700777708).
- Validation Linux DONE pour le parcours SSH/SFTP de cette campagne : sélecteur natif corrigé dans le pilote, calage Xvfb explicite, assertion0755 et échec de sauvegarde atteints. Voir le rapport debug15.
- P0 Windows : terminer WIN-01 (rendu ANSI interactif, clavier, resize et arrêt), puis frontière OS et stockage sécurisé WIN-02/03 ; aucun MSI fonctionnel disponible.
- P1 Linux : admission des mutations de file bornée DONE par navigateur. Migration des files après changement de profil/chiffrement, endurance CPU/RAM et qualification KDE/Xfce/Cinnamon restent ouvertes.
- P1 distribution Windows : tester installation/mise à jour/désinstallation MSI et runtime avant WinGet ; MSIX/signature ensuite.

## Incrément Windows ANSI — 2026-11-15

WIN-01 reste PARTIAL : écran ANSI de preuve ajouté au prototype indépendant
`experiments/windows-probe` (vt100 0.18.1 + GTK/Pango), avec couleurs ANSI/256/RGB,
styles, curseur, UTF-8 et cellules larges. Aperçu déterministe au démarrage et
rendu de la sortie du test ConPTY après son arrêt ; pas encore de flux interactif.
Entrée bornée à64Kio, grille maximale60×160, scrollback désactivé, texte échappé,
aucune action OSC/presse-papiers/lien. Tests du modèle et smoke GTK local PASS.
Le workflow manuel Windows exécute désormais ces tests ; ce nouvel incrément
n’a pas encore été rejoué sur runner Windows.

Prochaine tâche WIN-01 : connexion ConPTY persistante avec commandes bornées,
clavier/IME et redimensionnement GUI synchronisé, rendu incrémental et annulation
à la fermeture (arrêt des seuls processus appartenant à la session). Qualifier
copie/sélection, polices/DPI et consommation avant migration de l’application.
Pas de MSI/WinGet avant cette preuve et les adaptations WIN-02 à WIN-07.

## WIN-01 interactif — preuve Server2022 validée, qualification restante

Transport ConPTY persistant ajouté au prototype : saisie UTF-8/IME, touches ANSI,
resize mesuré, copie/sélection, collage confirmé et déconnexion. Job Object privé
kill-on-close, création suspendue/assignation avant exécution ; I/O hors GTK,
queues bornées, état d’écran coalescé, OSC fixe et zéro scrollback.
Workflow manuel sur Windows Server2022 : run43698455015 PASS (source a29f8f1),
build/tests, flux UTF-8/resize/exit0/annulation et GUI IM commit/affichage/resize/
déconnexion. WIN-01 reste PARTIAL pour la qualification Windows11, IME/DPI,
polices/sélection, applications terminal plein écran et endurance. Validation Windows11, protocoles/profils/coffre, parité UI et MSI
restent TODO ; ne pas confondre le prototype avec l’application complète.

Dernière preuve native : [run43699123993](https://github.com/aurelson101/rustremote/actions/runs/43699123993)
sur5cca68a,8tests et deux captures GTK Server2022 inspectées. Évidence/hash dans
`docs/validation/windows-probe.json`. Windows11 sans VM disponible : simulations
communes uniquement, pas de validation OS. v0.9.1 retirée après sauvegarde vérifiée,
v0.9.2 conservée ; aucun MSI ni paquet Windows public livré.

## Priorité active : stabilisation GTK puis portage Windows

Le correctif du focus GTK est prioritaire avant le portage. Windows n'est pas
encore implémenté : ni MSI, ni MSIX, ni compatibilité obtenue par conversion de
l'AppImage. Cibles minimales : Windows11 x64 et Windows Server2022 x64 avec Desktop Experience. Server Core exclu pour cette interface GTK.
Les interfaces/protocoles Linux fonctionnels sont conservés.

| Ordre | Lot | Priorité | Dépend de | Statut | Critère de passage |
|---|---|---|---|---|---|
| 0 | GTK-FOCUS | P0 | — | DONE | Focus libéré avant désactivation des champs ; test GTK4.8 avec fatal-warnings et rejeu GUI sans GtkText focus-out. |
| 1 | WIN-01 : preuves GUI/terminal | P0 | GTK-FOCUS | PARTIAL | Sur Windows réel/VM dédiée : fenêtre GTK4 Rust empaquetable et prototype terminal ConPTY avec affichage ANSI/UTF-8, saisie, resize et arrêt propre, I/O hors GTK et tampons bornés. Choisir le rendu après ce PoC ; ConPTY ne fournit pas le widget graphique. |
| 2 | WIN-02 : compilation et frontière OS | P0 | WIN-01 | En cours (CI Linux/macOS/sonde Windows verte 2026-11-28 ; crate principal non compilable sous Windows) | Build Windows natif reproductible et job CI ; isoler dépendances VTE/Linux, chemins /usr/bin, sockets Unix, pidfd, permissions/flock. Modèles et tests communs compilent sur Linux/Windows ; pas de cfg dispersés dans les protocoles. |
| 3 | WIN-03 : stockage et secrets | P0 | WIN-02 | TODO | Dossier de configuration Windows, fichiers atomiques/ACL privées/verrou interprocessus, coffre Windows Credential Manager ou backend keyring Windows validé. Conservation du chiffrement optionnel, export sans secrets, aucune clé/mot de passe dans argv ou logs. |
| 4 | WIN-04 : SSH/SFTP natifs | P0 | WIN-03 | TODO | Terminal Windows via ConPTY et OpenSSH Windows ; routage/askpass/agent à adapter explicitement. SFTP libssh2 portable, chemins Windows/UNC/Unicode, transferts, collisions, édition, annulation, file persistante. known_hosts strict et refus des clés modifiées testés sur labo isolé. |
| 5 | WIN-05 : RDP Windows | P1 | WIN-03 | TODO | Client système mstsc via profils .rdp, domaine/NLA/certificat, taille/moniteurs, son et redirections opt-in. Prompt natif/coffre pour les secrets ; jamais de mot de passe en ligne de commande. Clipboard texte/fichiers/dossiers et déconnexion vérifiés en labo Windows autorisé. |
| 6 | WIN-06 : ergonomie et outils | P1 | WIN-04, WIN-05 | TODO | Dossiers/profils/import-export, glisser-déposer, copier-coller, DPI125–200%, tests UI Automation Windows (AT-SPI reste Linux), multi-écrans et commandes ping/tracert adaptés ; résolution de noms Windows sans getent. Linux reste couvert par les tests. |
| 7 | WIN-07 : paquet et qualification | P1 | WIN-06 | TODO | ZIP de test autonome puis MSI en priorité : runtime GTK/DLL, icônes, installation compte standard, mise à jour/désinstallation, GUID UpgradeCode stable et ProductCode/version corrects ; profils préservés. Profil utilisateur et secrets préservés à la mise à jour. |
| 8 | WIN-08 : distribution/signature | P1 | WIN-07 | TODO | Après MSI validé : manifeste WinGet avec URL immuable/SHA256 du MSI réel, installation silencieuse et désinstallation testées ; aucune soumission de manifeste fictif. MSIX/Store et signature viennent ensuite. SignPath soumis à admission et provenance CI/approbation manuelle/MFA. Aucun certificat ni compte fictif, aucune garantie d'absence de faux positifs. |
| 9 | WIN-09 : extensions | P2 | WIN-07 | TODO | VNC/Apple avec client et tunnel validés, série via API COM/backend portable en remplacement de picocom. Intégration RDP dans les onglets après validation séparée ; matériel réel et endurance distincts de simulations. |

### Pourquoi cet ordre

Le terminal actuel (`src/terminal.rs`) dépend de VTE/PTY et de pidfd Linux : c'est
le risque principal à lever avant de multiplier les adaptations. `store.rs`,
`transfers.rs`, `ssh_route.rs`, `ssh_auth.rs`, `credentials.rs`, `external.rs` et
`rustremote-askpass` contiennent également des intégrations Unix/Linux. Une simple
compilation croisée ne donne pas une application Windows fonctionnelle.

Le MVP Windows comprend organisation des sessions, SSH/SFTP, coffre et RDP externe.
VNC/Apple, série et intégration RDP en onglets passent après ce socle. La vérification
Windows nécessite un runner/VM Windows ; les tests Linux ne la remplacent pas.
Aucun nouveau langage applicatif ni remplacement global de l'interface n'est décidé
sans la preuve WIN-01. Les durées ne sont pas promises avant ces preuves.

Références primaires vérifiées le15novembre2026 : [GTK pour Windows](https://www.gtk.org/docs/installations/windows/),
[session ConPTY](https://learn.microsoft.com/en-us/windows/console/creating-a-pseudoconsole-session),
[Credential Manager](https://learn.microsoft.com/en-us/windows/win32/api/wincred/nf-wincred-credwritew).

## Roadmap Linux et fonctionnalités existantes

| ID | Phase | Priorité | Statut initial | Livraison vérifiable |
|---|---|---|---|---|
| P0-01 | Base | Critique | DONE | Versions Ubuntu, Rust, GTK4, pkg-config et plan de prérequis relevés. |
| P0-02 | Base | Critique | DONE | Git/Cargo initialisés, binaire minimal compilé, Cargo.lock suivi. |
| P0-03 | UI | Critique | DONE | Maquette GTK4 fonctionnelle (3 panneaux/onglets), événement de navigation + capture locale. |
| P0-04 | Modèle | Critique | DONE | Types de profils sans secrets, sérialisation, tests unitaires. |
| P1-01 | SSH | Critique | DONE | Connexion à laboratoire, vérification clé hôte, SSH Agent, gestion des refus ; tests. |
| P1-02 | Terminal | Critique | DONE | PTY interactif, resize, ANSI, UTF-8, copie/collage, fermeture propre. |
| P1-03 | SFTP | Haute | DONE | SFTP natif : listing, transferts, progression, reprise et édition validés ; vue locale/distante et DND rejoués en 0.6.0. File persistante et limites configurables ajoutées en 0.9.0. |
| P1-04 | Ergonomie | Haute | PARTIAL | Favoris, édition, recherche/raccourcis et dossiers hiérarchiques avec DND livrés ; migration SQLite reste ouverte. |
| P2-01 | RDP | Haute | PARTIAL | PoC IronRDP sécurisée avec labo Windows, certificat, UI basique. |
| P2-02 | VNC | Moyenne | PARTIAL | Lanceur TigerVNC avec tunnel SSH ; Apple ARD type30 via DH ajouté en 0.7.0. Moteur intégré et qualification Mac réel restent ouverts. |
| P2-07 | Découverte | Moyenne | PARTIAL | Découverte TCP privée bornée et création de profils via clic droit ; identification OS indicative, qualification des réseaux réels ouverte. |
| P2-06 | Série | Moyenne | PARTIAL | Profil picocom et switch simulé ; liaison PTY testée, matériel physique non testé. |
| P2-03 | Réseau | Moyenne | PARTIAL | Tunnels local/distant/SOCKS et ProxyJump livrés ; X11 forwarding reste ouvert. |
| P2-04 | Outils | Moyenne | DONE | Macros et Multi-Exec sur sessions SSH authentifiées avec prévisualisation, annulation et approbation testées en laboratoire. |
| P2-05 | Coffre | Haute | PARTIAL | Mots de passe par profil dans Secret Service, cache temporaire et export sans références testés ; verrouillage automatique/politique du coffre à compléter. |
| P3-01 | KVM | Exploration | TODO | Matériel/protocole VKC exact documenté, décision intégration/launcher. |
| P3-02 | Distribution | Haute | PARTIAL | .deb/AppImage signables, CI, licences, SBOM, audit et test installation. |

## Relevé P0-01 — 2026-11-07

Ubuntu 26.04.1 LTS, noyau Linux 7.0.0-43-generic x86_64 ; compilateur C et Git présents.
Rust/Cargo, Rustfmt, Clippy, pkg-config et les en-têtes GTK4 sont absents.
Candidats APT locaux : Rust/Cargo 1.93.1ubuntu1, pkg-config 2.5.1-4,
GTK4 4.22.4+ds-0ubuntu0.1. Installation simulée avec succès, non effectuée.

Préparation à exécuter dans un terminal local (saisie sudo locale) :

```bash
sudo apt install rustc cargo rustfmt rust-clippy pkg-config libgtk-4-dev
```

### Historique 0.1.0 — prototype et AppImage de test

Prérequis installés via authentification système : rustc/cargo 1.93.1, GTK4 4.22.4.
Git et package/workspace initialisés ; Cargo.lock ajouté à l'index. Prototype GTK4
français sombre à trois panneaux, recherche et onglets de démonstration opérationnels.
`cargo fmt/check/clippy/test` réussis (0 test unitaire) ; test UI AT-SPI réussi.
Capture locale inspectée : `dist/RustRemote-preview.png`.

À la demande de l'utilisateur, AppImage de test créé et lancé :
`dist/RustRemote-0.1.0-preview-x86_64.AppImage` (~23 Mo), reconstruction par
`./scripts/build-appimage.sh`. Contrôles de version, checksum et lancement graphique
X11/Xwayland réussis. Aucun protocole réseau implémenté. P3-02 reste partielle :
portabilité, AppStream, .deb, CI, SBOM et publication non validés. Détails dans `MEMORY.md`.
Révision visuelle P0-03 (2026-11-07) : disposition compacte conforme aux grandes
zones de la maquette, menus et barre d'outils, arborescence dépliable, terminal noir
avec couleurs, listing SFTP fictif et zone de transferts. AppImage reconstruite et
capture remplacée ; contrôles Cargo et test d'onglets AT-SPI réussis. Aucun protocole
ajouté à cette étape. P0-04 a ensuite été livrée en 0.2.0.

## Incrément 0.2.0 — profils, sécurité et finition

- P0-04 : modèles SessionProfile/SessionKind/SessionState, sérialisation sans secrets,
  validation et tests livrés. Huit tests passent, ainsi que format/check/Clippy.
- Formulaires SSH (agent et clé existante) et RDP (domaine/résolution/plein écran/clipboard),
  favoris, création/édition et réouverture des profils ; stockage JSON privé ou age chiffré.
- Menus/actions branchés, verrouillage manuel, aspect arrondi et profils dans la colonne
  gauche. Lanceurs externes OpenSSH/FreeRDP après confirmation ; pas de connexion réelle testée.
- P1-04 partielle : SQLite, migrations, import/export et suppression de profils restent à faire.
  P2-01 partielle : configuration/lanceur disponibles, moteur IronRDP et labo non réalisés.
- AppImage 0.2.0 créée : `dist/RustRemote-0.2.0-preview-x86_64.AppImage`.
  Les outils non implémentés restent désactivés ; le coffre de mots de passe reste TODO.
- Test graphique final isolé : sauvegarde/réouverture chiffrée SSH/RDP, erreur de phrase,
  options RDP, annulation de connexion, gestionnaire/paramètres/outils/aide, chemin réel
  Fichier → Nouveau profil, ouverture et fermeture d'onglet : réussi.
  Menus GTK rendus accessibles avec des entrées explicitement nommées ; capture finale inspectée.

Priorités suggérées après cette étape : terminal SSH intégré et fermeture suivie des
sessions ; laboratoire RDP avec certificat valide ; SFTP réel avec protections
d'écrasement ; verrouillage automatique ; migration/import-export versionnés et sauvegardes.

## Incrément 0.3.0 — import, dossiers et laboratoire

P1-04 reste PARTIAL : dossiers de sessions, import JSON avec prévisualisation/refus des
doublons, export JSON consenti sans écrasement, reprise du format 0.2 et menus contextuels ajoutés ; SQLite, renommage
et suppression de dossiers/profils restent à faire. Le fichier d'exemple IA est explicitement
une simulation et ne déclenche aucune connexion à ia.aurelson.com.

Laboratoire local : création de dossiers, édition de texte, copie récursive bidirectionnelle
bornée, refus d'écrasement, presse-papiers texte UTF-8 ; ce n'est pas une livraison P1-03
SFTP réel ni une validation de canal RDP. Diagnostics Ping/Traceroute configurables et
simulés par défaut. `--offline` bloque les lanceurs actuels ; verrou interprocessus des
profils et sérialisation des écritures ajoutés. AppImage 0.3.0 reconstruite.

20 tests Rust passent, ainsi que format/check/Clippy. Options RDP : taille automatique,
smart sizing, multi-écrans, son, microphone et imprimantes avec opt-in, empreinte SHA256.
Icônes SVG originales Linux/Windows/dossiers embarquées ; démo temporaire `--demo`.
Les résultats des tests graphiques,
mesures et limites sont détaillés dans `docs/VALIDATION-0.3.0.md` et `MEMORY.md`.
Laboratoires loopback explicitement autorisés : OpenSSH/SFTP avec serveur éphémère
Paramiko, NLA FreeRDP avec bureau Xvfb distinct. Clés/certificats incorrects refusés,
édition/copies SFTP et rendu RDP vérifiés. Ces scripts ne livrent ni Russh ni IronRDP :
P1-01 reste TODO pour l'intégration applicative, P2-01 reste PARTIAL sans Windows réel.
VNC : configuration et lanceur TigerVNC, tunnel SSH activé par défaut, passerelle et
port, agent/config OpenSSH, known_hosts ; sans tunnel, X509Vnc/CA système. Série :
picocom avec débit/bits/parité/arrêt/flux, icône switch et console simulée en lecture
seule. VNC authentifié à travers un tunnel loopback avec rendu vérifié, réponse
d'authentification invalide refusée ; picocom 115200 8N1 bidirectionnel sur PTY réussi.
Les moteurs intégrés et ports série physiques restent non validés.
Prochaine tâche réseau : P1-01, moteur/PTY intégré sur ce laboratoire autorisé.

Rejeu du 2026-11-07 demandé par l'utilisateur : 20 tests Rust, parcours X11/Wayland,
laboratoires SSH/SFTP/RDP/VNC/série et outils loopback réussis. Test sans chiffrement
et menus VNC/série renforcés, causes de refus RDP vérifiées ; icône RDP par défaut corrigée
et AppImage reconstruite/revalidée. Journaux/captures : `dist/revalidation-0.3.0-_rzp_wzw/`.
Les statuts restent inchangés : cette batterie ne livre pas les moteurs intégrés et
ne valide ni Windows/KDE ni les périphériques physiques. Voir `docs/VALIDATION-0.3.0.md`.

## Ordonnancement et règles de tranche

- **P0** : créer une base légère réellement exécutable, sans fonctionnalités fictivement marquées comme livrées.
- **P1** : stabiliser un cœur SSH/SFTP utilisable avant d'étendre les protocoles.
- **P2** : prototypes protocolaires isolés puis intégration progressive, sans dégrader le MVP.
- **P3** : publication et intégration spécifique, après validation sécurité et juridique.

### Checklist de sortie de chaque tâche

- [ ] Le résultat est visible ou démontré par un test exact.
- [ ] Les erreurs/régressions sont couvertes par tests là où cela a du sens.
- [ ] Le format/lint/test applicables ont été exécutés, ou l'indisponibilité expliquée.
- [ ] Aucun secret ni endpoint sensible ajouté au dépôt.
- [ ] `MEMORY.md` et la table ci-dessus reflètent l'état réel.

## Backlog optionnel à n'ajouter qu'après MVP

Profil exportable (sans secret), recherche avancée, favoris partagés sans secrets, tunnels multiples, SSH certificates, accès via FIDO2, sessions déconnectées/reconnexion, journaux de transferts, packaging Flatpak, thème clair/sombre, i18n anglaise.

## Incrément 0.4.0 — terminal et SFTP intégrés

- P1-01/P1-02 : VTE/OpenSSH intégré, authentification clé/agent, empreinte d’hôte
  contrôlée, PTY, ANSI/UTF-8, resize de fenêtre, copier/coller contrôlé, déconnexion.
  Choix D-004 actualisé : Russh reste candidat, le moteur effectif est OpenSSH/VTE.
- P1-03 PARTIAL : navigateur réel ssh2/libssh2, listing/édition/création de dossiers,
  transferts fichiers/dossiers bidirectionnels, refus d’écrasement, détection de conflit,
  délais/annulation et budgets. Restent progression détaillée, reprise, bastions/alias
  OpenSSH et partage du transport SSH. Pas d’authentification SFTP par mot de passe.
- Série : terminal picocom intégré ; RDP/VNC restent externes, matériel physique et
  canaux Windows non validés. P1-04 demeure partielle (SQLite/coffre OS absents).
- Mode normal sans exemples au démarrage ; sortie de démonstration vers profils
  persistants, maintien de --offline ; accueil création Linux/Windows et dossiers.
  Paramètres persistants : police, historique borné, fond noir, outils activables.
- Vérifications et livrable : `docs/VALIDATION-0.4.0.md`, `MEMORY.md` et dossier
  `dist/validation-0.4.0-38n_04hi/`. Version de test Ubuntu 26.04, pas un produit achevé.

Finition demandée : fenêtre de configuration (RDP comprise) redimensionnable avec
pied d’actions fixe et repliable, formulaire défilant, onglets défilables. Test géométrique
et captures à 600×480, 480×520 et 800×680 réussis. Accessibilité du texte VTE lors de
changements d’onglet à qualifier : le test utilise la copie réelle de la sélection.

Rejeu final ciblé SSH/SFTP sur l’AppImage 0.4.0 : code 0, contrôles clé/agent,
hôte, PTY, clipboard et opérations SFTP réussis ; série explicitement exclue de ce
rejeu (échange intégré après plusieurs onglets encore en échec dans le parcours complet).

## Correctif 0.4.1 — 2026-11-08

Sélection/listing SFTP invalidés lors du changement de chemin et du rafraîchissement ;
contrôles inactifs après déconnexion ; aperçu multiligne et statut série corrigés.
Parcours complet GUI SSH/SFTP/série de l’AppImage : PASS, sans exclusion série,
29 octets et deux commandes/réponses au PTY ; capture inspectée. L’ancien scénario
série s’auto-influençait via le clipboard, corrigé dans le laboratoire. AT-SPI VTE et
avertissement GTK focus-out restent à qualifier. P1-03 et P2-06 demeurent PARTIAL.
Runtime AppImage désormais explicite, mis en cache et vérifié par SHA256 ; build réussi
malgré l’indisponibilité HTTP 504 de GitHub. Détails : docs/VALIDATION-0.4.1.md.
Prochaine priorité : progression/reprise SFTP et prise en charge des bastions ; voir
le tableau des manques du rapport. Métadonnées JSON/age compatibles avec 0.4.0.


## Audit avant nouvelle AppImage — 2026-11-08, incrément non publié

Voir `AUDIT-DEMANDES-2026-11-08.md`. Toutes les demandes ne sont pas terminées :
aucune nouvelle AppImage produite, anciens artefacts conservés. Cargo reste en 0.4.1
jusqu’au choix de la prochaine version de test ; ne pas écraser l’image précédente.

Ajouts : mots de passe SSH/RDP dans les profils via Secret Service, domaine commun,
askpass privé, SFTP par mot de passe, Coffre distinct de Paramètres, actions de dossiers,
menu Affichage, renommage/permissions/suppression SFTP et lots/multisélection.
29 tests Rust et contrôles Cargo réussis ; connexion SSH/SFTP par mot de passe et
relecture coffre après redémarrage, RDP depuis le profil avec domaine, pixels,
déconnexion et arrêt à la sortie de l’application validés en laboratoires locaux.

P1-03/P1-04/P2-01 restent PARTIAL ; P2-05 passe à PARTIAL. Progression/reprise/bastions,
CLIPRDR réel, Windows/AD, périphériques physiques et KDE restent à qualifier/livrer.
P2-03/P2-04 restent TODO : leurs boutons désactivés ne sont pas des fonctionnalités livrées.
Les commandes de rejeu et les contrôles souris complémentaires figurent dans l’audit.

Dernier rejeu complet : clic droit SSH, zoom, commandes SFTP et sélection multiple,
clés/agent et série PASS ; quatre bascules Affichage/dossiers/coffre PASS.
Dernier parcours Wayland au démarrage en échec (dialogue de protection reste ouvert) :
correction/qualification encore ouverte. Résultats structurés : validation/2026-11-08.json.

Publication du projet complet sur le dépôt privé `aurelson101/rustremote` demandée.
README avec captures réelles, licence personnalisée d’usage professionnel et de
redistribution gratuite interdisant la revente, NOTICE et métadonnées préparés.
Les droits des composants tiers restent inchangés ; il ne s’agit pas d’une licence OSI.

Publication du projet complet effectuée sur `main` du dépôt privé
`aurelson101/rustremote` : premier commit complet
`604a029040ec5fa9d6d6d9aeb0300bd33cd2abe1` ; 86 fichiers, captures et licence incluses.
Arbre source local/distant identique, contrôles de liens README et métadonnées Cargo
réussis ; aucun binaire ni secret publié. P3-02 reste PARTIAL : cette publication des
sources ne constitue pas une nouvelle AppImage ni une qualification des limites restantes.

## Aperçu 0.5.0 — 2026-11-08

Voir `VALIDATION-0.5.0.md`. P1-03 satisfait ses critères par le navigateur SFTP
natif et ses transferts/édition réellement rejoués sur serveur local. Progression,
annulation, reprise de fichiers, sélecteurs multiples et alias/bastion ajoutés.
P2-04 livré avec commandes transitoires et cibles authentifiées ; P2-03 reste
PARTIAL sans X11 forwarding. P1-04, P2-01/02/05/06 et P3-02 restent PARTIAL.

34 tests Rust et contrôles Cargo réussis. AppImage 0.5.0 construite ; mots de passe
SSH/SFTP en coffre après redémarrage, RDP depuis profil, Wayland/plein écran et
série native testés sur artefact. CLIPRDR réel validé séparément sous GNOME Remote
Desktop : texte/fichiers/dossiers dans les deux sens, 10 cas. Aucune qualification
Windows/AD, KDE ou périphériques physiques. Les anciens résultats ci-dessus sont
historiques ; la validation 0.5.0 prévaut pour l’état courant.

Publication 0.5.0 effectuée sur GitHub privé, AppImage et SHA256 téléversés puis
retéléchargés et comparés. Anciennes AppImages locales 0.1.0–0.4.1 retirées après
validation ; aucune ancienne pièce jointe GitHub n’existait. P3-02 reste PARTIAL
(portabilité, signature, CI/.deb et installation multi-distribution non qualifiées).

## Aperçu 0.6.0 — 2026-11-09

Anglais par défaut et français configurable après redémarrage. Navigateur SFTP à
panneaux local/distant, boutons de transfert, DND et édition ; arborescence des
sessions avec sous-dossiers et déplacements par souris ou clic droit. Nouveaux
contrôles Affichage et outils d’information/empreinte publique.

AppImage x86_64 construite dans Debian 12 avec ABI glibc minimale 2.36 auditée ;
GTK/VTE/SVG et modules de saisie embarqués, pilotes graphiques de l’hôte conservés.
Démarrage réel Debian 12 et essais Ubuntu 26.04 X11/Wayland ; pas de certification
universelle des distributions, ARM/musl ni des bureaux KDE/Xfce/Cinnamon complets.
Voir `VALIDATION-0.6.0.md` et `validation/0.6.0.json` pour résultats et limites.

39 tests Rust et contrôles Cargo réussis. P1-04 reste PARTIAL (SQLite), P3-02 reste
PARTIAL (signature, CI, .deb et autres architectures). Les diagnostics de focus
GTK 4.8 connus restent ouverts ; la nouvelle ergonomie n’implique pas une parité
complète avec MobaXterm/FileZilla ni une qualification Windows/AD ou du matériel.

Publication 0.6.0 du 2026-11-10 effectuée : tag sur le commit
`86615985090071be9a0d44227d5cb6f342d361b2`, préversion GitHub privée disponible.
Les quatre assets (AppImage, SHA256, ABI et versions des paquets) ont été
retéléchargés puis comparés aux fichiers locaux. v0.5.0 conservée. P3-02 reste
PARTIAL pour les critères encore ouverts ci-dessus.

## Incrément 0.7.0 — 2026-11-14

Demande utilisateur prioritaire : compatibilité Apple comme le mode compte macOS
MGRemote et amélioration SFTP. Preset Apple, plateforme MacOs et icône originale ;
VNC TigerVNC DH/type30 uniquement dans un tunnel SSH vers le même Mac, pas de repli.
Le mot de passe macOS est demandé par TigerVNC, pas stocké dans le profil RustRemote.
SFTP : recherche des deux panneaux, fichiers cachés, tri dossiers en premier et
sélection vidée lors du filtrage ; rafraîchir le panneau local préserve le distant.
42 tests Rust, laboratoire Apple positif/négatif et VNC-SSH/série locaux réussis.
Voir VALIDATION-0.7.0.md pour le rejeu graphique sur AppImage et limites. P2-02 reste
PARTIAL : pas de Mac réel, pas de moteur intégré ni de qualification SRP/haute performance.

Publication 0.7.0 effectuée : sources/README/captures sur GitHub, tag
`a132fbb72e2f8df8fe4a42b30815cef1a1e3f7f5`, AppImage et trois fichiers annexes
retéléchargés et comparés. Rejeu final AppImage SSH/SFTP/Apple et Wayland PASS.
Le dépôt est public au moment de cette livraison ; visibilité inchangée.

## Incrément 0.8.0 — 2026-11-14

Clic droit de dossier : Apple et VNC ajoutés, dossier conservé dans l’éditeur.
Outils/Découvrir les équipements : simulation par défaut, TCP IPv4 privé /24 max,
autorisation explicite réinitialisée quand la cible change, refus public/link-local
et --offline, huit workers et budget 30s avec annulation. Sonde RFB limitée à
l’annonce de sécurité, sans sélection d’authentification. Les systèmes ne sont pas
certifiés à partir de ports. Clic droit d’un résultat : éditeur prérempli, choix du
dossier existant/nouveau, compte et options SSH/RDP/Apple/VNC ; pas de connexion
automatique, simulations conservées. 45 tests Rust et contrôles Cargo réussis.
La qualification GUI finale et la publication sont décrites dans VALIDATION-0.8.0.md.

Publication 0.8.0 vérifiée sur le commit
`7b6492f390b18f1832841abcfec20826c622065e` : AppImage et trois fichiers annexes
retéléchargés et comparés. Rejeu complet GUI/discovery/SSH/SFTP et Wayland PASS.
P2-07 reste PARTIAL pour l’identification OS et la qualification de réseaux réels.

## Correctif 0.8.1 — 2026-11-14

Découverte : un seul scan réel par processus, états terminés/annulés/expirés distincts
avec progression traités/total. Modifier cible/mode efface les résultats et invalide
les callbacks obsolètes. 49 tests Rust et contrôles Cargo réussis ; rejeu ciblé dans
VALIDATION-0.8.1.md. P2-07 reste PARTIAL pour OS/IPv6/réseaux réels ; focus GTK et
file de transferts SFTP restent des améliorations ouvertes.

Préversion 0.8.1 publiée sur `ed08e9e420a75d13ad30bd31e0f422d06d87ca9d` ;
quatre assets retéléchargés et comparés. Rejeu ciblé découverte et Wayland PASS.

## Incrément 0.9.0 — 2026-11-14

SFTP : clic simple sur dossiers locaux/distants, activation souris/clavier via le
signal du ListBox, navigation différée hors événement GTK et chemins distants
canoniques. File privée par profil, verrouillée et chiffrée comme le magasin actif ;
reprise explicite, pause et réessai, sans écrasement implicite. Limites par transfert
configurables dans Paramètres. Le changement d'onglet RDP attend son attachement
à la fenêtre : diagnostic critique GTK4.8 reproduit sous GDB puis corrigé.
Découverte : IPv6 ULA /120–/128 et loopback ; résolution inverse DNS/mDNS optionnelle
via NSS système, sans utiliser les noms pour les connexions. mDNS dépend du système
hôte ; pas d'inventaire DNS-SD. P2-07 reste PARTIAL pour qualification LAN/OS réels.
Tests et état de publication : voir VALIDATION-0.9.0.md.

Préversion0.9.0 publiée sur `ed643426f7f86f701ff4bc1c52d6fea73ece8f27` ; quatre assets
retéléchargés et comparés avant publication. Rejeu final SSH/SFTP GUI et Wayland PASS.

## Confidentialité / licence — 2026-11-14

DONE : politique bilingue PRIVACY.md, licence des nouvelles révisions GPL-3.0-only
(texte intégral, NOTICE/Cargo/README cohérents), politique de signature préparatoire.
Anciens artefacts et licences tierces inchangés. fmt/check/Clippy et55tests PASS.
OPEN : admission SignPath, version/CI Windows et qualification de sa découverte ;
le changement de licence ne constitue pas une acceptation ni une signature.

## Distribution 0.9.1 — 2026-11-14

DONE : rapport SignPath HTML local exclu du dépôt ; notices GPL/confidentialité dans
À propos et dans l'AppImage, liste de copie du builder isolé corrigée. 55tests et
contrôles Cargo PASS ; GUI anglais SSH/SFTP/About PASS. Vérification du rapport dans
Brave headless isolé. Voir VALIDATION-0.9.1.md. Publication/nettoyage après backup et
comparaison des nouveaux assets ; tags historiques conservés. SignPath non acquis.

Publication0.9.1 vérifiée : quatre assets retéléchargés/comparés. Six anciennes
releases retirées après sauvegarde intégrale et contrôle d'absence de changement ;
leurs tags Git sont conservés.17anciens artefacts locaux archivés. Le rapport HTML
personnel reste exclu de Git et des assets. État de reprise dans MEMORY.md.

## Stabilisation GTK — 2026-11-15

GTK-FOCUS DONE : cause reproduite avec GTK4.8.3 (désactivation du panneau actif),
focus libéré avant désactivation et préservé hors panneau. Test dédié sous warnings
bloquants PASS,55tests normaux PASS ; rejeu GUI final SSH/SFTP sans GtkText et sans
warning/critical GTK RustRemote. AppImage0.9.2 locale validée ; aucune release GitHub
publiée dans cette étape. Voir VALIDATION-0.9.2.md et validation/0.9.2.json.
Prochaine priorité Windows : WIN-01 ; tous les lots Windows restent TODO.

## Démarrage Windows / MSI–WinGet — 2026-11-15

WIN-01 commencé dans experiments/windows-probe, package Cargo indépendant pour
préserver Linux. Fenêtre GTK4, test natif ConPTY borné (entrée/sortie UTF-8,
redimensionnement, processus système local et arrêt), lecture sur worker et sortie
retenue64KiB maximum. Pas de connexion SSH réelle ni d'identifiant. Le rendu ANSI
interactif et l'intégration au produit restent ouverts : ne pas qualifier le lot DONE
sur le seul transport. Workflow natif Windows explicite, actions épinglées, rapports
de diagnostic ; aucun faux MSI/MSIX de produit complet.

L'ordre demandé est désormais : MVP Windows fonctionnel → MSI testé → WinGet →
MSIX/signature. WinGet utilise des métadonnées et un hash d'installateur, et ne signe
pas le programme à la place de l'éditeur. Choix entre dépôt communautaire et source
privée à faire au moment de la distribution ; pas d'ouverture de PR communautaire
avant livrable réel et validation. Référence : [manifestes WinGet](https://learn.microsoft.com/en-us/windows/package-manager/package/manifest).

WIN-01 PARTIAL : [CI native Windows réussie](https://github.com/aurelson101/rustremote/actions/runs/43693375903)
sur57f93fa : GTK4.24.1, Rust1.100.0 MSYS2 UCRT64, transport ConPTY UTF-8, resize et
exit normal PASS, fenêtre GTK mappée PASS. Rendu terminal ANSI/saisie interactive
et intégration des profils restent à faire. Le test passe avec stdout du parent
redirigé : STARTF_USESTDHANDLES + handles nuls évitent son détournement hors PTY.
Preuves dans validation/windows-probe.json ; pas de validation complète Windows11,
DPI, AD ou matériel. La0.9.2 Linux est publiée avec quatre assets vérifiés.
