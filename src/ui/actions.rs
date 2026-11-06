//! Actions de l'application : ouverture, analyse, installation, gestion, mises à jour.

use super::*;

impl App {
    pub(super) fn open_path(&mut self, p: PathBuf) {
        if is_bundle(&p) {
            self.start_install_review(p);
        } else if is_archive(&p) {
            self.start_analysis(p, Intent::Create);
        } else {
            self.error = Some(format!(
                "Fichier non reconnu : {} (attendu : archive .tar.gz, .tar.xz, .tar.zst, .tar.bz2 ou fichier .flatpak).",
                p.file_name()
                    .map(|s| s.to_string_lossy().into_owned())
                    .unwrap_or_default()
            ));
            self.screen = Screen::Home;
        }
    }

    // ------------------------------------------------------------------ tâches
    pub(super) fn start_pick(&mut self, purpose: Purpose) {
        if self.picker.is_some() {
            return;
        }
        let (tx, rx) = channel();
        let ctx = self.ctx.clone();
        let default_name = match (&self.cfg, purpose) {
            (Some(c), Purpose::Save(_)) => format!("{}-{}.flatpak", c.slug(), c.version),
            _ => String::new(),
        };
        std::thread::spawn(move || {
            let dl = dirs::download_dir()
                .or_else(dirs::home_dir)
                .unwrap_or_default();
            let started = Instant::now();
            let archive_exts = ["gz", "tgz", "xz", "txz", "zst", "bz2", "tbz2"];
            let mut r = match purpose {
                Purpose::Archive(_) => rfd::FileDialog::new()
                    .set_title("Choisir une archive")
                    .add_filter("Archives tar compressées", &archive_exts)
                    .set_directory(&dl)
                    .pick_file(),
                Purpose::Bundle => rfd::FileDialog::new()
                    .set_title("Choisir un paquet Flatpak")
                    .add_filter("Paquets Flatpak", &["flatpak"])
                    .set_directory(&dl)
                    .pick_file(),
                Purpose::Save(_) => rfd::FileDialog::new()
                    .set_title("Enregistrer le paquet Flatpak")
                    .add_filter("Paquet Flatpak", &["flatpak"])
                    .set_directory(&dl)
                    .set_file_name(&default_name)
                    .save_file(),
                Purpose::Icon => rfd::FileDialog::new()
                    .set_title("Choisir une icône")
                    .add_filter("Images", &["png", "svg"])
                    .pick_file(),
            };
            // Réponse quasi immédiate sans fichier : le portail n'est probablement pas disponible
            // (hors KDE notamment). On bascule sur zenity / kdialog.
            if r.is_none() && started.elapsed() < Duration::from_millis(600) {
                r = match purpose {
                    Purpose::Archive(_) => chooser::open(
                        "Choisir une archive",
                        &dl,
                        &chooser::Filter {
                            label: "Archives tar compressées",
                            exts: &archive_exts,
                        },
                    ),
                    Purpose::Bundle => chooser::open(
                        "Choisir un paquet Flatpak",
                        &dl,
                        &chooser::Filter {
                            label: "Paquets Flatpak",
                            exts: &["flatpak"],
                        },
                    ),
                    Purpose::Icon => chooser::open(
                        "Choisir une icône",
                        &dl,
                        &chooser::Filter {
                            label: "Images",
                            exts: &["png", "svg"],
                        },
                    ),
                    Purpose::Save(_) => {
                        let f = chooser::Filter {
                            label: "Paquet Flatpak",
                            exts: &["flatpak"],
                        };
                        // Aucun sélecteur du tout : on enregistre dans le dossier Téléchargements.
                        chooser::save("Enregistrer le paquet Flatpak", &dl, &default_name, &f)
                            .or_else(|| {
                                (!chooser::available()).then(|| unique_path(&dl, &default_name))
                            })
                    }
                };
            }
            let _ = tx.send(r);
            ctx.request_repaint();
        });
        self.picker = Some((rx, purpose));
    }

