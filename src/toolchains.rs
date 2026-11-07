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
    Python,
    Node,
    /// Application Java déjà empaquetée (.jar exécutable) : rien à compiler, seul le JRE est ajouté.
    Java,
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
            BuildSystem::Python => "Python",
            BuildSystem::Node => "Node.js",
            BuildSystem::Java => "Java (JAR)",
        }
    }

    /// Extensions du SDK Freedesktop pouvant fournir l'outil, de la préférée à la moins préférée
    /// (le C/C++ et Python sont déjà dans le SDK). Les identifiants proposés dépendent de la branche.
    pub fn extension_candidates(&self) -> &'static [&'static str] {
        match self {
            BuildSystem::Cargo => &["org.freedesktop.Sdk.Extension.rust-stable"],
            BuildSystem::Go => &["org.freedesktop.Sdk.Extension.golang"],
            BuildSystem::Node => &[
                "org.freedesktop.Sdk.Extension.node24",
                "org.freedesktop.Sdk.Extension.node22",
                "org.freedesktop.Sdk.Extension.node20",
            ],
            BuildSystem::Java => &[
                "org.freedesktop.Sdk.Extension.openjdk25",
                "org.freedesktop.Sdk.Extension.openjdk21",
                "org.freedesktop.Sdk.Extension.openjdk",
            ],
            _ => &[],
        }
    }

    /// Vrai si une extension n'existe que pour le SDK Freedesktop (le runtime est alors imposé).
    pub fn needs_fdo(&self) -> bool {
        !self.extension_candidates().is_empty()
    }

    pub fn extension_label(&self) -> &'static str {
        match self {
            BuildSystem::Cargo => tr!("Extension Rust (rustc, cargo)"),
            BuildSystem::Go => tr!("Extension Go"),
            BuildSystem::Node => tr!("Extension Node.js"),
            BuildSystem::Java => tr!("Extension OpenJDK (Java)"),
            _ => "",
        }
    }

    /// Vrai si le projet est compilé ; faux pour Java (JAR déjà construit).
    pub fn compiles(&self) -> bool {
        !matches!(self, BuildSystem::Java)
    }

    /// Phrase d'introduction affichée dans l'analyse.
    pub fn intro(&self) -> &'static str {
        match self {
            BuildSystem::Python => {
                tr!(
                    "Les dépendances Python seront installées dans le bac à sable Flatpak (pip) puis le programme sera empaqueté avec l'interpréteur du runtime."
                )
            }
            BuildSystem::Node => {
                tr!(
                    "Les dépendances seront installées (npm) dans le bac à sable Flatpak, puis le programme sera empaqueté avec Node.js fourni par l'extension du SDK."
                )
            }
            BuildSystem::Java => {
                tr!(
                    "Ce programme Java est déjà construit : il sera empaqueté tel quel avec un environnement Java (JRE) fourni par l'extension OpenJDK."
                )
            }
            _ => {
                tr!(
                    "L'archive sera compilée dans le bac à sable Flatpak avec le SDK Freedesktop. La première compilation peut durer plusieurs minutes."
                )
            }
        }
    }

    /// Outils recherchés sur la machine (information seulement).
    fn host_tools(&self) -> &'static [(&'static str, &'static [&'static str])] {
        match self {
            BuildSystem::Cargo => &[("rustc", &["--version"]), ("cargo", &["--version"])],
            BuildSystem::CMake => &[
                ("gcc", &["--version"]),
                ("g++", &["--version"]),
                ("cmake", &["--version"]),
            ],
            BuildSystem::Meson => &[("gcc", &["--version"]), ("meson", &["--version"])],
            BuildSystem::Autotools => &[("gcc", &["--version"]), ("make", &["--version"])],
            BuildSystem::Make => &[("gcc", &["--version"]), ("make", &["--version"])],
            BuildSystem::Go => &[("go", &["version"])],
            BuildSystem::Python => &[("python3", &["--version"]), ("pip3", &["--version"])],
            BuildSystem::Node => &[("node", &["--version"]), ("npm", &["--version"])],
            BuildSystem::Java => &[("java", &["--version"])],
        }
    }
}

/// Dossier d'une extension du SDK : `/usr/lib/sdk/<fin de l'identifiant>`.
pub fn extension_dir(id: &str) -> String {
    format!("/usr/lib/sdk/{}", id.rsplit('.').next().unwrap_or(id))
}

/// Point d'entrée d'un projet (ce que le script de lancement exécute).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum Entry {
    #[default]
    None,
    /// Fichier du projet (chemin relatif à la racine) : script Python, point d'entrée Node.js ou JAR.
    File(String),
    /// Commande installée par pip (`[project.scripts]`).
    Console(String),
    /// Module Python lancé par `python3 -m`.
    Module(String),
}

#[derive(Debug, Clone)]
pub struct SourceProject {
    pub system: BuildSystem,
    pub bin_name: String,
    /// Vrai si le nom du binaire a été lu dans les fichiers du projet (et non deviné).
    pub bin_certain: bool,
    pub gui: bool,
    /// Accès direct au matériel (USB, HID).
    pub devices: bool,
    /// Accès réseau probablement nécessaire à l'exécution.
    pub network: bool,
    pub qt: bool,
    pub gtk: bool,
    pub notes: Vec<String>,
    /// Raison pour laquelle le projet ne produit visiblement aucun programme (bibliothèque seule…).
    pub blocker: Option<String>,
    /// Python, Node.js, Java : ce que le script de lancement exécute.
    pub entry: Entry,
    /// Autres fichiers possibles pour le point d'entrée (choix proposé à l'utilisateur).
    pub alternatives: Vec<String>,
    /// Python : le projet s'installe avec pip (pyproject.toml, setup.py).
    pub installable: bool,
    /// Python : un fichier requirements.txt est présent.
    pub requirements: bool,
    /// Node.js : package.json définit un script « build » à lancer.
    pub build_script: bool,
}

#[derive(Debug, Clone)]
pub struct BuildFile {
    /// Nom du fichier en minuscules.
    pub name: String,
    pub content: String,
}

/// Noms de fichiers (en minuscules) dont le contenu est lu pendant l'analyse.
pub const BUILD_FILE_NAMES: &[&str] = &[
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
    "setup.cfg",
    "pyproject.toml",
    "requirements.txt",
    "pom.xml",
    "build.gradle",
    "build.gradle.kts",
    "gemfile",
    "composer.json",
    "build.zig",
    "mix.exs",
];

