//! Historique des opérations, conservé dans ~/.local/share/flatpak-manager.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Action {
    Created,
    Installed,
    CreatedAndInstalled,
}

impl Action {
    pub fn label(&self) -> &'static str {
        match self {
            Action::Created => tr!("Créé"),
            Action::Installed => tr!("Installé"),
            Action::CreatedAndInstalled => tr!("Créé et installé"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Entry {
    pub timestamp: i64,
    pub action: Action,
    pub name: String,
    pub app_id: String,
    pub version: String,
    pub bundle: Option<PathBuf>,
    pub success: bool,
}

fn file() -> Option<PathBuf> {
    dirs::data_dir().map(|d| d.join("flatpak-manager").join("history.json"))
}

pub fn load() -> Vec<Entry> {
    // Ancien emplacement (avant le renommage en Flatpak Manager) en repli.
    let old = dirs::data_dir().map(|d| d.join("flatpak-creator").join("history.json"));
    file()
        .and_then(|p| {
            std::fs::read_to_string(p)
                .ok()
                .or_else(|| old.and_then(|o| std::fs::read_to_string(o).ok()))
        })
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

pub fn save(entries: &[Entry]) {
    if let Some(p) = file() {
        if let Some(dir) = p.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        if let Ok(s) = serde_json::to_string_pretty(entries) {
            let _ = std::fs::write(p, s);
        }
    }
}
