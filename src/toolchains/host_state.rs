//! État de la machine : outils installés, Flatpaks présents, branches proposées, nettoyage.

use super::model::BuildSystem;
use crate::manifest::FDO_VERSION;
use std::process::Stdio;

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
