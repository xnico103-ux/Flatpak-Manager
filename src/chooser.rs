//! Sélecteur de fichiers de secours, utilisé quand le portail `xdg-desktop-portal` est absent
//! (certains environnements légers ou mal configurés) : zenity (GNOME/GTK) ou kdialog (KDE).

use std::path::{Path, PathBuf};
use std::process::Stdio;

pub struct Filter<'a> {
    pub label: &'a str,
    pub exts: &'a [&'a str],
}

fn kde() -> bool {
    std::env::var("XDG_CURRENT_DESKTOP").is_ok_and(|d| d.to_uppercase().contains("KDE"))
}

fn run(prog: &str, args: &[String]) -> Option<Option<PathBuf>> {
    let out = crate::host::cmd(prog)
        .args(args)
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output();
    match out {
        // Programme absent : on essaie le suivant.
        Err(_) => None,
        Ok(o) => {
            // Code de sortie 127 : commande introuvable (via flatpak-spawn par exemple).
            if o.status.code() == Some(127) {
                return None;
            }
            let s = String::from_utf8_lossy(&o.stdout)
                .lines()
                .next()
                .unwrap_or("")
                .trim()
                .to_string();
            Some((o.status.success() && !s.is_empty()).then(|| PathBuf::from(s)))
        }
    }
}

fn order() -> [&'static str; 2] {
    if kde() {
        ["kdialog", "zenity"]
    } else {
        ["zenity", "kdialog"]
    }
}

/// Choisir un fichier existant. `None` : annulé, ou aucun sélecteur installé.
pub fn open(title: &str, dir: &Path, filter: &Filter) -> Option<PathBuf> {
    for prog in order() {
        let args: Vec<String> = if prog == "zenity" {
            let pats: Vec<String> = filter.exts.iter().map(|e| format!("*.{e}")).collect();
            vec![
                "--file-selection".into(),
                format!("--title={title}"),
                format!("--filename={}/", dir.display()),
                format!("--file-filter={} | {}", filter.label, pats.join(" ")),
            ]
        } else {
            let pats: Vec<String> = filter.exts.iter().map(|e| format!("*.{e}")).collect();
            vec![
                "--title".into(),
                title.into(),
                "--getopenfilename".into(),
                dir.display().to_string(),
                format!("{}|{}", pats.join(" "), filter.label),
            ]
        };
        if let Some(r) = run(prog, &args) {
            return r;
        }
    }
    None
}

/// Choisir où enregistrer. `None` : annulé, ou aucun sélecteur installé.
pub fn save(title: &str, dir: &Path, name: &str, filter: &Filter) -> Option<PathBuf> {
    for prog in order() {
        let target = dir.join(name);
        let args: Vec<String> = if prog == "zenity" {
            vec![
                "--file-selection".into(),
                "--save".into(),
                "--confirm-overwrite".into(),
                format!("--title={title}"),
                format!("--filename={}", target.display()),
            ]
        } else {
            let pats: Vec<String> = filter.exts.iter().map(|e| format!("*.{e}")).collect();
            vec![
                "--title".into(),
                title.into(),
                "--getsavefilename".into(),
                target.display().to_string(),
                format!("{}|{}", pats.join(" "), filter.label),
            ]
        };
        if let Some(r) = run(prog, &args) {
            return r;
        }
    }
    None
}

/// Vrai si au moins un des deux sélecteurs de secours est installé.
pub fn available() -> bool {
    ["zenity", "kdialog"].iter().any(|p| {
        crate::host::cmd(p)
            .arg("--version")
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .is_ok_and(|s| s.success())
    })
}
