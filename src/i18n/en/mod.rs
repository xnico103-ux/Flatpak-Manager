//! Catalogue anglais : texte français -> traduction.
//!
//! Un fichier par zone du code, pour que chaque fichier reste lisible. Chaque fichier expose
//! `ENTRIES: &[(&str, &str)]` ; le texte français doit être identique à celui passé à `tr!`.

mod analyze;
mod engine;
mod manifest;
mod settings;
mod ui_chrome;
mod ui_flow;

/// Toutes les paires (français, anglais) de tous les fichiers.
pub fn entries() -> impl Iterator<Item = (&'static str, &'static str)> {
    [
        analyze::ENTRIES,
        engine::ENTRIES,
        manifest::ENTRIES,
        settings::ENTRIES,
        ui_chrome::ENTRIES,
        ui_flow::ENTRIES,
    ]
    .into_iter()
    .flatten()
    .copied()
}
