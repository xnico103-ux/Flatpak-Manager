//! Traduction de l'interface.
//!
//! Le français est la langue source : le texte français sert de clé. Pour traduire un
//! texte, on l'écrit avec la macro `tr!` ; le catalogue de chaque autre langue (dossier
//! `en/` pour l'anglais) associe ce texte français à sa traduction. Un texte absent du
//! catalogue s'affiche en français : l'application ne plante jamais à cause d'une
//! traduction manquante, et le test `catalogue_et_sources_concordent` la signale.
//!
//! ```ignore
//! tr!("Mises à jour")                                  // -> &'static str
//! tr!("Tout mettre à jour ({n})", n = total)           // -> String
//! ```
//!
//! Les `{nom}` du texte français sont remplacés par les arguments nommés. Ils doivent
//! figurer à l'identique dans la traduction. Les formats (`{:.1}`, `{:?}`…) ne sont pas
//! gérés : on formate la valeur avant (`n = format!("{:.1}", x)`).
//!
//! Pour ajouter une langue : une variante dans `Language`, un dossier de catalogue
//! comme `en/`, et une ligne dans `catalogue()`.

mod en;

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::OnceLock;
use std::sync::atomic::{AtomicU8, Ordering};

/// Traduit un texte français. Sans argument : renvoie un `&'static str`.
/// Avec des arguments nommés : renvoie un `String` où les `{nom}` sont remplacés.
macro_rules! tr {
    ($key:literal) => {
        $crate::i18n::translate($key)
    };
    ($key:literal, $($name:ident = $val:expr),+ $(,)?) => {
        $crate::i18n::translate_with($key, &[$((stringify!($name), $val.to_string())),+])
    };
}

/// Langue choisie dans les paramètres. `Auto` suit la langue du système.
/// Une valeur inconnue dans le fichier (version future, édition à la main) retombe sur `Auto`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Language {
    Fr,
    En,
    /// Doit rester la dernière variante : `serde(other)` l'exige.
    #[default]
    #[serde(other)]
    Auto,
}

impl Language {
    pub const ALL: [Language; 3] = [Language::Auto, Language::Fr, Language::En];

    /// Nom affiché dans le sélecteur. Les langues portent leur propre nom : on doit
    /// pouvoir retrouver la sienne même quand l'interface est dans une langue inconnue.
    pub fn label(self) -> &'static str {
        match self {
            Language::Auto => tr!("Automatique (système)"),
            Language::Fr => "Français",
            Language::En => "English",
        }
    }

    /// Langue réellement utilisée (jamais `Auto`).
    fn resolved(self) -> Language {
        match self {
            Language::Auto => from_locale(&system_locale()),
            other => other,
        }
    }

    fn code(self) -> u8 {
        match self {
            Language::En => 1,
            _ => 0,
        }
    }
}

/// Langue active, lue par `translate` depuis n'importe quel thread.
static ACTIVE: AtomicU8 = AtomicU8::new(0);

/// Applique la langue choisie (résout `Auto` d'après le système).
pub fn set_language(choice: Language) {
    ACTIVE.store(choice.resolved().code(), Ordering::Relaxed);
}

/// Langue active : `Fr` ou `En`.
pub fn active() -> Language {
    match ACTIVE.load(Ordering::Relaxed) {
        1 => Language::En,
        _ => Language::Fr,
    }
}

/// Déduit la langue d'un nom de locale (`fr_FR.UTF-8`, `en_US`, `C`…).
/// Le français est reconnu ; tout le reste donne l'anglais.
fn from_locale(locale: &str) -> Language {
    let l = locale.trim().to_ascii_lowercase();
    if l.starts_with("fr") {
        Language::Fr
    } else {
        Language::En
    }
}

/// Locale du système : première variable non vide parmi les variables usuelles.
fn system_locale() -> String {
    ["LC_ALL", "LC_MESSAGES", "LANGUAGE", "LANG"]
        .iter()
        .filter_map(|v| std::env::var(v).ok())
        .find(|v| !v.trim().is_empty())
        .unwrap_or_default()
}

