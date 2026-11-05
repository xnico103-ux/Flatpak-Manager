mod analyze;
mod chooser;
mod diagnose;
mod history;
mod host;
mod manager;
mod manifest;
mod pipeline;
mod toolchains;
mod ui;

use std::path::PathBuf;
use std::sync::Arc;

fn load_window_icon() -> Option<egui::IconData> {
    let img = image::load_from_memory(ui::LOGO_BYTES).ok()?.to_rgba8();
    let (width, height) = (img.width(), img.height());
    Some(egui::IconData { rgba: img.into_raw(), width, height })
}

fn main() -> eframe::Result {
    // Wayland uniquement : l'application est compilée sans X11.
    if std::env::var_os("WAYLAND_DISPLAY").is_none() {
        eprintln!("Flatpak Manager fonctionne uniquement sous Wayland (WAYLAND_DISPLAY est absent).");
        std::process::exit(1);
    }
    // Un fichier passé en argument (ex. : « Ouvrir avec » depuis Dolphin) est ouvert directement.
    let initial: Option<PathBuf> = std::env::args_os().nth(1).map(PathBuf::from).filter(|p| p.exists());

    // Détermine tout de suite si l'application peut appeler l'hôte (évite un blocage au premier affichage).
    let _ = host::sandbox_blocked();

    let mut viewport = egui::ViewportBuilder::default()
        .with_title("Flatpak Manager")
        .with_app_id("flatpak-manager")
        .with_inner_size([1100.0, 760.0])
        .with_min_inner_size([980.0, 640.0]);
    if let Some(icon) = load_window_icon() {
        viewport = viewport.with_icon(Arc::new(icon));
    }
    let options = eframe::NativeOptions { viewport, ..Default::default() };
    eframe::run_native(
        "Flatpak Manager",
        options,
        Box::new(move |cc| Ok(Box::new(ui::App::new(cc, initial)))),
    )
}
