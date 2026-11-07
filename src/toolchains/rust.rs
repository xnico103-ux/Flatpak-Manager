//! Projets Rust (Cargo).

use super::helpers::*;
use super::model::*;

pub(super) fn cargo(
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
