//! Écran des mises à jour.

use super::*;

impl App {
    pub(super) fn ui_updates(&mut self, ui: &mut Ui) {
        ui.add_space(8.0);
        ui.label(RichText::new(tr!("Mises à jour")).heading().strong());
        ui.label(muted(tr!("Mettez à jour une application ou tout d'un coup. Les commandes lancées sont affichées dans le journal.")));
        ui.add_space(10.0);
        if let Some(id) = crate::host::sandbox_blocked() {
            sandbox_banner(ui, &id);
            return;
        }
        let busy = self.up.install.is_some();
        let mut action: Option<(&'static str, usize)> = None;
        let visible = |u: &manager::Update, rt: bool| rt || !u.runtime;
        let n_apps = self.up.items.iter().filter(|u| !u.runtime).count();
        let n_all = self.up.items.len();

        ui.horizontal_wrapped(|ui| {
            ui.add_enabled_ui(!busy && n_all > 0, |ui| {
                if button(
                    ui,
                    tr!("Tout mettre à jour ({n})", n = n_all).as_str(),
                    BtnKind::Primary,
                )
                .clicked()
                {
                    action = Some(("all", 0));
                }
            });
            if button(ui, tr!("Rechercher"), BtnKind::Secondary).clicked()
                && !busy
                && self.up.rx.is_none()
            {
                action = Some(("reload", 0));
            }
            ui.checkbox(&mut self.up.show_runtimes, tr!("Afficher les runtimes"));
        });
        ui.add_space(6.0);
        if let Some(e) = &self.up.error {
            ui.add(egui::Label::new(RichText::new(e.as_str()).color(DANGER).size(13.0)).wrap());
        }
        if let Some(n) = &self.up.notice {
            ui.label(RichText::new(n.as_str()).color(SUCCESS).size(13.0));
        }
        if busy {
            card(ui, None, |ui| {
                progress_bar(ui, self.up.progress, 8.0);
                let start = self.up.lines.len().saturating_sub(6);
                for l in &self.up.lines[start..] {
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
        if self.up.rx.is_some() {
            ui.horizontal(|ui| {
                ui.spinner();
                ui.label(muted(tr!("Recherche des mises à jour…")));
            });
        }
        let show_rt = self.up.show_runtimes;
        ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            let mut shown = 0;
            for (i, u) in self.up.items.iter().enumerate() {
                if !visible(u, show_rt) {
                    continue;
                }
                shown += 1;
                card(ui, None, |ui| {
                    ui.horizontal(|ui| {
                        ui.vertical(|ui| {
                            ui.horizontal(|ui| {
                                ui.label(RichText::new(&u.name).strong());
                                badge(ui, if u.is_user() { tr!("utilisateur") } else { &u.installation }, if u.is_user() { ACCENT } else { MUTED });
                                if u.runtime {
                                    badge(ui, "runtime", MUTED);
                                }
                            });
                            ui.label(muted(format!("{}//{}  ·  {}", u.id, u.branch, u.size)).size(12.5));
                        });
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            ui.add_enabled_ui(!busy, |ui| {
                                if button(ui, tr!("Mettre à jour"), BtnKind::Secondary).clicked() {
                                    action = Some(("one", i));
                                }
                            });
                        });
                    });
                });
            }
            if self.up.loaded && self.up.rx.is_none() && shown == 0 {
                card(ui, None, |ui| {
                    let hidden = n_all - n_apps;
                    if n_all == 0 {
                        ui.label(RichText::new(tr!("Tout est à jour.")).color(SUCCESS));
                    } else {
                        ui.label(muted(tr!("Aucune application à mettre à jour ({hidden} runtime(s) disponible(s) : cochez « Afficher les runtimes »).", hidden = hidden)));
                    }
                });
            }
        });
        match action {
            Some(("reload", _)) => {
                self.up.manual = true;
                self.up.notice = None;
                self.up.error = None;
                self.up_reload();
            }
            Some(("one", i)) => {
                if let Some(u) = self.up.items.get(i).cloned() {
                    self.up_run(manager::update_one_plan(&u));
                }
            }
            Some(("all", _)) => {
                let plan = manager::update_all_plan(&self.up.items);
                if !plan.is_empty() {
                    self.up_run(plan);
                }
            }
            Some(("stop", _)) => {
                toast(&self.ctx, tr!("Annulation demandée…"));
                if let Some(t) = &self.up.install {
                    t.cancel.store(true, std::sync::atomic::Ordering::SeqCst);
                }
            }
            _ => {}
        }
    }
}
