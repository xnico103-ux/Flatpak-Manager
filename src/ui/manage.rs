//! Écran de gestion des applications installées.

use super::*;

impl App {
    pub(super) fn ui_manage(&mut self, ui: &mut Ui) {
        ui.add_space(8.0);
        ui.label(RichText::new(tr!("Gérer les Flatpak")).heading().strong());
        ui.label(muted(tr!("Applications et composants installés sur cette machine. La désinstallation est définitive.")));
        ui.add_space(10.0);
        if let Some(id) = crate::host::sandbox_blocked() {
            sandbox_banner(ui, &id);
            return;
        }
        let busy = self.mg.install.is_some();
        let own_id = crate::host::sandbox_app_id();
        let mut action: Option<(&'static str, usize)> = None;

        ui.horizontal_wrapped(|ui| {
            ui.add(
                egui::TextEdit::singleline(&mut self.mg.filter)
                    .hint_text(tr!("Rechercher…"))
                    .desired_width(240.0),
            );
            ui.checkbox(
                &mut self.mg.show_runtimes,
                tr!("Afficher les runtimes et SDK"),
            );
            if button(ui, tr!("Actualiser"), BtnKind::Ghost).clicked() && !busy {
                action = Some(("reload", 0));
            }
            if button(
                ui,
                tr!("Retirer les composants inutilisés…"),
                BtnKind::Secondary,
            )
            .clicked()
                && !busy
            {
                action = Some(("unused_ask", 0));
            }
        });
        ui.add_space(6.0);

        if let Some(e) = &self.mg.error {
            ui.add(egui::Label::new(RichText::new(e.as_str()).color(DANGER).size(13.0)).wrap());
        }
        if let Some(n) = &self.mg.notice {
            ui.label(RichText::new(n.as_str()).color(SUCCESS).size(13.0));
        }

        // Confirmation : la commande exacte est affichée avant toute suppression.
        let plan = if let Some(it) = &self.mg.pending {
            Some((
                tr!("Désinstaller {name} ?", name = it.name),
                manager::uninstall_plan(it, self.mg.delete_data),
            ))
        } else if self.mg.pending_unused {
            Some((
                tr!("Retirer les runtimes qu'aucune application n'utilise ?").to_string(),
                manager::unused_plan(),
            ))
        } else {
            None
        };
        if let Some((title, cmds)) = plan {
            let offer_wipe = self.mg.pending.as_ref().is_some_and(|i| !i.runtime);
            let system = self.mg.pending.as_ref().is_some_and(|i| !i.is_user());
            card(ui, None, |ui| {
                ui.label(RichText::new(format!("⚠ {title}")).strong().color(WARN));
                ui.add(
                    egui::Label::new(
                        muted(tr!(
                            "Commande destructive : elle sera exécutée telle quelle."
                        ))
                        .size(13.0),
                    )
                    .wrap(),
                );
                if system {
                    ui.add(egui::Label::new(muted(tr!("Installation système : une demande d'authentification peut s'afficher.")).size(13.0)).wrap());
                }
                ui.add_space(4.0);
                for argv in &cmds {
                    ui.add(
                        egui::Label::new(
                            RichText::new(format!("$ {}", toolchains::shell_line(argv)))
                                .monospace()
                                .size(12.5)
                                .color(WARN),
                        )
                        .wrap(),
                    );
                }
                if offer_wipe {
                    ui.add_space(4.0);
                    ui.checkbox(
                        &mut self.mg.delete_data,
                        tr!("Supprimer aussi les données de l'application (--delete-data)"),
                    );
                }
                ui.add_space(6.0);
                ui.horizontal_wrapped(|ui| {
                    if button(ui, tr!("Supprimer maintenant"), BtnKind::Danger).clicked() {
                        action = Some(("confirm", 0));
                    }
                    if button(ui, tr!("Annuler"), BtnKind::Ghost).clicked() {
                        action = Some(("cancel", 0));
                    }
                });
            });
            ui.add_space(6.0);
        }

        if busy {
            card(ui, None, |ui| {
                progress_bar(ui, self.mg.progress, 8.0);
                let start = self.mg.lines.len().saturating_sub(6);
                for l in &self.mg.lines[start..] {
                    ui.add(
                        egui::Label::new(
                            RichText::new(single_line_trunc(l, 90))
                                .monospace()
                                .color(MUTED)
                                .size(12.0),
                        )
                        .truncate(),
                    );
                }
                if button(ui, tr!("Annuler"), BtnKind::Danger).clicked() {
                    action = Some(("stop", 0));
                }
            });
            ui.add_space(6.0);
        }

        if !self.mg.loaded {
            ui.horizontal(|ui| {
                ui.spinner();
                ui.label(muted(tr!("Lecture des paquets installés…")));
            });
        }
        let q = self.mg.filter.trim().to_lowercase();
        ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                let mut shown = 0;
                for (i, it) in self.mg.items.iter().enumerate() {
                    if (it.runtime && !self.mg.show_runtimes)
                        || (!q.is_empty()
                            && !it.name.to_lowercase().contains(&q)
                            && !it.id.to_lowercase().contains(&q))
                    {
                        continue;
                    }
                    shown += 1;
                    card(ui, None, |ui| {
                        ui.horizontal(|ui| {
                            ui.vertical(|ui| {
                                ui.horizontal(|ui| {
                                    ui.label(RichText::new(&it.name).strong());
                                    if !it.version.is_empty() {
                                        ui.label(muted(&it.version).size(12.5));
                                    }
                                    badge(
                                        ui,
                                        if it.is_user() {
                                            tr!("utilisateur")
                                        } else {
                                            &it.installation
                                        },
                                        if it.is_user() { ACCENT } else { MUTED },
                                    );
                                    if it.runtime {
                                        badge(ui, "runtime", MUTED);
                                    }
                                });
                                ui.label(
                                    muted(format!("{}//{}  ·  {}", it.id, it.branch, it.size))
                                        .size(12.5),
                                );
                            });
                            ui.with_layout(
                                egui::Layout::right_to_left(egui::Align::Center),
                                |ui| {
                                    let is_self = own_id.as_deref() == Some(it.id.as_str());
                                    if is_self {
                                        ui.label(muted(tr!("application en cours")).size(12.5));
                                    } else if button(ui, tr!("Désinstaller…"), BtnKind::Danger)
                                        .clicked()
                                        && !busy
                                    {
                                        action = Some(("ask", i));
                                    }
                                },
                            );
                        });
                    });
                }
                if self.mg.loaded && shown == 0 {
                    card(ui, None, |ui| {
                        ui.label(muted(tr!("Aucun paquet à afficher.")));
                    });
                }
            });

        match action {
            Some(("reload", _)) => {
                self.mg.manual = true;
                self.mg.notice = None;
                self.mg.error = None;
                self.mg_reload();
            }
            Some(("unused_ask", _)) => {
                self.mg.pending = None;
                self.mg.pending_unused = true;
            }
            Some(("ask", i)) => {
                self.mg.pending = self.mg.items.get(i).cloned();
                self.mg.pending_unused = false;
                self.mg.delete_data = false;
            }
            Some(("cancel", _)) => {
                self.mg.pending = None;
                self.mg.pending_unused = false;
            }
            Some(("confirm", _)) => {
                let plan = if let Some(it) = &self.mg.pending {
                    manager::uninstall_plan(it, self.mg.delete_data)
                } else if self.mg.pending_unused {
                    manager::unused_plan()
                } else {
                    vec![]
                };
                if !plan.is_empty() {
                    self.mg.expect_gone = self.mg.pending.clone();
                    self.mg_run(plan);
                }
            }
            Some(("stop", _)) => {
                toast(&self.ctx, tr!("Annulation demandée…"));
                if let Some(t) = &self.mg.install {
                    t.cancel.store(true, std::sync::atomic::Ordering::SeqCst);
                }
            }
            _ => {}
        }
    }
}