    pub(super) fn start_analysis(&mut self, path: PathBuf, intent: Intent) {
        let name = path
            .file_name()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_default();
        let (tx, rx) = channel();
        let ctx = self.ctx.clone();
        let p = path.clone();
        std::thread::spawn(move || {
            let r = analyze::analyze(&p).map_err(|e| format!("{e:#}"));
            let _ = tx.send(r);
            ctx.request_repaint();
        });
        self.intent = intent;
        self.analysis = Some((rx, name));
        self.error = None;
        self.screen = Screen::Analyzing;
    }

    pub(super) fn start_install_review(&mut self, path: PathBuf) {
        let (tx, rx) = channel();
        let ctx = self.ctx.clone();
        let p = path.clone();
        std::thread::spawn(move || {
            let r = pipeline::verify_bundle_only(&p).map_err(|e| format!("{e:#}"));
            let _ = tx.send(r);
            ctx.request_repaint();
        });
        self.install = Some(InstallTarget {
            path,
            report: None,
            rx: Some(rx),
        });
        self.error = None;
        self.screen = Screen::InstallReview;
    }

    pub(super) fn tc_scan(&mut self) {
        let Some(src) = self.cfg.as_ref().and_then(|c| c.source.as_ref()) else {
            return;
        };
        let system = src.system;
        let (tx, rx) = channel();
        let ctx = self.ctx.clone();
        std::thread::spawn(move || {
            let _ = tx.send((
                toolchains::scan_flatpak(),
                toolchains::scan_host_tools(system),
            ));
            ctx.request_repaint();
        });
        self.tc.scan_rx = Some(rx);
    }

    /// Composants Flatpak manquants pour la compilation (None si l'état n'est pas encore connu).
    pub(super) fn tc_missing(&self) -> Option<Vec<toolchains::Component>> {
        let cfg = self.cfg.as_ref()?;
        let src = cfg.source.as_ref()?;
        let state = self.tc.state.as_ref()?;
        Some(toolchains::missing(&toolchains::components(
            src.system,
            cfg.runtime,
            &cfg.runtime_version,
            state,
        )))
    }

    pub(super) fn tc_install(&mut self) {
        let Some(miss) = self.tc_missing() else {
            return;
        };
        let Some(state) = &self.tc.state else { return };
        let plan = toolchains::install_plan(&miss, state.has_flathub);
        if plan.is_empty() {
            return;
        }
        let ctx = self.ctx.clone();
        let notify: Arc<dyn Fn() + Send + Sync> = Arc::new(move || ctx.request_repaint());
        self.tc.install = Some(toolchains::spawn_install(plan, notify));
        self.tc.lines.clear();
        self.tc.progress = 0.0;
        self.tc.error = None;
    }

    pub(super) fn mg_reload(&mut self) {
        let (tx, rx) = channel();
        let ctx = self.ctx.clone();
        std::thread::spawn(move || {
            let _ = tx.send(manager::list_installed());
            ctx.request_repaint();
        });
        self.mg.rx = Some(rx);
    }

    pub(super) fn mg_run(&mut self, plan: Vec<Vec<String>>) {
        let ctx = self.ctx.clone();
        let notify: Arc<dyn Fn() + Send + Sync> = Arc::new(move || ctx.request_repaint());
        self.mg.install = Some(toolchains::spawn_install(plan, notify));
        self.mg.lines.clear();
        self.mg.progress = 0.0;
        self.mg.error = None;
        self.mg.notice = None;
        self.mg.pending = None;
        self.mg.pending_unused = false;
        toast(&self.ctx, "Désinstallation lancée…");
    }

