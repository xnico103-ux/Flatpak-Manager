# Contribuer à Flatpak Manager

Merci de votre intérêt. Les contributions (corrections, traductions, idées) sont les bienvenues. Le projet est sous licence GPL-3.0-or-later : en proposant du code, vous acceptez qu'il soit distribué sous cette licence.

## Avant de commencer

- Pour un changement important, ouvrez d'abord une *issue* pour en discuter.
- Lisez [ARCHITECTURE.md](ARCHITECTURE.md) : il explique quel module fait quoi.

## Environnement

- Rust : la version est fixée dans `rust-toolchain.toml` (rustup l'installe seul).
- Bibliothèques système (Debian/Ubuntu) : `libwayland-dev libxkbcommon-dev libvulkan-dev`.
- Pour essayer l'application : une session Wayland, `flatpak`, et un `xdg-desktop-portal`.

```bash
cargo run --release
```

L'application refuse de démarrer sans `WAYLAND_DISPLAY`.

## Avant d'ouvrir une *pull request*

La CI lance ces trois commandes ; passez-les en local :

```bash
cargo fmt --check
cargo clippy --all-targets --locked -- -D warnings
cargo test --locked
```

Gardez les commits petits, avec un message qui dit ce qui change et pourquoi. Un changement de comportement s'accompagne d'un test quand c'est possible.

## Conventions

- **Un fichier, une responsabilité.** Au-delà de quelques centaines de lignes, pensez à découper en sous-modules (voir `analyze/` et `toolchains/`).
- **Pas d'`unwrap()` ni d'`expect()`** dans le code de production : renvoyez une erreur (`anyhow`) ou prévoyez un repli.
- **Toute commande externe passe par `host::cmd`**, jamais par `Command::new("flatpak")` directement, sinon elle échouera dans un bac à sable.
- **L'interface ne contient pas de logique métier** : elle appelle les modules moteur et affiche leurs résultats. Les tâches longues tournent dans un thread et répondent par un canal.
- **Toute action destructive** affiche la commande exacte et demande une confirmation.
- Commentaires et documentation en français, comme le reste du projet.

## Ajouter ou modifier un texte

Tout texte visible passe par `tr!` avec le **français** comme clé :

```rust
ui.label(tr!("Installer l'application"));
ui.label(tr!("{n} paquets mis à jour", n = count));
```

Ajoutez ensuite la traduction anglaise dans le fichier correspondant de `src/i18n/en/`. Le test `catalogue_et_sources_concordent` vous dira si un texte manque ou est en double.

Pour ajouter une langue : créez un dossier à côté de `src/i18n/en/`, ajoutez la valeur dans `Language` (`src/i18n/mod.rs`) et son nom dans l'écran Paramètres.

## Ajouter un langage de construction

Un langage de sources = un fichier dans `src/toolchains/` (détection et composants requis) et les modules de génération correspondants dans `manifest.rs`. Inspirez-vous de `node.rs` ou `python.rs`, puis mettez à jour le tableau « Ce qui peut être converti » du README.

## Signaler un problème

Indiquez votre distribution et votre bureau, si l'application tourne en Flatpak ou non, la version (écran Paramètres › À propos) et le journal affiché dans l'écran d'exécution. Ne joignez pas d'archive que vous n'avez pas le droit de partager.
