//! Archives de sources : détection du langage et du système de build, composants
//! Flatpak nécessaires (runtime, SDK, extensions) et installation de ceux qui manquent.
//!
//! La compilation se fait toujours dans le bac à sable de flatpak-builder avec le
//! SDK Flatpak : les compilateurs installés sur la machine (rustc, gcc…) ne sont
//! affichés qu'à titre d'information.

use crate::diagnose::diagnose;
use crate::manifest::{FDO_VERSION, RuntimeKind, slugify};
use crate::pipeline::{FLATHUB_URL, parse_percent, pump};
use std::os::unix::process::CommandExt;
use std::process::{Command, Stdio};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, channel};
use std::time::Duration;

// ---------------------------------------------------------------- détection

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BuildSystem {
    Cargo,
    CMake,
    Meson,
    Autotools,
    Make,
    Go,
}

impl BuildSystem {
    pub fn label(&self) -> &'static str {
        match self {
            BuildSystem::Cargo => "Rust (Cargo)",
            BuildSystem::CMake => "C / C++ (CMake)",
            BuildSystem::Meson => "C / C++ (Meson)",
            BuildSystem::Autotools => "C / C++ (Autotools)",
            BuildSystem::Make => "C / C++ (Makefile)",
            BuildSystem::Go => "Go",
        }
    }

    /// Extension du SDK Freedesktop nécessaire (le C/C++ est déjà dans le SDK).
    pub fn extension(&self) -> Option<&'static str> {
        match self {
            BuildSystem::Cargo => Some("org.freedesktop.Sdk.Extension.rust-stable"),
            BuildSystem::Go => Some("org.freedesktop.Sdk.Extension.golang"),
            _ => None,
        }
    }

    pub fn extension_label(&self) -> &'static str {
        match self {
            BuildSystem::Cargo => "Extension Rust (rustc, cargo)",
            BuildSystem::Go => "Extension Go",
            _ => "",
        }
    }

    /// Chemin des outils de l'extension à l'intérieur du SDK.
    pub fn extension_bin(&self) -> Option<&'static str> {
        match self {
            BuildSystem::Cargo => Some("/usr/lib/sdk/rust-stable/bin"),
            BuildSystem::Go => Some("/usr/lib/sdk/golang/bin"),
            _ => None,
        }
    }

    /// Outils recherchés sur la machine (information seulement).
    fn host_tools(&self) -> &'static [(&'static str, &'static [&'static str])] {
        match self {
            BuildSystem::Cargo => &[("rustc", &["--version"]), ("cargo", &["--version"])],
            BuildSystem::CMake => &[("gcc", &["--version"]), ("g++", &["--version"]), ("cmake", &["--version"])],
            BuildSystem::Meson => &[("gcc", &["--version"]), ("meson", &["--version"])],
            BuildSystem::Autotools => &[("gcc", &["--version"]), ("make", &["--version"])],
            BuildSystem::Make => &[("gcc", &["--version"]), ("make", &["--version"])],
            BuildSystem::Go => &[("go", &["version"])],
        }
    }
}

#[derive(Debug, Clone)]
pub struct SourceProject {
    pub system: BuildSystem,
    pub bin_name: String,
    /// Vrai si le nom du binaire a été lu dans les fichiers du projet (et non deviné).
    pub bin_certain: bool,
    pub gui: bool,
    pub qt: bool,
    pub gtk: bool,
    pub notes: Vec<String>,
    /// Raison pour laquelle le projet ne produit visiblement aucun programme (bibliothèque seule…).
    pub blocker: Option<String>,
}

#[derive(Debug, Clone)]
pub struct BuildFile {
    /// Nom du fichier en minuscules.
    pub name: String,
    pub content: String,
}

/// Noms de fichiers (en minuscules) dont le contenu est lu pendant l'analyse.
pub const BUILD_FILE_NAMES: [&str; 12] = [
    "cargo.toml",
    "go.mod",
    "meson.build",
    "cmakelists.txt",
    "configure.ac",
    "configure.in",
    "makefile.am",
    "configure",
    "makefile",
    "gnumakefile",
    "package.json",
    "setup.py",
];

