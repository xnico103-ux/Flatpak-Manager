//! Composants Flatpak nécessaires à la compilation et plan d'installation.

use super::host_state::*;
use super::model::*;
use crate::manifest::RuntimeKind;
use crate::pipeline::FLATHUB_URL;

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
