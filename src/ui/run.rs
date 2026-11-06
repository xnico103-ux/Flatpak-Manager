//! Revue d'installation, progression et écran de fin.

use super::*;

impl App {
    pub(super) fn ui_install_review(&mut self, ui: &mut Ui) {
        let advanced = self.advanced;
        let Some(t) = &self.install else { return };
        let mut action = None;
        ui.add_space(8.0);
        ui.label(RichText::new("Installer un Flatpak").heading().strong());
        ui.label(muted(t.path.display().to_string()));
        ui.add_space(10.0);
        card(ui, Some("Vérification du fichier"), |ui| match &t.report {
            None => {
                ui.horizontal(|ui| {
                    ui.spinner();
                    ui.label("Lecture du fichier…");
                });
            }
            Some(Err(e)) => dot_row(ui, DANGER, "Fichier illisible", e),
            Some(Ok(r)) => {
                for (s, m) in &r.items {
                    dot_row(ui, severity_color(*s), m, "");
                }
                ui.add_space(4.0);
                ui.label(muted(format!("SHA-256 : {}", r.sha256)).size(12.0));
            }
        });
        ui.add_space(6.0);
        card(ui, Some("Options"), |ui| {
            ui.label(
                muted("Par défaut, l'application est installée pour votre utilisateur uniquement.")
                    .size(13.0),
            );
            if advanced {
                perm_row(
                    ui,
                    &mut self.opts.system_install,
                    "Installer pour tous les utilisateurs",
                    "Installation système (demande une authentification).",
                    Some(Severity::Info),
                );
            }
            perm_row(
                ui,
                &mut self.opts.replace_existing,
                "Remplacer une installation existante",
                "Réinstalle l'application ; vos données sont conservées.",
                None,
            );
        });
        ui.add_space(10.0);
        let ok = matches!(&t.report, Some(Ok(r)) if !r.items.iter().any(|(s, _)| *s == Severity::Blocker));
        ui.horizontal(|ui| {
            if button(ui, "Retour", BtnKind::Ghost).clicked() {
                action = Some(false);
            }
            ui.add_enabled_ui(ok, |ui| {
                if button(ui, "Installer", BtnKind::Primary).clicked() {
                    action = Some(true);
                }
            });
        });
        match action {
            Some(true) => self.start_install(),
            Some(false) => self.screen = Screen::Home,
            None => {}
        }
    }

