//! Chemins d'archive, noms de fichiers et fichiers .desktop : fonctions pures.

use super::model::DesktopInfo;
use std::path::{Component, Path};

/// Normalise un chemin d'archive. Erreur si le chemin sort de la racine.
pub fn clean_path(p: &Path) -> std::result::Result<Vec<String>, String> {
    let mut parts = Vec::new();
    for c in p.components() {
        match c {
            Component::Normal(s) => parts.push(s.to_string_lossy().into_owned()),
            Component::CurDir => {}
            Component::ParentDir => return Err(tr!("contient « .. »").into()),
            Component::RootDir | Component::Prefix(_) => return Err(tr!("chemin absolu").into()),
        }
    }
    Ok(parts)
}

/// Vrai si la cible d'un lien sort de l'archive (ou est absolue).
pub(super) fn link_escapes(entry_parts: &[String], target: &Path, hard: bool) -> bool {
    let mut stack: Vec<String> = if hard {
        Vec::new()
    } else {
        entry_parts[..entry_parts.len().saturating_sub(1)].to_vec()
    };
    for c in target.components() {
        match c {
            Component::Normal(s) => stack.push(s.to_string_lossy().into_owned()),
            Component::CurDir => {}
            Component::ParentDir => {
                if stack.pop().is_none() {
                    return true;
                }
            }
            _ => return true,
        }
    }
    false
}

pub(super) fn is_lib_name(name: &str) -> bool {
    name.ends_with(".so") || name.contains(".so.")
}

pub(super) fn norm(s: &str) -> String {
    s.chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .collect::<String>()
        .to_lowercase()
}

pub(super) fn parse_desktop(text: &str) -> DesktopInfo {
    let mut d = DesktopInfo::default();
    let mut in_entry = false;
    for line in text.lines() {
        let line = line.trim();
        if line.starts_with('[') {
            in_entry = line == "[Desktop Entry]";
            continue;
        }
        if !in_entry {
            continue;
        }
        if let Some((k, v)) = line.split_once('=') {
            let v = v.trim().to_string();
            match k.trim() {
                "Name" => d.name = Some(v),
                "Comment" => d.comment = Some(v),
                "Exec" => d.exec = Some(v),
                "Icon" => d.icon = Some(v),
                "Categories" => d.categories = Some(v),
                "Terminal" => d.terminal = v.eq_ignore_ascii_case("true"),
                _ => {}
            }
        }
    }
    d
}

pub(super) fn desktop_exec_base(d: &DesktopInfo) -> Option<String> {
    let exec = d.exec.as_ref()?;
    let first = exec
        .split_whitespace()
        .find(|t| !t.contains('=') && *t != "env")?
        .trim_matches('"');
    Path::new(first)
        .file_name()
        .map(|s| s.to_string_lossy().into_owned())
}

/// Devine un nom et une version à partir d'un nom d'archive.
pub fn guess_name_version(file_name: &str) -> (String, String) {
    let mut stem = file_name.to_string();
    for ext in [
        ".tar.gz", ".tgz", ".tar.xz", ".txz", ".tar.zst", ".tar.bz2", ".tbz2",
    ] {
        if let Some(s) = stem.strip_suffix(ext) {
            stem = s.to_string();
            break;
        }
    }
    const NOISE: &[&str] = &[
        "linux", "x64", "x86", "x86_64", "amd64", "gnu", "portable", "bin", "64bit", "64",
        "release", "aarch64", "arm64", "generic", "static",
    ];
    let mut name = Vec::new();
    let mut version = String::new();
    for tok in stem.split(['-', '_', ' ']) {
        if tok.is_empty() {
            continue;
        }
        let lower = tok.to_lowercase();
        let looks_version = lower.chars().next().is_some_and(|c| c.is_ascii_digit())
            || (lower.starts_with('v') && lower.chars().nth(1).is_some_and(|c| c.is_ascii_digit()));
        if version.is_empty() && looks_version {
            version = tok.trim_start_matches(['v', 'V']).to_string();
        } else if NOISE.contains(&lower.as_str()) {
            continue;
        } else if version.is_empty() {
            name.push(tok.to_string());
        }
    }
    let name = if name.is_empty() {
        stem.clone()
    } else {
        name.join(" ")
    };
    (
        name,
        if version.is_empty() {
            "1.0".into()
        } else {
            version
        },
    )
}

pub(super) fn clean_name(n: &str) -> String {
    let t = n.trim();
    if t.is_empty() {
        "Application".into()
    } else {
        t.to_string()
    }
}
