//! Petites fonctions partagées par la détection et les modules de langage.

use super::model::{BuildSystem, Entry, SourceProject};
use crate::manifest::slugify;

pub(super) fn valid_bin(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 80
        && !s.starts_with('-')
        && s.chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '+' | '-'))
}

/// Premier argument d'un appel `nom(...)`, sans guillemets ni crochets.
pub(super) fn call_arg(text: &str, call: &str) -> Option<String> {
    let low = text.to_ascii_lowercase();
    let needle = format!("{call}(");
    let mut from = 0;
    while let Some(i) = low[from..].find(&needle) {
        let start = from + i;
        // Le nom doit commencer un mot (pas `my_project(`).
        let boundary = start == 0
            || !low.as_bytes()[start - 1].is_ascii_alphanumeric()
                && low.as_bytes()[start - 1] != b'_';
        if boundary {
            let rest = &text[start + needle.len()..];
            let rest = rest
                .trim_start_matches(|c: char| c.is_whitespace() || matches!(c, '\'' | '"' | '['));
            let arg: String = rest
                .chars()
                .take_while(|c| !c.is_whitespace() && !matches!(c, ',' | '\'' | '"' | ')' | ']'))
                .collect();
            if !arg.is_empty() {
                return Some(arg);
            }
        }
        from = start + needle.len();
    }
    None
}

pub(super) fn any_of(text: &str, words: &[&str]) -> bool {
    let low = text.to_ascii_lowercase();
    words.iter().any(|w| low.contains(w))
}

pub(super) fn pick(name: Option<String>, fallback: &str) -> (String, bool) {
    match name.filter(|n| valid_bin(n) && !n.contains('$')) {
        Some(n) => (n, true),
        None => (slugify(fallback), false),
    }
}

pub(super) fn project(
    system: BuildSystem,
    bin: (String, bool),
    content: &str,
    gui_words: &[&str],
) -> SourceProject {
    SourceProject {
        system,
        bin_name: bin.0,
        bin_certain: bin.1,
        gui: any_of(content, gui_words),
        devices: false,
        network: false,
        qt: any_of(
            content,
            &[
                "qt5",
                "qt6",
                "find_package(qt",
                "dependency('qt",
                "dependency(\"qt",
            ],
        ),
        gtk: any_of(
            content,
            &[
                "gtk+-3",
                "gtk4",
                "gtk3",
                "find_package(gtk",
                "dependency('gtk",
                "dependency(\"gtk",
            ],
        ),
        notes: vec![],
        blocker: None,
        entry: Entry::None,
        alternatives: vec![],
        installable: false,
        requirements: false,
        build_script: false,
    }
}

pub(super) fn is_js(p: &str) -> bool {
    let l = p.to_ascii_lowercase();
    l.ends_with(".js") || l.ends_with(".mjs") || l.ends_with(".cjs")
}

/// Chemin sans « ./ » initial.
pub(super) fn clean_rel(p: &str) -> String {
    p.trim().trim_start_matches("./").to_string()
}

pub(super) fn root_names(src: &[String], ext: fn(&str) -> bool) -> Vec<&String> {
    src.iter().filter(|p| !p.contains('/') && ext(p)).collect()
}

pub(super) fn find_ci<'a>(list: &[&'a String], name: &str) -> Option<&'a String> {
    list.iter().find(|p| p.eq_ignore_ascii_case(name)).copied()
}

pub(super) fn contains_any(text: &str, words: &[&str]) -> bool {
    any_of(text, words)
}
