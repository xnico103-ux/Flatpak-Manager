//! Écran des paramètres.

use super::*;
use crate::i18n::Language;
use crate::settings::{SCALE_MAX, SCALE_MIN};

impl App {
    /// Reporte l'état courant dans le fichier de paramètres.
    pub(super) fn save_settings(&mut self) {
        self.settings.advanced = self.advanced;
        self.settings.system_install = self.opts.system_install;
        self.settings.replace_existing = self.opts.replace_existing;
        self.settings.confirm_before_install = self.opts.confirm_before_install;
        self.settings.clone().sanitized().save();
    }

    fn reset_settings(&mut self) {
        self.settings = Settings::default();
        self.advanced = false;
        self.opts = Options::default();
        self.ctx.set_zoom_factor(self.settings.ui_scale);
        crate::i18n::set_language(self.settings.language);
        self.save_settings();
        toast(&self.ctx, tr!("Paramètres réinitialisés"));
    }

    pub(super) fn ui_settings(&mut self, ui: &mut Ui) {
        ui.add_space(8.0);
        ui.label(RichText::new(tr!("Paramètres")).heading().strong());
        ui.label(muted(tr!(
            "Vos préférences sont enregistrées automatiquement."
        )));
        ui.add_space(10.0);
        ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                self.settings_appearance(ui);
                ui.add_space(6.0);
                self.settings_behavior(ui);
                ui.add_space(6.0);
                self.settings_data(ui);
                ui.add_space(6.0);
                self.settings_about(ui);
                ui.add_space(12.0);
            });
    }

    fn settings_appearance(&mut self, ui: &mut Ui) {
        card(ui, Some(tr!("Apparence")), |ui| {
            ui.label(RichText::new(tr!("Taille de l'interface")).strong());
            ui.add(
                egui::Label::new(
                    muted(tr!("Agrandit ou réduit tous les textes et éléments.")).size(13.0),
                )
                .wrap(),
            );
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                let mut pct = (self.settings.ui_scale * 100.0).round();
                let r = ui.add(
                    egui::Slider::new(&mut pct, SCALE_MIN * 100.0..=SCALE_MAX * 100.0)
                        .suffix(" %")
                        .step_by(5.0),
                );
                self.settings.ui_scale = pct / 100.0;
                // On applique au relâchement : redimensionner pendant le glissement
                // déplacerait le curseur sous la souris.
                if r.drag_stopped() || (r.changed() && !r.dragged()) {
                    self.ctx.set_zoom_factor(self.settings.ui_scale);
                    self.save_settings();
                }
                if button(ui, "100 %", BtnKind::Ghost).clicked() {
                    self.settings.ui_scale = 1.0;
                    self.ctx.set_zoom_factor(1.0);
                    self.save_settings();
                }
            });
            ui.add_space(8.0);
            ui.label(RichText::new(tr!("Langue")).strong());
            ui.add(
                egui::Label::new(muted(tr!("Langue des menus et des messages.")).size(13.0)).wrap(),
            );
            ui.add_space(4.0);
            let mut lang = self.settings.language;
            egui::ComboBox::from_id_salt("language")
                .selected_text(lang.label())
                .show_ui(ui, |ui| {
                    for l in Language::ALL {
                        ui.selectable_value(&mut lang, l, l.label());
                    }
                });
            if lang != self.settings.language {
                self.settings.language = lang;
                crate::i18n::set_language(lang);
                self.save_settings();
            }
        });
    }

    fn settings_behavior(&mut self, ui: &mut Ui) {
        card(ui, Some(tr!("Comportement")), |ui| {
            let mut changed = false;
            changed |= perm_row(
                ui,
                &mut self.advanced,
                tr!("Mode avancé"),
                tr!("Affiche les détails et les réglages fins dans chaque écran."),
                None,
            );
            ui.add_space(4.0);
            ui.label(muted(tr!("Valeurs proposées au démarrage de chaque opération")).size(12.5));
            changed |= perm_row(
                ui,
                &mut self.opts.replace_existing,
                tr!("Remplacer une installation existante"),
                tr!("Réinstalle l'application ; vos données dans ~/.var/app sont conservées."),
                None,
            );
            changed |= perm_row(
                ui,
                &mut self.opts.confirm_before_install,
                tr!("Confirmer avant d'installer"),
                tr!("Affiche le rapport de vérification et attend votre accord (mode avancé)."),
                None,
            );
            changed |= perm_row(
                ui,
                &mut self.opts.system_install,
                tr!("Installer pour tous les utilisateurs"),
                tr!("Installation système : demande une authentification à chaque fois."),
                Some(Severity::Info),
            );
            if changed {
                self.save_settings();
            }
        });
    }

    fn settings_data(&mut self, ui: &mut Ui) {
        card(ui, Some(tr!("Données")), |ui| {
            if let Some(p) = Settings::path() {
                ui.label(muted(tr!("Paramètres : {chemin}", chemin = p.display())).size(12.5));
            }
            ui.label(muted(tr!("Historique : {n} entrée(s)", n = self.history.len())).size(12.5));
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                if button(
                    ui,
                    tr!("Ouvrir le dossier des paramètres"),
                    BtnKind::Secondary,
                )
                .clicked()
                    && let Some(dir) =
                        Settings::path().and_then(|p| p.parent().map(Path::to_path_buf))
                {
                    let _ = std::fs::create_dir_all(&dir);
                    let _ = crate::host::cmd("xdg-open").arg(dir).spawn();
                }
                if button(ui, tr!("Réinitialiser les paramètres"), BtnKind::Ghost).clicked() {
                    self.reset_settings();
                }
            });
            ui.add_space(8.0);
            ui.separator();
            ui.add_space(6.0);
            if self.clear_history_ask {
                ui.label(
                    RichText::new(tr!(
                        "Effacer tout l'historique ? Cette action est définitive."
                    ))
                    .color(DANGER)
                    .strong(),
                );
                ui.label(
                    muted(tr!(
                        "Les fichiers .flatpak déjà créés ne sont pas supprimés."
                    ))
                    .size(12.5),
                );
                ui.add_space(4.0);
                ui.horizontal(|ui| {
                    if button(ui, tr!("Oui, effacer"), BtnKind::Danger).clicked() {
                        self.history.clear();
                        history::save(&self.history);
                        self.clear_history_ask = false;
                        toast(&self.ctx, tr!("Historique effacé"));
                    }
                    if button(ui, tr!("Annuler"), BtnKind::Ghost).clicked() {
                        self.clear_history_ask = false;
                    }
                });
            } else {
                ui.add_enabled_ui(!self.history.is_empty(), |ui| {
                    if button(ui, tr!("Effacer l'historique…"), BtnKind::Danger).clicked() {
                        self.clear_history_ask = true;
                    }
                });
            }
        });
    }

    fn settings_about(&mut self, ui: &mut Ui) {
        card(ui, Some(tr!("À propos")), |ui| {
            ui.label(
                RichText::new(format!("Flatpak Manager {}", env!("CARGO_PKG_VERSION"))).strong(),
            );
            ui.label(muted(tr!("Logiciel libre sous licence GPL-3.0 ou ultérieure.")).size(13.0));
            ui.add_space(4.0);
            ui.hyperlink_to(
                tr!("Code source et signalement de bugs"),
                "https://github.com/xnico103-ux/Flatpak-Manager",
            );
        });
    }
}
