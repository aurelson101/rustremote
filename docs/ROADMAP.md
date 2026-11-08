# Roadmap pilotée par critères — RustRemote

**Règle :** le statut `DONE` exige résultat observable + commande de vérification enregistrée dans `MEMORY.md`. Si l'outil est indisponible, marquer `BLOCKED`, jamais `DONE`. Une seule tâche principale ouverte par agent.

| ID | Phase | Priorité | Statut initial | Livraison vérifiable |
|---|---|---|---|---|
| P0-01 | Base | Critique | DONE | Versions Ubuntu, Rust, GTK4, pkg-config et plan de prérequis relevés. |
| P0-02 | Base | Critique | DONE | Git/Cargo initialisés, binaire minimal compilé, Cargo.lock suivi. |
| P0-03 | UI | Critique | DONE | Maquette GTK4 fonctionnelle (3 panneaux/onglets), événement de navigation + capture locale. |
| P0-04 | Modèle | Critique | DONE | Types de profils sans secrets, sérialisation, tests unitaires. |
| P1-01 | SSH | Critique | DONE | Connexion à laboratoire, vérification clé hôte, SSH Agent, gestion des refus ; tests. |
| P1-02 | Terminal | Critique | DONE | PTY interactif, resize, ANSI, UTF-8, copie/collage, fermeture propre. |
| P1-03 | SFTP | Haute | DONE | SFTP natif : listing, transferts réels, progression, erreurs et refus d’écrasement validés en 0.5.0 ; reprise de fichiers ajoutée. File persistante/limites configurables restent des évolutions. |
| P1-04 | Ergonomie | Haute | PARTIAL | Favoris, stockage SQLite, édition de profils, recherches et raccourcis. |
| P2-01 | RDP | Haute | PARTIAL | PoC IronRDP sécurisée avec labo Windows, certificat, UI basique. |
| P2-02 | VNC | Moyenne | PARTIAL | Lanceur TigerVNC avec tunnel SSH ; moteur intégré/redimensionnement à livrer. |
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