/// Projets reconnus mais pas encore pris en charge : (fichier, langage).
const UNSUPPORTED: &[(&str, &str)] = &[
    ("pom.xml", "Java (Maven)"),
    ("build.gradle", "Java (Gradle)"),
    ("build.gradle.kts", "Java (Gradle)"),
    ("gemfile", "Ruby"),
    ("composer.json", "PHP"),
    ("build.zig", "Zig"),
    ("mix.exs", "Elixir"),
];

/// Langages pris en charge, pour les messages.
#[allow(dead_code)]
pub const SUPPORTED_LANGS: &str = "Rust, C/C++, Go, Python, Node.js et Java (JAR)";

/// Même liste que `SUPPORTED_LANGS`, dans la langue active.
#[allow(dead_code)]
pub fn supported_langs() -> &'static str {
    tr!("Rust, C/C++, Go, Python, Node.js et Java (JAR)")
}

#[derive(Debug, Clone)]
pub enum Detection {
    Supported(SourceProject),
    Unsupported(&'static str),
}

fn valid_bin(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 80
        && !s.starts_with('-')
        && s.chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '+' | '-'))
}

/// Premier argument d'un appel `nom(...)`, sans guillemets ni crochets.
fn call_arg(text: &str, call: &str) -> Option<String> {
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

fn project(
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

const GUI_NATIVE: [&str; 9] = [
    "qt5",
    "qt6",
    "gtk",
    "sdl2",
    "sdl3",
    "sfml",
    "glfw",
    "wxwidgets",
    "raylib",
];

/// `src_files` : chemins (relatifs à la racine du projet, casse d'origine) des fichiers .rs, .go, .py, .js
/// et des .jar exécutables de l'archive.
pub fn detect(
    files: &[BuildFile],
    exec_names: &[String],
    has_elf: bool,
    fallback_name: &str,
    src_files: &[String],
) -> Option<Detection> {
    if has_elf {
        return None;
    }
    let get = |n: &str| files.iter().find(|f| f.name == n);

    if let Some(f) = get("cargo.toml") {
        return Some(Detection::Supported(cargo(
            f,
            files,
            fallback_name,
            src_files,
        )));
    }
    if let Some(f) = get("go.mod") {
        let module = f
            .content
            .lines()
            .find_map(|l| l.trim().strip_prefix("module "))
            .map(|m| m.trim().trim_matches('"').to_string());
        let name = module.and_then(|m| {
            let mut segs: Vec<&str> = m.split('/').collect();
            if segs.len() > 1
                && segs.last().is_some_and(|s| {
                    s.len() > 1 && s.starts_with('v') && s[1..].chars().all(|c| c.is_ascii_digit())
                })
            {
                segs.pop();
            }
            segs.last().map(|s| s.to_string())
        });
        let mut bin = pick(name, fallback_name);
        let has_main = src_files.iter().any(|x| {
            let l = x.to_ascii_lowercase();
            l == "main.go" || l.starts_with("cmd/")
        });
        if !has_main {
            bin.1 = false;
        }
        let mut p = project(
            BuildSystem::Go,
            bin,
            &f.content,
            &["fyne.io", "gioui.org", "gotk3", "gotk4", "ebiten"],
        );
        if !has_main {
            p.notes.push(
                tr!("Aucun main.go ni dossier cmd/ : le point d'entrée n'a pas été repéré.").into(),
            );
        }
        p.notes
            .push(tr!("Les modules Go sont téléchargés pendant la compilation.").into());
        return Some(Detection::Supported(p));
    }
    if let Some(f) = get("meson.build") {
        let name = call_arg(&f.content, "executable").or_else(|| call_arg(&f.content, "project"));
        let certain_exe = call_arg(&f.content, "executable").is_some();
        let (n, c) = pick(name, fallback_name);
        let mut p = project(
            BuildSystem::Meson,
            (n, c && certain_exe),
            &f.content,
            &GUI_NATIVE,
        );
        if !certain_exe {
            p.notes
                .push(tr!("Nom du binaire déduit du nom du projet.").into());
        }
        return Some(Detection::Supported(p));
    }
    if let Some(f) = get("cmakelists.txt") {
        let exe = call_arg(&f.content, "add_executable");
        let literal = exe.as_ref().is_some_and(|e| !e.contains('$'));
        let name = if literal {
            exe
        } else {
            call_arg(&f.content, "project")
        };
        let (n, c) = pick(name, fallback_name);
        let mut p = project(
            BuildSystem::CMake,
            (n, c && literal),
            &f.content,
            &GUI_NATIVE,
        );
        if !literal {
            p.notes
                .push(tr!("Nom du binaire déduit du nom du projet.").into());
        }
        return Some(Detection::Supported(p));
    }
    let ac = get("configure.ac").or_else(|| get("configure.in"));
    let am = get("makefile.am");
    let only_autotools_scripts = exec_names.iter().all(|n| {
        matches!(
            n.as_str(),
            "autogen.sh"
                | "bootstrap"
                | "config.guess"
                | "config.sub"
                | "install-sh"
                | "missing"
                | "depcomp"
                | "compile"
                | "ltmain.sh"
        )
    });
    if ac.is_some() || am.is_some() || (get("configure").is_some() && only_autotools_scripts) {
        let from_am = am.and_then(|f| {
            f.content.lines().find_map(|l| {
                let t = l.trim();
                let rest = t
                    .strip_prefix("bin_PROGRAMS")?
                    .trim_start()
                    .strip_prefix('=')?;
                rest.split_whitespace().next().map(String::from)
            })
        });
        let from_ac = ac.and_then(|f| call_arg(&f.content, "ac_init"));
        let certain = from_am
            .as_ref()
            .is_some_and(|n| valid_bin(n) && !n.contains('$'));
        let (n, c) = pick(from_am.or(from_ac), fallback_name);
        let text = format!(
            "{}\n{}",
            ac.map(|f| f.content.as_str()).unwrap_or(""),
            am.map(|f| f.content.as_str()).unwrap_or("")
        );
        let mut p = project(
            BuildSystem::Autotools,
            (n, c && certain),
            &text,
            &GUI_NATIVE,
        );
        if !(c && certain) {
            p.notes
                .push(tr!("Nom du binaire déduit du nom du projet.").into());
        }
        return Some(Detection::Supported(p));
    }
    // Langages interprétés : seul un script .sh de lancement fait passer l'archive pour une application à base de script
    // (les autres exécutables, comme bin/outil ou setup.py, sont des fichiers du projet lui-même).
    if !exec_names.iter().any(|n| n.ends_with(".sh")) {
        let has_js = src_files.iter().any(|p| is_js(p));
        let root_make = get("makefile").or_else(|| get("gnumakefile")).is_some();
        if let Some(f) = get("package.json")
            && (has_js || !root_make)
        {
            return Some(Detection::Supported(node(f, fallback_name, src_files)));
        }
        if let Some(p) = python(files, fallback_name, src_files, root_make) {
            return Some(Detection::Supported(p));
        }
        let jars: Vec<String> = src_files
            .iter()
            .filter(|p| p.to_ascii_lowercase().ends_with(".jar"))
            .cloned()
            .collect();
        if !jars.is_empty() {
            return Some(Detection::Supported(java(&jars, fallback_name)));
        }
    }
    if exec_names.is_empty() {
        if let Some(f) = get("makefile").or_else(|| get("gnumakefile")) {
            let name = f.content.lines().find_map(|l| {
                let (k, v) = l.split_once('=')?;
                let k = k.trim().trim_end_matches([':', '?', '+']).trim();
                if matches!(
                    k,
                    "TARGET" | "BIN" | "BINARY" | "PROG" | "PROGRAM" | "EXEC" | "NAME" | "APP"
                ) {
                    v.split_whitespace().next().map(String::from)
                } else {
                    None
                }
            });
            let (n, c) = pick(name, fallback_name);
            let mut p = project(BuildSystem::Make, (n, c), &f.content, &GUI_NATIVE);
            if !c {
                p.notes
                    .push(tr!("Nom du binaire deviné : vérifiez-le.").into());
            }
            return Some(Detection::Supported(p));
        }
        for (file, lang) in UNSUPPORTED {
            if get(file).is_some() {
                return Some(Detection::Unsupported(lang));
            }
        }
    }
    None
}

fn is_js(p: &str) -> bool {
    let l = p.to_ascii_lowercase();
    l.ends_with(".js") || l.ends_with(".mjs") || l.ends_with(".cjs")
}

/// Chemin sans « ./ » initial.
fn clean_rel(p: &str) -> String {
    p.trim().trim_start_matches("./").to_string()
}

fn root_names(src: &[String], ext: fn(&str) -> bool) -> Vec<&String> {
    src.iter().filter(|p| !p.contains('/') && ext(p)).collect()
}

fn find_ci<'a>(list: &[&'a String], name: &str) -> Option<&'a String> {
    list.iter().find(|p| p.eq_ignore_ascii_case(name)).copied()
}