#[derive(Debug, Clone)]
pub enum Detection {
    Supported(SourceProject),
    Unsupported(&'static str),
}

fn valid_bin(s: &str) -> bool {
    !s.is_empty() && s.len() <= 80 && !s.starts_with('-') && s.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '+' | '-'))
}

/// Premier argument d'un appel `nom(...)`, sans guillemets ni crochets.
fn call_arg(text: &str, call: &str) -> Option<String> {
    let low = text.to_ascii_lowercase();
    let needle = format!("{call}(");
    let mut from = 0;
    while let Some(i) = low[from..].find(&needle) {
        let start = from + i;
        // Le nom doit commencer un mot (pas `my_project(`).
        let boundary = start == 0 || !low.as_bytes()[start - 1].is_ascii_alphanumeric() && low.as_bytes()[start - 1] != b'_';
        if boundary {
            let rest = &text[start + needle.len()..];
            let rest = rest.trim_start_matches(|c: char| c.is_whitespace() || matches!(c, '\'' | '"' | '['));
            let arg: String = rest.chars().take_while(|c| !c.is_whitespace() && !matches!(c, ',' | '\'' | '"' | ')' | ']')).collect();
            if !arg.is_empty() {
                return Some(arg);
            }
        }
        from = start + needle.len();
    }
    None
}

fn any_of(text: &str, words: &[&str]) -> bool {
    let low = text.to_ascii_lowercase();
    words.iter().any(|w| low.contains(w))
}

fn pick(name: Option<String>, fallback: &str) -> (String, bool) {
    match name.filter(|n| valid_bin(n) && !n.contains('$')) {
        Some(n) => (n, true),
        None => (slugify(fallback), false),
    }
}

fn project(system: BuildSystem, bin: (String, bool), content: &str, gui_words: &[&str]) -> SourceProject {
    SourceProject {
        system,
        bin_name: bin.0,
        bin_certain: bin.1,
        gui: any_of(content, gui_words),
        qt: any_of(content, &["qt5", "qt6", "find_package(qt", "dependency('qt", "dependency(\"qt"]),
        gtk: any_of(content, &["gtk+-3", "gtk4", "gtk3", "find_package(gtk", "dependency('gtk", "dependency(\"gtk"]),
        notes: vec![],
        blocker: None,
    }
}

const GUI_NATIVE: [&str; 9] = ["qt5", "qt6", "gtk", "sdl2", "sdl3", "sfml", "glfw", "wxwidgets", "raylib"];

