//! Analyse d'une archive `.tar.gz` : sécurité, détection de l'exécutable,
//! de l'icône, du fichier `.desktop` et des dépendances probables.
//!
//! Rien n'est extrait sur le disque : l'archive est seulement parcourue.

use crate::toolchains::{self, BUILD_FILE_NAMES, BuildFile, Detection, SourceProject};
use anyhow::{Context, Result, anyhow};
use flate2::read::GzDecoder;
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::fs::File;
use std::io::{BufReader, Read};
use std::path::{Component, Path, PathBuf};

pub const MAX_UNPACKED: u64 = 12 * 1024 * 1024 * 1024;
pub const MAX_ENTRIES: usize = 300_000;
pub const MAX_ELF_PARSE: u64 = 400 * 1024 * 1024;
const MAX_ICON_SIZE: u64 = 5 * 1024 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Severity {
    Info,
    Warning,
    Blocker,
}

#[derive(Debug, Clone)]
pub struct Finding {
    pub severity: Severity,
    pub title: String,
    pub detail: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Arch {
    X86_64,
    Aarch64,
    Other(u16),
}

impl Arch {
    pub fn label(&self) -> String {
        match self {
            Arch::X86_64 => "x86_64".into(),
            Arch::Aarch64 => "aarch64".into(),
            Arch::Other(m) => format!("machine {m}"),
        }
    }
    pub fn matches_host(&self) -> bool {
        match (self, std::env::consts::ARCH) {
            (Arch::X86_64, "x86_64") | (Arch::Aarch64, "aarch64") => true,
            _ => false,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExecKind {
    Elf { arch: Arch, bits: u8 },
    Script { interpreter: String },
}

#[derive(Debug, Clone)]
pub struct ExecCandidate {
    /// Chemin dans l'archive, tel quel.
    pub raw: String,
    /// Chemin relatif après suppression du dossier racine.
    pub rel: String,
    pub kind: ExecKind,
    pub size: u64,
    pub score: i32,
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct IconCandidate {
    pub raw: String,
    pub rel: String,
    pub is_svg: bool,
    pub width: u32,
    pub score: i32,
}

#[derive(Debug, Clone, Default)]
pub struct DesktopInfo {
    pub name: Option<String>,
    pub comment: Option<String>,
    pub exec: Option<String>,
    pub icon: Option<String>,
    pub categories: Option<String>,
    pub terminal: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AppKind {
    Native,
    Script,
    Source,
    Java,
    AppImage,
    Unknown,
}

impl AppKind {
    pub fn label(&self) -> &'static str {
        match self {
            AppKind::Native => "Binaire natif précompilé",
            AppKind::Script => "Application à base de script",
            AppKind::Source => "Code source à compiler",
            AppKind::Java => "Application Java",
            AppKind::AppImage => "AppImage",
            AppKind::Unknown => "Type inconnu",
        }
    }
}

#[derive(Debug, Clone, Copy, Default)]
pub struct Hints {
    pub electron: bool,
    pub qt: bool,
    pub gtk: bool,
    pub gui: bool,
    pub network_likely: bool,
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct ArchiveInfo {
    pub path: PathBuf,
    pub sha256: String,
    pub size: u64,
    pub unpacked_size: u64,
    pub file_count: usize,
    pub top_dir: Option<String>,
    pub kind: AppKind,
    pub executables: Vec<ExecCandidate>,
    pub main_exec: Option<usize>,
    pub desktop: Option<DesktopInfo>,
    pub icons: Vec<IconCandidate>,
    pub lib_dirs: Vec<String>,
    pub needed_libs: Vec<String>,
    pub missing_libs: Vec<String>,
    pub hints: Hints,
    pub guessed_name: String,
    pub guessed_version: String,
    pub findings: Vec<Finding>,
    /// Projet à compiler (archive de sources reconnue).
    pub source: Option<SourceProject>,
}

impl ArchiveInfo {
    pub fn blockers(&self) -> impl Iterator<Item = &Finding> {
        self.findings
            .iter()
            .filter(|f| f.severity == Severity::Blocker)
    }
    pub fn is_compatible(&self) -> bool {
        self.blockers().next().is_none() && (self.main_exec.is_some() || self.source.is_some())
    }
    pub fn main(&self) -> Option<&ExecCandidate> {
        self.main_exec.and_then(|i| self.executables.get(i))
    }
    pub fn best_icon(&self) -> Option<&IconCandidate> {
        self.icons.first()
    }
    pub fn strip_components(&self) -> u32 {
        u32::from(self.top_dir.is_some())
    }
}

fn push(f: &mut Vec<Finding>, severity: Severity, title: &str, detail: impl Into<String>) {
    f.push(Finding {
        severity,
        title: title.to_string(),
        detail: detail.into(),
    });
}

/// Normalise un chemin d'archive. Erreur si le chemin sort de la racine.
pub fn clean_path(p: &Path) -> std::result::Result<Vec<String>, String> {
    let mut parts = Vec::new();
    for c in p.components() {
        match c {
            Component::Normal(s) => parts.push(s.to_string_lossy().into_owned()),
            Component::CurDir => {}
            Component::ParentDir => return Err("contient « .. »".into()),
            Component::RootDir | Component::Prefix(_) => return Err("chemin absolu".into()),
        }
    }
    Ok(parts)
}

/// Vrai si la cible d'un lien sort de l'archive (ou est absolue).
fn link_escapes(entry_parts: &[String], target: &Path, hard: bool) -> bool {
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

fn is_lib_name(name: &str) -> bool {
    name.ends_with(".so") || name.contains(".so.")
}

fn norm(s: &str) -> String {
    s.chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .collect::<String>()
        .to_lowercase()
}

/// Ouvre un flux décompressé selon les octets magiques (gzip, xz, zstd, bzip2).
pub fn open_decoder(path: &Path) -> Result<Option<Box<dyn Read>>> {
    let mut magic = [0u8; 6];
    let n = File::open(path)?.read(&mut magic)?;
    let m = &magic[..n];
    let file = || -> Result<BufReader<File>> { Ok(BufReader::with_capacity(1 << 20, File::open(path)?)) };
    Ok(if m.starts_with(&[0x1f, 0x8b]) {
        Some(Box::new(GzDecoder::new(file()?)))
    } else if m.starts_with(&[0xfd, b'7', b'z', b'X', b'Z', 0x00]) {
        Some(Box::new(xz2::read::XzDecoder::new(file()?)))
    } else if m.starts_with(&[0x28, 0xb5, 0x2f, 0xfd]) {
        let d = ruzstd::decoding::StreamingDecoder::new(file()?).map_err(|e| anyhow!("archive zstd invalide : {e}"))?;
        Some(Box::new(d))
    } else if m.starts_with(b"BZh") {
        Some(Box::new(bzip2::read::BzDecoder::new(file()?)))
    } else {
        None
    })
}

pub fn sha256_file(path: &Path) -> Result<String> {
    let mut f = BufReader::with_capacity(1 << 20, File::open(path)?);
    let mut h = Sha256::new();
    let mut buf = vec![0u8; 1 << 20];
    loop {
        let n = f.read(&mut buf)?;
        if n == 0 {
            break;
        }
        h.update(&buf[..n]);
    }
    Ok(h.finalize().iter().map(|b| format!("{b:02x}")).collect())
}

fn parse_desktop(text: &str) -> DesktopInfo {
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

fn desktop_exec_base(d: &DesktopInfo) -> Option<String> {
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
    for ext in [".tar.gz", ".tgz", ".tar.xz", ".txz", ".tar.zst", ".tar.bz2", ".tbz2"] {
        if let Some(s) = stem.strip_suffix(ext) {
            stem = s.to_string();
            break;
        }
    }
    const NOISE: &[&str] = &[
        "linux", "x64", "x86", "x86_64", "amd64", "gnu", "portable", "bin", "64bit", "64", "release",
        "aarch64", "arm64", "generic", "static",
    ];
    let mut name = Vec::new();
    let mut version = String::new();
    for tok in stem.split(['-', '_', ' ']) {
        if tok.is_empty() {
            continue;
        }
        let lower = tok.to_lowercase();
        let looks_version = lower.chars().next().is_some_and(|c| c.is_ascii_digit())
            || (lower.starts_with('v')
                && lower.chars().nth(1).is_some_and(|c| c.is_ascii_digit()));
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

struct Raw {
    parts: Vec<String>,
    raw: String,
    size: u64,
}

pub fn analyze(path: &Path) -> Result<ArchiveInfo> {
    let meta = std::fs::metadata(path).with_context(|| format!("lecture de {}", path.display()))?;
    let size = meta.len();
    let mut findings: Vec<Finding> = Vec::new();
    let file_name = path
        .file_name()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default();
    let (guess_name, guess_version) = guess_name_version(&file_name);

    // Format (détecté par les premiers octets, pas par l'extension)
    let Some(decoder) = open_decoder(path)? else {
        push(&mut findings, Severity::Blocker, "Format non pris en charge", "Le fichier n'est pas une archive .tar compressée en gzip, xz, zstd ou bzip2.");
        return Ok(empty_info(path, size, findings, guess_name, guess_version));
    };

    let sha256 = sha256_file(path)?;

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

    let entries = ar.entries().context("lecture de l'archive")?;
    for entry in entries {
        let mut entry = entry.map_err(|e| anyhow!("archive corrompue ou tronquée : {e}"))?;
        count += 1;
        if count > MAX_ENTRIES {
            push(&mut findings, Severity::Blocker, "Trop de fichiers", format!("L'archive contient plus de {MAX_ENTRIES} entrées."));
            break;
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
                    unsafe_example = format!("{} (passe par un lien symbolique)", epath.display());
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
                && link_escapes(&parts, &target, ty.is_hard_link()) {
                    unsafe_paths += 1;
                    if unsafe_example.is_empty() {
                        unsafe_example = format!("{} → {} (lien hors de l'archive)", epath.display(), target.display());
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
            push(&mut findings, Severity::Blocker, "Archive trop volumineuse une fois décompressée", format!("Plus de {} Gio décompressés.", MAX_UNPACKED >> 30));
            break;
        }
        if mode & 0o6000 != 0 {
            setuid += 1;
        }
        let fname = parts.last().unwrap().clone();
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
        if (lower.ends_with(".rs") || lower.ends_with(".go")) && parts.len() <= 5 && src_raw.len() < 20_000 {
            src_raw.push(parts.iter().map(|x| x.to_lowercase()).collect());
        }
        match lower.as_str() {
            "makefile" | "cmakelists.txt" | "configure" | "meson.build" | "cargo.toml" | "setup.py" | "pyproject.toml" => has_makefile = true,
            "app.asar" => has_asar = true,
            "chrome-sandbox" => has_chrome_sandbox = true,
            _ => {}
        }
        if BUILD_FILE_NAMES.contains(&lower.as_str()) && parts.len() <= 3 && esize <= 512 * 1024 {
            let mut buf = Vec::new();
            let _ = (&mut entry).take(512 * 1024).read_to_end(&mut buf);
            build_raw.push((parts.clone(), BuildFile { name: lower.clone(), content: String::from_utf8_lossy(&buf).into_owned() }));
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
        if (lower.ends_with(".png") || lower.ends_with(".svg")) && esize > 0 && esize <= MAX_ICON_SIZE {
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
        push(&mut findings, Severity::Blocker, "Chemins dangereux dans l'archive", format!("{unsafe_paths} entrée(s) tentent d'écrire hors du dossier d'extraction ou à travers un lien symbolique, par exemple : {unsafe_example}. Archive refusée par sécurité."));
    }
    if special > 0 {
        push(&mut findings, Severity::Blocker, "Fichiers spéciaux", format!("{special} fichier(s) de périphérique ou tube nommé : refusés par sécurité."));
    }
    if size > 0 && unpacked > (1 << 30) && unpacked / size.max(1) > 1000 {
        push(&mut findings, Severity::Blocker, "Taux de compression suspect", "Possible « bombe de décompression ».");
    }
    if setuid > 0 {
        push(&mut findings, Severity::Warning, "Bits setuid/setgid", format!("{setuid} fichier(s) ont des bits spéciaux ; ils sont sans effet dans Flatpak."));
    }

    // Dossier racine
    let top_dir = if tops.len() == 1 && root_files == 0 { tops.iter().next().cloned() } else { None };
    let strip = |parts: &[String]| -> String {
        if top_dir.is_some() { parts[1..].join("/") } else { parts.join("/") }
    };

    // Bureau
    let desktop = desktops
        .into_iter()
        .min_by_key(|(r, _)| r.parts.len())
        .map(|(_, d)| d);
    let desktop_exec = desktop.as_ref().and_then(desktop_exec_base);

    // Nom
    let top_name = top_dir.as_deref().map(|t| guess_name_version(t).0);
    let base_name = desktop
        .as_ref()
        .and_then(|d| d.name.clone())
        .or(top_name)
        .unwrap_or_else(|| guess_name.clone());
    let nbase = norm(&base_name);

    // Exécutables
    let mut executables: Vec<ExecCandidate> = raws_exec
        .into_iter()
        .filter_map(|(r, kind)| {
            let rel = strip(&r.parts);
            if rel.is_empty() {
                return None;
            }
            let fname = r.parts.last().unwrap().clone();
            let nf = norm(&fname);
            let mut s = 0i32;
            if desktop_exec.as_deref() == Some(fname.as_str()) {
                s += 120;
            }
            if !nbase.is_empty() && nf == nbase {
                s += 60;
            } else if !nbase.is_empty() && (nf.contains(&nbase) || nbase.contains(&nf)) && nf.len() > 2 {
                s += 20;
            }
            let rparts: Vec<&str> = rel.split('/').collect();
            s -= (rparts.len() as i32 - 1) * 8;
            if rparts.len() >= 2 && rparts[rparts.len() - 2] == "bin" {
                s += 15;
            }
            if rparts[..rparts.len() - 1].iter().any(|p| {
                matches!(*p, "lib" | "lib64" | "libexec" | "share" | "include" | "node_modules" | "plugins" | "site-packages" | "locales")
            }) {
                s -= 40;
            }
            for bad in ["install", "uninstall", "update", "setup", "crash", "sandbox", "helper", "test"] {
                if nf.contains(bad) {
                    s -= 45;
                }
            }
            if matches!(kind, ExecKind::Elf { .. }) {
                s += 10;
            }
            if r.size > (1 << 20) {
                s += 10;
            }
            Some(ExecCandidate { raw: r.raw, rel, kind, size: r.size, score: s })
        })
        .collect();
    executables.sort_by(|a, b| b.score.cmp(&a.score).then(a.rel.len().cmp(&b.rel.len())));

    // Archive de sources ? Seuls les fichiers de build à la racine du projet comptent.
    let root_len = if top_dir.is_some() { 2 } else { 1 };
    let root_build: Vec<BuildFile> = build_raw.into_iter().filter(|(p, _)| p.len() == root_len).map(|(_, f)| f).collect();
    // Un ELF isolé au fond d'un projet (outil de test, bibliothèque fournie) ne fait pas de l'archive une application déjà compilée.
    let has_elf = executables.iter().any(|e| matches!(e.kind, ExecKind::Elf { .. }) && e.rel.split('/').count() <= 2 && e.score >= 10);
    let skip = if top_dir.is_some() { 1 } else { 0 };
    let src_files: Vec<String> = src_raw.iter().filter(|p| p.len() > skip).map(|p| p[skip..].join("/")).collect();
    let exec_names: Vec<String> = executables.iter().map(|e| e.rel.rsplit('/').next().unwrap_or("").to_lowercase()).collect();
    let detection = toolchains::detect(&root_build, &exec_names, has_elf, &guess_name, &src_files);
    let mut source: Option<SourceProject> = None;
    let mut unsupported_source: Option<&'static str> = None;
    match detection {
        Some(Detection::Supported(p)) => {
            executables.clear();
            source = Some(p);
        }
        Some(Detection::Unsupported(lang)) => unsupported_source = Some(lang),
        None => {}
    }
    let main_exec = if executables.is_empty() { None } else { Some(0) };

    // Icônes
    let icon_name = desktop.as_ref().and_then(|d| d.icon.clone());
    let nicon = icon_name.as_deref().map(norm);
    let mut icons: Vec<IconCandidate> = raws_icon
        .into_iter()
        .filter(|(r, _, w)| {
            let low = r.raw.to_lowercase();
            !(low.contains("node_modules") || low.contains("/locale") || low.contains("/doc") || low.contains("/test"))
                && (*w == 0 || (16..=1024).contains(w))
        })
        .map(|(r, is_svg, width)| {
            let stem = Path::new(r.parts.last().unwrap()).file_stem().map(|s| norm(&s.to_string_lossy())).unwrap_or_default();
            let mut s = 0i32;
            if nicon.as_deref() == Some(stem.as_str()) {
                s += 100;
            } else if !nbase.is_empty() && (stem == nbase || stem.contains(&nbase)) {
                s += 60;
            } else if matches!(stem.as_str(), "icon" | "logo" | "appicon" | "app") {
                s += 30;
            }
            if is_svg {
                s += 12;
            } else if width > 0 {
                s -= ((width as f32 / 256.0).log2().abs() * 4.0) as i32;
            }
            s -= (r.parts.len() as i32 - 1) * 2;
            IconCandidate { rel: strip(&r.parts), raw: r.raw, is_svg, width, score: s }
        })
        .filter(|i| i.score > 0)
        .collect();
    icons.sort_by(|a, b| b.score.cmp(&a.score));
    icons.truncate(8);

    // Dossiers de bibliothèques embarquées
    let mut lib_dir_vec: Vec<(String, usize)> = lib_dirs.into_iter().collect();
    lib_dir_vec.sort_by(|a, b| b.1.cmp(&a.1));
    let lib_dirs: Vec<String> = lib_dir_vec
        .into_iter()
        .take(6)
        .map(|(d, _)| {
            let p: Vec<String> = d.split('/').map(String::from).collect();
            if top_dir.is_some() && !p.is_empty() { p[1..].join("/") } else { d }
        })
        .collect();

    // Type d'application
    let mut hints = Hints {
        electron: has_asar || (has_chrome_sandbox && lib_names.contains("libffmpeg.so")),
        qt: qt_bundled || source.as_ref().is_some_and(|s| s.qt),
        gtk: gtk_bundled || source.as_ref().is_some_and(|s| s.gtk),
        gui: source.as_ref().is_some_and(|s| s.gui),
        ..Default::default()
    };
    let kind = match main_exec.map(|i| &executables[i].kind) {
        _ if source.is_some() => AppKind::Source,
        Some(ExecKind::Elf { .. }) => AppKind::Native,
        Some(ExecKind::Script { .. }) => AppKind::Script,
        None if has_appimage => AppKind::AppImage,
        None if has_jar => AppKind::Java,
        None if has_makefile || unsupported_source.is_some() => AppKind::Source,
        None => AppKind::Unknown,
    };

    // Verdict sur le type
    match kind {
        AppKind::Source => match (&source, unsupported_source) {
            (Some(p), _) => {
                push(&mut findings, Severity::Info, &format!("Code source à compiler : {}", p.system.label()), "L'archive sera compilée dans le bac à sable Flatpak avec le SDK Freedesktop. La première compilation peut durer plusieurs minutes.");
                if let Some(why) = &p.blocker {
                    push(&mut findings, Severity::Blocker, "Aucun programme à compiler", why.clone());
                } else if !p.bin_certain {
                    push(&mut findings, Severity::Warning, "Nom du binaire à confirmer", format!("Le programme installé s'appellerait « {} », mais ce nom est deviné. Vous devrez le confirmer (ou le corriger) avant de lancer la compilation.", p.bin_name));
                }
                for n in &p.notes {
                    push(&mut findings, Severity::Info, "Compilation", n.clone());
                }
            }
            (None, Some(lang)) => push(&mut findings, Severity::Blocker, &format!("Projet {lang} non pris en charge"), format!("Cette archive contient un projet {lang} à compiler ; seuls Rust, C/C++ et Go sont pris en charge pour l'instant.")),
            (None, None) => push(&mut findings, Severity::Blocker, "Archive de sources", "Cette archive contient du code source mais aucun système de build reconnu (Cargo, CMake, Meson, Autotools, Makefile, Go)."),
        },
        AppKind::AppImage => push(&mut findings, Severity::Blocker, "AppImage détecté", "L'archive contient un AppImage. Ce format est déjà autonome ; il ne peut pas être converti directement."),
        AppKind::Java => push(&mut findings, Severity::Blocker, "Application Java", "Les applications .jar nécessitent l'extension OpenJDK ; non prises en charge pour l'instant."),
        AppKind::Unknown => push(&mut findings, Severity::Blocker, "Aucun exécutable trouvé", "Impossible de repérer un programme exécutable dans cette archive."),
        _ => {}
    }

    // Une archive qui ne contient que des scripts d'installation n'est pas une application.
    if kind == AppKind::Script
        && desktop.is_none()
        && let Some(m) = main_exec.and_then(|i| executables.get(i))
    {
        let f = m.rel.rsplit('/').next().unwrap_or("").to_lowercase();
        if ["install", "setup", "uninstall", "configure", "bootstrap", "autogen", "build", "update", "make"].iter().any(|w| f.contains(w)) {
            push(&mut findings, Severity::Blocker, "Archive non prévue pour être convertie", format!("Le seul programme trouvé est « {} », qui ressemble à un script d'installation ou de compilation, pas à une application.", m.rel));
        }
    }

    // Analyse de l'exécutable principal
    let mut needed_libs: Vec<String> = Vec::new();
    let mut missing_libs: Vec<String> = Vec::new();
    if let Some(m) = main_exec.map(|i| executables[i].clone()) {
        push(&mut findings, Severity::Info, "Exécutable principal", format!("{} ({})", m.rel, kind.label()));
        if executables.len() > 1 {
            let others = executables.iter().skip(1).take(3).map(|e| e.rel.clone()).collect::<Vec<_>>().join(", ");
            push(&mut findings, Severity::Info, &format!("{} autres exécutables détectés", executables.len() - 1), format!("Au besoin, choisissez-en un autre en mode avancé (ex. : {others})."));
        }
        match &m.kind {
            ExecKind::Elf { arch, bits } => {
                if !arch.matches_host() {
                    push(&mut findings, Severity::Blocker, "Architecture incompatible", format!("Le binaire est compilé pour {} mais cette machine est en {}.", arch.label(), std::env::consts::ARCH));
                }
                if *bits == 32 {
                    push(&mut findings, Severity::Blocker, "Binaire 32 bits", "Les binaires 32 bits nécessitent l'extension de compatibilité i386 ; non prise en charge.");
                }
                if m.size <= MAX_ELF_PARSE {
                    match read_member(path, &m.raw, MAX_ELF_PARSE) {
                        Ok(data) => {
                            if let Ok(elf) = goblin::elf::Elf::parse(&data) {
                                needed_libs = elf.libraries.iter().map(|s| s.to_string()).collect();
                            } else {
                                push(&mut findings, Severity::Warning, "Dépendances non analysées", "L'en-tête ELF n'a pas pu être lu entièrement.");
                            }
                        }
                        Err(e) => push(&mut findings, Severity::Warning, "Dépendances non analysées", e.to_string()),
                    }
                }
            }
            ExecKind::Script { interpreter } => {
                let base = Path::new(interpreter).file_name().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
                if !matches!(base.as_str(), "sh" | "bash" | "dash") {
                    push(&mut findings, Severity::Warning, "Interpréteur à vérifier", format!("Le script utilise « {interpreter} ». Cet interpréteur doit exister dans le runtime ou être embarqué dans l'archive."));
                }
            }
        }
    }

    // Classification des bibliothèques
    for lib in &needed_libs {
        if lib_names.contains(lib) || is_runtime_lib(lib) {
            if lib.starts_with("libssl") || lib.starts_with("libcurl") {
                hints.network_likely = true;
            }
            if is_gui_lib(lib) {
                hints.gui = true;
            }
            continue;
        }
        if lib.starts_with("libgtk-") || lib.starts_with("libgdk-") || lib.starts_with("libadwaita") {
            hints.gtk = true;
            hints.gui = true;
        } else if lib.starts_with("libQt5") || lib.starts_with("libQt6") {
            hints.qt = true;
            hints.gui = true;
        } else {
            missing_libs.push(lib.clone());
        }
    }
    if lib_names.iter().any(|l| is_gui_lib(l)) || hints.electron || hints.qt || hints.gtk {
        hints.gui = true;
    }
    if hints.electron {
        hints.network_likely = true;
    }
    if !missing_libs.is_empty() {
        push(&mut findings, Severity::Warning, "Bibliothèques possiblement manquantes", format!("Non embarquées et absentes du runtime standard (analyse heuristique) : {}.", missing_libs.join(", ")));
    }
    if hints.electron {
        push(&mut findings, Severity::Warning, "Application Electron détectée", "Le bac à sable Chromium ne fonctionne pas sans composant supplémentaire ; l'application sera lancée avec --no-sandbox (le bac à sable Flatpak reste actif).");
    }
    if matches!(kind, AppKind::Native | AppKind::Script) {
        if hints.gui {
            push(&mut findings, Severity::Info, "Application graphique", "Accès à l'affichage et à l'accélération graphique proposés par défaut.");
        } else {
            push(&mut findings, Severity::Info, "Application en ligne de commande probable", "Aucune bibliothèque graphique détectée : l'entrée de menu s'ouvrira dans un terminal.");
        }
    }
    push(&mut findings, Severity::Info, "Contenu", format!("{count} fichiers, {} Mio décompressés.", unpacked >> 20));
    // Tri : blocages d'abord
    findings.sort_by(|a, b| b.severity.cmp(&a.severity));

    let name = clean_name(&base_name);
    Ok(ArchiveInfo {
        path: path.to_path_buf(),
        sha256,
        size,
        unpacked_size: unpacked,
        file_count: count,
        top_dir,
        kind,
        executables,
        main_exec,
        desktop,
        icons,
        lib_dirs,
        needed_libs,
        missing_libs,
        hints,
        guessed_name: name,
        guessed_version: guess_version,
        findings,
        source,
    })
}

fn clean_name(n: &str) -> String {
    let t = n.trim();
    if t.is_empty() { "Application".into() } else { t.to_string() }
}

fn empty_info(path: &Path, size: u64, findings: Vec<Finding>, name: String, version: String) -> ArchiveInfo {
    ArchiveInfo {
        path: path.to_path_buf(),
        sha256: String::new(),
        size,
        unpacked_size: 0,
        file_count: 0,
        top_dir: None,
        kind: AppKind::Unknown,
        executables: vec![],
        main_exec: None,
        desktop: None,
        icons: vec![],
        lib_dirs: vec![],
        needed_libs: vec![],
        missing_libs: vec![],
        hints: Hints::default(),
        guessed_name: name,
        guessed_version: version,
        findings,
        source: None,
    }
}

fn sniff_exec(head: &[u8]) -> Option<ExecKind> {
    if head.len() >= 20 && &head[..4] == b"\x7fELF" {
        let bits = if head[4] == 2 { 64 } else { 32 };
        let machine = if head[5] == 2 { u16::from_be_bytes([head[18], head[19]]) } else { u16::from_le_bytes([head[18], head[19]]) };
        let arch = match machine {
            62 => Arch::X86_64,
            183 => Arch::Aarch64,
            m => Arch::Other(m),
        };
        // Les objets partagés (ET_DYN) peuvent être des exécutables PIE : on les garde.
        return Some(ExecKind::Elf { arch, bits });
    }
    if head.starts_with(b"#!") {
        let line = String::from_utf8_lossy(&head[2..]);
        let line = line.lines().next().unwrap_or("").trim();
        let mut it = line.split_whitespace();
        let first = it.next()?;
        let interp = if first.ends_with("/env") { it.next().unwrap_or(first) } else { first };
        return Some(ExecKind::Script { interpreter: interp.to_string() });
    }
    None
}

/// Lit un membre précis de l'archive en mémoire (borné).
pub fn read_member(archive: &Path, raw: &str, max: u64) -> Result<Vec<u8>> {
    let mut ar = tar::Archive::new(open_decoder(archive)?.ok_or_else(|| anyhow!("format d'archive non reconnu"))?);
    for e in ar.entries()? {
        let mut e = e?;
        let p = e.path()?.to_string_lossy().trim_start_matches("./").to_string();
        if p == raw && e.header().entry_type().is_file() {
            let mut v = Vec::new();
            (&mut e).take(max).read_to_end(&mut v)?;
            return Ok(v);
        }
    }
    Err(anyhow!("fichier introuvable dans l'archive : {raw}"))
}

fn is_gui_lib(l: &str) -> bool {
    ["libX11", "libxcb", "libwayland-client", "libGL", "libEGL", "libgtk", "libQt5Gui", "libQt6Gui", "libSDL2", "libglfw", "libvulkan"]
        .iter()
        .any(|p| l.starts_with(p))
}

/// Bibliothèques présentes dans org.freedesktop.Platform (liste heuristique).
fn is_runtime_lib(l: &str) -> bool {
    const P: &[&str] = &[
        "libc.so", "libm.so", "libdl.so", "libpthread.so", "librt.so", "libutil.so", "libresolv.so", "libnsl.so",
        "libgcc_s", "libstdc++", "ld-linux", "libz.so", "libbz2", "liblzma", "libX11", "libXext", "libXrender", "libXi.so",
        "libXcursor", "libXrandr", "libXfixes", "libXcomposite", "libXdamage", "libXtst", "libXss", "libXinerama", "libxcb",
        "libGL", "libEGL", "libOpenGL", "libgbm", "libdrm", "libasound", "libpulse", "libfontconfig", "libfreetype",
        "libharfbuzz", "libglib-2.0", "libgobject-2.0", "libgio-2.0", "libgmodule-2.0", "libgthread-2.0", "libdbus-1",
        "libwayland-", "libxkbcommon", "libssl", "libcrypto", "libcups", "libnss3", "libnspr4", "libnssutil3", "libsmime3",
        "libexpat", "libpng16", "libjpeg", "libuuid", "libffi", "libpcre2", "libudev", "libsystemd", "libatomic", "libcurl",
        "libvulkan", "libxshmfence", "libxkbfile", "libcairo", "libpango", "libpangocairo", "libpangoft2", "libgdk_pixbuf",
        "libatk-1.0", "libatk-bridge", "libatspi", "libepoxy", "libsndfile", "libogg", "libvorbis", "libopus", "libSDL2",
    ];
    P.iter().any(|p| l.starts_with(p))
}

// --------------------------------------------------------------------------
#[cfg(test)]
mod tests {
    use super::*;
    use flate2::{Compression, write::GzEncoder};
    use std::io::Write;

    fn elf_header(machine: u16) -> Vec<u8> {
        let mut v = vec![0u8; 128];
        v[..4].copy_from_slice(b"\x7fELF");
        v[4] = 2;
        v[5] = 1;
        v[18..20].copy_from_slice(&machine.to_le_bytes());
        v
    }

    fn build(dir: &Path, name: &str, f: impl FnOnce(&mut tar::Builder<GzEncoder<File>>)) -> PathBuf {
        let p = dir.join(name);
        let enc = GzEncoder::new(File::create(&p).unwrap(), Compression::fast());
        let mut b = tar::Builder::new(enc);
        f(&mut b);
        b.into_inner().unwrap().finish().unwrap().flush().unwrap();
        p
    }

    fn add(b: &mut tar::Builder<GzEncoder<File>>, path: &str, data: &[u8], mode: u32) {
        let mut h = tar::Header::new_gnu();
        h.set_size(data.len() as u64);
        h.set_mode(mode);
        h.set_entry_type(tar::EntryType::Regular);
        b.append_data(&mut h, path, data).unwrap();
    }

    fn host_machine() -> u16 {
        if std::env::consts::ARCH == "aarch64" { 183 } else { 62 }
    }

    #[test]
    fn simple_native_app() {
        let d = tempfile::tempdir().unwrap();
        let p = build(d.path(), "monappli-1.2.3-linux-x64.tar.gz", |b| {
            add(b, "monappli-1.2.3/monappli", &elf_header(host_machine()), 0o755);
            add(b, "monappli-1.2.3/README.txt", b"hello", 0o644);
            add(b, "monappli-1.2.3/monappli.desktop", b"[Desktop Entry]\nName=Mon Appli\nExec=monappli %U\nIcon=monappli\nCategories=Utility;\n", 0o644);
        });
        let i = analyze(&p).unwrap();
        assert!(i.is_compatible(), "{:?}", i.findings);
        assert_eq!(i.top_dir.as_deref(), Some("monappli-1.2.3"));
        assert_eq!(i.main().unwrap().rel, "monappli");
        assert_eq!(i.guessed_version, "1.2.3");
        assert_eq!(i.kind, AppKind::Native);
        assert_eq!(i.desktop.as_ref().unwrap().name.as_deref(), Some("Mon Appli"));
    }

    #[test]
    fn path_traversal_is_blocked() {
        let d = tempfile::tempdir().unwrap();
        let p = build(d.path(), "evil.tar.gz", |b| {
            let data = elf_header(host_machine());
            let mut h = tar::Header::new_gnu();
            h.set_size(data.len() as u64);
            h.set_mode(0o755);
            h.set_entry_type(tar::EntryType::Regular);
            let name = b"../../evil";
            h.as_old_mut().name[..name.len()].copy_from_slice(name);
            h.set_cksum();
            b.append(&h, &data[..]).unwrap();
        });
        let i = analyze(&p).unwrap();
        assert!(!i.is_compatible());
        assert!(i.blockers().any(|f| f.title.contains("dangereux")));
    }

    #[test]
    fn escaping_symlink_is_blocked() {
        let d = tempfile::tempdir().unwrap();
        let p = build(d.path(), "link.tar.gz", |b| {
            let mut h = tar::Header::new_gnu();
            h.set_entry_type(tar::EntryType::Symlink);
            h.set_size(0);
            h.set_mode(0o777);
            b.append_link(&mut h, "app/evil", "../../etc").unwrap();
            add(b, "app/evil/passwd", b"x", 0o644);
            add(b, "app/run", &elf_header(host_machine()), 0o755);
        });
        let i = analyze(&p).unwrap();
        assert!(i.blockers().any(|f| f.title.contains("dangereux")));
    }

    #[test]
    fn makefile_project_is_a_source_build() {
        let d = tempfile::tempdir().unwrap();
        let p = build(d.path(), "src.tar.gz", |b| {
            add(b, "proj/Makefile", b"TARGET = hello\nall:\n", 0o644);
            add(b, "proj/main.c", b"int main(){}", 0o644);
        });
        let i = analyze(&p).unwrap();
        assert_eq!(i.kind, AppKind::Source);
        assert!(i.is_compatible());
        assert!(i.executables.is_empty());
        let s = i.source.unwrap();
        assert_eq!((s.system, s.bin_name.as_str(), s.bin_certain), (crate::toolchains::BuildSystem::Make, "hello", true));
    }

    #[test]
    fn rust_project_is_a_source_build() {
        let d = tempfile::tempdir().unwrap();
        let p = build(d.path(), "outil-1.2.0.tar.gz", |b| {
            add(b, "outil-1.2.0/Cargo.toml", b"[package]\nname = \"outil\"\nversion = \"1.2.0\"\n", 0o644);
            add(b, "outil-1.2.0/src/main.rs", b"fn main(){}", 0o644);
            // un Cargo.toml de sous-dossier ne compte pas
            add(b, "outil-1.2.0/vendor/x/Cargo.toml", b"[package]\nname = \"x\"\n", 0o644);
        });
        let i = analyze(&p).unwrap();
        assert!(i.is_compatible());
        let s = i.source.as_ref().unwrap();
        assert_eq!(s.bin_name, "outil");
        assert_eq!(i.strip_components(), 1);
        let cfg = crate::manifest::BuildConfig::from_info(&i);
        let m = cfg.manifest(&i, "icon.png", "256x256");
        assert_eq!(m["sdk-extensions"][0], "org.freedesktop.Sdk.Extension.rust-stable");
        assert_eq!(m["command"], "outil");
        assert_eq!(m["modules"][0]["build-options"]["build-args"][0], "--share=network");
        assert_eq!(m["modules"].as_array().unwrap().len(), 2);
    }

    #[test]
    fn unsupported_or_unknown_sources_are_blocked() {
        let d = tempfile::tempdir().unwrap();
        let p = build(d.path(), "js.tar.gz", |b| add(b, "proj/package.json", b"{}", 0o644));
        let i = analyze(&p).unwrap();
        assert!(!i.is_compatible());
        assert!(i.blockers().any(|f| f.title.contains("Node.js")));
        let p = build(d.path(), "c.tar.gz", |b| add(b, "proj/main.c", b"int main(){}", 0o644));
        assert!(!analyze(&p).unwrap().is_compatible());
    }

    #[test]
    fn prebuilt_elf_with_cmakelists_stays_native() {
        let d = tempfile::tempdir().unwrap();
        let p = build(d.path(), "mix.tar.gz", |b| {
            add(b, "app/CMakeLists.txt", b"project(x)\n", 0o644);
            add(b, "app/run", &elf_header(host_machine()), 0o755);
        });
        let i = analyze(&p).unwrap();
        assert_eq!(i.kind, AppKind::Native);
        assert!(i.source.is_none());
    }

    #[test]
    fn wrong_arch_is_blocked() {
        let d = tempfile::tempdir().unwrap();
        let other = if host_machine() == 62 { 183 } else { 62 };
        let p = build(d.path(), "arm.tar.gz", |b| add(b, "app/run", &elf_header(other), 0o755));
        let i = analyze(&p).unwrap();
        assert!(i.blockers().any(|f| f.title.contains("Architecture")));
    }

    #[test]
    fn script_app_is_detected() {
        let d = tempfile::tempdir().unwrap();
        let p = build(d.path(), "tool.tar.gz", |b| add(b, "tool/tool", b"#!/usr/bin/env bash\necho hi\n", 0o755));
        let i = analyze(&p).unwrap();
        assert_eq!(i.kind, AppKind::Script);
        assert!(i.is_compatible());
    }

    fn tar_bytes() -> Vec<u8> {
        let mut b = tar::Builder::new(Vec::new());
        let data = elf_header(host_machine());
        let mut h = tar::Header::new_gnu();
        h.set_size(data.len() as u64);
        h.set_mode(0o755);
        h.set_entry_type(tar::EntryType::Regular);
        b.append_data(&mut h, "app/run", &data[..]).unwrap();
        b.into_inner().unwrap()
    }

    #[test]
    fn root_directory_entry_is_stripped() {
        // Les archives créées par `tar` contiennent une entrée pour le dossier racine.
        let d = tempfile::tempdir().unwrap();
        let p = build(d.path(), "tool-3.1.tar.gz", |b| {
            let mut h = tar::Header::new_gnu();
            h.set_entry_type(tar::EntryType::Directory);
            h.set_size(0);
            h.set_mode(0o755);
            b.append_data(&mut h, "tool-3.1/", &[][..]).unwrap();
            add(b, "tool-3.1/tool", &elf_header(host_machine()), 0o755);
        });
        let i = analyze(&p).unwrap();
        assert_eq!(i.top_dir.as_deref(), Some("tool-3.1"));
        assert_eq!(i.strip_components(), 1);
        assert_eq!(i.main().unwrap().rel, "tool");
    }

    #[test]
    fn xz_and_bzip2_archives_are_supported() {
        let d = tempfile::tempdir().unwrap();
        let raw = tar_bytes();
        let mut x = xz2::write::XzEncoder::new(Vec::new(), 1);
        x.write_all(&raw).unwrap();
        let px = d.path().join("app-1.0.tar.xz");
        std::fs::write(&px, x.finish().unwrap()).unwrap();
        let mut z = bzip2::write::BzEncoder::new(Vec::new(), bzip2::Compression::fast());
        z.write_all(&raw).unwrap();
        let pb = d.path().join("app-1.0.tar.bz2");
        std::fs::write(&pb, z.finish().unwrap()).unwrap();
        for p in [px, pb] {
            let i = analyze(&p).unwrap();
            assert!(i.is_compatible(), "{:?}", i.findings);
            assert_eq!(i.main().unwrap().rel, "run");
            assert_eq!(i.guessed_name, "app");
        }
    }

    #[test]
    fn not_gzip_is_rejected() {
        let d = tempfile::tempdir().unwrap();
        let p = d.path().join("x.tar.gz");
        std::fs::write(&p, b"not a gzip").unwrap();
        let i = analyze(&p).unwrap();
        assert!(!i.is_compatible());
    }

    #[test]
    fn name_version_guess() {
        assert_eq!(guess_name_version("Foo-Bar_2.4.1_linux_x64.tar.gz"), ("Foo Bar".into(), "2.4.1".into()));
        assert_eq!(guess_name_version("tool.tgz"), ("tool".into(), "1.0".into()));
    }

    #[test]
    fn rust_library_archive_is_refused() {
        let d = tempfile::tempdir().unwrap();
        let p = build(d.path(), "lib.tar.gz", |b| {
            add(b, "mylib/Cargo.toml", b"[package]\nname = \"mylib\"\nversion = \"0.1.0\"\n", 0o644);
            add(b, "mylib/src/lib.rs", b"pub fn f() {}", 0o644);
        });
        let i = analyze(&p).unwrap();
        assert!(!i.is_compatible());
        assert!(i.blockers().any(|f| f.title.contains("Aucun programme")));
    }

    #[test]
    fn installer_script_archive_is_refused() {
        let d = tempfile::tempdir().unwrap();
        let p = build(d.path(), "inst.tar.gz", |b| {
            add(b, "pkg/install.sh", b"#!/bin/sh\necho hi\n", 0o755);
            add(b, "pkg/data.bin", b"zzzz", 0o644);
        });
        let i = analyze(&p).unwrap();
        assert!(!i.is_compatible());
    }
}