fn contains_any(text: &str, words: &[&str]) -> bool {
    any_of(text, words)
}

// ---------------------------------------------------------------- Node.js

fn node(f: &BuildFile, fallback: &str, src: &[String]) -> SourceProject {
    let json: Option<serde_json::Value> = serde_json::from_str(&f.content).ok();
    let text = |k: &str| {
        json.as_ref()
            .and_then(|j| j.get(k))
            .and_then(|v| v.as_str())
            .map(String::from)
    };
    let pkg_name = text("name")
        .map(|n| n.rsplit('/').next().unwrap_or("").to_string())
        .filter(|n| !n.is_empty());
    let dep_keys: Vec<String> = ["dependencies", "devDependencies", "optionalDependencies"]
        .iter()
        .filter_map(|k| {
            json.as_ref()
                .and_then(|j| j.get(*k))
                .and_then(|v| v.as_object())
        })
        .flat_map(|o| o.keys().cloned())
        .collect();
    let electron = dep_keys.iter().any(|d| d == "electron");
    let scripts = json
        .as_ref()
        .and_then(|j| j.get("scripts"))
        .and_then(|v| v.as_object());
    let build_script = scripts.is_some_and(|s| s.contains_key("build"));
    let start = scripts
        .and_then(|s| s.get("start"))
        .and_then(|v| v.as_str())
        .map(String::from);

    // Fichier à lancer : « bin », puis « main », puis « scripts.start » (node fichier.js), puis des noms usuels.
    let mut bin_key: Option<String> = None;
    let mut entry: Option<String> = None;
    if let Some(b) = json.as_ref().and_then(|j| j.get("bin")) {
        if let Some(path) = b.as_str() {
            entry = Some(clean_rel(path));
        } else if let Some(o) = b.as_object() {
            let pick_key = pkg_name
                .as_ref()
                .filter(|n| o.contains_key(*n))
                .cloned()
                .or_else(|| o.keys().next().cloned());
            if let Some(k) = pick_key {
                entry = o.get(&k).and_then(|v| v.as_str()).map(clean_rel);
                bin_key = Some(k);
            }
        }
    }
    if entry.is_none() {
        entry = text("main").map(|m| clean_rel(&m));
    }
    if entry.is_none()
        && let Some(st) = &start
    {
        let toks: Vec<&str> = st.split_whitespace().collect();
        if let Some(i) = toks.iter().position(|t| *t == "node")
            && let Some(file) = toks[i + 1..].iter().find(|t| !t.starts_with('-'))
            && *file != "."
        {
            entry = Some(clean_rel(file));
        }
    }
    let roots = root_names(src, is_js);
    if entry.as_deref().is_none_or(|e| e.is_empty() || e == ".") {
        entry = [
            "index.js",
            "server.js",
            "app.js",
            "main.js",
            "cli.js",
            "index.mjs",
            "main.mjs",
        ]
        .iter()
        .find_map(|c| find_ci(&roots, c).cloned());
    }
    let mut alternatives: Vec<String> = roots.iter().map(|s| (*s).clone()).collect();
    if let Some(e) = &entry
        && !alternatives.contains(e)
    {
        alternatives.insert(0, e.clone());
    }
    if alternatives.len() < 2 {
        alternatives.clear();
    }

    let name = bin_key.or(pkg_name);
    let (bin, _) = pick(name, fallback);
    let mut p = project(BuildSystem::Node, (bin, true), "", &[]);
    p.build_script = build_script;
    p.alternatives = alternatives;
    p.entry = entry.clone().map(Entry::File).unwrap_or_default();
    p.notes.push(tr!("Les dépendances npm sont téléchargées pendant la création du paquet (accès réseau nécessaire à ce moment-là).").into());
    if build_script {
        p.notes.push(
            tr!("Le script « build » de package.json sera exécuté avant l'empaquetage.").into(),
        );
    }
    if electron {
        p.blocker = Some(tr!("Ce projet utilise Electron : sa construction depuis les sources n'est pas prise en charge. Utilisez plutôt l'archive de l'application déjà compilée (celle que le projet distribue pour Linux).").into());
    } else if entry.is_none() {
        p.blocker = Some(tr!("Aucun point d'entrée Node.js trouvé (ni « bin », ni « main », ni « scripts.start », ni index.js) : il n'y a rien à lancer.").into());
    } else if let Some(e) = &entry
        && !build_script
        && !src.iter().any(|s| s.eq_ignore_ascii_case(e))
        && !e.contains('/')
    {
        p.notes.push(tr!("Le fichier « {e} » n'a pas été repéré à la racine de l'archive ; vérifiez-le dans « Point d'entrée ».", e = e));
    }
    let net = [
        "express",
        "fastify",
        "koa",
        "axios",
        "node-fetch",
        "got",
        "undici",
        "ws",
        "socket.io",
        "discord.js",
        "telegraf",
        "next",
        "nuxt",
        "request",
    ];
    p.network = dep_keys.iter().any(|d| net.contains(&d.as_str()));
    p
}

