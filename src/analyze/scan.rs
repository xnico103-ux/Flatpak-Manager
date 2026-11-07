//! Parcours de l'archive : une seule lecture qui collecte les fichiers utiles
//! (exécutables, icônes, .desktop, fichiers de build…) et applique les contrôles de sécurité.

use super::model::*;
use super::paths::*;
use super::sniff::sniff_exec;
use crate::toolchains::{BUILD_FILE_NAMES, BuildFile};
use anyhow::{Context, Result, anyhow};
use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::io::Read;

/// Un fichier retenu pendant le parcours.
pub(super) struct Raw {
    pub parts: Vec<String>,
    pub raw: String,
    pub size: u64,
}

/// Ce que le parcours a trouvé dans l'archive.
pub(super) struct Scan {
    pub count: usize,
    pub unpacked: u64,
    tops: BTreeSet<String>,
    root_files: usize,
    pub raws_exec: Vec<(Raw, ExecKind)>,
    pub raws_icon: Vec<(Raw, bool, u32)>,
    pub desktops: Vec<(Raw, DesktopInfo)>,
    pub lib_names: HashSet<String>,
    pub lib_dirs: BTreeMap<String, usize>,
    pub has_makefile: bool,
    pub has_jar: bool,
    pub has_appimage: bool,
    pub has_asar: bool,
    pub has_chrome_sandbox: bool,
    pub qt_bundled: bool,
    pub gtk_bundled: bool,
    pub build_raw: Vec<(Vec<String>, BuildFile)>,
    /// Fichiers .rs / .go / .py / .js (peu profonds) : servent à vérifier qu'un projet produit bien un programme.
    pub src_raw: Vec<Vec<String>>,
    /// Fichiers .jar : (chemin, chemin brut, taille).
    pub jar_raw: Vec<(Vec<String>, String, u64)>,
}

/// Dossier racine unique de l'archive (s'il y en a un) et conversions de chemins.
pub(super) struct Layout {
    pub top_dir: Option<String>,
}

impl Layout {
    /// Chemin sans le dossier racine.
    pub fn strip(&self, parts: &[String]) -> String {
        if self.top_dir.is_some() {
            parts[1..].join("/")
        } else {
            parts.join("/")
        }
    }
}

impl Scan {
    pub fn layout(&self) -> Layout {
        let top_dir = if self.tops.len() == 1 && self.root_files == 0 {
            self.tops.iter().next().cloned()
        } else {
            None
        };
        Layout { top_dir }
    }
}