    pub(super) fn ui_running(&mut self, ui: &mut Ui) {
        let show_log = &mut self.show_log;
        let Some(run) = &mut self.run else { return };
        let title = match run.action {
            Action::Created => "Création du paquet",
            Action::CreatedAndInstalled => "Création et installation",
            Action::Installed => "Installation",
        };
        ui.add_space(8.0);
        ui.label(
            RichText::new(format!("{title} en cours…"))
                .heading()
                .strong(),
        );
        if !run.name.is_empty() {
            ui.label(muted(format!("{} {}", run.name, run.version)));
        }
        ui.add_space(14.0);
        card(ui, None, |ui| {
            let cur = run.plan.iter().position(|p| *p == run.phase).unwrap_or(0);
            let steps: Vec<(&str, StepState)> = run
                .plan
                .iter()
                .enumerate()
                .map(|(i, p)| {
                    (
                        p.label(),
                        if i < cur {
                            StepState::Done
                        } else if i == cur {
                            StepState::Active
                        } else {
                            StepState::Pending
                        },
                    )
                })
                .collect();
            stepper(ui, &steps, false);
            ui.add_space(6.0);
            progress_bar(ui, run.progress, 12.0);
            ui.horizontal(|ui| {
                ui.label(muted(single_line_trunc(&run.status, 90)).size(12.5));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label(RichText::new(format!("{:.0} %", run.progress * 100.0)).strong());
                });
            });
        });
        if let Some(rep) = run.awaiting.clone() {
            ui.add_space(8.0);
            card(
                ui,
                Some("Vérification terminée : confirmer l'installation ?"),
                |ui| {
                    for (s, m) in &rep.items {
                        dot_row(ui, severity_color(*s), m, "");
                    }
                    ui.label(muted(format!("SHA-256 du paquet : {}", rep.sha256)).size(12.0));
                    ui.add_space(6.0);
                    ui.horizontal(|ui| {
                        if button(ui, "Installer maintenant", BtnKind::Primary).clicked() {
                            let _ = run.handle.confirm_tx.send(true);
                            run.awaiting = None;
                        }
                        if button(ui, "Ne pas installer", BtnKind::Secondary).clicked() {
                            let _ = run.handle.confirm_tx.send(false);
                            run.awaiting = None;
                        }
                    });
                },
            );
        }
        ui.add_space(8.0);
        ui.horizontal(|ui| {
            if run.cancelling {
                ui.spinner();
                ui.label(muted("Annulation en cours…"));
            } else if button(ui, "Annuler", BtnKind::Danger).clicked() {
                run.cancelling = true;
                run.handle
                    .cancel
                    .store(true, std::sync::atomic::Ordering::SeqCst);
                let _ = run.handle.confirm_tx.send(false);
            }
            if button(
                ui,
                if *show_log {
                    "Masquer les détails"
                } else {
                    "Afficher les détails"
                },
                BtnKind::Ghost,
            )
            .clicked()
            {
                *show_log = !*show_log;
            }
        });
        if *show_log {
            log_view(ui, &run.log, 340.0);
        }
    }

    pub(super) fn ui_done(&mut self, ui: &mut Ui) {
        let Some(d) = &self.done else { return };
        let mut action = None;
        ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                ui.add_space(12.0);
                match &d.result {
                    Ok(o) => {
                        ui.horizontal(|ui| {
                            big_status_icon(ui, true);
                            ui.vertical(|ui| {
                                ui.add_space(6.0);
                                let t = if o.installed {
                                    if d.action == Action::Installed {
                                        "Installation terminée"
                                    } else {
                                        "Paquet créé et installé"
                                    }
                                } else {
                                    "Paquet créé"
                                };
                                ui.label(RichText::new(t).heading().strong());
                                ui.label(muted(format!("Terminé en {} s", d.elapsed.as_secs())));
                            });
                        });
                        ui.add_space(10.0);
                        card(ui, Some("Résultat"), |ui| {
                            if !o.name.is_empty() {
                                ui.label(
                                    RichText::new(format!("{} {}", o.name, o.version)).strong(),
                                );
                            }
                            if !o.app_id.is_empty() {
                                ui.label(muted(o.app_id.clone()));
                            }
                            if let Some(b) = &o.bundle {
                                ui.add_space(4.0);
                                ui.label(muted(format!("Fichier : {}", b.display())));
                            }
                            if let Some(r) = &d.report {
                                ui.add_space(6.0);
                                for (s, m) in &r.items {
                                    dot_row(ui, severity_color(*s), m, "");
                                }
                            }
                        });
                        ui.add_space(8.0);
                        ui.horizontal(|ui| {
                            if o.installed
                                && !o.app_id.is_empty()
                                && button(ui, "Lancer l'application", BtnKind::Primary).clicked()
                            {
                                action = Some("launch");
                            }
                            if o.bundle.is_some()
                                && button(ui, "Ouvrir le dossier", BtnKind::Secondary).clicked()
                            {
                                action = Some("folder");
                            }
                            if o.bundle.is_some()
                                && !o.installed
                                && d.action != Action::Installed
                                && button(ui, "Installer maintenant", BtnKind::Secondary).clicked()
                            {
                                action = Some("install");
                            }
                            if button(ui, "Accueil", BtnKind::Ghost).clicked() {
                                action = Some("home");
                            }
                        });
                    }
                    Err(f) => {
                        ui.horizontal(|ui| {
                            big_status_icon(ui, false);
                            ui.vertical(|ui| {
                                ui.add_space(6.0);
                                ui.label(
                                    RichText::new(if f.cancelled {
                                        "Opération annulée"
                                    } else {
                                        "Échec de l'opération"
                                    })
                                    .heading()
                                    .strong(),
                                );
                                ui.add(egui::Label::new(muted(f.message.clone())).wrap());
                            });
                        });
                        if !d.plan.is_empty() {
                            ui.add_space(8.0);
                            card(ui, None, |ui| {
                                let cur = d.plan.iter().position(|p| *p == d.phase).unwrap_or(0);
                                let steps: Vec<(&str, StepState)> = d
                                    .plan
                                    .iter()
                                    .enumerate()
                                    .map(|(i, p)| {
                                        (
                                            p.label(),
                                            if i < cur {
                                                StepState::Done
                                            } else if i == cur {
                                                StepState::Active
                                            } else {
                                                StepState::Pending
                                            },
                                        )
                                    })
                                    .collect();
                                stepper(ui, &steps, !f.cancelled);
                            });
                        }
                        if !f.diagnoses.is_empty() {
                            ui.add_space(8.0);
                            card(ui, Some("Diagnostic"), |ui| {
                                for dg in &f.diagnoses {
                                    dot_row(ui, WARN, &dg.title, &dg.explanation);
                                    if let Some(fix) = &dg.fix {
                                        ui.horizontal(|ui| {
                                            ui.add_space(22.0);
                                            ui.add(
                                                egui::Label::new(
                                                    RichText::new(format!("Piste : {fix}"))
                                                        .color(ACCENT)
                                                        .size(13.5),
                                                )
                                                .wrap(),
                                            );
                                        });
                                    }
                                }
                            });
                        }
                        ui.add_space(8.0);
                        ui.horizontal(|ui| {
                            if self.info.is_some()
                                && button(ui, "Modifier et réessayer", BtnKind::Primary).clicked()
                            {
                                action = Some("review");
                            }
                            if button(ui, "Accueil", BtnKind::Ghost).clicked() {
                                action = Some("home");
                            }
                        });
                    }
                }
                ui.add_space(8.0);
                egui::CollapsingHeader::new("Journal détaillé")
                    .show(ui, |ui| log_view(ui, &d.log, 320.0));
            });
        let bundle = d.result.as_ref().ok().and_then(|o| o.bundle.clone());
        let app_id = d
            .result
            .as_ref()
            .ok()
            .map(|o| o.app_id.clone())
            .unwrap_or_default();
        match action {
            Some("home") => self.screen = Screen::Home,
            Some("review") => self.screen = Screen::Review,
            Some("launch") => {
                toast(&self.ctx, "Lancement de l'application…");
                let _ = crate::host::cmd("flatpak").args(["run", &app_id]).spawn();
            }
            Some("folder") => {
                toast(&self.ctx, "Ouverture du dossier…");
                if let Some(dir) = bundle.as_ref().and_then(|b| b.parent()) {
                    let _ = crate::host::cmd("xdg-open").arg(dir).spawn();
                }
            }
            Some("install") => {
                if let Some(b) = bundle {
                    self.start_install_review(b);
                }
            }
            _ => {}
        }
    }
}