// ---------------------------------------------------------------- Python

fn toml_path<'a>(t: &'a toml::Table, path: &[&str]) -> Option<&'a toml::Value> {
    let mut cur = t.get(*path.first()?)?;
    for k in &path[1..] {
        cur = cur.get(k)?;
    }
    Some(cur)
}

/// Valeur entre guillemets qui suit `name=` ou `name =` (setup.py).
fn py_kwarg(text: &str, key: &str) -> Option<String> {
    let mut from = 0;
    while let Some(i) = text[from..].find(key) {
        let start = from + i;
        from = start + key.len();
        let before_ok = start == 0
            || !(text.as_bytes()[start - 1].is_ascii_alphanumeric()
                || text.as_bytes()[start - 1] == b'_');
        let rest = text[from..].trim_start();
        if before_ok && let Some(rest) = rest.strip_prefix('=') {
            let rest = rest.trim_start();
            if let Some(q) = rest.chars().next().filter(|c| matches!(c, '\'' | '"')) {
                let val: String = rest[1..]
                    .chars()
                    .take_while(|c| *c != q && *c != '\n')
                    .collect();
                if !val.is_empty() {
                    return Some(val);
                }
            }
        }
    }
    None
}

/// Commandes déclarées dans setup.py (`console_scripts`, `gui_scripts`) et setup.cfg (`[options.entry_points]`).
fn setup_scripts(setup_py: &str, setup_cfg: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let mut add = |name: &str| {
        let n = name.trim();
        if valid_bin(n) && !out.iter().any(|x| x == n) {
            out.push(n.to_string());
        }
    };
    for key in ["gui_scripts", "console_scripts"] {
        let mut from = 0;
        while let Some(i) = setup_py[from..].find(key) {
            let start = from + i + key.len();
            from = start;
            let mut end = (start + 600).min(setup_py.len());
            while !setup_py.is_char_boundary(end) {
                end -= 1;
            }
            let window = &setup_py[start..end];
            let window = window.split(']').next().unwrap_or(window);
            for (n, seg) in window.split(['\'', '"']).enumerate() {
                if n % 2 == 1
                    && let Some((name, target)) = seg.split_once('=')
                    && target.contains(':')
                {
                    add(name);
                }
            }
        }
    }
    let mut inside = false;
    for line in setup_cfg.lines() {
        let t = line.trim();
        if t.starts_with("console_scripts") || t.starts_with("gui_scripts") {
            inside = true;
            continue;
        }
        if inside {
            if t.is_empty() || !line.starts_with([' ', '\t']) {
                inside = false;
                continue;
            }
            if let Some((name, target)) = t.split_once('=')
                && target.contains(':')
            {
                add(name);
            }
        }
    }
    out
}

fn valid_module(s: &str) -> bool {
    !s.is_empty()
        && s.chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '.'))
        && !s.starts_with(|c: char| c.is_ascii_digit())
}

fn python(
    files: &[BuildFile],
    fallback: &str,
    src: &[String],
    root_make: bool,
) -> Option<SourceProject> {
    let get = |n: &str| files.iter().find(|f| f.name == n);
    let pyproject = get("pyproject.toml");
    let setup_py = get("setup.py");
    let setup_cfg = get("setup.cfg");
    let reqs = get("requirements.txt");
    let table: Option<toml::Table> = pyproject.and_then(|f| f.content.parse().ok());
    let has = |path: &[&str]| table.as_ref().and_then(|t| toml_path(t, path)).is_some();
    let installable = has(&["build-system"])
        || has(&["project"])
        || has(&["tool", "poetry"])
        || setup_py.is_some()
        || setup_cfg.is_some();

    let is_py = |p: &str| p.to_ascii_lowercase().ends_with(".py");
    let root_py = root_names(src, is_py);
    let loose = ["main.py", "__main__.py", "app.py", "run.py"]
        .iter()
        .find_map(|c| find_ci(&root_py, c).cloned());
    let helper =
        |p: &&String| !matches!(p.to_ascii_lowercase().as_str(), "setup.py" | "conftest.py");
    let candidates: Vec<&String> = root_py.iter().copied().filter(helper).collect();
    let only = if candidates.len() == 1 {
        Some(candidates[0].clone())
    } else {
        None
    };
    if !installable && loose.is_none() && !(reqs.is_some() && only.is_some()) {
        return None;
    }
    // Un Makefile à la racine et rien de propre à Python : c'est sans doute un projet C.
    if root_make && !installable && loose.is_none() {
        return None;
    }

    let proj_name = table
        .as_ref()
        .and_then(|t| {
            toml_path(t, &["project", "name"]).or_else(|| toml_path(t, &["tool", "poetry", "name"]))
        })
        .and_then(|v| v.as_str())
        .map(String::from)
        .or_else(|| setup_py.and_then(|f| py_kwarg(&f.content, "name")));

    let mut scripts: Vec<String> = Vec::new();
    if let Some(t) = &table {
        for path in [
            &["project", "gui-scripts"][..],
            &["project", "scripts"][..],
            &["tool", "poetry", "scripts"][..],
        ] {
            if let Some(tbl) = toml_path(t, path).and_then(|v| v.as_table()) {
                scripts.extend(tbl.keys().filter(|k| valid_bin(k)).cloned());
            }
        }
    }
    for n in setup_scripts(
        setup_py.map(|f| f.content.as_str()).unwrap_or(""),
        setup_cfg.map(|f| f.content.as_str()).unwrap_or(""),
    ) {
        if !scripts.contains(&n) {
            scripts.push(n);
        }
    }
    // Un script portant le nom du projet est le plus probable.
    if let Some(n) = &proj_name
        && let Some(i) = scripts.iter().position(|s| s == n)
    {
        let s = scripts.remove(i);
        scripts.insert(0, s);
    }

    // Module lançable (`python3 -m paquet`) : pkg/__main__.py ou src/pkg/__main__.py
    let module = src.iter().find_map(|p| {
        let parts: Vec<&str> = p.split('/').collect();
        let ok = |m: &str| valid_module(m).then(|| m.to_string());
        match parts.as_slice() {
            [m, f] if f.eq_ignore_ascii_case("__main__.py") => ok(m),
            ["src", m, f] if f.eq_ignore_ascii_case("__main__.py") => ok(m),
            _ => None,
        }
    });

    let mut entry = Entry::None;
    let mut bin_src = proj_name.clone();
    if installable && let Some(c) = scripts.first() {
        entry = Entry::Console(c.clone());
        bin_src = Some(c.clone());
    } else if installable && let Some(m) = &module {
        entry = Entry::Module(m.clone());
    } else if let Some(l) = loose.or(only) {
        entry = Entry::File(l);
    }

    let mut blob = String::new();
    for f in [pyproject, setup_py, setup_cfg, reqs].into_iter().flatten() {
        blob.push_str(&f.content);
        blob.push('\n');
    }
    let (bin, _) = pick(bin_src.map(|n| slugify(&n)), fallback);
    let mut p = project(BuildSystem::Python, (bin, true), "", &[]);
    p.gui = contains_any(
        &blob,
        &[
            "pyqt5",
            "pyqt6",
            "pyside2",
            "pyside6",
            "pygobject",
            "tkinter",
            "customtkinter",
            "pygame",
            "kivy",
            "wxpython",
            "dearpygui",
            "flet",
            "pyglet",
            "arcade",
            "toga",
            "textual-web",
        ],
    );
    p.gtk = contains_any(&blob, &["pygobject"]);
    p.installable = installable;
    p.requirements = reqs.is_some();
    p.network = contains_any(
        &blob,
        &[
            "requests",
            "httpx",
            "aiohttp",
            "urllib3",
            "flask",
            "django",
            "fastapi",
            "tornado",
            "websockets",
            "discord",
            "telegram",
            "pycurl",
            "paramiko",
        ],
    );
    if matches!(entry, Entry::File(_)) {
        let mut alts: Vec<String> = candidates.iter().map(|s| (*s).clone()).collect();
        if let Entry::File(e) = &entry
            && !alts.contains(e)
        {
            alts.insert(0, e.clone());
        }
        if alts.len() > 1 {
            p.alternatives = alts;
        }
    }
    if matches!(entry, Entry::None) {
        p.blocker = Some(if installable {
            tr!("Ce projet Python n'installe aucune commande (pas de [project.scripts]) et n'a ni __main__.py ni main.py : c'est une bibliothèque, il n'y a rien à lancer.").into()
        } else {
            tr!("Aucun point d'entrée Python trouvé (main.py, __main__.py, app.py ou run.py à la racine).").into()
        });
    }
    p.entry = entry;
    if !installable && reqs.is_none() {
        p.notes.push(tr!("Aucun requirements.txt ni pyproject.toml : seules les bibliothèques standard de Python seront disponibles.").into());
    }
    p.notes.push(tr!("Les dépendances Python sont téléchargées pendant la création du paquet (accès réseau nécessaire à ce moment-là).").into());
    if contains_any(&blob, &["tkinter", "customtkinter"]) {
        p.notes.push(tr!("Tkinter n'est pas toujours fourni par le runtime : si la fenêtre ne s'ouvre pas, ce projet n'est sans doute pas adapté.").into());
    }
    Some(p)
}

