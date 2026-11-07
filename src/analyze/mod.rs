//! Analyse d'une archive `.tar.gz` : sécurité, détection de l'exécutable,
//! de l'icône, du fichier `.desktop` et des dépendances probables.
//!
//! Rien n'est extrait sur le disque : l'archive est seulement parcourue.
//!
//! Étapes, dans l'ordre où `analyze` les enchaîne :
//! - `io` : ouverture et décompression, empreinte, lecture d'un membre
//! - `scan` : un seul parcours de l'archive (collecte et contrôles de sécurité)
//! - `candidates` : identité de l'application, classement des exécutables et des icônes
//! - `sources` : archive de sources à compiler ?
//! - `verdict` : type d'application, exécutable principal, bibliothèques, constats
//!
//! `model`, `paths` et `sniff` regroupent les types et les fonctions pures.

mod candidates;
mod io;
mod model;
mod paths;
mod scan;
mod sniff;
mod sources;
#[cfg(test)]
mod tests;
mod verdict;

pub use io::{read_member, sha256_file};
pub use model::*;

use anyhow::{Context, Result, anyhow};
use std::path::Path;

pub fn analyze(path: &Path) -> Result<ArchiveInfo> {
    let meta =
        std::fs::metadata(path).with_context(|| tr!("lecture de {path}", path = path.display()))?;
    let size = meta.len();
    let mut findings: Vec<Finding> = Vec::new();
    let file_name = path
        .file_name()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default();
    let (guess_name, guess_version) = paths::guess_name_version(&file_name);

    // Format (détecté par les premiers octets, pas par l'extension)
    let Some(decoder) = io::open_decoder(path)? else {
        push(
            &mut findings,
            Severity::Blocker,
            tr!("Format non pris en charge"),
            tr!("Le fichier n'est pas une archive .tar compressée en gzip, xz, zstd ou bzip2."),
        );
        return Ok(empty_info(path, size, findings, guess_name, guess_version));
    };

    // Le hash se calcule en parallèle de la lecture de l'archive.
    let sha_path = path.to_path_buf();
    let sha_job = std::thread::spawn(move || io::sha256_file(&sha_path));

    let mut scan = scan::scan_archive(decoder, size, &mut findings)?;
    let layout = scan.layout();
    let ident = candidates::identify(std::mem::take(&mut scan.desktops), &layout, &guess_name);

    let mut executables =
        candidates::rank_executables(std::mem::take(&mut scan.raws_exec), &layout, &ident);
    let (source, unsupported_source) =
        sources::detect_source(path, &scan, &layout, &executables, &guess_name);
    if source.is_some() {
        // Une archive de sources se compile : ses binaires éventuels ne sont pas l'application.
        executables.clear();
    }
    let main_exec = if executables.is_empty() {
        None
    } else {
        Some(0)
    };
    let icons = candidates::rank_icons(std::mem::take(&mut scan.raws_icon), &ident);
    let lib_dirs = candidates::top_lib_dirs(std::mem::take(&mut scan.lib_dirs), &layout);

    let mut hints = verdict::initial_hints(&scan, &source);
    let kind = verdict::classify(
        main_exec.map(|i| &executables[i].kind),
        &scan,
        source.is_some(),
        unsupported_source.is_some(),
    );
    verdict::report_kind(&mut findings, kind, &source, unsupported_source);
    verdict::report_install_script(&mut findings, kind, &ident.desktop, main_exec, &executables);
    let (needed_libs, dlopen_gui) =
        verdict::inspect_main_exec(path, &mut findings, kind, main_exec, &executables);
    verdict::classify_libs(
        &mut findings,
        kind,
        &mut hints,
        &scan.lib_names,
        needed_libs,
        dlopen_gui,
    );
    push(
        &mut findings,
        Severity::Info,
        tr!("Contenu"),
        tr!(
            "{count} fichiers, {size} Mio décompressés.",
            count = scan.count,
            size = scan.unpacked >> 20
        ),
    );
    // Tri : blocages d'abord
    findings.sort_by_key(|b| std::cmp::Reverse(b.severity));

    Ok(ArchiveInfo {
        path: path.to_path_buf(),
        sha256: sha_job
            .join()
            .map_err(|_| anyhow!(tr!("calcul de l'empreinte interrompu")))??,
        size,
        unpacked_size: scan.unpacked,
        top_dir: layout.top_dir,
        kind,
        executables,
        main_exec,
        desktop: ident.desktop,
        icons,
        lib_dirs,
        hints,
        guessed_name: paths::clean_name(&ident.base_name),
        guessed_version: guess_version,
        findings,
        source,
    })
}
