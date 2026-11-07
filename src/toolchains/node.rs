//! Projets Node.js / Electron.

use super::helpers::*;
use super::model::*;

pub(super) fn node(f: &BuildFile, fallback: &str, src: &[String]) -> SourceProject {
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