// ---------------------------------------------------------------- Java

fn java(jars: &[String], fallback: &str) -> SourceProject {
    let slug = slugify(fallback);
    let mut sorted = jars.to_vec();
    sorted.sort_by_key(|p| {
        (
            p.matches('/').count(),
            !p.to_ascii_lowercase().contains(&slug),
            p.len(),
            p.clone(),
        )
    });
    let main = sorted[0].clone();
    let (bin, _) = pick(Some(slug), fallback);
    let mut p = project(BuildSystem::Java, (bin, true), "", &[]);
    // Swing, JavaFX et SWT sont des bibliothèques de fenêtres : l'écran est accordé par défaut, à retirer pour un outil en ligne de commande.
    p.gui = true;
    p.entry = Entry::File(main);
    if sorted.len() > 1 {
        p.alternatives = sorted;
    }
    p.notes.push(tr!("Application graphique supposée : pour un outil en ligne de commande, cochez « Ouvrir dans un terminal » en mode avancé.").into());
    p
}

fn cargo(
    f: &BuildFile,
    files: &[BuildFile],
    fallback: &str,
    src_files: &[String],
) -> SourceProject {
    let table: Option<toml::Table> = f.content.parse().ok();
    let pkg = table
        .as_ref()
        .and_then(|t| t.get("package"))
        .and_then(|p| p.get("name"))
        .and_then(|n| n.as_str())
        .map(String::from);
    let bins: Vec<String> = table
        .as_ref()
        .and_then(|t| t.get("bin"))
        .and_then(|b| b.as_array())
        .map(|a| {
            a.iter()
                .filter_map(|x| x.get("name").and_then(|n| n.as_str()).map(String::from))
                .collect()
        })
        .unwrap_or_default();
    let workspace = table.as_ref().is_some_and(|t| t.contains_key("workspace")) && pkg.is_none();
    let name = match &pkg {
        Some(p) if bins.is_empty() || bins.contains(p) => Some(p.clone()),
        _ => bins.first().cloned().or(pkg.clone()),
    };
    let gui_words = [
        "eframe",
        "egui",
        "iced",
        "slint",
        "gtk4",
        "relm4",
        "tauri",
        "fltk",
        "floem",
        "winit",
        "sdl2",
        "bevy",
        "macroquad",
    ];
    // Espace de travail : le programme à installer est celui du membre qui dépend d'une bibliothèque graphique.
    let mut name = name;
    let mut workspace_gui_bin = false;
    if workspace {
        for m in files.iter().filter(|m| m.name == "member:cargo.toml") {
            if !any_of(&m.content, &gui_words) {
                continue;
            }
            let t: Option<toml::Table> = m.content.parse().ok();
            let bin = t
                .as_ref()
                .and_then(|t| t.get("bin"))
                .and_then(|b| b.as_array())
                .and_then(|a| a.first())
                .and_then(|x| x.get("name"))
                .and_then(|n| n.as_str())
                .or_else(|| {
                    t.as_ref()
                        .and_then(|t| t.get("package"))
                        .and_then(|p| p.get("name"))
                        .and_then(|n| n.as_str())
                });
            if let Some(b) = bin {
                name = Some(b.to_string());
                workspace_gui_bin = true;
                break;
            }
        }
    }
    let certain = name.is_some() && (!workspace || workspace_gui_bin);
    let (n, _) = pick(name, fallback);
    let all_toml: String = files
        .iter()
        .filter(|m| m.name == "cargo.toml" || m.name == "member:cargo.toml")
        .map(|m| m.content.as_str())
        .collect::<Vec<_>>()
        .join("\n");
    let mut p = project(BuildSystem::Cargo, (n, certain), &all_toml, &gui_words);
    p.devices = any_of(
        &all_toml,
        &["hidapi", "rusb", "libusb", "evdev", "udev", "serialport"],
    ) || src_files.iter().any(|x| {
        let l = x.to_ascii_lowercase();
        l.contains("hidraw") || l.contains("usb")
    });
    if p.devices {
        p.notes.push(tr!("Accès direct au matériel (USB/HID) détecté : la permission « tous les périphériques » est accordée.").into());
    }
    let has_bin_file = src_files.iter().any(|x| {
        let l = x.to_ascii_lowercase();
        l == "src/main.rs" || l.starts_with("src/bin/")
    });
    if !workspace && bins.is_empty() && !has_bin_file {
        p.blocker = Some(tr!("Ce projet Rust ne contient pas de programme (ni src/main.rs, ni section [[bin]]) : c'est une bibliothèque, il n'y a rien à lancer.").into());
    }
    if workspace && !workspace_gui_bin {
        p.notes
            .push(tr!("Espace de travail Cargo : indiquez le nom du binaire à installer.").into());
    }
    p.notes.push(
        tr!("Les dépendances Rust sont téléchargées pendant la compilation (crates.io).").into(),
    );
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
            let out = crate::host::cmd(prog)
                .args(*args)
                .env("LC_ALL", "C")
                .stdin(Stdio::null())
                .stderr(Stdio::null())
                .output();
            let version = out
                .ok()
                .filter(|o| o.status.success())
                .and_then(|o| {
                    String::from_utf8_lossy(&o.stdout)
                        .lines()
                        .next()
                        .map(|l| l.trim().to_string())
                })
                .filter(|l| !l.is_empty());
            HostTool {
                name: (*prog).to_string(),
                version,
            }
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
    let run = |args: &[&str]| {
        crate::host::cmd("flatpak")
            .args(args)
            .env("LC_ALL", "C")
            .stdin(Stdio::null())
            .stderr(Stdio::null())
            .output()
            .ok()
            .filter(|o| o.status.success())
    };
    let Some(_) = run(&["--version"]) else {
        return FlatpakState::default();
    };
    let refs = run(&["list", "--runtime", "--columns=application,branch"])
        .map(|o| parse_refs(&String::from_utf8_lossy(&o.stdout)))
        .unwrap_or_default();
    let has_flathub = run(&["remotes", "--user", "--columns=name"]).is_some_and(|o| {
        String::from_utf8_lossy(&o.stdout)
            .lines()
            .any(|l| l.trim() == "flathub")
    });
    // Catalogue Flathub (utilisateur ou système) pour connaître les branches les plus récentes.
    // D'abord le cache local (instantané), puis le réseau (limité à 25 s) seulement si le cache est vide.
    let ls = |cached: bool, secs: u64| {
        let mut c = crate::host::cmd("flatpak");
        c.args(["remote-ls", "--runtime", "--columns=application,branch"]);
        if cached {
            c.arg("--cached");
        }
        c.arg("flathub").env("LC_ALL", "C");
        crate::host::output_timeout(c, secs)
            .filter(|o| o.status.success())
            .map(|o| parse_refs(&String::from_utf8_lossy(&o.stdout)))
            .unwrap_or_default()
    };
    let mut remote = ls(true, 10);
    if !remote.iter().any(|(i, _)| i == "org.freedesktop.Sdk") {
        remote = ls(false, 25);
    }
    let user_refs = run(&[
        "list",
        "--user",
        "--runtime",
        "--columns=application,branch",
    ])
    .map(|o| parse_refs(&String::from_utf8_lossy(&o.stdout)))
    .unwrap_or_default();
    FlatpakState {
        available: true,
        refs,
        has_flathub,
        remote,
        user_refs,
    }
}