fn catalogue(lang: Language) -> Option<&'static HashMap<&'static str, &'static str>> {
    static EN: OnceLock<HashMap<&'static str, &'static str>> = OnceLock::new();
    match lang {
        Language::En => Some(EN.get_or_init(|| en::entries().collect())),
        _ => None,
    }
}

/// Texte dans la langue active (le texte français lui-même quand il n'y a pas de traduction).
pub fn translate(key: &'static str) -> &'static str {
    catalogue(active())
        .and_then(|c| c.get(key).copied())
        .unwrap_or(key)
}

/// Comme `translate`, avec remplacement des `{nom}`. Un `{nom}` sans argument reste tel quel.
pub fn translate_with(key: &'static str, args: &[(&str, String)]) -> String {
    let text = translate(key);
    let mut out = String::with_capacity(text.len() + 16);
    let mut rest = text;
    while let Some(start) = rest.find('{') {
        out.push_str(&rest[..start]);
        let after = &rest[start + 1..];
        match after.find('}') {
            Some(end) => {
                let name = &after[..end];
                match args.iter().find(|(n, _)| *n == name) {
                    Some((_, value)) => out.push_str(value),
                    None => {
                        out.push('{');
                        out.push_str(name);
                        out.push('}');
                    }
                }
                rest = &after[end + 1..];
            }
            None => {
                out.push_str(&rest[start..]);
                rest = "";
            }
        }
    }
    out.push_str(rest);
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;
    use std::path::Path;

    #[test]
    fn locale_detection() {
        assert_eq!(from_locale("fr_FR.UTF-8"), Language::Fr);
        assert_eq!(from_locale("FR"), Language::Fr);
        assert_eq!(from_locale("fr_CA"), Language::Fr);
        assert_eq!(from_locale("en_US.UTF-8"), Language::En);
        assert_eq!(from_locale("de_DE"), Language::En);
        assert_eq!(from_locale("C"), Language::En);
        assert_eq!(from_locale(""), Language::En);
    }

    #[test]
    fn unknown_language_in_settings_falls_back_to_auto() {
        let l: Language = serde_json::from_str("\"klingon\"").unwrap();
        assert_eq!(l, Language::Auto);
        let l: Language = serde_json::from_str("\"en\"").unwrap();
        assert_eq!(l, Language::En);
        assert_eq!(serde_json::to_string(&Language::Fr).unwrap(), "\"fr\"");
    }

    #[test]
    fn placeholders_are_replaced() {
        let args = [("n", "3".to_string()), ("nom", "Zed".to_string())];
        assert_eq!(
            translate_with("{n} élément(s) — {nom}", &args),
            "3 élément(s) — Zed"
        );
        assert_eq!(
            translate_with("sans argument {x}", &args),
            "sans argument {x}"
        );
        assert_eq!(
            translate_with("accolade { ouverte", &args),
            "accolade { ouverte"
        );
        assert_eq!(translate_with("fin {n}", &args), "fin 3");
    }

    #[test]
    fn macro_forms_in_both_languages() {
        set_language(Language::Fr);
        assert_eq!(tr!("Paramètres"), "Paramètres");
        assert_eq!(
            tr!("Historique : {n} entrée(s)", n = 2),
            "Historique : 2 entrée(s)"
        );
        set_language(Language::En);
        assert_eq!(tr!("Paramètres"), "Settings");
        assert_eq!(
            tr!("Historique : {n} entrée(s)", n = 2),
            "History: 2 entry(ies)"
        );
        // Texte sans traduction : on retombe sur le français.
        assert_eq!(
            translate("Texte inconnu du catalogue"),
            "Texte inconnu du catalogue"
        );
        set_language(Language::Fr);
    }

    // ---- Cohérence entre le code et les catalogues ----

    /// Décode le contenu d'un littéral de chaîne Rust (jusqu'au guillemet fermant).
    fn read_literal(src: &str) -> Option<String> {
        let mut out = String::new();
        let mut chars = src.chars().peekable();
        while let Some(c) = chars.next() {
            match c {
                '"' => return Some(out),
                '\\' => match chars.next()? {
                    'n' => out.push('\n'),
                    't' => out.push('\t'),
                    'r' => out.push('\r'),
                    '0' => out.push('\0'),
                    '\\' => out.push('\\'),
                    '"' => out.push('"'),
                    '\'' => out.push('\''),
                    '\n' => {
                        // Continuation de ligne : on saute l'indentation suivante.
                        while chars.peek().is_some_and(|c| c.is_whitespace()) {
                            chars.next();
                        }
                    }
                    other => panic!("échappement non géré dans un tr! : \\{other}"),
                },
                c => out.push(c),
            }
        }
        None
    }

    fn rust_files(dir: &Path, out: &mut Vec<std::path::PathBuf>) {
        for e in std::fs::read_dir(dir).unwrap().flatten() {
            let p = e.path();
            if p.is_dir() {
                // Le catalogue lui-même n'est pas du code à analyser.
                if p.ends_with("i18n/en") {
                    continue;
                }
                rust_files(&p, out);
            } else if p.extension().is_some_and(|x| x == "rs") {
                out.push(p);
            }
        }
    }

    /// Tous les textes passés à `tr!` dans le code source (hors dossier i18n).
    fn keys_in_sources() -> BTreeSet<String> {
        let mut files = Vec::new();
        rust_files(
            &Path::new(env!("CARGO_MANIFEST_DIR")).join("src"),
            &mut files,
        );
        let mut keys = BTreeSet::new();
        // Écrit en deux morceaux pour que ce fichier ne se détecte pas lui-même.
        let pattern = concat!("tr", "!(");
        for f in files {
            // Les commentaires (dont les exemples de la documentation) ne comptent pas.
            let text: String = std::fs::read_to_string(&f)
                .unwrap()
                .lines()
                .filter(|l| !l.trim_start().starts_with("//"))
                .collect::<Vec<_>>()
                .join("\n");
            let mut from = 0;
            while let Some(i) = text[from..].find(pattern) {
                let at = from + i;
                from = at + 4;
                // `str!(` ou `xtr!(` ne sont pas notre macro.
                if text[..at]
                    .chars()
                    .next_back()
                    .is_some_and(|c| c.is_alphanumeric() || c == '_')
                {
                    continue;
                }
                let arg = text[from..].trim_start();
                if let Some(lit) = arg.strip_prefix('"') {
                    let key = read_literal(lit)
                        .unwrap_or_else(|| panic!("littéral non terminé dans {}", f.display()));
                    keys.insert(key);
                }
            }
        }
        keys
    }

    fn placeholders(s: &str) -> BTreeSet<String> {
        let mut set = BTreeSet::new();
        let mut rest = s;
        while let Some(i) = rest.find('{') {
            let after = &rest[i + 1..];
            let Some(j) = after.find('}') else { break };
            set.insert(after[..j].to_string());
            rest = &after[j + 1..];
        }
        set
    }

    #[test]
    fn catalogue_et_sources_concordent() {
        let used = keys_in_sources();
        let en: Vec<(&str, &str)> = en::entries().collect();

        let missing: Vec<&String> = used
            .iter()
            .filter(|k| !en.iter().any(|(fr, _)| fr == &k.as_str()))
            .collect();
        assert!(
            missing.is_empty(),
            "textes passés à tr! sans traduction anglaise :\n{missing:#?}"
        );

        let orphans: Vec<&str> = en
            .iter()
            .map(|(fr, _)| *fr)
            .filter(|fr| !used.contains(*fr))
            .collect();
        assert!(
            orphans.is_empty(),
            "entrées du catalogue absentes du code (faute de frappe ou texte supprimé) :\n{orphans:#?}"
        );
    }

    #[test]
    fn catalogue_anglais_sain() {
        let mut seen: HashMap<&str, &str> = HashMap::new();
        for (fr, en) in en::entries() {
            assert!(!en.trim().is_empty(), "traduction vide pour : {fr}");
            assert_eq!(
                placeholders(fr),
                placeholders(en),
                "les {{noms}} diffèrent entre le français et l'anglais :\n  {fr}\n  {en}"
            );
            if let Some(prev) = seen.insert(fr, en) {
                assert_eq!(prev, en, "deux traductions différentes pour : {fr}");
            }
        }
    }
}
