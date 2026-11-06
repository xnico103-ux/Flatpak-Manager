//! Réception des messages des tâches de fond (pipeline, installation, outils).

use super::*;

impl App {
    pub(super) fn poll(&mut self) {
        if let Some(rx) = &self.flatpak_rx
            && let Ok(s) = rx.try_recv()
        {
            self.flatpak_state = Some(s);
            self.flatpak_rx = None;
        }
        if let Some((rx, purpose)) = &self.picker
            && let Ok(res) = rx.try_recv()
        {
            let purpose = *purpose;
            self.picker = None;
            if let Some(path) = res {
                match purpose {
                    Purpose::Archive(i) => self.start_analysis(path, i),
                    Purpose::Bundle => self.start_install_review(path),
                    Purpose::Save(i) => self.start_run(i, path),
                    Purpose::Icon => {
                        if let Some(c) = &mut self.cfg {
                            c.icon = IconSource::File(path);
                        }
                        self.load_icon_preview();
                    }
                }
            }
        }
        let mut analysis_done = None;
        if let Some((rx, _)) = &self.analysis
            && let Ok(r) = rx.try_recv()
        {
            analysis_done = Some(r);
        }
        if let Some(r) = analysis_done {
            self.analysis = None;
            match r {
                Ok(info) => {
                    self.cfg = Some(BuildConfig::from_info(&info));
                    self.info = Some(Box::new(info));
                    self.id_edited = false;
                    self.screen = Screen::Review;
                    self.load_icon_preview();
                    if let Some(t) = self.tc.install.take() {
                        t.cancel.store(true, std::sync::atomic::Ordering::SeqCst);
                    }
                    self.tc = Toolchain::default();
                    self.tc_scan();
                }
                Err(e) => {
                    self.error = Some(e);
                    self.screen = Screen::Home;
                }
            }
        }
        if let Some(rx) = &self.tc.scan_rx
            && let Ok((state, tools)) = rx.try_recv()
        {
            self.tc.scan_rx = None;
            // Première analyse : on s'aligne sur le SDK déjà présent sur la machine.
            if !self.tc.version_set {
                self.tc.version_set = true;
                if let Some(c) = &mut self.cfg
                    && c.source.is_some()
                    && c.runtime == RuntimeKind::Freedesktop
                {
                    c.runtime_version = toolchains::pick_branch(
                        &state,
                        c.source
                            .as_ref()
                            .map(|s| s.system.extension_candidates())
                            .unwrap_or(&[]),
                    );
                }
            }
            self.tc.state = Some(state);
            self.tc.tools = tools;
        }
        self.mg_poll();
        self.up_poll();
        let mut tc_rescan = false;
        if let Some(inst) = &self.tc.install {
            while let Ok(m) = inst.rx.try_recv() {
                match m {
                    ToolMsg::Line(l) => {
                        self.tc.lines.push(l);
                        if self.tc.lines.len() > 300 {
                            self.tc.lines.drain(..100);
                        }
                    }
                    ToolMsg::Progress(p) => self.tc.progress = p,
                    ToolMsg::Done(r) => {
                        self.tc.error = r.err();
                        toast(
                            &self.ctx,
                            if self.tc.error.is_none() {
                                "Opération terminée"
                            } else {
                                "Échec : voir le message affiché"
                            },
                        );
                        self.tc.progress = 1.0;
                        tc_rescan = true;
                    }
                }
            }
            if tc_rescan {
                self.tc.install = None;
            }
        }
        if tc_rescan {
            self.tc_scan();
        }
        if let Some(rx) = &self.icon_rx
            && let Ok(img) = rx.try_recv()
        {
            self.icon_rx = None;
            self.icon_tex = img.map(|i| {
                self.ctx
                    .load_texture("app_icon", i, egui::TextureOptions::LINEAR)
            });
        }
        if let Some(t) = &mut self.install
            && let Some(rx) = &t.rx
            && let Ok(r) = rx.try_recv()
        {
            t.report = Some(r);
            t.rx = None;
        }
        // Événements du pipeline
        let mut finished: Option<Result<Outcome, Failure>> = None;
        if let Some(run) = &mut self.run {
            while let Ok(ev) = run.handle.rx.try_recv() {
                match ev {
                    Event::Plan(p) => run.plan = p,
                    Event::Phase(p) => run.phase = p,
                    Event::Progress(f) => run.progress = f,
                    Event::Status(s) => run.status = s,
                    Event::Log(l, s) => {
                        run.log.push((l, s));
                        if run.log.len() > 3000 {
                            run.log.drain(..500);
                        }
                    }
                    Event::AwaitConfirm(r) => run.awaiting = Some(r),
                    Event::Verified(r) => run.report = Some(r),
                    Event::Finished(r) => finished = Some(r),
                }
            }
        }
        if let Some(result) = finished
            && let Some(run) = self.run.take()
        {
            let cancelled = result.as_ref().err().is_some_and(|f| f.cancelled);
            if !cancelled {
                let (success, action, bundle) = match &result {
                    Ok(o) => (
                        true,
                        if run.action == Action::CreatedAndInstalled && !o.installed {
                            Action::Created
                        } else {
                            run.action
                        },
                        o.bundle.clone(),
                    ),
                    Err(_) => (false, run.action, None),
                };
                let (name, app_id, version) = match &result {
                    Ok(o) if !o.name.is_empty() => {
                        (o.name.clone(), o.app_id.clone(), o.version.clone())
                    }
                    _ => (run.name.clone(), run.app_id.clone(), run.version.clone()),
                };
                self.history.insert(
                    0,
                    Entry {
                        timestamp: chrono::Utc::now().timestamp(),
                        action,
                        name,
                        app_id,
                        version,
                        bundle,
                        success,
                    },
                );
                self.history.truncate(200);
                history::save(&self.history);
            }
            let report = run.report.clone();
            self.done = Some(Done {
                result,
                report,
                log: run.log,
                action: run.action,
                elapsed: run.started.elapsed(),
                plan: run.plan,
                phase: run.phase,
            });
            self.screen = Screen::Done;
        }
        // Glisser-déposer
        let dropped: Vec<PathBuf> = self.ctx.input(|i| {
            i.raw
                .dropped_files
                .iter()
                .map(|f| f.path().to_path_buf())
                .collect()
        });
        if let Some(p) = dropped.into_iter().next()
            && !matches!(self.screen, Screen::Running | Screen::Analyzing)
        {
            self.open_path(p);
        }
    }
}
