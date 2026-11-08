# Baseline de sécurité — RustRemote

## Principes

Application d'administration réseau à privilèges potentiellement élevés : moindre privilège, secure-by-default, sans télémétrie ; aucune exécution distante implicite.

## Règles sur les secrets

- Utiliser **SSH Agent** et les clés SSH système lorsque possible ; aucun mot de passe ni clé privée dans SQLite, profils, logs, tests, captures, tickets ou documentations.
- Pour les mots de passe utilisateur : API du **trousseau du système** (Secret Service sous GNOME, éventuellement libsecret) ; seulement des `secret_ref` en base. Ne jamais concevoir un coffre cryptographique maison.
- Fichier `credentials.txt` ou `.env` fourni ponctuellement par l'utilisateur : **lecture seule, local, jamais versionné, jamais dupliqué, affiché, exfiltré ou partagé** ; préférer l'absence de fichier et le trousseau. Si le contenu ne peut être utilisé sans exposition, arrêter et expliquer la limitation.
- Pas de mot de passe en argument de ligne de commande (`ps`, historique et logs), ni dans les variables de debug persistées.
- Redaction des erreurs, journaux et trace réseau ; purger les données sensibles en mémoire lorsque possible, tout en reconnaissant les limites du langage et de l'OS.

## Authentification et transport

- SSH : vérification **known_hosts** stricte, alerte de changement, approbation humaine explicite d'empreinte inconnue ; ne jamais activer un contournement global pour accélérer un test.
- Ne pas activer automatiquement les algorithmes SSH faibles ; exceptions explicites documentées et limitées, si absolument nécessaires.
- RDP : NLA/TLS selon support, vérification correcte du certificat, prévention MITM ; aucune option d'ignorance systématique.
- VNC : reconnaître les limites des modes non chiffrés, privilégier TLS/SSH tunnel ou refuser l'usage risqué par défaut ; avertissements explicites.
- SFTP : chemin canonique local, prévention de path traversal, contrôle écrasement, permissions et liens symboliques, fichiers temporaires sûrs, annulation et reprise maîtrisées.
- X11 : exploiter infrastructure X11/Xwayland locale ; ne pas lancer d'accès X non authentifié.

## UI / actions dangereuses

- Confirmation avant exécution « Multi-Exec » avec inventaire des hôtes ciblés.
- Clipboard : pas de copie de secrets par défaut ; notifications pour actions sensibles.
- Ne jamais cliquer/exécuter de lien ou commande incorporé dans la sortie terminal sans action explicite.
- Macros : affichage du contenu et des cibles avant exécution ; pas d'auto-run non consentie.

## Qualité logicielle et chaîne d'approvisionnement

- Garder Cargo.lock suivi, revue des versions/licences, `cargo audit` et `cargo deny` **si installés et configurés**.
- Revue des `unsafe`, FFI et scripts d'installation ; pas de téléchargement puis exécution non vérifié (`curl | sh`).
- Build reproductible autant que possible, dépendances minimales, releases signées une fois le packaging prêt.
- Tests sans machines de production ; secrets factices et serveurs locaux de labo uniquement.
- Ne jamais modifier les ACL/règles réseau du poste, `sudo`, domaines AD ou serveurs à distance sans consentement préalable explicite.

## Threat model minimal

| Menace | Contrôle |
|---|---|
| Vol d'identifiants sur le disque | Trousseau OS, aucune valeur en clair dans profils ou logs. |
| SSH MITM / changement de serveur | known_hosts et vérification d'empreinte. |
| Faux certificat de bureau distant | Validation TLS / NLA ; pas de mode insecure caché. |
| Commande sur le mauvais serveur | Nom d'hôte et groupe visibles ; confirmation multi-exec. |
| Destruction de fichier par transfert | Écrasement demandé, fichiers temporaires et erreurs affichées. |
| Dépendance compromise | Versions verrouillées, audit, revue des licences. |
| Fuite via mémoire d'agents | MEMORY.md sans secrets et retour de test redigé. |

## Profils implémentés — 0.2.0

- `profiles.json` ou `profiles.age` : choix initial explicite ; un seul magasin accepté.
  Chiffrement standard age/scrypt via la crate age (MIT ou Apache-2.0), pas de coffre maison.
- Dossier 0700, fichier 0600 ; sauvegarde atomique avec synchronisation, création sans
  écrasement, refus des liens symboliques en lecture et limite de taille de 2 Mio.
