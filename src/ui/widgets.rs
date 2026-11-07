//! Widgets réutilisables.

use super::*;

pub(super) fn nav_item(ui: &mut Ui, label: &str, selected: bool) -> egui::Response {
    let (rect, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 40.0), Sense::click());
    let t = ui
        .ctx()
        .animate_bool_with_time(resp.id, resp.hovered(), 0.12);
    let p = ui.painter();
    if selected {
        p.rect_filled(rect, CornerRadius::same(10), ACCENT.gamma_multiply(0.16));
        p.rect_filled(
            egui::Rect::from_min_size(rect.min + vec2(0.0, 9.0), vec2(3.0, 22.0)),
            CornerRadius::same(2),
            ACCENT,
        );
    } else if t > 0.0 {
        p.rect_filled(
            rect,
            CornerRadius::same(10),
            Color32::from_white_alpha((14.0 * t) as u8),
        );
    }
    p.text(
        rect.left_center() + vec2(16.0, 0.0),
        egui::Align2::LEFT_CENTER,
        label,
        egui::FontId::new(15.0, egui::FontFamily::Proportional),
        if selected { TEXT } else { MUTED },
    );
    resp.on_hover_cursor(egui::CursorIcon::PointingHand)
}

/// Carte « Environnement de compilation » : outils détectés, composants Flatpak requis,
/// commande d'installation affichée avant exécution.
pub(super) fn toolchain_card(
    ui: &mut Ui,
    system: BuildSystem,
    runtime: RuntimeKind,
    version: &str,
    tc: &Toolchain,
    action: &mut Option<&'static str>,
) {
    card(ui, Some(tr!("Environnement de compilation")), |ui| {
        ui.horizontal(|ui| {
            badge(ui, system.label(), ACCENT);
            ui.label(muted(tr!("détecté dans l'archive")).size(13.0));
        });
        ui.add_space(4.0);
        let Some(state) = &tc.state else {
            ui.horizontal(|ui| {
                ui.spinner();
                ui.label(muted(tr!("Analyse de votre système…")));
            });
            return;
        };
        if let Some(id) = crate::host::sandbox_blocked() {
            sandbox_banner(ui, &id);
            if button(ui, tr!("Revérifier"), BtnKind::Secondary).clicked() {
                *action = Some("tc_rescan");
            }
            return;
        }
        if !state.available {
            dot_row(
                ui,
                DANGER,
                tr!("Flatpak est introuvable"),
                tr!(
                    "Installez le paquet « flatpak » avec votre gestionnaire de paquets, puis cliquez sur « Revérifier »."
                ),
            );
            if button(ui, tr!("Revérifier"), BtnKind::Secondary).clicked() {
                *action = Some("tc_rescan");
            }
            return;
        }
        if !tc.tools.is_empty() {
            ui.label(RichText::new(tr!("Sur votre machine")).strong());
            for t in &tc.tools {
                let (c, d) = match &t.version {
                    Some(v) => (SUCCESS, v.clone()),
                    None => (
                        MUTED,
                        tr!("non trouvé (sans importance : la compilation utilise le SDK Flatpak)")
                            .to_string(),
                    ),
                };
                dot_row(ui, c, &t.name, &d);
            }
            ui.add_space(6.0);
        }
        ui.label(RichText::new(tr!("Composants Flatpak requis")).strong());
        let comps = toolchains::components(system, runtime, version, state);
        for c in &comps {
            let d = if c.installed {
                tr!("{reference} — installé", reference = c.reference())
            } else {
                tr!("{reference} — manquant", reference = c.reference())
            };
            dot_row(ui, if c.installed { SUCCESS } else { DANGER }, &c.label, &d);
        }
        let miss = toolchains::missing(&comps);
        if miss.is_empty() {
            ui.add_space(4.0);
            ui.label(RichText::new(tr!("Tout est prêt pour compiler.")).color(SUCCESS));
            cleanup_section(ui, tc, state, version, action);
            return;
        }
        ui.add_space(8.0);
        ui.label(RichText::new(tr!("Commande qui sera exécutée")).strong());
        Frame::new()
            .fill(INPUT)
            .stroke(Stroke::new(1.0, BORDER))
            .corner_radius(CornerRadius::same(10))
            .inner_margin(Margin::same(10))
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                ScrollArea::horizontal()
                    .id_salt("tc_cmd")
                    .auto_shrink([false, true])
                    .show(ui, |ui| {
                        for argv in toolchains::install_plan(&miss, state.has_flathub) {
                            ui.add(
                                egui::Label::new(
                                    RichText::new(format!("$ {}", toolchains::shell_line(&argv)))
                                        .monospace()
                                        .size(12.5)
                                        .color(ACCENT),
                                )
                                .extend(),
                            );
                        }
                        ui.add_space(4.0);
                    });
            });
        ui.add(egui::Label::new(muted(tr!("« -y » évite que Flatpak pose la question dans un terminal. Seul l'utilisateur courant est concerné (--user).")).size(12.5)).wrap());
        if let Some(e) = &tc.error {
            ui.add_space(4.0);
            ui.add(egui::Label::new(RichText::new(e.as_str()).color(DANGER).size(13.0)).wrap());
        }
        ui.add_space(6.0);
        if tc.install.is_some() {
            progress_bar(ui, tc.progress, 8.0);
            ui.add_space(4.0);
            let start = tc.lines.len().saturating_sub(5);
            for l in &tc.lines[start..] {
                ui.add(
                    egui::Label::new(
                        RichText::new(single_line_trunc(l, 70))
                            .monospace()
                            .color(MUTED)
                            .size(12.0),
                    )
                    .truncate(),
                );
            }
            ui.add_space(4.0);
            if button(ui, tr!("Annuler"), BtnKind::Danger).clicked() {
                *action = Some("tc_cancel");
            }
        } else {
            ui.horizontal_wrapped(|ui| {
                if button(ui, tr!("Installer maintenant"), BtnKind::Primary).clicked() {
                    *action = Some("tc_install");
                }
                if button(ui, tr!("Copier la commande"), BtnKind::Secondary).clicked() {
                    *action = Some("tc_copy");
                }
                if button(ui, tr!("Revérifier"), BtnKind::Ghost).clicked() {
                    *action = Some("tc_rescan");
                }
            });
            cleanup_section(ui, tc, state, version, action);
        }
    });
}

