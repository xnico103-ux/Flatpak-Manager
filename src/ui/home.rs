//! Barre latérale, accueil et écran d'analyse.

use super::*;

impl App {
    /// Écrans accessibles depuis la barre latérale (par opposition au parcours de création).
    pub(super) fn is_sub_screen(&self) -> bool {
        matches!(
            self.screen,
            Screen::History | Screen::Manage | Screen::Updates | Screen::Settings
        )
    }

    pub(super) fn sidebar(&mut self, ui: &mut Ui) {
        ui.add_space(6.0);
        ui.horizontal(|ui| {
            if let Some(tex) = self.logo.texture(ui.ctx(), 40.0) {
                ui.add(
                    egui::Image::new(tex)
                        .fit_to_exact_size(vec2(40.0, 40.0))
                        .corner_radius(10),
                );
            } else {
                ui.add_space(40.0);
            }
            ui.vertical(|ui| {
                ui.spacing_mut().item_spacing.y = 0.0;
                ui.label(RichText::new("Flatpak").strong().size(17.0));
                ui.label(muted("Manager").size(14.0));
            });
        });
        ui.add_space(22.0);
        let busy = matches!(self.screen, Screen::Running);
        if nav_item(ui, "Accueil", !self.is_sub_screen()).clicked() && !busy && self.is_sub_screen()
        {
            self.screen = Screen::Home;
        }
        let ulabel = if self.up.loaded && !self.up.items.is_empty() {
            format!("Mises à jour ({})", self.up.items.len())
        } else {
            "Mises à jour".to_string()
        };
        if nav_item(ui, &ulabel, self.screen == Screen::Updates).clicked() && !busy {
            self.screen = Screen::Updates;
            if !self.up.loaded && self.up.rx.is_none() {
                self.up_reload();
            }
        }
        if nav_item(ui, "Gérer les Flatpak", self.screen == Screen::Manage).clicked() && !busy {
            self.screen = Screen::Manage;
            if !self.mg.loaded && self.mg.rx.is_none() {
                self.mg_reload();
            }
        }
        let label = format!("Historique ({})", self.history.len());
        if nav_item(ui, &label, self.screen == Screen::History).clicked() && !busy {
            self.screen = Screen::History;
        }
        if nav_item(ui, "Paramètres", self.screen == Screen::Settings).clicked() && !busy {
            self.screen = Screen::Settings;
        }
        ui.with_layout(egui::Layout::bottom_up(egui::Align::Min), |ui| {
            ui.label(muted(format!("Version {}", env!("CARGO_PKG_VERSION"))).size(11.5));
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                ui.vertical(|ui| {
                    ui.label(RichText::new("Mode avancé").strong());
                    ui.label(muted("Détails et réglages fins").size(12.0));
                });
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if toggle(ui, &mut self.advanced).changed() {
                        self.save_settings();
                        toast(
                            ui.ctx(),
                            if self.advanced {
                                "Mode avancé activé"
                            } else {
                                "Mode avancé désactivé"
                            },
                        );
                    }
                });
            });
            ui.add_space(4.0);
            ui.separator();
        });
    }

    pub(super) fn ui_home(&mut self, ui: &mut Ui) {
        ui.add_space(8.0);
        ui.label(RichText::new("Que voulez-vous faire ?").heading().strong());
        ui.label(muted(
            "Créez ou installez une application Flatpak en quelques clics, sans terminal.",
        ));
        ui.add_space(14.0);

        if let Some(id) = crate::host::sandbox_blocked() {
            sandbox_banner(ui, &id);
        } else if let Some((fp, _)) = self.flatpak_state {
            if !fp {
                banner(
                    ui,
                    DANGER,
                    "Flatpak n'est pas détecté sur ce système",
                    "Installez le paquet « flatpak » avec votre gestionnaire de paquets (ex. : sudo pacman -S flatpak, sudo apt install flatpak, sudo dnf install flatpak), puis relancez Flatpak Manager.",
                );
            } else if let Some((_, false)) = self.flatpak_state {
                banner(
                    ui,
                    ACCENT,
                    "Flatpak Builder sera installé automatiquement",
                    "Au premier usage, l'outil de construction (org.flatpak.Builder) sera téléchargé depuis Flathub pour votre utilisateur.",
                );
            }
        }
        let mut dismiss = false;
        if let Some(e) = &self.error {
            ui.horizontal_top(|ui| {
                Frame::new()
                    .fill(DANGER.gamma_multiply(0.12))
                    .stroke(Stroke::new(1.0, DANGER.gamma_multiply(0.5)))
                    .corner_radius(CornerRadius::same(12))
                    .inner_margin(Margin::same(14))
                    .show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        ui.horizontal(|ui| {
                            ui.vertical(|ui| {
                                ui.set_width(ui.available_width() - 90.0);
                                ui.label(
                                    RichText::new("Impossible d'ouvrir ce fichier")
                                        .strong()
                                        .color(DANGER),
                                );
                                ui.add(egui::Label::new(muted(e.clone())).wrap());
                            });
                            if button(ui, "Fermer", BtnKind::Ghost).clicked() {
                                dismiss = true;
                            }
                        });
                    });
            });
            ui.add_space(6.0);
        }
        if dismiss {
            self.error = None;
        }

        let busy = self.picker.is_some();
        let avail = ui.available_width();
        let gap = 16.0;
        let stacked = avail < 640.0;
        let w = if stacked {
            avail
        } else {
            (avail - 2.0 * gap) / 3.0
        };
        let mut pick: Option<Purpose> = None;
        let mut cards = |ui: &mut Ui| {
            if action_card(ui, w, IconKind::Package, "Créer un Flatpak", "Choisissez une archive (.tar.gz, .tar.xz…) : elle est analysée, puis convertie en fichier .flatpak.", ACCENT, !busy).clicked() {
                pick = Some(Purpose::Archive(Intent::Create));
            }
            if action_card(
                ui,
                w,
                IconKind::Install,
                "Installer un Flatpak",
                "Sélectionnez un fichier .flatpak existant pour l'installer sur ce système.",
                SUCCESS,
                !busy,
            )
            .clicked()
            {
                pick = Some(Purpose::Bundle);
            }
            if action_card(ui, w, IconKind::Bolt, "Créer et installer", "Tout en une fois : création du paquet depuis l'archive, puis installation automatique.", Color32::from_rgb(192, 132, 252), !busy).clicked() {
                pick = Some(Purpose::Archive(Intent::CreateInstall));
            }
        };
        if stacked {
            ui.vertical(|ui| cards(ui));
        } else {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = gap;
                cards(ui);
            });
        }
        if let Some(p) = pick {
            self.start_pick(p);
        }
        ui.add_space(8.0);
        ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            card(ui, Some("Ce qui peut être converti"), |ui| {
                dot_row(ui, SUCCESS, "Binaires précompilés et scripts", "Archives contenant un programme déjà prêt à l'emploi (ELF ou script), avec ou sans bibliothèques embarquées.");
                dot_row(ui, SUCCESS, "Code source Rust, C/C++ et Go", "Compilé dans le SDK Flatpak. Si le projet ne produit aucun programme, ou si son nom est incertain, l'analyse vous le dit avant toute compilation.");
                dot_row(ui, DANGER, "AppImage, Java, Node.js, Python", "Non pris en charge pour l'instant : l'analyse l'indiquera clairement avant toute construction.");
                let dnd = "Le glisser-déposer n'est pas disponible sous Wayland natif : utilisez « Parcourir » ou collez le chemin ci-dessous.";
                dot_row(ui, ACCENT, "Astuce", dnd);
            });
            card(ui, Some("Ou saisir un chemin"), |ui| {
                ui.horizontal(|ui| {
                    let r = ui.add(egui::TextEdit::singleline(&mut self.path_input).hint_text("/chemin/vers/archive.tar.gz").desired_width(ui.available_width() - 110.0));
                    let go = button(ui, "Ouvrir", BtnKind::Secondary).clicked() || (r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)));
                    if go && !self.path_input.trim().is_empty() {
                        let t = self.path_input.trim().trim_matches(|c| c == '"' || c == '\'').to_string();
                        let t = match (t.strip_prefix("~/"), dirs::home_dir()) {
                            (Some(rest), Some(h)) => h.join(rest),
                            _ => PathBuf::from(t),
                        };
                        if t.is_file() {
                            self.open_path(t);
                        } else {
                            self.error = Some(format!("Fichier introuvable : {}", t.display()));
                        }
                    }
                });
                ui.label(muted("Utile si le sélecteur de fichiers ne s'ouvre pas sur votre environnement de bureau.").size(12.5));
            });
            ui.add_space(4.0);
            if !self.history.is_empty() {
                let recent: Vec<Entry> = self.history.iter().take(3).cloned().collect();
                card(ui, Some("Activité récente"), |ui| {
                    for e in &recent {
                        history_line(ui, e);
                    }
                });
            }
        });
    }

    pub(super) fn ui_analyzing(&mut self, ui: &mut Ui) {
        ui.vertical_centered(|ui| {
            ui.add_space(ui.available_height() * 0.32);
            ui.spinner();
            ui.add_space(10.0);
            ui.label(RichText::new("Analyse de l'archive…").size(20.0).strong());
            if let Some((_, name)) = &self.analysis {
                ui.label(muted(name.clone()));
            }
            ui.label(muted(
                "Contrôles de sécurité, recherche de l'exécutable et des dépendances.",
            ));
        });
    }
}
