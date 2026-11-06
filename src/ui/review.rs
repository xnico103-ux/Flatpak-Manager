//! Écran de revue / configuration avant la création du Flatpak.

use super::*;

impl App {
    pub(super) fn ui_review(&mut self, ui: &mut Ui) {
        let advanced = self.advanced;
        // Les archives de sources ont besoin du SDK Flatpak avant de pouvoir être compilées.
        let tc_block: Option<String> = if self.cfg.as_ref().is_some_and(|c| c.source.is_some()) {
            match (&self.tc.state, self.tc_missing()) {
                _ if self.tc.install.is_some() => Some("Installation en cours…".into()),
                (None, _) => Some("Analyse du système…".into()),
                (Some(st), _) if !st.available => {
                    Some("Flatpak est introuvable sur ce système.".into())
                }
                (_, Some(m)) if !m.is_empty() => {
                    Some("Installez d'abord les composants manquants.".into())
                }
                _ => None,
            }
        } else {
            None
        };
        let Some(info) = self.info.as_deref() else {
            return;
        };
        let Some(cfg) = self.cfg.as_mut() else { return };
        let problems = manifest::validate(cfg, info);
        let compat = info.is_compatible();
        let warn = info
            .findings
            .iter()
            .any(|f| f.severity == Severity::Warning);
        let mut action: Option<&'static str> = None;
        let mut icon_changed = false;

        // Barre d'actions
        egui::Panel::bottom("review_bar")
            .frame(Frame::new().fill(BG).inner_margin(Margin::symmetric(0, 10)))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    if button(ui, "Retour", BtnKind::Ghost).clicked() {
                        action = Some("back");
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let ok = compat
                            && problems.is_empty()
                            && self.picker.is_none()
                            && tc_block.is_none();
                        let (pk, sk) = if self.intent == Intent::Create {
                            (BtnKind::Primary, BtnKind::Secondary)
                        } else {
                            (BtnKind::Secondary, BtnKind::Primary)
                        };
                        ui.add_enabled_ui(ok, |ui| {
                            // Dans un layout droite→gauche, le dernier ajouté est le plus à gauche.
                            let second_is_install = self.intent == Intent::Create;
                            let (a, b) = if second_is_install {
                                (
                                    ("Créer et installer", "ci", sk),
                                    ("Créer le paquet", "c", pk),
                                )
                            } else {
                                (
                                    ("Créer le paquet", "c", sk),
                                    ("Créer et installer", "ci", pk),
                                )
                            };
                            // a est placé à droite, b à sa gauche : on veut l'action principale à droite.
                            let (right, left) = if pk == BtnKind::Primary {
                                (b, a)
                            } else {
                                (a, b)
                            };
                            if button(ui, right.0, right.2).clicked() {
                                action = Some(right.1);
                            }
                            if button(ui, left.0, left.2).clicked() {
                                action = Some(left.1);
                            }
                        });
                        if let Some(p) = problems.first() {
                            ui.label(RichText::new(p.clone()).color(DANGER).size(13.0));
                        } else if let Some(b) = compat.then_some(()).and(tc_block.as_ref()) {
                            ui.label(RichText::new(b.clone()).color(WARN).size(13.0));
                        } else if !compat {
                            ui.label(
                                RichText::new("Cette archive ne peut pas être convertie.")
                                    .color(DANGER)
                                    .size(13.0),
                            );
                        }
                    });
                });
            });

        egui::CentralPanel::default().frame(Frame::NONE).show(ui, |ui| {
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                ui.label(RichText::new("Vérification de l'archive").heading().strong());
                ui.add_space(6.0);
                if !compat {
                    badge(ui, "Non convertible", DANGER);
                } else if warn {
                    badge(ui, "Convertible avec avertissements", WARN);
                } else {
                    badge(ui, "Convertible", SUCCESS);
                }
            });
            ui.label(muted(format!("{}  ·  {}  ·  {}  ·  SHA-256 {}…", info.path.file_name().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default(), info.kind.label(), human(info.size), info.sha256.chars().take(12).collect::<String>())));
            ui.add_space(8.0);

            ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                ui.columns(2, |cols| {
                    // ------------------------------------------------ colonne gauche
                    let ui = &mut cols[0];
                    card(ui, Some("Application"), |ui| {
                        ui.horizontal(|ui| {
                            let tex = self.icon_tex.as_ref().unwrap_or(&self.fallback_icon);
                            ui.add(egui::Image::new(tex).fit_to_exact_size(vec2(64.0, 64.0)).corner_radius(14));
                            ui.vertical(|ui| {
                                ui.label(RichText::new("Icône").strong());
                                let src = match &cfg.icon {
                                    IconSource::FromArchive { .. } => "Trouvée dans l'archive".to_string(),
                                    IconSource::File(p) => p.file_name().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default(),
                                    IconSource::Generated => "Générée automatiquement".to_string(),
                                };
                                ui.label(muted(src).size(13.0));
                                ui.horizontal(|ui| {
                                    if button(ui, "Choisir…", BtnKind::Secondary).clicked() {
                                        action = Some("icon");
                                    }
                                    if button(ui, "Automatique", BtnKind::Ghost).clicked() {
                                        cfg.icon = match info.best_icon() {
                                            Some(i) => IconSource::FromArchive { raw: i.raw.clone(), is_svg: i.is_svg },
                                            None => IconSource::Generated,
                                        };
                                        icon_changed = true;
                                    }
                                });
                            });
                        });
                        ui.add_space(6.0);
                        egui::Grid::new("fields").num_columns(2).spacing([14.0, 12.0]).min_col_width(92.0).show(ui, |ui| {
                            ui.label(muted("Nom"));
                            let r = ui.add(egui::TextEdit::singleline(&mut cfg.name).desired_width(f32::INFINITY));
                            if r.changed() && !self.id_edited {
                                cfg.app_id = manifest::default_app_id(&cfg.name);
                            }
                            ui.end_row();
                            ui.label(muted("Version"));
                            ui.add(egui::TextEdit::singleline(&mut cfg.version).desired_width(f32::INFINITY));
                            ui.end_row();
                            ui.label(muted("Description"));
                            ui.add(egui::TextEdit::singleline(&mut cfg.summary).hint_text("Facultatif").desired_width(f32::INFINITY));
                            ui.end_row();
                            ui.label(muted("Catégorie"));
                            let cur = CATEGORIES.iter().find(|(k, _)| *k == cfg.category).map(|(_, l)| *l).unwrap_or("Utilitaire");
                            egui::ComboBox::from_id_salt("category").selected_text(cur).width(220.0).show_ui(ui, |ui| {
                                for (k, l) in CATEGORIES {
                                    ui.selectable_value(&mut cfg.category, k.to_string(), l);
                                }
                            });
                            ui.end_row();
                            if let Some(src) = cfg.source.as_mut() {
                                ui.label(muted("Programme"));
                                ui.vertical(|ui| {
                                    // Modifier le nom vaut confirmation.
                                    if ui.add(egui::TextEdit::singleline(&mut src.bin_name).desired_width(f32::INFINITY)).changed() {
                                        src.bin_certain = true;
                                    }
                                    if src.bin_certain {
                                        ui.add(egui::Label::new(RichText::new("Nom du binaire installé (lu dans le projet ou confirmé).").color(MUTED).size(12.5)).wrap());
                                    } else {
                                        ui.add(egui::Label::new(RichText::new("Nom deviné : il faut le confirmer avant de compiler.").color(WARN).size(12.5)).wrap());
                                        ui.checkbox(&mut src.bin_certain, format!("Oui, « {} » est bien le programme à installer", src.bin_name));
                                    }
                                });
                                ui.end_row();
                            }
                            if let Some(src) = cfg.source.as_mut()
                                && src.alternatives.len() > 1
                            {
                                ui.label(muted("Point d'entrée"));
                                let cur = match &src.entry {
                                    toolchains::Entry::File(f) => f.clone(),
                                    _ => String::new(),
                                };
                                egui::ComboBox::from_id_salt("entry").selected_text(single_line_trunc(&cur, 26)).width(215.0).show_ui(ui, |ui| {
                                    for a in src.alternatives.clone() {
                                        ui.selectable_value(&mut src.entry, toolchains::Entry::File(a.clone()), &a);
                                    }
                                });
                                ui.end_row();
                            }
                            if advanced {
                                ui.label(muted("Identifiant"));
                                ui.vertical(|ui| {
                                    let r = ui.add(egui::TextEdit::singleline(&mut cfg.app_id).desired_width(f32::INFINITY));
                                    if r.changed() {
                                        self.id_edited = true;
                                    }
                                    if let Some(e) = manifest::validate_app_id(&cfg.app_id) {
                                        ui.label(RichText::new(e).color(DANGER).size(12.5));
                                    }
                                });
                                ui.end_row();
                                if cfg.source.is_none() {
                                    ui.label(muted("Exécutable"));
                                    let shown = single_line_trunc(&cfg.exec_rel, 26);
                                    egui::ComboBox::from_id_salt("exec").selected_text(shown).width(215.0).show_ui(ui, |ui| {
                                        for e in info.executables.iter().take(40) {
                                            ui.selectable_value(&mut cfg.exec_rel, e.rel.clone(), &e.rel);
                                        }
                                    });
                                    ui.end_row();
                                }
                                ui.label(muted("Runtime"));
                                ui.horizontal(|ui| {
                                    let before = cfg.runtime;
                                    let fdo_only = cfg.source.as_ref().is_some_and(|s| s.system.needs_fdo());
                                    egui::ComboBox::from_id_salt("runtime").selected_text(cfg.runtime.label()).width(150.0).show_ui(ui, |ui| {
                                        for r in RuntimeKind::ALL {
                                            if fdo_only && r != RuntimeKind::Freedesktop {
                                                continue;
                                            }
                                            ui.selectable_value(&mut cfg.runtime, r, r.label());
                                        }
                                    });
                                    if before != cfg.runtime {
                                        cfg.runtime_version = cfg.runtime.fallback_version().into();
                                    }
                                    ui.add(egui::TextEdit::singleline(&mut cfg.runtime_version).desired_width(64.0));
                                });
                                ui.end_row();
                                ui.label(muted("Lancement"));
                                ui.checkbox(&mut cfg.terminal, "Ouvrir dans un terminal");
                                ui.end_row();
                            }
                        });
                        if !advanced {
                            ui.add_space(2.0);
                            ui.label(muted(format!("Identifiant : {}", cfg.app_id)).size(12.5));
                        }
                    });
                    ui.add_space(6.0);
                    card(ui, Some("Analyse"), |ui| {
                        for f in &info.findings {
                            dot_row(ui, severity_color(f.severity), &f.title, &f.detail);
                        }
                    });

                    // ------------------------------------------------ colonne droite
                    let ui = &mut cols[1];
                    if let Some(system) = cfg.source.as_ref().map(|s| s.system) {
                        toolchain_card(ui, system, cfg.runtime, &cfg.runtime_version, &self.tc, &mut action);
                        ui.add_space(6.0);
                    }
                    card(ui, Some("Permissions"), |ui| {
                        ui.add(egui::Label::new(muted("Le bac à sable Flatpak bloque tout accès non accordé. Seul le nécessaire est activé par défaut.").size(13.0)).wrap());
                        ui.add_space(4.0);
                        let p = &mut cfg.perms;
                        egui::CollapsingHeader::new(RichText::new("Personnaliser les permissions").strong()).id_salt(("perm", advanced)).default_open(advanced).show(ui, |ui| {
                            perm_row(ui, &mut p.display, "Affichage", "Fenêtres sous Wayland ou X11.", None);
                            perm_row(ui, &mut p.gpu, "Accélération graphique", "Accès au processeur graphique (DRI).", None);
                            perm_row(ui, &mut p.audio, "Audio", "Lecture et capture via PulseAudio / PipeWire.", None);
                            perm_row(ui, &mut p.network, "Réseau", "Accès à Internet et au réseau local.", None);
                            perm_row(ui, &mut p.notifications, "Notifications", "Afficher des notifications de bureau.", None);
                            perm_row(ui, &mut p.tray, "Zone de notification", "Icône dans la barre système.", None);
                            perm_row(ui, &mut p.printing, "Impression", "Envoyer des documents à CUPS.", None);
                            perm_row(ui, &mut p.removable_media, "Supports amovibles", "Clés USB et disques externes.", Some(Severity::Info));
                            perm_row(ui, &mut p.all_devices, "Tous les périphériques", "Manettes, webcams, etc. Affaiblit le bac à sable.", Some(Severity::Warning));
                            ui.add_space(4.0);
                            ui.label(RichText::new("Accès aux fichiers").strong());
                            egui::ComboBox::from_id_salt("files").selected_text(p.files.label()).width(ui.available_width()).show_ui(ui, |ui| {
                                for f in FileAccess::ALL {
                                    ui.selectable_value(&mut p.files, f, f.label());
                                }
                            });
                        });
                        let risks = cfg.perms.risks();
                        if !risks.is_empty() {
                            ui.add_space(6.0);
                            for (s, t) in risks {
                                dot_row(ui, severity_color(s), if s == Severity::Warning { "Attention" } else { "À noter" }, &t);
                            }
                        }
                    });
                    ui.add_space(6.0);
                    card(ui, Some("Aperçu du futur Flatpak"), |ui| {
                        let n = cfg.perms.finish_args().len();
                        let rows = [
                            ("Nom", format!("{} {}", cfg.name, cfg.version)),
                            ("Identifiant", cfg.app_id.clone()),
                            ("Runtime", format!("{}//{}", cfg.runtime.platform(), cfg.runtime_version)),
                            ("Commande", cfg.command()),
                            ("Permissions", format!("{n} argument(s) de bac à sable")),
                            ("Taille décompressée", human(info.unpacked_size)),
                        ];
                        egui::Grid::new("preview").num_columns(2).spacing([14.0, 6.0]).show(ui, |ui| {
                            for (k, v) in rows {
                                ui.label(muted(k));
                                ui.add(egui::Label::new(v).wrap());
                                ui.end_row();
                            }
                        });
                        if advanced {
                            ui.add_space(6.0);
                            egui::CollapsingHeader::new("Manifeste généré").show(ui, |ui| {
                                let m = serde_json::to_string_pretty(&cfg.manifest(info, "icon.png", "256x256")).unwrap_or_default();
                                ScrollArea::both().max_height(260.0).show(ui, |ui| {
                                    ui.add(egui::Label::new(RichText::new(m).monospace().color(MUTED)).extend());
                                });
                            });
                        }
                    });
                    if advanced {
                        ui.add_space(6.0);
                        card(ui, Some("Options d'installation"), |ui| {
                            perm_row(ui, &mut self.opts.system_install, "Installer pour tous les utilisateurs", "Installation système (demande une authentification).", Some(Severity::Info));
                            perm_row(ui, &mut self.opts.replace_existing, "Remplacer une installation existante", "Réinstalle l'application ; vos données dans ~/.var/app sont conservées.", None);
                            perm_row(ui, &mut self.opts.confirm_before_install, "Confirmer avant d'installer", "Affiche le rapport de vérification et attend votre accord.", None);
                        });
                    }
                });
                ui.add_space(8.0);
            });
        });

        if icon_changed {
            self.load_icon_preview();
        }
        match action {
            Some("back") => {
                self.screen = Screen::Home;
            }
            Some("icon") => self.start_pick(Purpose::Icon),
            Some("tc_install") => {
                self.tc_install();
                toast(&self.ctx, "Installation des composants lancée…");
            }
            Some("tc_clean_ask") => self.tc.clean_ask = true,
            Some("tc_clean_no") => self.tc.clean_ask = false,
            Some("tc_clean") => {
                self.tc_clean();
                toast(&self.ctx, "Ménage lancé…");
            }
            Some("tc_rescan") => {
                toast(&self.ctx, "Environnement revérifié");
                self.tc.error = None;
                self.tc_scan();
            }
            Some("tc_cancel") => {
                toast(&self.ctx, "Annulation demandée…");
                if let Some(t) = &self.tc.install {
                    t.cancel.store(true, std::sync::atomic::Ordering::SeqCst);
                }
            }
            Some("tc_copy") => {
                if let Some(m) = self.tc_missing()
                    && let Some(st) = &self.tc.state
                {
                    let text = toolchains::install_plan(&m, st.has_flathub)
                        .iter()
                        .map(|a| toolchains::shell_line(a))
                        .collect::<Vec<_>>()
                        .join("\n");
                    self.ctx.copy_text(text);
                    toast(&self.ctx, "Commande copiée dans le presse-papiers");
                }
            }
            Some("c") => {
                self.intent = Intent::Create;
                self.start_pick(Purpose::Save(Intent::Create));
            }
            Some("ci") => {
                self.intent = Intent::CreateInstall;
                self.start_pick(Purpose::Save(Intent::CreateInstall));
            }
            _ => {}
        }
    }
}
