//! Projets Python.

use super::helpers::*;
use super::model::*;
use crate::manifest::slugify;

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

pub(super) fn python(
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