/// `src_files` : chemins (relatifs à la racine du projet, en minuscules) des fichiers .rs et .go de l'archive.
pub fn detect(files: &[BuildFile], exec_names: &[String], has_elf: bool, fallback_name: &str, src_files: &[String]) -> Option<Detection> {
    if has_elf {
        return None;
    }
    let get = |n: &str| files.iter().find(|f| f.name == n);

    if let Some(f) = get("cargo.toml") {
        return Some(Detection::Supported(cargo(f, fallback_name, src_files)));
    }
    if let Some(f) = get("go.mod") {
        let module = f.content.lines().find_map(|l| l.trim().strip_prefix("module ")).map(|m| m.trim().trim_matches('"').to_string());
        let name = module.and_then(|m| {
            let mut segs: Vec<&str> = m.split('/').collect();
            if segs.len() > 1 && segs.last().is_some_and(|s| s.len() > 1 && s.starts_with('v') && s[1..].chars().all(|c| c.is_ascii_digit())) {
                segs.pop();
            }
            segs.last().map(|s| s.to_string())
        });
        let mut bin = pick(name, fallback_name);
        let has_main = src_files.iter().any(|x| x == "main.go" || x.starts_with("cmd/"));
        if !has_main {
            bin.1 = false;
        }
        let mut p = project(BuildSystem::Go, bin, &f.content, &["fyne.io", "gioui.org", "gotk3", "gotk4", "ebiten"]);
        if !has_main {
            p.notes.push("Aucun main.go ni dossier cmd/ : le point d'entrée n'a pas été repéré.".into());
        }
        p.notes.push("Les modules Go sont téléchargés pendant la compilation.".into());
        return Some(Detection::Supported(p));
    }
    if let Some(f) = get("meson.build") {
        let name = call_arg(&f.content, "executable").or_else(|| call_arg(&f.content, "project"));
        let certain_exe = call_arg(&f.content, "executable").is_some();
        let (n, c) = pick(name, fallback_name);
        let mut p = project(BuildSystem::Meson, (n, c && certain_exe), &f.content, &GUI_NATIVE);
        if !certain_exe {
            p.notes.push("Nom du binaire déduit du nom du projet.".into());
        }
        return Some(Detection::Supported(p));
    }
    if let Some(f) = get("cmakelists.txt") {
        let exe = call_arg(&f.content, "add_executable");
        let literal = exe.as_ref().is_some_and(|e| !e.contains('$'));
        let name = if literal { exe } else { call_arg(&f.content, "project") };
        let (n, c) = pick(name, fallback_name);
        let mut p = project(BuildSystem::CMake, (n, c && literal), &f.content, &GUI_NATIVE);
        if !literal {
            p.notes.push("Nom du binaire déduit du nom du projet.".into());
        }
        return Some(Detection::Supported(p));
    }
    let ac = get("configure.ac").or_else(|| get("configure.in"));
    let am = get("makefile.am");
    let only_autotools_scripts = exec_names.iter().all(|n| matches!(n.as_str(), "autogen.sh" | "bootstrap" | "config.guess" | "config.sub" | "install-sh" | "missing" | "depcomp" | "compile" | "ltmain.sh"));
    if ac.is_some() || am.is_some() || (get("configure").is_some() && only_autotools_scripts) {
        let from_am = am.and_then(|f| {
            f.content.lines().find_map(|l| {
                let t = l.trim();
                let rest = t.strip_prefix("bin_PROGRAMS")?.trim_start().strip_prefix('=')?;
                rest.split_whitespace().next().map(String::from)
            })
        });
        let from_ac = ac.and_then(|f| call_arg(&f.content, "ac_init"));
        let certain = from_am.as_ref().is_some_and(|n| valid_bin(n) && !n.contains('$'));
        let (n, c) = pick(from_am.or(from_ac), fallback_name);
        let text = format!("{}\n{}", ac.map(|f| f.content.as_str()).unwrap_or(""), am.map(|f| f.content.as_str()).unwrap_or(""));
        let mut p = project(BuildSystem::Autotools, (n, c && certain), &text, &GUI_NATIVE);
        if !(c && certain) {
            p.notes.push("Nom du binaire déduit du nom du projet.".into());
        }
        return Some(Detection::Supported(p));
    }
    if exec_names.is_empty() {
        if let Some(f) = get("makefile").or_else(|| get("gnumakefile")) {
            let name = f.content.lines().find_map(|l| {
                let (k, v) = l.split_once('=')?;
                let k = k.trim().trim_end_matches([':', '?', '+']).trim();
                if matches!(k, "TARGET" | "BIN" | "BINARY" | "PROG" | "PROGRAM" | "EXEC" | "NAME" | "APP") {
                    v.split_whitespace().next().map(String::from)
                } else {
                    None
                }
            });
            let (n, c) = pick(name, fallback_name);
            let mut p = project(BuildSystem::Make, (n, c), &f.content, &GUI_NATIVE);
            if !c {
                p.notes.push("Nom du binaire deviné : vérifiez-le.".into());
            }
            return Some(Detection::Supported(p));
        }
        if get("package.json").is_some() {
            return Some(Detection::Unsupported("Node.js"));
        }
        if get("setup.py").is_some() {
            return Some(Detection::Unsupported("Python"));
        }
    }
    None
}