    /// Reçoit la liste et suit l'avancement d'une désinstallation.
    pub(super) fn mg_poll(&mut self) {
        if let Some(rx) = &self.mg.rx
            && let Ok(r) = rx.try_recv()
        {
            self.mg.rx = None;
            match r {
                Ok(v) => {
                    if let Some(it) = self.mg.expect_gone.take()
                        && v.iter().any(|x| {
                            x.id == it.id
                                && x.branch == it.branch
                                && x.installation == it.installation
                        })
                    {
                        let tail: Vec<&str> = self
                            .mg
                            .lines
                            .iter()
                            .rev()
                            .take(4)
                            .map(String::as_str)
                            .collect::<Vec<_>>()
                            .into_iter()
                            .rev()
                            .collect();
                        self.mg.notice = None;
                        self.mg.error = Some(format!(
                            "flatpak a répondu « succès », mais {} est toujours installé. Dernières lignes : {}",
                            it.name,
                            if tail.is_empty() {
                                "(aucune)".to_string()
                            } else {
                                tail.join(" | ")
                            }
                        ));
                    }
                    self.mg.items = v;
                    self.mg.loaded = true;
                    if std::mem::take(&mut self.mg.manual) {
                        toast(&self.ctx, "Liste actualisée");
                    }
                }
                Err(e) => {
                    self.mg.loaded = true;
                    self.mg.error = Some(e);
                }
            }
        }
        let mut done = false;
        if let Some(inst) = &self.mg.install {
            while let Ok(m) = inst.rx.try_recv() {
                match m {
                    ToolMsg::Line(l) => {
                        self.mg.lines.push(l);
                        if self.mg.lines.len() > 300 {
                            self.mg.lines.drain(..100);
                        }
                    }
                    ToolMsg::Progress(p) => self.mg.progress = p,
                    ToolMsg::Done(r) => {
                        match r {
                            Ok(()) => {
                                self.mg.notice = Some("Désinstallation terminée.".into());
                                toast(&self.ctx, "Désinstallation terminée");
                            }
                            Err(e) => self.mg.error = Some(e),
                        }
                        done = true;
                    }
                }
            }
        }
        if done {
            self.mg.install = None;
            self.mg_reload();
        }
    }

    pub(super) fn up_reload(&mut self) {
        let (tx, rx) = channel();
        let ctx = self.ctx.clone();
        self.up.loaded = false;
        std::thread::spawn(move || {
            let _ = tx.send(manager::list_updates());
            ctx.request_repaint();
        });
        self.up.rx = Some(rx);
    }

    pub(super) fn up_run(&mut self, plan: Vec<Vec<String>>) {
        let ctx = self.ctx.clone();
        let notify: Arc<dyn Fn() + Send + Sync> = Arc::new(move || ctx.request_repaint());
        self.up.install = Some(toolchains::spawn_install(plan, notify));
        self.up.lines.clear();
        self.up.progress = 0.0;
        self.up.error = None;
        self.up.notice = None;
        toast(&self.ctx, "Mise à jour lancée…");
    }

    pub(super) fn up_poll(&mut self) {
        if let Some(rx) = &self.up.rx
            && let Ok(r) = rx.try_recv()
        {
            self.up.rx = None;
            self.up.loaded = true;
            match r {
                Ok(v) => {
                    if std::mem::take(&mut self.up.manual) {
                        let n = v.len();
                        toast(
                            &self.ctx,
                            if n == 0 {
                                "Tout est à jour".to_string()
                            } else {
                                format!("{n} mise(s) à jour disponible(s)")
                            },
                        );
                    }
                    self.up.items = v;
                }
                Err(e) => self.up.error = Some(e),
            }
        }
        let mut done = false;
        if let Some(inst) = &self.up.install {
            while let Ok(m) = inst.rx.try_recv() {
                match m {
                    ToolMsg::Line(l) => {
                        self.up.lines.push(l);
                        if self.up.lines.len() > 300 {
                            self.up.lines.drain(..100);
                        }
                    }
                    ToolMsg::Progress(p) => self.up.progress = p,
                    ToolMsg::Done(r) => {
                        match r {
                            Ok(()) => {
                                self.up.notice = Some("Mise à jour terminée.".into());
                                toast(&self.ctx, "Mise à jour terminée");
                            }
                            Err(e) => self.up.error = Some(e),
                        }
                        done = true;
                    }
                }
            }
        }
        if done {
            self.up.install = None;
            self.up_reload();
            // La liste des paquets installés (versions) a changé.
            self.mg.loaded = false;
        }
    }

