# RustRemote Privacy Policy / Politique de confidentialité

Effective date / Date d'effet: 14 November / novembre 2026.
Maintainer / Responsable du projet: AurelSon (@aurelson101).
Contact: contact@aurelson.com.

## English

### No automatic collection by the maintainer

RustRemote does not send telemetry, usage analytics, advertising identifiers,
connection histories, credentials, file contents or automatic crash reports to its
maintainer. It has no built-in advertising or tracking SDK, and using the desktop
application does not require a RustRemote account or a maintainer-operated server.
The maintainer does not sell or share application usage data, because the application
does not collect or transmit such data to the maintainer.

### Local data used to provide the application

The application processes data you provide to operate your sessions. Profiles may
contain hostnames, IP addresses, usernames, session folders and connection options.
Preferences and persistent SFTP queues (including local and remote paths and job
states) are stored locally, normally under `$XDG_CONFIG_HOME/rustremote` or
`~/.config/rustremote`. Profile metadata encryption is optional; unencrypted local
metadata is not secret merely because it is stored locally. Queue encryption follows
the active profile store. Renaming profiles or changing encryption does not
currently migrate or remove old queue files.

Passwords may be held for an active connection and, if you choose to save them,
stored in the operating system's Secret Service keyring. SSH keys, the SSH agent and
known-host records are handled locally by the selected SSH tools and configuration.
Terminal output, editor buffers, transfers, downloaded files, clipboard contents and
local diagnostics can contain sensitive information. They are not automatically
uploaded to the maintainer. External clients and the operating system may retain
their own settings, caches, host records or diagnostic logs.

### Network operations you request

Remote connections and transfers send the data required for SSH/SFTP, RDP, VNC,
tunnels, commands or explicitly enabled clipboard/device redirection to the endpoints
you select. Those servers may log account names, IP addresses and session activity
under their own policies. Network diagnostics and opt-in device discovery contact
the selected targets. Name resolution may contact DNS/mDNS services according to
your system configuration, including optional reverse-name resolution during
discovery. These operations are not telemetry sent to the RustRemote maintainer.

GitHub downloads, websites opened in a browser, system clients and other external
services have their own privacy policies. This policy concerns the RustRemote
application, not data independently processed by GitHub, SignPath, the Microsoft
Store, your operating system or your remote servers. Development/build tools may
also download dependencies when you choose to build the software.

### Support, retention and control

If you contact the maintainer by email or submit a GitHub issue, the information you
choose to send is used to handle that request and retained as needed for support
and project maintenance. Public GitHub issues are publicly visible. Do not include
passwords, private keys, tokens, confidential profiles or sensitive logs.

You control your local files and connections. You can remove profiles, queue files,
settings, downloaded files and any stored keyring entries using the application or
appropriate operating-system tools. Uninstalling the executable does not necessarily
remove those separate files or keyring entries; removing a local profile does not
erase records held by a remote server. Contact the address above about information
you have voluntarily provided to the maintainer, including access or deletion
requests. Third-party platforms may require a separate request under their policies.

### Changes

This policy will be updated if RustRemote's data handling changes. The published
revision date and Git history identify changes. No automatic data collection is
introduced or authorized by this policy.

## Français

### Aucune collecte automatique par le responsable du projet

RustRemote ne transmet au responsable du projet aucune télémétrie, statistique
d'utilisation, identifiant publicitaire, historique de connexion, identifiant
secret, contenu de fichier ou rapport automatique de plantage. L'application
n'intègre ni publicité ni SDK de suivi et ne nécessite ni compte RustRemote ni
serveur exploité par le responsable du projet. Celui-ci ne vend ni ne partage de
données d'utilisation collectées par l'application : elle ne lui en transmet pas.

### Données traitées localement

Les données nécessaires aux fonctions demandées restent sous votre contrôle :
profils (hôtes, adresses IP, utilisateurs, dossiers et options), préférences, files
SFTP persistantes (chemins et états), terminaux, éditeur, fichiers transférés,
presse-papiers et diagnostics locaux. La configuration se trouve normalement dans
`$XDG_CONFIG_HOME/rustremote` ou `~/.config/rustremote`. Le chiffrement des profils
est optionnel ; une donnée locale non chiffrée n'est pas confidentielle par nature.
La file suit le chiffrement du magasin actif. Les anciennes files ne sont pas
actuellement migrées ni supprimées automatiquement lors d'un renommage de profil
ou d'un changement de chiffrement.

Les mots de passe peuvent être conservés pendant une connexion et, si vous choisissez
de les mémoriser, dans le trousseau Secret Service du système. Les clés SSH, l'agent
et les empreintes d'hôtes sont gérés localement selon votre configuration. Les
clients externes et le système peuvent conserver leurs propres réglages, caches,
empreintes ou journaux. Ces éléments ne sont pas envoyés automatiquement au projet.

### Échanges réseau demandés par l'utilisateur

Les connexions SSH/SFTP, RDP, VNC, tunnels, commandes et redirections explicitement
activées transmettent les données nécessaires aux destinations choisies. Les
serveurs distants peuvent enregistrer les comptes, adresses IP et activités selon
leurs règles. Les diagnostics et la découverte réseau volontaire contactent les
cibles sélectionnées. La résolution de noms peut interroger les services DNS/mDNS
configurés par le système, y compris pour les noms inverses optionnels de la
découverte. Ces échanges ne constituent pas une télémétrie vers le projet.

GitHub, SignPath, le Microsoft Store, les sites ouverts dans votre navigateur, le
système et les serveurs distants appliquent leurs propres politiques. La présente
politique couvre l'application RustRemote. Les outils de développement peuvent
aussi télécharger des dépendances si vous construisez le logiciel.

### Assistance, conservation et contrôle

Si vous écrivez au responsable du projet ou publiez une issue GitHub, les données
volontairement fournies servent à traiter votre demande et sont conservées selon
les besoins de l'assistance et de la maintenance. Les issues publiques sont visibles
par tous : n'y publiez aucun secret, profil confidentiel ou journal sensible.

Vous pouvez supprimer vos profils, files, réglages, fichiers et entrées de trousseau
avec l'application ou les outils du système. Désinstaller l'exécutable ne supprime
pas nécessairement ces éléments séparés. Supprimer un profil local n'efface pas les
journaux d'un serveur distant. Pour les informations volontairement adressées au
responsable, notamment une demande d'accès ou de suppression, utilisez le contact
ci-dessus. Les plateformes tierces peuvent nécessiter une démarche distincte.

### Évolutions

La politique sera actualisée si les traitements changent. La date et l'historique
Git permettent de suivre les révisions. Ce texte n'introduit ni n'autorise de
collecte automatique.