fn cargo(f: &BuildFile, fallback: &str, src_files: &[String]) -> SourceProject {
    let table: Option<toml::Table> = f.content.parse().ok();
    let pkg = table.as_ref().and_then(|t| t.get("package")).and_then(|p| p.get("name")).and_then(|n| n.as_str()).map(String::from);
    let bins: Vec<String> = table
        .as_ref()
        .and_then(|t| t.get("bin"))
        .and_then(|b| b.as_array())
        .map(|a| a.iter().filter_map(|x| x.get("name").and_then(|n| n.as_str()).map(String::from)).collect())
        .unwrap_or_default();
    let workspace = table.as_ref().is_some_and(|t| t.contains_key("workspace")) && pkg.is_none();
    let name = match &pkg {
        Some(p) if bins.is_empty() || bins.contains(p) => Some(p.clone()),
        _ => bins.first().cloned().or(pkg.clone()),
    };
    let certain = name.is_some() && !workspace;
    let (n, _) = pick(name, fallback);
    let mut p = project(BuildSystem::Cargo, (n, certain), &f.content, &["eframe", "egui", "iced", "slint", "gtk4", "relm4", "tauri", "fltk", "floem", "winit", "sdl2", "bevy", "macroquad"]);
    let has_bin_file = src_files.iter().any(|x| x == "src/main.rs" || x.starts_with("src/bin/"));
    if !workspace && bins.is_empty() && !has_bin_file {
        p.blocker = Some("Ce projet Rust ne contient pas de programme (ni src/main.rs, ni section [[bin]]) : c'est une bibliothèque, il n'y a rien à lancer.".into());
    }
    if workspace {
        p.notes.push("Espace de travail Cargo : indiquez le nom du binaire à installer.".into());
    }
    p.notes.push("Les dépendances Rust sont téléchargées pendant la compilation (crates.io).".into());
    p
}

// ---------------------------------------------------------------- état de la machine

#[derive(Debug, Clone)]
pub struct HostTool {
    pub name: String,
    pub version: Option<String>,
}

pub fn scan_host_tools(system: BuildSystem) -> Vec<HostTool> {
    system
        .host_tools()
        .iter()
        .map(|(prog, args)| {
            let out = crate::host::cmd(prog).args(*args).env("LC_ALL", "C").stdin(Stdio::null()).stderr(Stdio::null()).output();
            let version = out.ok().filter(|o| o.status.success()).and_then(|o| String::from_utf8_lossy(&o.stdout).lines().next().map(|l| l.trim().to_string())).filter(|l| !l.is_empty());
            HostTool { name: (*prog).to_string(), version }
        })
        .collect()
}

#[derive(Debug, Clone, Default)]
pub struct FlatpakState {
    pub available: bool,
    /// (identifiant, branche) des runtimes et extensions installés.
    pub refs: Vec<(String, String)>,
    pub has_flathub: bool,
    /// (identifiant, branche) des runtimes et extensions proposés par Flathub (vide si injoignable).
    pub remote: Vec<(String, String)>,
    /// Idem `refs`, mais uniquement ceux installés pour l'utilisateur courant (les seuls qu'on peut retirer sans droits).
    pub user_refs: Vec<(String, String)>,
}

impl FlatpakState {
    pub fn has(&self, id: &str, branch: &str) -> bool {
        self.refs.iter().any(|(i, b)| i == id && b == branch)
    }
}

pub fn parse_refs(out: &str) -> Vec<(String, String)> {
    out.lines()
        .filter_map(|l| {
            let mut it = l.split_whitespace();
            let (id, branch) = (it.next()?, it.next()?);
            (id.contains('.') && !id.contains('/')).then(|| (id.to_string(), branch.to_string()))
        })
        .collect()
}

pub fn scan_flatpak() -> FlatpakState {
    let run = |args: &[&str]| crate::host::cmd("flatpak").args(args).env("LC_ALL", "C").stdin(Stdio::null()).stderr(Stdio::null()).output().ok().filter(|o| o.status.success());
    let Some(_) = run(&["--version"]) else { return FlatpakState::default() };
    let refs = run(&["list", "--runtime", "--columns=application,branch"]).map(|o| parse_refs(&String::from_utf8_lossy(&o.stdout))).unwrap_or_default();
    let has_flathub = run(&["remotes", "--user", "--columns=name"]).is_some_and(|o| String::from_utf8_lossy(&o.stdout).lines().any(|l| l.trim() == "flathub"));
    // Catalogue Flathub (utilisateur ou système) pour connaître les branches les plus récentes.
    // D'abord le cache local (instantané), puis le réseau (limité à 25 s) seulement si le cache est vide.
    let ls = |cached: bool, secs: u64| {
        let mut c = crate::host::cmd("flatpak");
        c.args(["remote-ls", "--runtime", "--columns=application,branch"]);
        if cached {
            c.arg("--cached");
        }
        c.arg("flathub").env("LC_ALL", "C");
        crate::host::output_timeout(c, secs).filter(|o| o.status.success()).map(|o| parse_refs(&String::from_utf8_lossy(&o.stdout))).unwrap_or_default()
    };
    let mut remote = ls(true, 10);
    if !remote.iter().any(|(i, _)| i == "org.freedesktop.Sdk") {
        remote = ls(false, 25);
    }
    let user_refs = run(&["list", "--user", "--runtime", "--columns=application,branch"]).map(|o| parse_refs(&String::from_utf8_lossy(&o.stdout))).unwrap_or_default();
    FlatpakState { available: true, refs, has_flathub, remote, user_refs }
}

