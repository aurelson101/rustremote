# Product brief — RustRemote

## Vision
Client Linux de sessions distantes, ergonomique pour administrateurs systèmes et sécurité. Logiciel neuf écrit principalement en Rust, indépendamment de MobaXterm. Nom de travail : **RustRemote**. Interface en français (internationalisation possible).

## Référence visuelle

`../assets/maquette-rustremote.png` est une **illustration conceptuelle générée**, pas une preuve d'exécution ni un écran réel. S'en inspirer pour :

- **En-tête :** menu Fichier/Sessions/Outils/Affichage/Aide ; barre d'actions Nouvelle session, SSH, RDP, VNC, SFTP, Port Forward, Tunnels, Macros, Multi-Exec, Coffre, Paramètres.
- **Colonne gauche :** Favoris et dossiers de sessions, recherche, regroupement par environnement ; noms d'exemple non réels.
- **Zone centrale :** onglets, terminal interactif avec copy/paste, recherche, resize, indicateurs de connexion.
- **Colonne droite :** explorateur SFTP distant, transfert, état, progression, reprise d'erreur.
- **Bas :** statut de connexion/protocole, bandeau d'alerte sécurité et files d'attente.

Ne jamais coder en dur les noms d'hôtes de la maquette, ne pas extraire de données d'identification de l'image.

## Périmètre et priorité

### MVP (P0 + P1)

1. UI Linux réactive : disposition en 3 panneaux, arborescence, onglets, raccourcis, préférences de thème.
2. Profil de session SSH et favoris, stockage SQLite sans secret.
3. Terminal SSH avec session interactive, shell PTY, resize, UTF-8, couleurs, presse-papiers.
4. Validation d'identité hôte SSH et authentification par clé / SSH Agent ; pas de bypass.
5. Navigateur SFTP sur la même session, upload/download, progression et erreurs explicites.
6. Journalisation locale minimaliste et sans secrets, tests unitaires des modèles.

### Après MVP (P2)

- RDP natif, VNC, jumphost, tunnels, X11 forwarding, macros et envoi de commandes à plusieurs sessions.
- Sessions RDP/VNC dans des onglets, redimensionnement, presse-papiers avec contrôle.
- Coffre compatible trousseau OS et verrouillage automatique ; gestion des risques de clipboard.

### Exploration (P3)

- Compatibilité Raritan VKC/KVM **après recherche du modèle** (Java/JNLP, navigateur HTML5, client externe ou autre), sans imiter un protocole propriétaire.
- Packaging .deb et AppImage, politique de versions, SBOM et signature des binaires.

## Non-objectifs initiaux

- Aucun fork, clonage de code ou ressource propriétaire MobaXterm.
- Aucun mot de passe stocké en clair, coffre artisanal ou copie des secrets dans SQLite.
- Aucun moteur RDP/VNC créé de zéro si une bibliothèque maintenue répond aux besoins.
- Pas de serveurs de production, scans réseau ou commandes distantes automatiques sans validation.
- Pas de backend SaaS, télémétrie distante ni compte utilisateur imposé.

## Exigences de qualité

- Application utilisable hors ligne et sans inscription.
- Fenêtre réactive lorsque SSH/SFTP est lent (I/O async et limites de concurrency).
- Pas d'acceptation silencieuse de clé d'hôte modifiée ni de certificat distant invalide.
- Messages d'erreur pertinents, tests sans accès au réseau de production.
- Logs désensibilisés et rotation configurable ; débogage désactivé pour secrets.
- UI compatible clavier, HiDPI et GNOME/Wayland ; fallback X11 si nécessaire.
- Dépendances épinglées par Cargo.lock, revue de licence avant publication.

## Définition d'un MVP terminé

Sur Ubuntu de test : ouverture de l'application, création d'un profil SSH de test, vérification d'hôte, connexion à un **hôte de laboratoire autorisé**, terminal interactif, navigation/transfert SFTP, fermeture propre, reconnexion ; lint et tests locaux exécutés, aucune donnée sensible sauvegardée en clair.
