//! Résultat de l'analyse d'une archive : types publics et constructeurs.

use crate::toolchains::SourceProject;
use std::path::{Path, PathBuf};

pub const MAX_UNPACKED: u64 = 12 * 1024 * 1024 * 1024;
pub const MAX_ENTRIES: usize = 300_000;
pub const MAX_ELF_PARSE: u64 = 400 * 1024 * 1024;
pub(super) const MAX_ICON_SIZE: u64 = 5 * 1024 * 1024;

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
            Arch::Other(m) => tr!("machine {m}", m = m),
        }
    }
    pub fn matches_host(&self) -> bool {
        matches!(
            (self, std::env::consts::ARCH),
            (Arch::X86_64, "x86_64") | (Arch::Aarch64, "aarch64")
        )
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
pub struct IconCandidate {
    pub raw: String,
    pub is_svg: bool,
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
            AppKind::Native => tr!("Binaire natif précompilé"),
            AppKind::Script => tr!("Application à base de script"),
            AppKind::Source => tr!("Code source à compiler"),
            AppKind::Java => tr!("Application Java"),
            AppKind::AppImage => "AppImage",
            AppKind::Unknown => tr!("Type inconnu"),
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
    /// Accès direct au matériel (USB, HID, hidraw…) : demande `--device=all`.
    pub devices: bool,
}

#[derive(Debug, Clone)]
pub struct ArchiveInfo {
    pub path: PathBuf,
    pub sha256: String,
    pub size: u64,
    pub unpacked_size: u64,
    pub top_dir: Option<String>,
    pub kind: AppKind,
    pub executables: Vec<ExecCandidate>,
    pub main_exec: Option<usize>,
    pub desktop: Option<DesktopInfo>,
    pub icons: Vec<IconCandidate>,
    pub lib_dirs: Vec<String>,
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

pub(super) fn push(
    f: &mut Vec<Finding>,
    severity: Severity,
    title: &str,
    detail: impl Into<String>,
) {
    f.push(Finding {
        severity,
        title: title.to_string(),
        detail: detail.into(),
    });
}

pub(super) fn empty_info(
    path: &Path,
    size: u64,
    findings: Vec<Finding>,
    name: String,
    version: String,
) -> ArchiveInfo {
    ArchiveInfo {
        path: path.to_path_buf(),
        sha256: String::new(),
        size,
        unpacked_size: 0,
        top_dir: None,
        kind: AppKind::Unknown,
        executables: vec![],
        main_exec: None,
        desktop: None,
        icons: vec![],
        lib_dirs: vec![],
        hints: Hints::default(),
        guessed_name: name,
        guessed_version: version,
        findings,
        source: None,
    }
}