fn version_key(v: &str) -> Vec<u32> {
    v.split('.').map(|x| x.parse().unwrap_or(0)).collect()
}

/// Branche du SDK Freedesktop à utiliser : la plus récente disponible (installée ou sur Flathub)
/// qui propose à la fois le runtime, le SDK et l'extension nécessaire (`ext`).
/// Les anciennes branches ont des compilateurs figés (ex. Rust 1.89 en 24.08).
pub fn pick_branch(state: &FlatpakState, ext: Option<&str>) -> String {
    let numeric = |b: &str| !b.is_empty() && b.split('.').all(|x| x.parse::<u32>().is_ok());
    let offered = |id: &str, b: &str| state.has(id, b) || state.remote.iter().any(|(i, br)| i == id && br == b);
    let mut ids = vec!["org.freedesktop.Platform", "org.freedesktop.Sdk"];
    ids.extend(ext);
    let mut best = FDO_VERSION.to_string();
    let candidates = state.refs.iter().chain(state.remote.iter()).filter(|(i, _)| i == "org.freedesktop.Sdk").map(|(_, b)| b.clone());
    for b in candidates {
        if numeric(&b) && version_key(&b) > version_key(&best) && ids.iter().all(|id| offered(id, &b)) {
            best = b;
        }
    }
    best
}

/// Anciennes branches des SDK et de leurs extensions (installées pour l'utilisateur) qu'on peut retirer :
/// celles strictement plus anciennes que la plus récente branche du SDK installée, hors `keep`.
pub fn cleanup_candidates(state: &FlatpakState, keep: &str) -> Vec<(String, String)> {
    let numeric = |b: &str| !b.is_empty() && b.split('.').all(|x| x.parse::<u32>().is_ok());
    let is_sdk = |id: &str| id == "org.freedesktop.Sdk" || id.starts_with("org.freedesktop.Sdk.Extension.");
    let newest = state.user_refs.iter().filter(|(i, b)| i == "org.freedesktop.Sdk" && numeric(b)).map(|(_, b)| version_key(b)).max();
    let Some(newest) = newest else { return vec![] };
    let mut v: Vec<_> = state
        .user_refs
        .iter()
        .filter(|(i, b)| is_sdk(i) && numeric(b) && b != keep && version_key(b) < newest)
        .cloned()
        .collect();
    v.sort();
    v
}

/// Commandes du ménage : retrait des anciens SDK, puis des runtimes devenus inutiles.
pub fn cleanup_plan(cands: &[(String, String)]) -> Vec<Vec<String>> {
    if cands.is_empty() {
        return vec![];
    }
    let mut c: Vec<String> = ["flatpak", "uninstall", "--user", "-y"].map(String::from).to_vec();
    c.extend(cands.iter().map(|(i, b)| format!("{i}//{b}")));
    let unused = ["flatpak", "uninstall", "--user", "--unused", "-y"].map(String::from).to_vec();
    vec![c, unused]
}

// ---------------------------------------------------------------- composants

#[derive(Debug, Clone)]
pub struct Component {
    pub id: String,
    pub branch: String,
    pub label: String,
    pub installed: bool,
}

impl Component {
    pub fn reference(&self) -> String {
        format!("{}//{}", self.id, self.branch)
    }
}

pub fn components(system: BuildSystem, runtime: RuntimeKind, version: &str, state: &FlatpakState) -> Vec<Component> {
    let mut v = vec![
        (runtime.platform().to_string(), "Runtime d'exécution"),
        (runtime.sdk().to_string(), "SDK (compilateurs et outils de build)"),
    ];
    if let Some(e) = system.extension() {
        v.push((e.to_string(), system.extension_label()));
    }
    v.into_iter().map(|(id, label)| Component { installed: state.has(&id, version), id, branch: version.to_string(), label: label.to_string() }).collect()
}

