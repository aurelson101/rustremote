# Prompt de reprise Claude Code ↔ Codex

Reprends le projet **RustRemote** depuis l'état réel du dépôt, **pas depuis les souvenirs d'une précédente conversation**. Lis d'abord `MEMORY.md`, les statuts de `docs/ROADMAP.md`, puis `CLAUDE.md` si tu es Claude Code ou `AGENTS.md` si tu es Codex. Vérifie `git status --short`, les derniers fichiers modifiés, les éventuels tests et les contradictions entre mémoire et code.

Résume en 4 lignes maximum ce qui est effectivement terminé, le blocage principal et la prochaine tâche. **Ensuite, effectue cette prochaine tâche** par un incrément testable, avec vérifications locales sûres. Ne refais pas le travail validé. Ne détruis pas les changements non liés. Ne lis ni n'affiche de secrets.

Avant de terminer, synchronise `MEMORY.md` et `docs/ROADMAP.md` avec preuves des tests, fichiers affectés, décisions durables et **la prochaine action exacte** pour l'autre agent. Un seul agent doit modifier le dépôt à la fois.