fn version_key(v: &str) -> Vec<u32> {
    v.split('.').map(|x| x.parse().unwrap_or(0)).collect()
}

fn offered(state: &FlatpakState, id: &str, branch: &str) -> bool {
    state.has(id, branch) || state.remote.iter().any(|(i, b)| i == id && b == branch)
}

/// Extension à utiliser sur la branche `version` : une extension déjà installée, sinon la plus récente
/// proposée par Flathub, sinon la préférée (qui sera à installer).
pub fn resolve_extension(
    system: BuildSystem,
    version: &str,
    state: &FlatpakState,
) -> Option<String> {
    let c = system.extension_candidates();
    c.iter()
        .find(|id| state.has(id, version))
        .or_else(|| c.iter().find(|id| offered(state, id, version)))
        .or(c.first())
        .map(|s| s.to_string())
}

/// Branche du SDK Freedesktop à utiliser : la plus récente disponible (installée ou sur Flathub)
/// qui propose le runtime, le SDK et au moins une des extensions `exts` (vide : aucune exigée).
/// Les anciennes branches ont des compilateurs figés (ex. Rust 1.89 en 24.08).
pub fn pick_branch(state: &FlatpakState, exts: &[&str]) -> String {
    let numeric = |b: &str| !b.is_empty() && b.split('.').all(|x| x.parse::<u32>().is_ok());
    let mut best = FDO_VERSION.to_string();
    let candidates = state
        .refs
        .iter()
        .chain(state.remote.iter())
        .filter(|(i, _)| i == "org.freedesktop.Sdk")
        .map(|(_, b)| b.clone());
    for b in candidates {
        let ok = offered(state, "org.freedesktop.Platform", &b)
            && offered(state, "org.freedesktop.Sdk", &b)
            && (exts.is_empty() || exts.iter().any(|e| offered(state, e, &b)));
        if numeric(&b) && version_key(&b) > version_key(&best) && ok {
            best = b;
        }
    }
    best
}