pub fn missing(c: &[Component]) -> Vec<Component> {
    c.iter().filter(|c| !c.installed).cloned().collect()
}

pub fn shell_line(argv: &[String]) -> String {
    argv.iter()
        .map(|a| if a.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '/' | ':' | '=' | '@' | '+' | '-')) { a.clone() } else { format!("'{}'", a.replace('\'', "'\\''")) })
        .collect::<Vec<_>>()
        .join(" ")
}

/// Commandes qui seront exécutées pour installer les composants manquants.
pub fn install_plan(missing: &[Component], has_flathub: bool) -> Vec<Vec<String>> {
    let mut plan = Vec::new();
    if missing.is_empty() {
        return plan;
    }
    if !has_flathub {
        plan.push(["flatpak", "remote-add", "--user", "--if-not-exists", "flathub", FLATHUB_URL].map(String::from).to_vec());
    }
    let mut c: Vec<String> = ["flatpak", "install", "--user", "-y", "flathub"].map(String::from).to_vec();
    c.extend(missing.iter().map(|m| m.reference()));
    plan.push(c);
    plan
}

// ---------------------------------------------------------------- installation

pub enum ToolMsg {
    Line(String),
    Progress(f32),
    Done(Result<(), String>),
}

pub struct ToolInstall {
    pub rx: Receiver<ToolMsg>,
    pub cancel: Arc<AtomicBool>,
}

fn explain(tail: &[String]) -> String {
    if let Some(d) = diagnose(tail).into_iter().next() {
        return format!("{} : {}", d.title, d.explanation);
    }
    tail.iter().rev().find(|l| !l.trim().is_empty()).cloned().unwrap_or_else(|| "L'installation a échoué.".into())
}