    pub(super) fn tc_clean(&mut self) {
        let (Some(cfg), Some(state)) = (&self.cfg, &self.tc.state) else {
            return;
        };
        let plan =
            toolchains::cleanup_plan(&toolchains::cleanup_candidates(state, &cfg.runtime_version));
        self.tc.clean_ask = false;
        if plan.is_empty() {
            return;
        }
        let ctx = self.ctx.clone();
        let notify: Arc<dyn Fn() + Send + Sync> = Arc::new(move || ctx.request_repaint());
        self.tc.install = Some(toolchains::spawn_install(plan, notify));
        self.tc.lines.clear();
        self.tc.progress = 0.0;
        self.tc.error = None;
    }

    pub(super) fn load_icon_preview(&mut self) {
        let (Some(info), Some(cfg)) = (&self.info, &self.cfg) else {
            return;
        };
        let src = cfg.icon.clone();
        let archive = info.path.clone();
        let (tx, rx) = channel();
        let ctx = self.ctx.clone();
        std::thread::spawn(move || {
            let bytes = match src {
                IconSource::FromArchive { raw, is_svg: false } => {
                    analyze::read_member(&archive, &raw, 5 << 20).ok()
                }
                IconSource::File(p)
                    if p.extension().is_some_and(|e| e.eq_ignore_ascii_case("png")) =>
                {
                    std::fs::read(p).ok()
                }
                _ => None,
            };
            let img = bytes
                .and_then(|b| image::load_from_memory(&b).ok())
                .map(|i| {
                    let i = i
                        .resize(128, 128, image::imageops::FilterType::Lanczos3)
                        .to_rgba8();
                    egui::ColorImage::from_rgba_unmultiplied(
                        [i.width() as usize, i.height() as usize],
                        i.as_raw(),
                    )
                });
            let _ = tx.send(img);
            ctx.request_repaint();
        });
        self.icon_rx = Some(rx);
        self.icon_tex = None;
    }

    pub(super) fn start_run(&mut self, intent: Intent, out: PathBuf) {
        let (Some(info), Some(mut cfg)) = (self.info.clone(), self.cfg.clone()) else {
            return;
        };
        if let (Some(src), Some(st)) = (cfg.source.as_mut(), self.tc.state.as_ref()) {
            src.extension = toolchains::resolve_extension(src.system, &cfg.runtime_version, st);
        }
        let mut opts = self.opts;
        opts.confirm_before_install =
            self.advanced && opts.confirm_before_install && intent == Intent::CreateInstall;
        let ctx = self.ctx.clone();
        let notify: Arc<dyn Fn() + Send + Sync> = Arc::new(move || ctx.request_repaint());
        let (name, app_id, version) = (cfg.name.clone(), cfg.app_id.clone(), cfg.version.clone());
        let job = match intent {
            Intent::Create => Job::Create {
                info,
                cfg: Box::new(cfg),
                out,
            },
            Intent::CreateInstall => Job::CreateInstall {
                info,
                cfg: Box::new(cfg),
                out,
            },
        };
        let handle = pipeline::spawn(job, opts, notify);
        self.run = Some(Run {
            handle,
            plan: vec![],
            phase: Phase::Prepare,
            progress: 0.0,
            status: "Démarrage…".into(),
            log: vec![],
            awaiting: None,
            report: None,
            action: if intent == Intent::Create {
                Action::Created
            } else {
                Action::CreatedAndInstalled
            },
            started: Instant::now(),
            name,
            app_id,
            version,
            cancelling: false,
        });
        self.screen = Screen::Running;
        self.show_log = false;
    }

    pub(super) fn start_install(&mut self) {
        let Some(t) = &self.install else { return };
        let handle = {
            let ctx = self.ctx.clone();
            let notify: Arc<dyn Fn() + Send + Sync> = Arc::new(move || ctx.request_repaint());
            pipeline::spawn(
                Job::Install {
                    bundle: t.path.clone(),
                },
                self.opts,
                notify,
            )
        };
        let name = t
            .path
            .file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_default();
        self.run = Some(Run {
            handle,
            plan: vec![],
            phase: Phase::Verify,
            progress: 0.0,
            status: "Démarrage…".into(),
            log: vec![],
            awaiting: None,
            report: None,
            action: Action::Installed,
            started: Instant::now(),
            name,
            app_id: String::new(),
            version: String::new(),
            cancelling: false,
        });
        self.screen = Screen::Running;
        self.show_log = false;
    }
}
