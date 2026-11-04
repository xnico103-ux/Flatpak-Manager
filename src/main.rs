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
    // winit 0.30 ne gère pas le glisser-déposer sous Wayland natif : si XWayland est disponible,
    // on lance la fenêtre en X11 (le glisser-déposer depuis Dolphin, Nautilus… fonctionne alors).
    // Pour forcer Wayland natif : FLATPAK_MANAGER_WAYLAND=1.
    if let Some(w) = std::env::var_os("WAYLAND_DISPLAY")
        && std::env::var_os("DISPLAY").is_some()
        && std::env::var_os("FLATPAK_MANAGER_WAYLAND").is_none()
    {
        // SAFETY : appelé au tout début de main, avant la création de tout thread.
        unsafe {
            std::env::set_var("FM_ORIG_WAYLAND_DISPLAY", w);
            std::env::remove_var("WAYLAND_DISPLAY");
        }
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
