//! Gestion des Flatpak déjà installés : liste et désinstallation.

use std::process::Stdio;

#[derive(Debug, Clone, PartialEq)]
pub struct Installed {
    pub id: String,
    pub name: String,
    pub version: String,
    pub branch: String,
    /// « user », « system » ou le nom d'une installation personnalisée.
    pub installation: String,
    pub size: String,
    pub runtime: bool,
}

impl Installed {
    pub fn reference(&self) -> String {
        format!("{}//{}", self.id, self.branch)
    }
    pub fn is_user(&self) -> bool {
        self.installation == "user"
    }
}

/// Lecture de la sortie de `flatpak list --columns=application,name,version,branch,installation,size`.
pub fn parse_list(out: &str, runtime: bool) -> Vec<Installed> {
    out.lines()
        .filter_map(|l| {
            let f: Vec<&str> = l.split('\t').map(str::trim).collect();
            if f.len() < 5 || !f[0].contains('.') || f[0].contains(' ') {
                return None;
            }
            Some(Installed {
                id: f[0].to_string(),
                name: if f[1].is_empty() {
                    f[0].to_string()
                } else {
                    f[1].to_string()
                },
                version: f[2].to_string(),
                branch: f[3].to_string(),
                installation: f[4].to_string(),
                size: f.get(5).copied().unwrap_or("").to_string(),
                runtime,
            })
        })
        .collect()
}

pub fn list_installed() -> Result<Vec<Installed>, String> {
    let run = |kind: &str| {
        crate::host::cmd("flatpak")
            .args([
                "list",
                kind,
                "--columns=application,name,version,branch,installation,size",
            ])
            .env("LC_ALL", "C")
            .stdin(Stdio::null())
            .stderr(Stdio::null())
            .output()
    };
    let apps = run("--app").map_err(|e| tr!("impossible de lancer flatpak : {e}", e = e))?;
    if !apps.status.success() {
        return Err(tr!("flatpak n'a pas pu lister les applications installées.").into());
    }
    let mut v = parse_list(&String::from_utf8_lossy(&apps.stdout), false);
    if let Ok(rt) = run("--runtime") {
        v.extend(parse_list(&String::from_utf8_lossy(&rt.stdout), true));
    }
    v.sort_by(|a, b| {
        a.runtime
            .cmp(&b.runtime)
            .then(a.name.to_lowercase().cmp(&b.name.to_lowercase()))
            .then(a.branch.cmp(&b.branch))
    });
    Ok(v)
}

fn scope(inst: &str) -> String {
    match inst {
        "user" => "--user".into(),
        "system" => "--system".into(),
        other => format!("--installation={other}"),
    }
}

/// `flatpak uninstall <portée> -y [--delete-data] <id>//<branche>`
pub fn uninstall_plan(item: &Installed, delete_data: bool) -> Vec<Vec<String>> {
    let mut c: Vec<String> = ["flatpak", "uninstall"].map(String::from).to_vec();
    c.push(scope(&item.installation));
    c.push("-y".into());
    if delete_data && !item.runtime {
        c.push("--delete-data".into());
    }
    c.push(item.reference());
    vec![c]
}

