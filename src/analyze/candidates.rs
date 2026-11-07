//! Classement des candidats : identité de l'application, exécutables, icônes, dossiers de bibliothèques.

use super::model::*;
use super::paths::{guess_name_version, norm};
use super::scan::{Layout, Raw};
use std::collections::BTreeMap;
use std::path::Path;

/// Ce qui permet de reconnaître « l'application » : son .desktop et son nom.
pub(super) struct Identity {
    pub desktop: Option<DesktopInfo>,
    pub desktop_exec: Option<String>,
    pub base_name: String,
    /// Nom normalisé (minuscules, sans ponctuation), pour les comparaisons.
    pub nbase: String,
}

/// Choisit le .desktop le moins profond et en déduit le nom de l'application.
pub(super) fn identify(
    desktops: Vec<(Raw, DesktopInfo)>,
    layout: &Layout,
    guess_name: &str,
) -> Identity {
    let top_dir = &layout.top_dir;
    let desktop = desktops
        .into_iter()
        .min_by_key(|(r, _)| r.parts.len())
        .map(|(_, d)| d);
    let desktop_exec = desktop.as_ref().and_then(super::paths::desktop_exec_base);
    let top_name = top_dir.as_deref().map(|t| guess_name_version(t).0);
    let base_name = desktop
        .as_ref()
        .and_then(|d| d.name.clone())
        .or(top_name)
        .unwrap_or_else(|| guess_name.to_string());
    let nbase = norm(&base_name);
    Identity {
        desktop,
        desktop_exec,
        base_name,
        nbase,
    }
}

/// Exécutables classés du plus probable au moins probable.
pub(super) fn rank_executables(
    raws_exec: Vec<(Raw, ExecKind)>,
    layout: &Layout,
    id: &Identity,
) -> Vec<ExecCandidate> {
    let strip = |parts: &[String]| layout.strip(parts);
    let desktop_exec = id.desktop_exec.clone();
    let nbase = id.nbase.clone();
    let mut executables: Vec<ExecCandidate> = raws_exec
        .into_iter()
        .filter_map(|(r, kind)| {
            let rel = strip(&r.parts);
            if rel.is_empty() {
                return None;
            }
            let fname = r.parts.last()?.clone();
            let nf = norm(&fname);
            let mut s = 0i32;
            if desktop_exec.as_deref() == Some(fname.as_str()) {
                s += 120;
            }
            if !nbase.is_empty() && nf == nbase {
                s += 60;
            } else if !nbase.is_empty()
                && (nf.contains(&nbase) || nbase.contains(&nf))
                && nf.len() > 2
            {
                s += 20;
            }
            let rparts: Vec<&str> = rel.split('/').collect();
            s -= (rparts.len() as i32 - 1) * 8;
            if rparts.len() >= 2 && rparts[rparts.len() - 2] == "bin" {
                s += 15;
            }
            if rparts[..rparts.len() - 1].iter().any(|p| {
                matches!(
                    *p,
                    "lib"
                        | "lib64"
                        | "libexec"
                        | "share"
                        | "include"
                        | "node_modules"
                        | "plugins"
                        | "site-packages"
                        | "locales"
                )
            }) {
                s -= 40;
            }
            for bad in [
                "install",
                "uninstall",
                "update",
                "setup",
                "crash",
                "sandbox",
                "helper",
                "test",
            ] {
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
            Some(ExecCandidate {
                raw: r.raw,
                rel,
                kind,
                size: r.size,
                score: s,
            })
        })
        .collect();
    executables.sort_by(|a, b| b.score.cmp(&a.score).then(a.rel.len().cmp(&b.rel.len())));
    executables
}

/// Icônes classées, huit au plus.
pub(super) fn rank_icons(raws_icon: Vec<(Raw, bool, u32)>, id: &Identity) -> Vec<IconCandidate> {
    let desktop = &id.desktop;
    let nbase = id.nbase.clone();
    let icon_name = desktop.as_ref().and_then(|d| d.icon.clone());
    let nicon = icon_name.as_deref().map(norm);
    let mut icons: Vec<IconCandidate> = raws_icon
        .into_iter()
        .filter(|(r, _, w)| {
            let low = r.raw.to_lowercase();
            !(low.contains("node_modules")
                || low.contains("/locale")
                || low.contains("/doc")
                || low.contains("/test"))
                && (*w == 0 || (16..=1024).contains(w))
        })
        .map(|(r, is_svg, width)| {
            let stem = r
                .parts
                .last()
                .and_then(|n| Path::new(n).file_stem())
                .map(|s| norm(&s.to_string_lossy()))
                .unwrap_or_default();
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
            IconCandidate {
                raw: r.raw,
                is_svg,
                score: s,
            }
        })
        .filter(|i| i.score > 0)
        .collect();
    icons.sort_by_key(|b| std::cmp::Reverse(b.score));
    icons.truncate(8);
    icons
}

/// Les dossiers de bibliothèques embarquées les plus fournis (six au plus), sans le dossier racine.
pub(super) fn top_lib_dirs(lib_dirs: BTreeMap<String, usize>, layout: &Layout) -> Vec<String> {
    let top_dir = &layout.top_dir;
    let mut lib_dir_vec: Vec<(String, usize)> = lib_dirs.into_iter().collect();
    lib_dir_vec.sort_by_key(|b| std::cmp::Reverse(b.1));
    let lib_dirs: Vec<String> = lib_dir_vec
        .into_iter()
        .take(6)
        .map(|(d, _)| {
            let p: Vec<String> = d.split('/').map(String::from).collect();
            if top_dir.is_some() && !p.is_empty() {
                p[1..].join("/")
            } else {
                d
            }
        })
        .collect();
    lib_dirs
}
