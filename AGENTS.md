# Codex — RustRemote

Projet : logiciel Linux natif en Rust, inspiré des usages d'un client SSH/SFTP/RDP multiprotocole, **créé indépendamment de MobaXterm**. Réponses en français.

## Reprise obligatoire
1. Lire `MEMORY.md` et `docs/ROADMAP.md` ; lire ensuite les sections pertinentes de `docs/PROJECT.md`, `docs/ARCHITECTURE.md`, `docs/SECURITY.md`.
2. Constater l'état du dépôt avant d'agir : `git status --short`, fichiers concernés, tests connus ; ne pas supposer l'historique du chat de Claude.
3. Choisir la première tâche prioritaire ouverte ; implémenter le plus petit incrément fonctionnel testable.
4. Vérifier : `cargo fmt --all -- --check`, `cargo check --workspace`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace` si le workspace existe et les dépendances sont disponibles. Documenter les résultats sans mentir.
5. Mettre à jour `MEMORY.md` et les statuts de `docs/ROADMAP.md` avant de terminer.

## Contraintes non négociables
- Rust pour le code applicatif ; GTK4/Relm4, Russh et IronRDP sont des candidats, pas des intégrations acquises.
- Découpler protocoles, UI, stockage et authentification. Interfaces testables ; pas de code complexe sans tests adaptés.
- Jamais de secrets (clé, token, mot de passe, credentials.txt, `.env` réel) dans le code, les patches, les journaux ou les fichiers Markdown partagés.
- Préférer keyring et SSH Agent, vérifier known_hosts et certificats, aucun `StrictHostKeyChecking=no`, aucun contournement TLS.
- Automatiser les actions locales sans danger (Rust/Python/Node/sed, format/lint/test) ; demander accord avant toute commande destructive, connexion sur infrastructure réelle ou changement externe.
- Ne pas importer de code propriétaire de MobaXterm ; vérifier licences, vulnérabilités, support et API des dépendances.
- Aucun fichier ou abstraction inutile. Ne pas écraser les changements non liés présents dans le dépôt.
- L'image fournie est une maquette conceptuelle ; ne pas déclarer le produit achevé.
- Une seule personne/agent écrit à la fois ; passer la main avec état vérifiable dans le dépôt.