- Phrase secrète détenue avec SecretString, traitement cryptographique hors thread GTK.
  Verrouillage manuel disponible ; verrouillage automatique et migration de mode à développer.
- Profils typés sans champ mot de passe ; champs inconnus rejetés, hôtes/utilisateurs/ports
  validés. Le chemin de clé SSH est une référence, jamais le contenu d'une clé.
- Clients système lancés sans shell, après confirmation de la cible ; aucun secret en
  arguments. SSH vérifie les clés d'hôte et désactive le transfert d'agent ; RDP impose
  NLA et refuse les certificats non validés, sans partage de disques ni presse-papiers par défaut.
- Le lanceur ne suit pas la connexion et fermer RustRemote ne ferme pas le client externe.
  Aucun test sur serveur réel n'a été effectué ; un laboratoire autorisé est nécessaire.

Références : https://docs.rs/age/0.11.5/age/ et manuel local `xfreerdp3 /help`.

## Contrôles ajoutés — 0.3.0

- Profils versionnés, dossiers sans segments de chemin, import JSON de 2 Mio maximum,
  champs inconnus/secret refusés, prévisualisation et fusion sans remplacement.
- Profil `simulation: true` : construction de commande réseau refusée. Mode `--offline`
  : aucun lancement de client SSH/RDP ni diagnostic réel ; il refuse une autre instance
  déjà ouverte au lieu d'en hériter du mode. Ce n'est pas une sandbox réseau OS.
- Verrou de session `.profiles.lock` via File::try_lock, refus des liens symboliques,
  et une seule écriture/crypto active par contrôleur. Le verrou nécessite que les autres
  versions coopèrent : fermer l'ancienne 0.2 avant la migration.
- Laboratoire : répertoire temporaire privé, chemins relatifs contrôlés, symlinks refusés,
  copies limitées à 128 entrées / 8 niveaux / 1 Mio, préparation temporaire et refus
  d'écrasement. Aucun transfert en dehors du laboratoire.
- Presse-papiers : lecture en flux bornée à 64 Kio, délai 2 s et annulation des futures ;
  aucun texte exécuté. L'éditeur de fichiers est limité à 1 Mio.
- RDP : texte et fichiers sont deux consentements distincts ; directions explicitement
  bidirectionnelles uniquement après opt-in. Cette configuration a été parsée par
  FreeRDP, mais aucun canal réel Windows n'a été testé.
- Diagnostics : validation de cible, arguments séparés, nombre de requêtes/sauts limité,
  durée 20 s, mode simulé par défaut ; aucun test de réseau sur le serveur IA.
- Scan OSV des dépendances Rust Linux : voir `docs/VALIDATION-0.3.0.md`. Les bibliothèques
  natives, le système et les clients distants ne sont pas couverts par ce scan Rust.
- Laboratoires SSH/SFTP et RDP autorisés exclusivement sur loopback : clés, SAM et
  certificats générés dans des répertoires temporaires privés puis nettoyés ; aucun
  compte système, shell personnel ou bureau personnel utilisé. RDP NLA avec empreinte
  épinglée ; authentification NTLM explicite dans ce seul test SAM, sans recherche KDC.
  Les résultats persistés excluent les secrets. Aucun test sur ia.aurelson.com.
- Export : consentement explicite aux métadonnées en clair, fichier 0600, refus
  d'écrasement ; une référence de chemin de clé peut apparaître, jamais son contenu.
- VNC : `VncAuth` seulement dans le tunnel SSH explicitement configuré ; accès direct
  limité à `X509Vnc` avec CA système, clipboard désactivé. Commande de tunnel statique
  TigerVNC, port numérique validé, passerelle/utilisateur sans syntaxe shell ; écoute
  locale 127.0.0.1, strict host-key checking et transfert d'agent désactivé. Le tunnel
  suit le mécanisme natif TigerVNC (`ssh -f ... sleep 20`), pas un gestionnaire de
  tunnels intégré. Le terminal Ptyxis est lancé en instance distincte pour transmettre
  l'environnement du client sans réutiliser un processus existant.
- Série : chemins /dev autorisés et paramètres limités, argv séparés, aucune élévation
  de privilèges ni modification du groupe dialout. La simulation ne lance que des
  réponses Rust prédéfinies en lecture seule ; le test réel n'utilise que des PTY privés.