/// Retire les runtimes qu'aucune application installée n'utilise.
pub fn unused_plan() -> Vec<Vec<String>> {
    vec![
        ["flatpak", "uninstall", "--user", "--unused", "-y"]
            .map(String::from)
            .to_vec(),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_tab_separated_list() {
        let out = "org.mozilla.firefox\tFirefox\t140.0\tstable\tsystem\t250.1 MB\ncom.example.App\tMon App\t\tstable\tuser\t1.0 MB\nbruit sans id\n";
        let v = parse_list(out, false);
        assert_eq!(v.len(), 2);
        assert_eq!(v[1].name, "Mon App");
        assert_eq!(v[1].version, "");
        assert!(v[1].is_user() && !v[0].is_user());
    }

    #[test]
    fn uninstall_commands() {
        let it = Installed {
            id: "com.example.App".into(),
            name: "A".into(),
            version: "".into(),
            branch: "stable".into(),
            installation: "user".into(),
            size: "".into(),
            runtime: false,
        };
        assert_eq!(
            uninstall_plan(&it, false)[0].join(" "),
            "flatpak uninstall --user -y com.example.App//stable"
        );
        assert_eq!(
            uninstall_plan(&it, true)[0].join(" "),
            "flatpak uninstall --user -y --delete-data com.example.App//stable"
        );
        let sys = Installed {
            installation: "system".into(),
            runtime: true,
            ..it
        };
        assert_eq!(
            uninstall_plan(&sys, true)[0].join(" "),
            "flatpak uninstall --system -y com.example.App//stable"
        );
    }
}

// ---------------------------------------------------------------- mises à jour

#[derive(Debug, Clone, PartialEq)]
pub struct Update {
    pub id: String,
    pub name: String,
    pub branch: String,
    pub installation: String,
    pub size: String,
    pub runtime: bool,
}

impl Update {
    pub fn reference(&self) -> String {
        format!("{}//{}", self.id, self.branch)
    }
}

/// Lecture de `flatpak remote-ls --updates --columns=application,name,branch,download-size,ref`
/// (cette commande n'a pas de colonne « installation » : elle est donnée par la portée interrogée).
pub fn parse_updates(out: &str, installation: &str) -> Vec<Update> {
    out.lines()
        .filter_map(|l| {
            let f: Vec<&str> = l.split('\t').map(str::trim).collect();
            if f.len() < 3 || !f[0].contains('.') || f[0].contains(' ') {
                return None;
            }
            // La colonne « ref » (app/... ou runtime/...) distingue applications et runtimes.
            let runtime = f.get(4).is_some_and(|r| r.starts_with("runtime/"));
            Some(Update {
                id: f[0].to_string(),
                name: if f[1].is_empty() {
                    f[0].to_string()
                } else {
                    f[1].to_string()
                },
                branch: f[2].to_string(),
                installation: installation.to_string(),
                size: f.get(3).copied().unwrap_or("").to_string(),
                runtime,
            })
        })
        .collect()
}

/// Mises à jour disponibles (Flathub et autres dépôts), pour les installations utilisateur et système.
pub fn list_updates() -> Result<Vec<Update>, String> {
    let mut all: Vec<Update> = Vec::new();
    let mut any_ok = false;
    for (scope, inst) in [("--user", "user"), ("--system", "system")] {
        let mut c = crate::host::cmd("flatpak");
        c.args([
            "remote-ls",
            scope,
            "--updates",
            "--columns=application,name,branch,download-size,ref",
        ])
        .env("LC_ALL", "C");
        if let Some(o) = crate::host::output_timeout(c, 60)
            && o.status.success()
        {
            any_ok = true;
            for u in parse_updates(&String::from_utf8_lossy(&o.stdout), inst) {
                if !all.iter().any(|x| {
                    x.id == u.id && x.branch == u.branch && x.installation == u.installation
                }) {
                    all.push(u);
                }
            }
        }
    }
    if !any_ok {
        return Err(tr!("Impossible de chercher les mises à jour (connexion à Flathub ?).").into());
    }
    all.sort_by(|a, b| {
        a.runtime
            .cmp(&b.runtime)
            .then(a.name.to_lowercase().cmp(&b.name.to_lowercase()))
    });
    Ok(all)
}

/// Met à jour un seul paquet.
pub fn update_one_plan(u: &Update) -> Vec<Vec<String>> {
    vec![vec![
        "flatpak".into(),
        "update".into(),
        scope(&u.installation),
        "-y".into(),
        "--noninteractive".into(),
        u.reference(),
    ]]
}

/// Met à jour tout (utilisateur, puis système si des mises à jour système existent).
pub fn update_all_plan(updates: &[Update]) -> Vec<Vec<String>> {
    let mut plan = Vec::new();
    if updates.iter().any(|u| u.is_user()) {
        plan.push(
            ["flatpak", "update", "--user", "-y", "--noninteractive"]
                .map(String::from)
                .to_vec(),
        );
    }
    if updates.iter().any(|u| !u.is_user()) {
        plan.push(
            ["flatpak", "update", "--system", "-y", "--noninteractive"]
                .map(String::from)
                .to_vec(),
        );
    }
    plan
}

impl Update {
    pub fn is_user(&self) -> bool {
        self.installation == "user"
    }
}

#[cfg(test)]
mod update_tests {
    use super::*;

    #[test]
    fn parses_updates_and_plans() {
        let out = "org.mozilla.firefox\tFirefox\tstable\t60.1 MB\tapp/org.mozilla.firefox/x86_64/stable\norg.freedesktop.Platform\tFreedesktop Platform\t25.08\t10 MB\truntime/org.freedesktop.Platform/x86_64/25.08\n";
        let mut v = parse_updates(out, "system");
        v[1].installation = "user".into();
        assert_eq!(v.len(), 2);
        assert!(!v[0].runtime && v[1].runtime);
        assert_eq!(
            update_one_plan(&v[0])[0].join(" "),
            "flatpak update --system -y --noninteractive org.mozilla.firefox//stable"
        );
        let all = update_all_plan(&v);
        assert_eq!(all.len(), 2);
        assert_eq!(
            all[0].join(" "),
            "flatpak update --user -y --noninteractive"
        );
    }
}
