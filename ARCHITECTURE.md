# Architecture

Flatpak Manager est une seule crate Rust (édition 2024) qui produit un seul binaire. L'interface est en egui/eframe (Wayland uniquement). Tout le travail long (analyse, construction, installation) se fait dans des threads dédiés qui parlent à l'interface par des messages.

## Vue d'ensemble

```
                    ┌──────────────────────────── ui/ ────────────────────────────┐
 main.rs ─────────► │ App (mod.rs) · home · review · run · manage · updates ·     │
 (langue, Wayland,  │ history_view · settings_view · widgets · theme · logo       │
  fenêtre)          │ actions (démarre les tâches) · poll (reçoit leurs messages) │
                    └───┬───────────┬───────────┬───────────┬───────────┬─────────┘
                        │           │           │           │           │
                   analyze/    manifest.rs  pipeline.rs  manager.rs  toolchains/
                  (archive)   (manifeste)  (build/export (liste,    (SDK, runtimes,
                                           /install)     updates)    extensions)
                        │           │           │           │           │
                        └───────────┴─────┬─────┴───────────┴───────────┘
                                          ▼
                                       host.rs  ──►  flatpak, flatpak-builder, xdg-open
                                (direct, ou via flatpak-spawn --host)

 Transverses : i18n/ (tr!) · settings.rs · history.rs · diagnose.rs · chooser.rs
```

Règle de dépendance : l'interface (`ui/`) connaît tous les modules « moteur », mais aucun module moteur ne connaît `ui/`. Seuls `host.rs` et `i18n/` sont utilisés partout. Entre eux, `toolchains`, `manifest` et `pipeline` s'échangent quelques constantes et fonctions utilitaires (`FDO_VERSION`, `slugify`, `pump`…) : ils forment un petit groupe étroitement lié.

## Modules

| Module | Rôle | Dépend de |
|---|---|---|
| `main.rs` | Fixe la langue, vérifie Wayland, sonde l'accès à l'hôte, ouvre la fenêtre. | `i18n`, `settings`, `host`, `ui` |
| `analyze/` | Parcourt l'archive **sans rien extraire** : contrôles de sécurité, SHA-256, détection du programme, de l'icône et des dépendances, verdict. Produit un `ArchiveInfo`. | `i18n` |
| `toolchains/` | Pour une archive de sources : langage et système de build, composants Flatpak requis (runtime, SDK, extensions), état sur la machine, installation de ceux qui manquent. Un fichier par langage (`rust`, `python`, `node`, `java`). | `analyze`, `manifest` (constantes), `pipeline` (lecture de sortie), `diagnose`, `host`, `i18n` |
| `manifest.rs` | Transforme un `ArchiveInfo` et les choix de l'utilisateur (`BuildConfig`, `Permissions`) en manifeste Flatpak, script de lancement, `.desktop` et icône. | `analyze`, `toolchains`, `i18n` |
| `pipeline.rs` | Exécute un `Job` (créer, installer, créer et installer) : `flatpak-builder`, export du `.flatpak`, vérification, installation. Gère la progression et l'annulation. | `analyze`, `manifest`, `toolchains`, `host`, `diagnose`, `i18n` |
| `manager.rs` | Liste les applications installées, désinstalle, retire l'inutilisé, recherche et applique les mises à jour. | `host`, `i18n` |
| `host.rs` | Seul point de lancement des commandes externes. Voir « Exécution sur l'hôte ». | `i18n` |
| `diagnose.rs` | Traduit une sortie d'erreur de Flatpak en explication et en action suggérée. | `i18n` |
| `history.rs` | Historique des opérations, JSON dans `~/.local/share/flatpak-manager/`. | `dirs` |
| `settings.rs` | Paramètres, JSON dans `~/.config/flatpak-manager/settings.json`. Ne connaît pas l'interface. | `i18n` (type `Language`) |
| `chooser.rs` | Sélecteur de fichiers de secours (`zenity`, `kdialog`) quand le portail est absent. | `host` |
| `i18n/` | Macro `tr!`, langue globale, catalogues (`en/`). Le français est la clé. | rien |
| `ui/` | Interface egui, un fichier par écran. | tous |

## Le flux principal

1. **Choix du fichier** (`ui/home`) : l'archive est choisie par le sélecteur, le glisser-déposer ou l'argument de la ligne de commande.
2. **Analyse** (`ui/actions` → `analyze::analyze`) dans un thread. Le SHA-256 est calculé en parallèle. Pour une archive de sources, `toolchains` détermine aussi l'environnement nécessaire.
3. **Revue** (`ui/review`) : l'utilisateur ajuste nom, catégorie, permissions. `manifest` fournit l'aperçu et les avertissements de sécurité (`Permissions::risks`).
4. **Exécution** (`ui/actions` → `pipeline::spawn`) : un thread exécute le `Job` et envoie des `Event` (`Plan`, `Phase`, `Progress`, `Log`, `AwaitConfirm`, `Finished`…) par un canal `mpsc`.
5. **Réception** (`ui/poll`) : à chaque image, l'interface vide le canal, met à jour l'état et l'écran `ui/run`. L'historique est complété à la fin.

Le même schéma (thread + canal + `poll`) sert pour la liste des applications, les mises à jour et l'installation des composants manquants.

## Exécution sur l'hôte

Toutes les commandes externes passent par `host::cmd`. Hors bac à sable, c'est un `Command` ordinaire. Dans un Flatpak (détecté par `/.flatpak-info`), la commande est préfixée par `flatpak-spawn --host --watch-bus` : la commande côté hôte s'arrête si l'application se ferme. Au démarrage, `host::sandbox_blocked` vérifie (5 s maximum) que cet appel est autorisé, sinon l'interface affiche la commande `flatpak override` à copier. Voir la section « Permissions » du [README](README.md).

## Traduction

Le texte français est la clé : `tr!("Texte")` renvoie la traduction de la langue active, ou le français s'il n'y en a pas. Avec des variables : `tr!("Bonjour {nom}", nom = n)`. Les catalogues sont dans `src/i18n/en/`, regroupés par partie du code. Le test `catalogue_et_sources_concordent` parcourt `src/**/*.rs` et échoue si un texte du code est absent du catalogue, si une traduction est vide ou si deux traductions se contredisent.

## Données écrites sur le disque

| Chemin | Contenu |
|---|---|
| `~/.config/flatpak-manager/settings.json` | paramètres (écriture atomique : fichier temporaire puis renommage) |
| `~/.local/share/flatpak-manager/history.json` | historique |
| `~/.cache/flatpak-manager/build/<nom>-<date>/` | dossier de travail d'une construction, supprimé à la fin |
| `~/.cache/flatpak-manager/demarrage.log` | erreurs de démarrage (quand aucune fenêtre n'a pu s'ouvrir) |

## Tests

Les tests sont dans le module concerné (`#[cfg(test)]`) ou dans `tests.rs` pour les gros modules (`analyze/`, `toolchains/`). Ils ne lancent ni Flatpak ni fenêtre : l'analyse est testée sur des archives construites en mémoire, le manifeste sur des structures. L'interface graphique n'est pas testée automatiquement.