/// Anciennes branches des SDK et de leurs extensions (installées pour l'utilisateur) qu'on peut retirer :
/// celles strictement plus anciennes que la plus récente branche du SDK installée, hors `keep`.
pub fn cleanup_candidates(state: &FlatpakState, keep: &str) -> Vec<(String, String)> {
    let numeric = |b: &str| !b.is_empty() && b.split('.').all(|x| x.parse::<u32>().is_ok());
    let is_sdk =
        |id: &str| id == "org.freedesktop.Sdk" || id.starts_with("org.freedesktop.Sdk.Extension.");
    let newest = state
        .user_refs
        .iter()
        .filter(|(i, b)| i == "org.freedesktop.Sdk" && numeric(b))
        .map(|(_, b)| version_key(b))
        .max();
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
    let mut c: Vec<String> = ["flatpak", "uninstall", "--user", "-y"]
        .map(String::from)
        .to_vec();
    c.extend(cands.iter().map(|(i, b)| format!("{i}//{b}")));
    let unused = ["flatpak", "uninstall", "--user", "--unused", "-y"]
        .map(String::from)
        .to_vec();
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

pub fn components(
    system: BuildSystem,
    runtime: RuntimeKind,
    version: &str,
    state: &FlatpakState,
) -> Vec<Component> {
    let mut v = vec![
        (runtime.platform().to_string(), tr!("Runtime d'exécution")),
        (
            runtime.sdk().to_string(),
            tr!("SDK (compilateurs et outils de build)"),
        ),
    ];
    if let Some(e) = resolve_extension(system, version, state) {
        v.push((e, system.extension_label()));
    }
    v.into_iter()
        .map(|(id, label)| Component {
            installed: state.has(&id, version),
            id,
            branch: version.to_string(),
            label: label.to_string(),
        })
        .collect()
}

pub fn missing(c: &[Component]) -> Vec<Component> {
    c.iter().filter(|c| !c.installed).cloned().collect()
}

pub fn shell_line(argv: &[String]) -> String {
    argv.iter()
        .map(|a| {
            if a.chars().all(|c| {
                c.is_ascii_alphanumeric()
                    || matches!(c, '.' | '_' | '/' | ':' | '=' | '@' | '+' | '-')
            }) {
                a.clone()
            } else {
                format!("'{}'", a.replace('\'', "'\\''"))
            }
        })
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
        plan.push(
            [
                "flatpak",
                "remote-add",
                "--user",
                "--if-not-exists",
                "flathub",
                FLATHUB_URL,
            ]
            .map(String::from)
            .to_vec(),
        );
    }
    let mut c: Vec<String> = ["flatpak", "install", "--user", "-y", "flathub"]
        .map(String::from)
        .to_vec();
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
        return tr!(
            "{titre} : {explication}",
            titre = d.title,
            explication = d.explanation
        );
    }
    tail.iter()
        .rev()
        .find(|l| !l.trim().is_empty())
        .cloned()
        .unwrap_or_else(|| tr!("L'installation a échoué.").into())
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
                cmd.args(&argv[1..])
                    .env("LC_ALL", "C")
                    .env("LANG", "C")
                    .stdin(Stdio::null())
                    .stdout(Stdio::piped())
                    .stderr(Stdio::piped())
                    .process_group(0);
                let mut child = cmd.spawn().map_err(|e| {
                    tr!(
                        "impossible de lancer {programme} : {erreur}",
                        programme = argv[0],
                        erreur = e
                    )
                })?;
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
                        let _ = Command::new("kill")
                            .args(["-TERM", "--", &format!("-{pid}")])
                            .status();
                        let _ = child.kill();
                        break;
                    }
                }
                for r in readers {
                    let _ = r.join();
                }
                let status = child.wait().map_err(|e| e.to_string())?;
                if flag.load(Ordering::SeqCst) {
                    return Err(tr!("Opération annulée.").into());
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
        BuildFile {
            name: name.into(),
            content: content.into(),
        }
    }

    #[test]
    fn cargo_package_and_bin() {
        let f = bf(
            "cargo.toml",
            "[package]\nname = \"mon-outil\"\nversion = \"1.0.0\"\n[dependencies]\neframe = \"0.30\"\n",
        );
        let Some(Detection::Supported(p)) = detect(
            &[f],
            &[],
            false,
            "x",
            &["src/main.rs".to_string(), "main.go".to_string()],
        ) else {
            panic!()
        };
        assert_eq!(p.system, BuildSystem::Cargo);
        assert_eq!(p.bin_name, "mon-outil");
        assert!(p.bin_certain && p.gui);
        let f = bf(
            "cargo.toml",
            "[package]\nname = \"lib-name\"\n[[bin]]\nname = \"cli\"\npath = \"src/main.rs\"\n",
        );
        let Some(Detection::Supported(p)) = detect(
            &[f],
            &[],
            false,
            "x",
            &["src/main.rs".to_string(), "main.go".to_string()],
        ) else {
            panic!()
        };
        assert_eq!(p.bin_name, "cli");
    }

    #[test]
    fn cargo_workspace_is_uncertain() {
        let f = bf("cargo.toml", "[workspace]\nmembers = [\"a\", \"b\"]\n");
        let Some(Detection::Supported(p)) = detect(&[f], &[], false, "Mon Projet", &[]) else {
            panic!()
        };
        assert!(!p.bin_certain);
        assert_eq!(p.bin_name, "mon-projet");
        assert!(!p.notes.is_empty());
    }

    #[test]
    fn cargo_workspace_finds_gui_member_and_usb() {
        let root = bf(
            "cargo.toml",
            "[workspace]\nmembers = [\"proto\", \"gui\"]\n",
        );
        let gui = bf(
            "member:cargo.toml",
            "[package]\nname = \"xblades-gui\"\n[[bin]]\nname = \"xblades-gui\"\npath = \"src/main.rs\"\n[dependencies]\neframe = \"0.29\"\n",
        );
        let src = [
            "proto/src/hidraw.rs".to_string(),
            "gui/src/main.rs".to_string(),
        ];
        let Some(Detection::Supported(p)) = detect(&[root, gui], &[], false, "x", &src) else {
            panic!()
        };
        assert!(p.gui && p.devices && p.bin_certain);
        assert_eq!(p.bin_name, "xblades-gui");
    }

    #[test]
    fn cmake_and_meson_names() {
        let f = bf(
            "cmakelists.txt",
            "cmake_minimum_required(VERSION 3.10)\nproject(Demo)\nfind_package(Qt6 REQUIRED)\nadd_executable(demo main.cpp)\n",
        );
        let Some(Detection::Supported(p)) = detect(
            &[f],
            &[],
            false,
            "x",
            &["src/main.rs".to_string(), "main.go".to_string()],
        ) else {
            panic!()
        };
        assert_eq!(
            (p.system, p.bin_name.as_str(), p.bin_certain, p.qt),
            (BuildSystem::CMake, "demo", true, true)
        );
        let f = bf(
            "cmakelists.txt",
            "project(Demo)\nadd_executable(${PROJECT_NAME} main.cpp)\n",
        );
        let Some(Detection::Supported(p)) = detect(
            &[f],
            &[],
            false,
            "x",
            &["src/main.rs".to_string(), "main.go".to_string()],
        ) else {
            panic!()
        };
        assert_eq!((p.bin_name.as_str(), p.bin_certain), ("Demo", false));
        let f = bf(
            "meson.build",
            "project('viewer', 'c')\nexecutable('viewer-bin', 'main.c', dependencies: dependency('gtk4'))\n",
        );
        let Some(Detection::Supported(p)) = detect(
            &[f],
            &[],
            false,
            "x",
            &["src/main.rs".to_string(), "main.go".to_string()],
        ) else {
            panic!()
        };
        assert_eq!(
            (p.system, p.bin_name.as_str(), p.gtk),
            (BuildSystem::Meson, "viewer-bin", true)
        );
    }

    #[test]
    fn autotools_make_go() {
        let f = bf("configure.ac", "AC_INIT([hello], [1.0])\n");
        let Some(Detection::Supported(p)) = detect(&[f], &["autogen.sh".into()], false, "x", &[])
        else {
            panic!()
        };
        assert_eq!(
            (p.system, p.bin_name.as_str()),
            (BuildSystem::Autotools, "hello")
        );
        let f = bf("makefile", "CC=gcc\nTARGET = tool\nall: $(TARGET)\n");
        let Some(Detection::Supported(p)) = detect(
            &[f],
            &[],
            false,
            "x",
            &["src/main.rs".to_string(), "main.go".to_string()],
        ) else {
            panic!()
        };
        assert_eq!(
            (p.system, p.bin_name.as_str(), p.bin_certain),
            (BuildSystem::Make, "tool", true)
        );
        let f = bf("go.mod", "module github.com/me/gotool/v2\n\ngo 1.22\n");
        let Some(Detection::Supported(p)) = detect(
            &[f],
            &[],
            false,
            "x",
            &["src/main.rs".to_string(), "main.go".to_string()],
        ) else {
            panic!()
        };
        assert_eq!((p.system, p.bin_name.as_str()), (BuildSystem::Go, "gotool"));
    }

    #[test]
    fn prebuilt_or_script_archives_are_not_sources() {
        let f = bf("makefile", "all:\n");
        assert!(detect(std::slice::from_ref(&f), &[], true, "x", &[]).is_none());
        assert!(detect(&[f], &["run.sh".into()], false, "x", &[]).is_none());
        let f = bf("package.json", "{}");
        // package.json sans aucun point d'entrée : projet Node.js reconnu mais sans rien à lancer.
        let Some(Detection::Supported(p)) = detect(std::slice::from_ref(&f), &[], false, "x", &[])
        else {
            panic!()
        };
        assert!(p.blocker.is_some());
        assert!(detect(&[f], &["start.sh".into()], false, "x", &[]).is_none());
    }

    #[test]
    fn branch_and_components() {
        let refs = parse_refs(
            "org.freedesktop.Platform\t24.08\norg.freedesktop.Sdk\t24.08\norg.freedesktop.Sdk\t23.08\norg.gnome.Platform\t49\n",
        );
        let st = FlatpakState {
            available: true,
            refs,
            has_flathub: true,
            remote: vec![],
            user_refs: vec![],
        };
        assert_eq!(pick_branch(&st, &[]), FDO_VERSION);
        let comps = components(BuildSystem::Cargo, RuntimeKind::Freedesktop, "24.08", &st);
        assert_eq!(comps.len(), 3);
        let miss = missing(&comps);
        assert_eq!(miss.len(), 1);
        assert_eq!(
            miss[0].reference(),
            "org.freedesktop.Sdk.Extension.rust-stable//24.08"
        );
        let plan = install_plan(&miss, true);
        assert_eq!(plan.len(), 1);
        assert_eq!(
            shell_line(&plan[0]),
            "flatpak install --user -y flathub org.freedesktop.Sdk.Extension.rust-stable//24.08"
        );
        let plan = install_plan(&miss, false);
        assert_eq!(plan.len(), 2);
        assert!(
            shell_line(&plan[0])
                .starts_with("flatpak remote-add --user --if-not-exists flathub https://")
        );
        assert_eq!(pick_branch(&FlatpakState::default(), &[]), FDO_VERSION);
        // C/C++ : pas d'extension
        assert_eq!(
            components(BuildSystem::CMake, RuntimeKind::Freedesktop, "25.08", &st).len(),
            2
        );
    }

    #[test]
    fn newest_remote_branch_with_extension() {
        let mut st = FlatpakState {
            refs: parse_refs("org.freedesktop.Sdk\t24.08\n"),
            ..Default::default()
        };
        st.remote = parse_refs(
            "org.freedesktop.Platform\t26.08\norg.freedesktop.Sdk\t26.08\norg.freedesktop.Platform\t25.08\norg.freedesktop.Sdk\t25.08\norg.freedesktop.Sdk.Extension.rust-stable\t25.08\norg.freedesktop.Sdk\t25.08beta\n",
        );
        // 26.08 n'a pas l'extension Rust : on reste sur 25.08.
        assert_eq!(
            pick_branch(&st, &["org.freedesktop.Sdk.Extension.rust-stable"]),
            "25.08"
        );
        // Sans extension requise, la plus récente gagne.
        assert_eq!(pick_branch(&st, &[]), "26.08");
    }

    #[test]
    fn cleanup_keeps_newest_and_selected() {
        let st = FlatpakState {
            user_refs: parse_refs(
                "org.freedesktop.Sdk\t23.08\norg.freedesktop.Sdk\t24.08\norg.freedesktop.Sdk\t25.08\norg.freedesktop.Sdk.Extension.rust-stable\t24.08\norg.freedesktop.Sdk.Extension.rust-stable\t25.08\norg.freedesktop.Platform\t24.08\norg.gnome.Sdk\t47\n",
            ),
            ..Default::default()
        };
        let c = cleanup_candidates(&st, "25.08");
        let refs: Vec<String> = c.iter().map(|(i, b)| format!("{i}//{b}")).collect();
        assert_eq!(
            refs,
            [
                "org.freedesktop.Sdk//23.08",
                "org.freedesktop.Sdk//24.08",
                "org.freedesktop.Sdk.Extension.rust-stable//24.08"
            ]
        );
        assert_eq!(cleanup_plan(&c).len(), 2);
        assert!(cleanup_candidates(&FlatpakState::default(), "25.08").is_empty());
    }

    #[test]
    fn rust_library_is_blocked_and_go_without_main_is_uncertain() {
        let lib = bf(
            "cargo.toml",
            "[package]\nname = \"mylib\"\nversion = \"1.0.0\"\n",
        );
        let Some(Detection::Supported(p)) =
            detect(&[lib], &[], false, "x", &["src/lib.rs".to_string()])
        else {
            panic!()
        };
        assert!(p.blocker.is_some());
        let go = bf("go.mod", "module example.com/tool\n");
        let Some(Detection::Supported(p)) =
            detect(&[go], &[], false, "x", &["util.go".to_string()])
        else {
            panic!()
        };
        assert!(!p.bin_certain && p.blocker.is_none());
    }
}
