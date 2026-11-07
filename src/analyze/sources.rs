//! Archive de sources : quels fichiers de build, de code et de JAR décrivent le projet,
//! et quel système de build en ressort.

use super::io::read_member;
use super::model::*;
use super::scan::{Layout, Scan};
use super::sniff::jar_has_main_class;
use crate::toolchains::{self, BuildFile, Detection, SourceProject};
use std::path::Path;

/// Cherche un projet à compiler. Renvoie le projet pris en charge, ou le nom du langage
/// reconnu mais non pris en charge. Seuls les fichiers de build à la racine du projet comptent.
pub(super) fn detect_source(
    path: &Path,
    scan: &Scan,
    layout: &Layout,
    executables: &[ExecCandidate],
    guess_name: &str,
) -> (Option<SourceProject>, Option<&'static str>) {
    let top_dir = &layout.top_dir;
    let build_raw = &scan.build_raw;
    let src_raw = &scan.src_raw;
    let jar_raw = &scan.jar_raw;
    let root_len = if top_dir.is_some() { 2 } else { 1 };
    let mut root_build: Vec<BuildFile> = build_raw
        .iter()
        .filter(|(p, _)| p.len() == root_len)
        .map(|(_, f)| f.clone())
        .collect();
    // Espace de travail Cargo : les Cargo.toml des sous-dossiers (gui/, cli/…) disent si le projet est graphique.
    if root_build.iter().any(|f| f.name == "cargo.toml") {
        for (p, f) in build_raw {
            if p.len() == root_len + 1 && f.name == "cargo.toml" {
                root_build.push(BuildFile {
                    name: "member:cargo.toml".into(),
                    content: f.content.clone(),
                });
            }
        }
    }
    // Un ELF isolé au fond d'un projet (outil de test, bibliothèque fournie) ne fait pas de l'archive une application déjà compilée.
    let has_elf = executables.iter().any(|e| {
        matches!(e.kind, ExecKind::Elf { .. }) && e.rel.split('/').count() <= 2 && e.score >= 10
    });
    let skip = if top_dir.is_some() { 1 } else { 0 };
    let mut src_files: Vec<String> = src_raw
        .iter()
        .filter(|p| p.len() > skip)
        .map(|p| p[skip..].join("/"))
        .collect();
    // JAR : seuls ceux qui ont une classe principale (Main-Class) peuvent être lancés.
    {
        let mut jars: Vec<&(Vec<String>, String, u64)> =
            jar_raw.iter().filter(|(p, _, _)| p.len() > skip).collect();
        jars.sort_by_key(|(p, _, _)| p.len());
        for (p, raw, size) in jars.into_iter().take(8) {
            let lowpath = p.join("/").to_lowercase();
            if [
                "/lib/",
                "/libs/",
                "/dependency/",
                "/deps/",
                "sources",
                "javadoc",
                "-tests",
            ]
            .iter()
            .any(|w| lowpath.contains(w))
                && p.len() > skip + 1
            {
                continue;
            }
            let runnable = match read_member(path, raw, 64 << 20) {
                Ok(data) if *size <= 64 << 20 => jar_has_main_class(&data).unwrap_or(true),
                _ => true,
            };
            if runnable {
                src_files.push(p[skip..].join("/"));
            }
        }
    }
    let exec_names: Vec<String> = executables
        .iter()
        .map(|e| e.rel.rsplit('/').next().unwrap_or("").to_lowercase())
        .collect();
    match toolchains::detect(&root_build, &exec_names, has_elf, guess_name, &src_files) {
        Some(Detection::Supported(p)) => (Some(p), None),
        Some(Detection::Unsupported(lang)) => (None, Some(lang)),
        None => (None, None),
    }
}