/// Ménage des anciennes versions de SDK : commande affichée, confirmation explicite avant suppression.
pub(super) fn cleanup_section(
    ui: &mut Ui,
    tc: &Toolchain,
    state: &FlatpakState,
    version: &str,
    action: &mut Option<&'static str>,
) {
    if tc.install.is_some() {
        // Un ménage (ou une installation) est en cours : on affiche sa progression ici.
        ui.add_space(8.0);
        progress_bar(ui, tc.progress, 8.0);
        let start = tc.lines.len().saturating_sub(5);
        for l in &tc.lines[start..] {
            ui.add(
                egui::Label::new(
                    RichText::new(single_line_trunc(l, 70))
                        .monospace()
                        .color(MUTED)
                        .size(12.0),
                )
                .truncate(),
            );
        }
        if button(ui, tr!("Annuler"), BtnKind::Danger).clicked() {
            *action = Some("tc_cancel");
        }
        return;
    }
    let cands = toolchains::cleanup_candidates(state, version);
    if cands.is_empty() {
        return;
    }
    ui.add_space(10.0);
    ui.separator();
    ui.label(RichText::new(tr!("Ménage des anciens SDK")).strong());
    ui.add(egui::Label::new(muted(tr!("{n} ancienne(s) version(s) installée(s) pour vous, devenues inutiles car une version plus récente est présente.", n = cands.len())).size(13.0)).wrap());
    if !tc.clean_ask {
        if button(ui, tr!("Faire le ménage…"), BtnKind::Secondary).clicked() {
            *action = Some("tc_clean_ask");
        }
        return;
    }
    ui.add_space(4.0);
    ui.add(
        egui::Label::new(
            RichText::new(tr!(
                "⚠ Ces commandes suppriment des composants (réinstallables depuis Flathub) :"
            ))
            .color(WARN)
            .size(13.0),
        )
        .wrap(),
    );
    Frame::new()
        .fill(INPUT)
        .stroke(Stroke::new(1.0, BORDER))
        .corner_radius(CornerRadius::same(10))
        .inner_margin(Margin::same(10))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ScrollArea::horizontal()
                .id_salt("tc_clean_cmd")
                .auto_shrink([false, true])
                .show(ui, |ui| {
                    for argv in toolchains::cleanup_plan(&cands) {
                        ui.add(
                            egui::Label::new(
                                RichText::new(format!("$ {}", toolchains::shell_line(&argv)))
                                    .monospace()
                                    .size(12.5)
                                    .color(WARN),
                            )
                            .extend(),
                        );
                    }
                    ui.add_space(4.0);
                });
        });
    ui.add(egui::Label::new(muted(tr!("La 2ᵉ commande retire aussi tout runtime qu'aucune application installée n'utilise. Les applications installées ne sont pas touchées.")).size(12.5)).wrap());
    ui.add_space(4.0);
    ui.horizontal_wrapped(|ui| {
        if button(ui, tr!("Supprimer maintenant"), BtnKind::Danger).clicked() {
            *action = Some("tc_clean");
        }
        if button(ui, tr!("Annuler"), BtnKind::Ghost).clicked() {
            *action = Some("tc_clean_no");
        }
    });
}