/// Lit toute l'archive. Les blocages de sécurité sont ajoutés à `findings`.
pub(super) fn scan_archive(
    decoder: Box<dyn Read>,
    size: u64,
    findings: &mut Vec<Finding>,
) -> Result<Scan> {
    let mut ar = tar::Archive::new(decoder);
    let mut count = 0usize;
    let mut unpacked = 0u64;
    let mut tops: BTreeSet<String> = BTreeSet::new();
    let mut root_files = 0usize;
    let mut symlinks: HashSet<String> = HashSet::new();
    let mut raws_exec: Vec<(Raw, ExecKind)> = Vec::new();
    let mut raws_icon: Vec<(Raw, bool, u32)> = Vec::new();
    let mut desktops: Vec<(Raw, DesktopInfo)> = Vec::new();
    let mut lib_names: HashSet<String> = HashSet::new();
    let mut lib_dirs: BTreeMap<String, usize> = BTreeMap::new();
    let mut unsafe_paths = 0usize;
    let mut unsafe_example = String::new();
    let mut setuid = 0usize;
    let mut has_makefile = false;
    let mut has_jar = false;
    let mut has_appimage = false;
    let mut has_asar = false;
    let mut has_chrome_sandbox = false;
    let mut qt_bundled = false;
    let mut gtk_bundled = false;
    let mut png_reads = 0usize;
    let mut special = 0usize;
    let mut build_raw: Vec<(Vec<String>, BuildFile)> = Vec::new();
    // Fichiers .rs / .go (peu profonds) : servent à vérifier qu'un projet produit bien un programme.
    let mut src_raw: Vec<Vec<String>> = Vec::new();
    // Fichiers .jar : (chemin, chemin brut, taille).
    let mut jar_raw: Vec<(Vec<String>, String, u64)> = Vec::new();

    let entries = ar.entries().context(tr!("lecture de l'archive"))?;
    for entry in entries {
        let mut entry =
            entry.map_err(|e| anyhow!(tr!("archive corrompue ou tronquée : {e}", e = e)))?;
        count += 1;
        if count > MAX_ENTRIES {
            push(
                findings,
                Severity::Blocker,
                tr!("Trop de fichiers"),
                tr!(
                    "L'archive contient plus de {max} entrées.",
                    max = MAX_ENTRIES
                ),
            );
            break;
        }
        // En-têtes étendus (pax) : ajoutés par `git archive`, ils ne sont pas des fichiers de l'archive.
        let ety = entry.header().entry_type();
        if ety.is_pax_global_extensions() || ety.is_pax_local_extensions() {
            count -= 1;
            continue;
        }
        let epath = entry.path()?.into_owned();
        let parts = match clean_path(&epath) {
            Ok(p) => p,
            Err(why) => {
                unsafe_paths += 1;
                if unsafe_example.is_empty() {
                    unsafe_example = format!("{} ({why})", epath.display());
                }
                continue;
            }
        };
        if parts.is_empty() {
            continue;
        }
        // Écriture à travers un lien symbolique présent plus tôt dans l'archive ?
        let mut prefix = String::new();
        for p in &parts[..parts.len() - 1] {
            if !prefix.is_empty() {
                prefix.push('/');
            }
            prefix.push_str(p);
            if symlinks.contains(&prefix) {
                unsafe_paths += 1;
                if unsafe_example.is_empty() {
                    unsafe_example = tr!(
                        "{path} (passe par un lien symbolique)",
                        path = epath.display()
                    );
                }
                break;
            }
        }
        let joined = parts.join("/");
        tops.insert(parts[0].clone());
        let ty = entry.header().entry_type();
        // Une entrée de dossier à la racine (« app/ ») n'est pas un fichier à la racine.
        if parts.len() == 1 && !ty.is_dir() {
            root_files += 1;
        }
        let esize = entry.size();
        let mode = entry.header().mode().unwrap_or(0);
        if ty.is_symlink() || ty.is_hard_link() {
            if let Ok(Some(target)) = entry.link_name()
                && link_escapes(&parts, &target, ty.is_hard_link())
            {
                unsafe_paths += 1;
                if unsafe_example.is_empty() {
                    unsafe_example = tr!(
                        "{path} → {target} (lien hors de l'archive)",
                        path = epath.display(),
                        target = target.display()
                    );
                }
            }
            if ty.is_symlink() {
                symlinks.insert(joined);
            }
            continue;
        }
        if ty.is_dir() {
            continue;
        }
        if !ty.is_file() {
            if ty.is_character_special() || ty.is_block_special() || ty.is_fifo() {
                special += 1;
            }
            continue;
        }
        unpacked = unpacked.saturating_add(esize);
        if unpacked > MAX_UNPACKED {
            push(
                findings,
                Severity::Blocker,
                tr!("Archive trop volumineuse une fois décompressée"),
                tr!(
                    "Plus de {size} Gio décompressés.",
                    size = MAX_UNPACKED >> 30
                ),
            );
            break;
        }
        if mode & 0o6000 != 0 {
            setuid += 1;
        }
        let Some(fname) = parts.last().cloned() else {
            continue;
        };
        let lower = fname.to_lowercase();
        let raw = Raw {
            parts: parts.clone(),
            raw: epath.to_string_lossy().trim_start_matches("./").to_string(),
            size: esize,
        };

        if is_lib_name(&fname) {
            lib_names.insert(fname.clone());
            if let Some(dir) = parts[..parts.len() - 1].join("/").into() {
                *lib_dirs.entry(dir).or_default() += 1;
            }
            if fname.starts_with("libQt5Core") || fname.starts_with("libQt6Core") {
                qt_bundled = true;
            }
            if fname.starts_with("libgtk-3") || fname.starts_with("libgtk-4") {
                gtk_bundled = true;
            }
        }
        let vendored = parts.iter().any(|x| {
            matches!(
                x.as_str(),
                "node_modules" | "site-packages" | ".git" | "venv" | ".venv"
            )
        });
        if [".rs", ".go", ".py", ".js", ".mjs", ".cjs"]
            .iter()
            .any(|e| lower.ends_with(e))
            && parts.len() <= 5
            && src_raw.len() < 20_000
            && !vendored
        {
            src_raw.push(parts.clone());
        }
        if lower.ends_with(".jar") && parts.len() <= 5 && jar_raw.len() < 200 {
            jar_raw.push((parts.clone(), raw.raw.clone(), esize));
        }
        match lower.as_str() {
            "makefile" | "cmakelists.txt" | "configure" | "meson.build" | "cargo.toml"
            | "setup.py" | "pyproject.toml" => has_makefile = true,
            "app.asar" => has_asar = true,
            "chrome-sandbox" => has_chrome_sandbox = true,
            _ => {}
        }
        if BUILD_FILE_NAMES.contains(&lower.as_str()) && parts.len() <= 3 && esize <= 512 * 1024 {
            let mut buf = Vec::new();
            let _ = (&mut entry).take(512 * 1024).read_to_end(&mut buf);
            build_raw.push((
                parts.clone(),
                BuildFile {
                    name: lower.clone(),
                    content: String::from_utf8_lossy(&buf).into_owned(),
                },
            ));
            continue;
        }
        if lower.ends_with(".jar") {
            has_jar = true;
        }
        if lower.ends_with(".appimage") {
            has_appimage = true;
        }
        if lower.ends_with(".desktop") && esize < 64 * 1024 {
            let mut s = String::new();
            let _ = (&mut entry).take(64 * 1024).read_to_string(&mut s);
            desktops.push((raw, parse_desktop(&s)));
            continue;
        }
        if (lower.ends_with(".png") || lower.ends_with(".svg"))
            && esize > 0
            && esize <= MAX_ICON_SIZE
        {
            let is_svg = lower.ends_with(".svg");
            let mut width = 0u32;
            if !is_svg && png_reads < 3000 {
                png_reads += 1;
                let mut head = Vec::with_capacity(32);
                let _ = (&mut entry).take(32).read_to_end(&mut head);
                if head.len() >= 24 && &head[1..4] == b"PNG" {
                    let w = u32::from_be_bytes([head[16], head[17], head[18], head[19]]);
                    let h = u32::from_be_bytes([head[20], head[21], head[22], head[23]]);
                    if w == h {
                        width = w;
                    }
                }
            }
            raws_icon.push((raw, is_svg, width));
            continue;
        }
        if (mode & 0o111 != 0 || !lower.contains('.')) && esize >= 4 && !is_lib_name(&fname) {
            let mut head = Vec::with_capacity(64);
            let _ = (&mut entry).take(64).read_to_end(&mut head);
            if let Some(k) = sniff_exec(&head) {
                raws_exec.push((raw, k));
            }
        }
    }

    if unsafe_paths > 0 {
        push(
            findings,
            Severity::Blocker,
            tr!("Chemins dangereux dans l'archive"),
            tr!(
                "{unsafe_paths} entrée(s) tentent d'écrire hors du dossier d'extraction ou à travers un lien symbolique, par exemple : {unsafe_example}. Archive refusée par sécurité.",
                unsafe_paths = unsafe_paths,
                unsafe_example = unsafe_example
            ),
        );
    }
    if special > 0 {
        push(
            findings,
            Severity::Blocker,
            tr!("Fichiers spéciaux"),
            tr!(
                "{special} fichier(s) de périphérique ou tube nommé : refusés par sécurité.",
                special = special
            ),
        );
    }
    if size > 0 && unpacked > (1 << 30) && unpacked / size.max(1) > 1000 {
        push(
            findings,
            Severity::Blocker,
            tr!("Taux de compression suspect"),
            tr!("Possible « bombe de décompression »."),
        );
    }
    if setuid > 0 {
        push(
            findings,
            Severity::Warning,
            tr!("Bits setuid/setgid"),
            tr!(
                "{setuid} fichier(s) ont des bits spéciaux ; ils sont sans effet dans Flatpak.",
                setuid = setuid
            ),
        );
    }

    Ok(Scan {
        count,
        unpacked,
        tops,
        root_files,
        raws_exec,
        raws_icon,
        desktops,
        lib_names,
        lib_dirs,
        has_makefile,
        has_jar,
        has_appimage,
        has_asar,
        has_chrome_sandbox,
        qt_bundled,
        gtk_bundled,
        build_raw,
        src_raw,
        jar_raw,
    })
}
