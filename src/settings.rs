//! Paramètres de l'application, conservés dans ~/.config/flatpak-manager/settings.json.
//!
//! Ce module ne connaît pas l'interface : il lit, valide et écrit un fichier JSON.
//! Un fichier absent, illisible ou partiel donne simplement les valeurs par défaut.

use crate::i18n::Language;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

pub const SCALE_MIN: f32 = 0.8;
pub const SCALE_MAX: f32 = 1.6;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    /// Affiche les détails et réglages fins.
    pub advanced: bool,
    /// Installation système par défaut (pour tous les utilisateurs).
    pub system_install: bool,
    /// Remplacer par défaut une installation existante.
    pub replace_existing: bool,
    /// Demander confirmation avant d'installer (mode avancé).
    pub confirm_before_install: bool,
    /// Facteur de zoom de l'interface.
    pub ui_scale: f32,
    /// Langue de l'interface (`Auto` : celle du système).
    pub language: Language,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            advanced: false,
            system_install: false,
            replace_existing: false,
            confirm_before_install: false,
            ui_scale: 1.0,
            language: Language::Auto,
        }
    }
}

impl Settings {
    /// Ramène les valeurs dans leurs bornes (fichier édité à la main, valeur NaN…).
    pub fn sanitized(mut self) -> Self {
        self.ui_scale = if self.ui_scale.is_finite() {
            self.ui_scale.clamp(SCALE_MIN, SCALE_MAX)
        } else {
            1.0
        };
        self
    }

    pub fn path() -> Option<PathBuf> {
        dirs::config_dir().map(|d| d.join("flatpak-manager").join("settings.json"))
    }

    pub fn load() -> Self {
        Self::path()
            .and_then(|p| std::fs::read_to_string(p).ok())
            .map(|s| Self::from_json(&s))
            .unwrap_or_default()
    }

    pub fn save(&self) {
        let Some(p) = Self::path() else { return };
        if let Some(dir) = p.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        if let Ok(s) = serde_json::to_string_pretty(self) {
            // Écriture dans un fichier temporaire puis renommage : jamais de fichier tronqué.
            let tmp = p.with_extension("json.tmp");
            if std::fs::write(&tmp, s).is_ok() {
                let _ = std::fs::rename(&tmp, &p);
            }
        }
    }

    fn from_json(s: &str) -> Self {
        serde_json::from_str::<Self>(s)
            .unwrap_or_default()
            .sanitized()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn invalid_or_partial_files_give_defaults() {
        assert_eq!(Settings::from_json("pas du json"), Settings::default());
        assert_eq!(Settings::from_json(""), Settings::default());
        let s = Settings::from_json(r#"{"advanced": true}"#);
        assert!(s.advanced);
        assert_eq!(s.ui_scale, 1.0);
    }

    #[test]
    fn scale_is_clamped() {
        assert_eq!(
            Settings::from_json(r#"{"ui_scale": 9.0}"#).ui_scale,
            SCALE_MAX
        );
        assert_eq!(
            Settings::from_json(r#"{"ui_scale": 0.1}"#).ui_scale,
            SCALE_MIN
        );
        assert_eq!(Settings::from_json(r#"{"ui_scale": null}"#).ui_scale, 1.0);
    }

    #[test]
    fn roundtrip() {
        let s = Settings {
            advanced: true,
            system_install: true,
            ui_scale: 1.25,
            language: Language::En,
            ..Settings::default()
        };
        let json = serde_json::to_string(&s).unwrap();
        assert_eq!(Settings::from_json(&json), s);
    }
}