/// Application lancée dans un Flatpak sans la permission d'appeler `flatpak` sur l'hôte.
pub(super) fn sandbox_banner(ui: &mut Ui, app_id: &str) {
    banner(
        ui,
        DANGER,
        tr!("Permission manquante : l'application tourne dans un Flatpak"),
        tr!(
            "Pour gérer les Flatpak de votre machine, elle doit être autorisée à lancer des commandes sur l'hôte. Copiez cette commande dans un terminal, puis relancez l'application :"
        ),
    );
    let cmd = crate::host::override_command(app_id);
    ui.horizontal_wrapped(|ui| {
        ui.add(
            egui::Label::new(
                RichText::new(format!("$ {cmd}"))
                    .monospace()
                    .size(12.5)
                    .color(ACCENT),
            )
            .wrap(),
        );
        if button(ui, tr!("Copier"), BtnKind::Ghost).clicked() {
            ui.ctx().copy_text(cmd.clone());
            toast(ui.ctx(), tr!("Commande copiée dans le presse-papiers"));
        }
    });
    ui.add_space(8.0);
}

pub(super) fn banner(ui: &mut Ui, color: Color32, title: &str, text: &str) {
    Frame::new()
        .fill(color.gamma_multiply(0.1))
        .stroke(Stroke::new(1.0, color.gamma_multiply(0.45)))
        .corner_radius(CornerRadius::same(12))
        .inner_margin(Margin::same(14))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.label(RichText::new(title).strong().color(color));
            ui.add(egui::Label::new(muted(text).size(13.5)).wrap());
        });
    ui.add_space(8.0);
}

pub(super) fn history_line(ui: &mut Ui, e: &Entry) {
    let when = chrono::DateTime::from_timestamp(e.timestamp, 0)
        .map(|d| {
            d.with_timezone(&chrono::Local)
                .format("%d/%m/%Y %H:%M")
                .to_string()
        })
        .unwrap_or_default();
    ui.horizontal(|ui| {
        let (rect, _) = ui.allocate_exact_size(vec2(14.0, 20.0), Sense::hover());
        ui.painter()
            .circle_filled(rect.center(), 4.5, if e.success { SUCCESS } else { DANGER });
        ui.label(RichText::new(format!("{} {}", e.name, e.version)).strong());
        badge(
            ui,
            if e.success {
                e.action.label()
            } else {
                tr!("Échec")
            },
            if e.success { ACCENT } else { DANGER },
        );
        ui.label(muted(when).size(12.5));
    });
    if !e.app_id.is_empty() {
        ui.label(muted(e.app_id.clone()).size(12.5));
    }
}

pub(super) fn log_view(ui: &mut Ui, log: &[(Level, String)], height: f32) {
    Frame::new()
        .fill(INPUT)
        .stroke(Stroke::new(1.0, BORDER))
        .corner_radius(CornerRadius::same(10))
        .inner_margin(Margin::same(10))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ScrollArea::vertical()
                .max_height(height)
                .stick_to_bottom(true)
                .auto_shrink([false, true])
                .show(ui, |ui| {
                    ui.spacing_mut().item_spacing.y = 2.0;
                    for (l, s) in log {
                        let c = match l {
                            Level::Info => TEXT,
                            Level::Cmd => ACCENT,
                            Level::Warn => WARN,
                            Level::Error => DANGER,
                            Level::Out => MUTED,
                        };
                        ui.add(
                            egui::Label::new(RichText::new(s.as_str()).monospace().color(c)).wrap(),
                        );
                    }
                });
        });
}