/// Exécute les commandes l'une après l'autre dans un thread.
pub fn spawn_install(cmds: Vec<Vec<String>>, notify: Arc<dyn Fn() + Send + Sync>) -> ToolInstall {
    let (tx, rx) = channel();
    let cancel = Arc::new(AtomicBool::new(false));
    let flag = cancel.clone();
    std::thread::spawn(move || {
        let mut tail: Vec<String> = Vec::new();
        let res = (|| -> Result<(), String> {
            for argv in &cmds {
                let _ = tx.send(ToolMsg::Line(format!("$ {}", shell_line(argv))));
                notify();
                let mut cmd = crate::host::cmd(&argv[0]);
                cmd.args(&argv[1..]).env("LC_ALL", "C").env("LANG", "C").stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped()).process_group(0);
                let mut child = cmd.spawn().map_err(|e| format!("impossible de lancer {} : {e}", argv[0]))?;
                let (ltx, lrx) = channel::<String>();
                let mut readers = Vec::new();
                if let Some(o) = child.stdout.take() {
                    readers.push(pump(o, ltx.clone()));
                }
                if let Some(e) = child.stderr.take() {
                    readers.push(pump(e, ltx.clone()));
                }
                drop(ltx);
                let pid = child.id();
                loop {
                    match lrx.recv_timeout(Duration::from_millis(120)) {
                        Ok(line) => {
                            tail.push(line.clone());
                            if let Some(p) = parse_percent(&line) {
                                let _ = tx.send(ToolMsg::Progress(p));
                            } else {
                                let _ = tx.send(ToolMsg::Line(line));
                            }
                            notify();
                        }
                        Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {}
                        Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => break,
                    }
                    if flag.load(Ordering::SeqCst) {
                        let _ = Command::new("kill").args(["-TERM", "--", &format!("-{pid}")]).status();
                        let _ = child.kill();
                        break;
                    }
                }
                for r in readers {
                    let _ = r.join();
                }
                let status = child.wait().map_err(|e| e.to_string())?;
                if flag.load(Ordering::SeqCst) {
                    return Err("Opération annulée.".into());
                }
                if !status.success() {
                    return Err(explain(&tail));
                }
            }
            Ok(())
        })();
        let _ = tx.send(ToolMsg::Done(res));
        notify();
    });
    ToolInstall { rx, cancel }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bf(name: &str, content: &str) -> BuildFile {
        BuildFile { name: name.into(), content: content.into() }
    }

    #[test]
    fn cargo_package_and_bin() {
        let f = bf("cargo.toml", "[package]\nname = \"mon-outil\"\nversion = \"1.0.0\"\n[dependencies]\neframe = \"0.30\"\n");
        let Some(Detection::Supported(p)) = detect(&[f], &[], false, "x", &["src/main.rs".to_string(), "main.go".to_string()]) else { panic!() };
        assert_eq!(p.system, BuildSystem::Cargo);
        assert_eq!(p.bin_name, "mon-outil");
        assert!(p.bin_certain && p.gui);
        let f = bf("cargo.toml", "[package]\nname = \"lib-name\"\n[[bin]]\nname = \"cli\"\npath = \"src/main.rs\"\n");
        let Some(Detection::Supported(p)) = detect(&[f], &[], false, "x", &["src/main.rs".to_string(), "main.go".to_string()]) else { panic!() };
        assert_eq!(p.bin_name, "cli");
    }

    #[test]
    fn cargo_workspace_is_uncertain() {
        let f = bf("cargo.toml", "[workspace]\nmembers = [\"a\", \"b\"]\n");
        let Some(Detection::Supported(p)) = detect(&[f], &[], false, "Mon Projet", &[]) else { panic!() };
        assert!(!p.bin_certain);
        assert_eq!(p.bin_name, "mon-projet");
        assert!(!p.notes.is_empty());
    }

    #[test]
    fn cmake_and_meson_names() {
        let f = bf("cmakelists.txt", "cmake_minimum_required(VERSION 3.10)\nproject(Demo)\nfind_package(Qt6 REQUIRED)\nadd_executable(demo main.cpp)\n");
        let Some(Detection::Supported(p)) = detect(&[f], &[], false, "x", &["src/main.rs".to_string(), "main.go".to_string()]) else { panic!() };
        assert_eq!((p.system, p.bin_name.as_str(), p.bin_certain, p.qt), (BuildSystem::CMake, "demo", true, true));
        let f = bf("cmakelists.txt", "project(Demo)\nadd_executable(${PROJECT_NAME} main.cpp)\n");
        let Some(Detection::Supported(p)) = detect(&[f], &[], false, "x", &["src/main.rs".to_string(), "main.go".to_string()]) else { panic!() };
        assert_eq!((p.bin_name.as_str(), p.bin_certain), ("Demo", false));
        let f = bf("meson.build", "project('viewer', 'c')\nexecutable('viewer-bin', 'main.c', dependencies: dependency('gtk4'))\n");
        let Some(Detection::Supported(p)) = detect(&[f], &[], false, "x", &["src/main.rs".to_string(), "main.go".to_string()]) else { panic!() };
        assert_eq!((p.system, p.bin_name.as_str(), p.gtk), (BuildSystem::Meson, "viewer-bin", true));
    }

    #[test]
    fn autotools_make_go() {
        let f = bf("configure.ac", "AC_INIT([hello], [1.0])\n");
        let Some(Detection::Supported(p)) = detect(&[f], &["autogen.sh".into()], false, "x", &[]) else { panic!() };
        assert_eq!((p.system, p.bin_name.as_str()), (BuildSystem::Autotools, "hello"));
        let f = bf("makefile", "CC=gcc\nTARGET = tool\nall: $(TARGET)\n");
        let Some(Detection::Supported(p)) = detect(&[f], &[], false, "x", &["src/main.rs".to_string(), "main.go".to_string()]) else { panic!() };
        assert_eq!((p.system, p.bin_name.as_str(), p.bin_certain), (BuildSystem::Make, "tool", true));
        let f = bf("go.mod", "module github.com/me/gotool/v2\n\ngo 1.22\n");
        let Some(Detection::Supported(p)) = detect(&[f], &[], false, "x", &["src/main.rs".to_string(), "main.go".to_string()]) else { panic!() };
        assert_eq!((p.system, p.bin_name.as_str()), (BuildSystem::Go, "gotool"));
    }

    #[test]
    fn prebuilt_or_script_archives_are_not_sources() {
        let f = bf("makefile", "all:\n");
        assert!(detect(std::slice::from_ref(&f), &[], true, "x", &[]).is_none());
        assert!(detect(&[f], &["run.sh".into()], false, "x", &[]).is_none());
        let f = bf("package.json", "{}");
        assert!(matches!(detect(std::slice::from_ref(&f), &[], false, "x", &[]), Some(Detection::Unsupported("Node.js"))));
        assert!(detect(&[f], &["start.sh".into()], false, "x", &[]).is_none());
    }

    #[test]
    fn branch_and_components() {
        let refs = parse_refs("org.freedesktop.Platform\t24.08\norg.freedesktop.Sdk\t24.08\norg.freedesktop.Sdk\t23.08\norg.gnome.Platform\t49\n");
        let st = FlatpakState { available: true, refs, has_flathub: true, remote: vec![], user_refs: vec![] };
        assert_eq!(pick_branch(&st, None), FDO_VERSION);
        let comps = components(BuildSystem::Cargo, RuntimeKind::Freedesktop, "24.08", &st);
        assert_eq!(comps.len(), 3);
        let miss = missing(&comps);
        assert_eq!(miss.len(), 1);
        assert_eq!(miss[0].reference(), "org.freedesktop.Sdk.Extension.rust-stable//24.08");
        let plan = install_plan(&miss, true);
        assert_eq!(plan.len(), 1);
        assert_eq!(shell_line(&plan[0]), "flatpak install --user -y flathub org.freedesktop.Sdk.Extension.rust-stable//24.08");
        let plan = install_plan(&miss, false);
        assert_eq!(plan.len(), 2);
        assert!(shell_line(&plan[0]).starts_with("flatpak remote-add --user --if-not-exists flathub https://"));
        assert_eq!(pick_branch(&FlatpakState::default(), None), FDO_VERSION);
        // C/C++ : pas d'extension
        assert_eq!(components(BuildSystem::CMake, RuntimeKind::Freedesktop, "25.08", &st).len(), 2);
    }

    #[test]
    fn newest_remote_branch_with_extension() {
        let mut st = FlatpakState { refs: parse_refs("org.freedesktop.Sdk\t24.08\n"), ..Default::default() };
        st.remote = parse_refs("org.freedesktop.Platform\t26.08\norg.freedesktop.Sdk\t26.08\norg.freedesktop.Platform\t25.08\norg.freedesktop.Sdk\t25.08\norg.freedesktop.Sdk.Extension.rust-stable\t25.08\norg.freedesktop.Sdk\t25.08beta\n");
        // 26.08 n'a pas l'extension Rust : on reste sur 25.08.
        assert_eq!(pick_branch(&st, Some("org.freedesktop.Sdk.Extension.rust-stable")), "25.08");
        // Sans extension requise, la plus récente gagne.
        assert_eq!(pick_branch(&st, None), "26.08");
    }

    #[test]
    fn cleanup_keeps_newest_and_selected() {
        let st = FlatpakState {
            user_refs: parse_refs("org.freedesktop.Sdk\t23.08\norg.freedesktop.Sdk\t24.08\norg.freedesktop.Sdk\t25.08\norg.freedesktop.Sdk.Extension.rust-stable\t24.08\norg.freedesktop.Sdk.Extension.rust-stable\t25.08\norg.freedesktop.Platform\t24.08\norg.gnome.Sdk\t47\n"),
            ..Default::default()
        };
        let c = cleanup_candidates(&st, "25.08");
        let refs: Vec<String> = c.iter().map(|(i, b)| format!("{i}//{b}")).collect();
        assert_eq!(refs, ["org.freedesktop.Sdk//23.08", "org.freedesktop.Sdk//24.08", "org.freedesktop.Sdk.Extension.rust-stable//24.08"]);
        assert_eq!(cleanup_plan(&c).len(), 2);
        assert!(cleanup_candidates(&FlatpakState::default(), "25.08").is_empty());
    }

    #[test]
    fn rust_library_is_blocked_and_go_without_main_is_uncertain() {
        let lib = bf("cargo.toml", "[package]\nname = \"mylib\"\nversion = \"1.0.0\"\n");
        let Some(Detection::Supported(p)) = detect(&[lib], &[], false, "x", &["src/lib.rs".to_string()]) else { panic!() };
        assert!(p.blocker.is_some());
        let go = bf("go.mod", "module example.com/tool\n");
        let Some(Detection::Supported(p)) = detect(&[go], &[], false, "x", &["util.go".to_string()]) else { panic!() };
        assert!(!p.bin_certain && p.blocker.is_none());
    }
}
