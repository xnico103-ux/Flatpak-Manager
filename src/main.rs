#[macro_use]
mod i18n;
mod analyze;
mod chooser;
mod diagnose;
mod history;
mod host;
mod manager;
mod manifest;
mod pipeline;
mod settings;
mod toolchains;
mod ui;

use std::path::PathBuf;
use std::sync::Arc;

/// Écrit un message de démarrage dans le journal (utile quand aucune fenêtre n'a pu s'ouvrir).
fn log_startup(msg: &str) {
    eprintln!("{msg}");
    if let Some(dir) = dirs::cache_dir().map(|d| d.join("flatpak-manager")) {
        let _ = std::fs::create_dir_all(&dir);
        let line = format!(
            "[{}] {msg}\n",
            chrono::Local::now().format("%Y-%m-%d %H:%M:%S")
        );
        use std::io::Write;
        if let Ok(mut f) = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(dir.join("demarrage.log"))
        {
            let _ = f.write_all(line.as_bytes());
        }
    }
}

fn main() -> eframe::Result {
    // La langue doit être connue avant tout message (démarrage, détection du bac à sable…).
    i18n::set_language(settings::Settings::load().language);
    std::panic::set_hook(Box::new(|info| {
        log_startup(&format!("Erreur interne : {info}"))
    }));
    // Wayland uniquement : l'application est compilée sans X11.
    if std::env::var_os("WAYLAND_DISPLAY").is_none() {
        log_startup(tr!(
            "Flatpak Manager fonctionne uniquement sous Wayland (WAYLAND_DISPLAY est absent). Si l'application tourne en Flatpak, il lui manque la permission --socket=wayland."
        ));
        std::process::exit(1);
    }
    // Un fichier passé en argument (ex. : « Ouvrir avec » depuis Dolphin) est ouvert directement.
    let initial: Option<PathBuf> = std::env::args_os()
        .nth(1)
        .map(PathBuf::from)
        .filter(|p| p.exists());

    // Détermine tout de suite si l'application peut appeler l'hôte (évite un blocage au premier affichage).
    let _ = host::sandbox_blocked();

    let mut viewport = egui::ViewportBuilder::default()
        .with_title("Flatpak Manager")
        .with_app_id("flatpak-manager")
        .with_inner_size([1100.0, 760.0])
        .with_min_inner_size([980.0, 640.0]);
    if let Some(icon) = ui::window_icon() {
        viewport = viewport.with_icon(Arc::new(icon));
    }
    let options = eframe::NativeOptions {
        viewport,
        ..Default::default()
    };
    let r = eframe::run_native(
        "Flatpak Manager",
        options,
        Box::new(move |cc| Ok(Box::new(ui::App::new(cc, initial)))),
    );
    if let Err(e) = &r {
        log_startup(&tr!("Impossible d'ouvrir la fenêtre : {e}", e = e));
    }
    r
}
