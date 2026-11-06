//! Écran de l'historique.

use super::*;

impl App {
    pub(super) fn ui_history(&mut self, ui: &mut Ui) {
        ui.add_space(8.0);
        ui.label(RichText::new("Historique").heading().strong());
        ui.label(muted(
            "Les paquets créés et installés avec Flatpak Manager.",
        ));
        ui.add_space(10.0);
        let mut action: Option<(&str, usize)> = None;
        ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                if self.history.is_empty() {
                    card(ui, None, |ui| {
                        ui.label(muted(
                            "Rien pour l'instant : vos opérations apparaîtront ici.",
                        ));
                    });
                }
                for (i, e) in self.history.iter().enumerate() {
                    card(ui, None, |ui| {
                        history_line(ui, e);
                        ui.horizontal(|ui| {
                            let exists = e.bundle.as_ref().is_some_and(|b| b.exists());
                            if exists && e.success {
                                if button(ui, "Installer", BtnKind::Secondary).clicked() {
                                    action = Some(("install", i));
                                }
                                if button(ui, "Ouvrir le dossier", BtnKind::Ghost).clicked() {
                                    action = Some(("folder", i));
                                }
                            } else if e.bundle.is_some() {
                                ui.label(muted("Fichier introuvable").size(12.5));
                            }
                            ui.with_layout(
                                egui::Layout::right_to_left(egui::Align::Center),
                                |ui| {
                                    if button(ui, "Retirer de la liste", BtnKind::Ghost)
                                        .on_hover_text("Ne supprime pas le fichier .flatpak")
                                        .clicked()
                                    {
                                        action = Some(("remove", i));
                                    }
                                },
                            );
                        });
                    });
                    ui.add_space(2.0);
                }
            });
        if let Some((a, i)) = action {
            match a {
                "install" => {
                    if let Some(b) = self.history[i].bundle.clone() {
                        self.start_install_review(b);
                    }
                }
                "folder" => {
                    toast(&self.ctx, "Ouverture du dossier…");
                    if let Some(dir) = self.history[i].bundle.as_ref().and_then(|b| b.parent()) {
                        let _ = crate::host::cmd("xdg-open").arg(dir).spawn();
                    }
                }
                "remove" => {
                    toast(&self.ctx, "Retiré de l'historique");
                    self.history.remove(i);
                    history::save(&self.history);
                }
                _ => {}
            }
        }
    }
}
