//! Détection du système de build d'une archive de sources.

use super::helpers::*;
use super::java::java;
use super::model::*;
use super::node::node;
use super::python::python;
use super::rust::cargo;

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
