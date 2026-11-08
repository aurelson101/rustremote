# Claude Code — RustRemote

Tu travailles dans **RustRemote**, un projet Linux natif Rust, indépendant de MobaXterm. Travaille en français. **Le pack contient une maquette, aucun programme développé à ce stade.**

## À chaque reprise (OBLIGATOIRE)
1. Lire `MEMORY.md`, puis `docs/ROADMAP.md`. Consulter `docs/PROJECT.md`, `docs/ARCHITECTURE.md` et `docs/SECURITY.md` uniquement selon le besoin.
2. Vérifier l'état réel du dépôt (`git status --short`, arborescence ciblée, fichiers touchés, tests possibles), sans scan massif.
3. Identifier la première tâche prioritaire non achevée. Exécuter **une tranche testable** ; ne pas générer tout le produit d'un seul coup.
4. Exécuter les contrôles applicables. Noter les résultats réels, modifications, décisions et blocages dans `MEMORY.md`. Actualiser les statuts de `docs/ROADMAP.md`.
5. Rendre une synthèse courte : fait, tests, fichiers modifiés, prochaine action.

## Invariants
- Rust pour le code applicatif ; GTK4 + Relm4 envisagé, décision finale sur PoC. Toute dépendance doit être vérifiée avant fixation de version.
- Séparer UI, modèles, services et protocoles. Pas de logique SSH/RDP directement dans les widgets.
- **Jamais de secrets en Git, dans les prompts, logs, issues, tests ou MEMORY.md**. Privilégier SSH Agent et Keyring OS ; fichiers d'identifiants éventuellement fournis par l'utilisateur = lecture locale seule et jamais recopiés.
- Refuser le contournement de la vérification d'hôte SSH et des certificats TLS/RDP ; aucune commande distante sans intention utilisateur explicite.
- Les commandes shell sûres et locales (Rust, Python, Node, sed, format, tests, build) sont exécutées de façon autonome lorsqu'elles servent la tâche ; pas de déploiement, suppression de données, ajout de clés ou accès à un système distant sans autorisation.
- Pas de duplication de code, pas de fichiers de notes multiples, pas de dépendances superflues, pas de refonte hors périmètre.
- Ne pas inventer d'état réalisé : la maquette est un visuel de référence, pas une preuve.
- Si une instruction utilisateur plus récente contredit la documentation, prioriser l'instruction, consigner la décision, mettre à jour les documents concernés.