## Incrément 0.4 — SSH/SFTP intégrés

- OpenSSH respecte known_hosts et le fichier de configuration explicitement choisi.
  SFTP vérifie l’hôte avant authentification via libssh2 ; hôte inconnu/modifié refusé,
  clé ou agent seulement, maximum trois identités agent. Une clé protégée explicite
  est retrouvée dans l’agent par son fichier public, sans demander/stocker sa phrase.
- SFTP direct uniquement : aucune reprise implicite des alias/bastions OpenSSH.
  Chemins/listings bornés, symlinks et fichiers spéciaux refusés pour les transferts ;
  cibles existantes refusées. Sauvegarde de l’éditeur par staging exclusif et renommage
  atomique avec comparaison du contenu initial. Ceci ne supprime pas toute course
  possible avec un serveur ou un acteur distant malveillant.
- Copies : 16 Mio/128 entrées/8 niveaux/30 s ; listing 1024 entrées ; éditeur 1 Mio.
  Annulation disponible ; fichiers sortants partiels possibles, sans effacement automatique.
- Terminal : environnement nettoyé avant spawn VTE, signaux par pidfd sans réemploi
  de PID ; lien automatique/sixel désactivés. Presse-papiers 64 Kio/2 s, confirmation
  multiligne, mêmes contrôles pour PRIMARY. Les commandes saisies restent des commandes.
- Paramètres non secrets dans settings.json (0600), écriture atomique, historique
  borné 100–20000 lignes. Quitter la démo ne copie pas les exemples et conserve
  l’interdiction réseau lorsque --offline a été explicitement fourni.
- Bibliothèques natives VTE/libssh2/OpenSSL ajoutées au périmètre de maintenance.
  Pas de garantie « zéro défaut » ni audit exhaustif ; voir le rapport 0.4.

## Identifiants par profil — incrément non publié après 0.4.1

- Secret Service via keyring 4.2.0 (MIT ou Apache-2.0) ; référence UUID seule dans
  les profils. Le mot de passe apparaît dans le formulaire du profil mais son
  enregistrement durable est facultatif. Sans sauvegarde, cache de session uniquement.
  Pas de repli en clair si le coffre est indisponible. Le chiffrement age des
  métadonnées reste un choix distinct de la protection des mots de passe.
- Cache indexé par protocole, profil, hôte, port, utilisateur et domaine RDP ; une
  modification de cible exige ressaisie ou retrait de la référence enregistrée.
  Verrouiller les profils purge le cache temporaire ; les sessions actives gardent
  leur snapshot jusqu’à la déconnexion. Les onglets terminés libèrent ce snapshot.
- Import : référence de coffre neutralisée ; export : référence exclue. Une simple
  référence UUID ne constitue pas une autorisation interprocessus face à un programme
  malveillant du même utilisateur : les garanties restent celles du trousseau OS.
- OpenSSH : helper dédié sans GTK, socket dans TempDir privé, trames et délais bornés,
  contrôle UID, exécutable et lien PPid avec le processus SSH. Jamais de secret dans
  les arguments ou l’environnement. Une empreinte inconnue nécessite une confirmation,
  une empreinte modifiée reste refusée ; passphrases/MFA ne réutilisent pas le secret
  de compte. SFTP vérifie known_hosts avant l’authentification par mot de passe.
- FreeRDP : secret sur pipe stdin, contrôle NLA et certificat conservé ; suivi pidfd,
  arrêt explicite et fermeture avec RustRemote. Codes de sortie affichés, flux de
  diagnostic du client non conservés pour éviter une fuite. CLIPRDR réel non qualifié.
- Valeurs gérées par zeroize quand possible ; les copies internes GTK, clients
  système, bibliothèques et OS empêchent de garantir un effacement mémoire absolu.
- Suppression SFTP : confirmation explicite, préanalyse bornée et refus de liens ;
  suppression non atomique avec erreur partielle observable. Le protocole ne fournit
  pas la sécurité d’un openat local face à une arborescence distante modifiée en course.
- Essais d’identifiants : serveur éphémère loopback et coffre privé dans un HOME/data
  temporaire, bus de session séparé ; aucun accès au trousseau personnel. Le domaine
  RDP de laboratoire utilise une configuration Kerberos éphémère sans recherche DNS.
