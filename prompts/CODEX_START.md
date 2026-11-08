# Prompt à coller dans Codex (dans la racine du dépôt RustRemote)

Prends en charge le développement de **RustRemote** dans CE dépôt. Respecte `AGENTS.md`, puis lis impérativement `MEMORY.md` et `docs/ROADMAP.md`. Lis `docs/PROJECT.md`, `docs/ARCHITECTURE.md` et `docs/SECURITY.md` pour les seules sections nécessaires. La seule source d'historique partageable avec Claude Code est **le dépôt**, en particulier `MEMORY.md`. Ne suppose rien de l'état du chat précédent.

**But :** exécuter le prochain incrément non achevé de priorité maximale, en modifiant effectivement le code et en vérifiant les résultats. Si le workspace n'existe pas encore, commencer par P0-01/P0-02 et créer le minimum de fichiers nécessaires. L'interface doit correspondre à l'esprit de la maquette `assets/maquette-rustremote.png` sans reprendre des éléments propriétaires. Application Linux native, logique Rust, GUI GTK4/Relm4 à valider, SSH Russh, SFTP, terminal interactif, puis IronRDP/VNC/KVM selon roadmap.

**Contraintes de sécurité :** ne jamais stocker ni exposer des mots de passe/jetons/clés ; ne jamais accepter silencieusement un hôte SSH inconnu, ne jamais ignorer une erreur TLS/RDP ; ne pas supposer que VKC = VNC. Pas d'action destructive ni de connexion à un système réel sans validation. Aucun fork/copiage du code propriétaire MobaXterm. Conserve la compatibilité d'un dépôt travaillé par deux agents alternés.

**Économie de contexte :** `git status`, recherche ciblée, lecture strictement utile, modifications minimales, pas de gros refactoring non demandé, pas de fichiers documentation redondants. Exécuter automatiquement les commandes locales sûres de compilation, tests, lint et format ; tracer uniquement le résultat utile.

**Handoff obligatoire :** actualise `MEMORY.md` avec date, agent, tâche, fichiers modifiés, commandes de test et véritables résultats, blocages, décision prise et prochaine action précise ; synchronise `docs/ROADMAP.md`. Réponds brièvement avec éléments vérifiés, sans déclarer une fonctionnalité aboutie si elle ne l'est pas.
