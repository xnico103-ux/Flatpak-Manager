//! Interface graphique (egui).

mod theme;

use crate::chooser;
use crate::analyze::{self, ArchiveInfo, Severity};
use crate::history::{self, Action, Entry};
use crate::manager::{self, Installed};
use crate::manifest::{self, BuildConfig, CATEGORIES, FileAccess, IconSource, RuntimeKind};
use crate::toolchains::{self, BuildSystem, FlatpakState, HostTool, ToolInstall, ToolMsg};
use crate::pipeline::{self, Event, Failure, Handle, Job, Level, Options, Outcome, Phase, VerifyReport};
use egui::{Color32, CornerRadius, Frame, Margin, RichText, ScrollArea, Sense, Stroke, TextureHandle, Ui, vec2};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::mpsc::{Receiver, channel};
use std::time::{Duration, Instant};
use theme::*;

pub const LOGO_BYTES: &[u8] = include_bytes!("../../assets/logo.png");

#[derive(Clone, Copy, PartialEq, Eq)]
enum Intent {
    Create,
    CreateInstall,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Purpose {
    Archive(Intent),
    Bundle,
    Save(Intent),
    Icon,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Screen {
    Home,
    Analyzing,
    Review,
    InstallReview,
    Running,
    Done,
    History,
    Manage,
    Updates,
}

struct InstallTarget {
    path: PathBuf,
    report: Option<Result<VerifyReport, String>>,
    rx: Option<Receiver<Result<VerifyReport, String>>>,
}

struct Run {
    handle: Handle,
    plan: Vec<Phase>,
    phase: Phase,
    progress: f32,
    status: String,
    log: Vec<(Level, String)>,
    awaiting: Option<VerifyReport>,
    report: Option<VerifyReport>,
    action: Action,
    started: Instant,
    name: String,
    app_id: String,
    version: String,
    cancelling: bool,
}

struct Done {
    result: Result<Outcome, Failure>,
    report: Option<VerifyReport>,
    log: Vec<(Level, String)>,
    action: Action,
    elapsed: Duration,
    plan: Vec<Phase>,
    phase: Phase,
}

/// Écran « Gérer les Flatpak » : liste des paquets installés et désinstallation.
#[derive(Default)]
struct Manage {
    rx: Option<Receiver<Result<Vec<Installed>, String>>>,
    items: Vec<Installed>,
    loaded: bool,
    filter: String,
    show_runtimes: bool,
    /// Élément dont la désinstallation attend une confirmation.
    pending: Option<Installed>,
    pending_unused: bool,
    delete_data: bool,
    install: Option<ToolInstall>,
    lines: Vec<String>,
    progress: f32,
    error: Option<String>,
    notice: Option<String>,
    manual: bool,
    /// Élément dont la disparition de la liste doit être confirmée après désinstallation.
    expect_gone: Option<Installed>,
}

/// Écran « Mises à jour ».
#[derive(Default)]
struct Updates {
    rx: Option<Receiver<Result<Vec<manager::Update>, String>>>,
    items: Vec<manager::Update>,
    loaded: bool,
    show_runtimes: bool,
    install: Option<ToolInstall>,
    lines: Vec<String>,
    progress: f32,
    error: Option<String>,
    notice: Option<String>,
    manual: bool,
}

/// État de l'environnement de compilation (archives de sources).
#[derive(Default)]
struct Toolchain {
    scan_rx: Option<Receiver<(FlatpakState, Vec<HostTool>)>>,
    state: Option<FlatpakState>,
    tools: Vec<HostTool>,
    install: Option<ToolInstall>,
    lines: Vec<String>,
    progress: f32,
    error: Option<String>,
    version_set: bool,
    clean_ask: bool,
}

pub struct App {
    ctx: egui::Context,
    logo: TextureHandle,
    fallback_icon: TextureHandle,
    screen: Screen,
    advanced: bool,
    opts: Options,
    history: Vec<Entry>,
    error: Option<String>,
    flatpak_rx: Option<Receiver<(bool, bool)>>,
    flatpak_state: Option<(bool, bool)>,
    picker: Option<(Receiver<Option<PathBuf>>, Purpose)>,
    analysis: Option<(Receiver<Result<ArchiveInfo, String>>, String)>,
    info: Option<Box<ArchiveInfo>>,
    cfg: Option<BuildConfig>,
    intent: Intent,
    id_edited: bool,
    icon_tex: Option<TextureHandle>,
    icon_rx: Option<Receiver<Option<egui::ColorImage>>>,
    install: Option<InstallTarget>,
    run: Option<Run>,
    done: Option<Done>,
    show_log: bool,
    tc: Toolchain,
    mg: Manage,
    up: Updates,
    path_input: String,
    toast: Option<(String, Instant)>,
}

/// `dir/name`, ou `dir/name-1.ext`, `-2`… si le fichier existe déjà (jamais d'écrasement silencieux).
fn unique_path(dir: &Path, name: &str) -> PathBuf {
    let first = dir.join(name);
    if !first.exists() {
        return first;
    }
    let (stem, ext) = match name.rsplit_once('.') {
        Some((s, e)) => (s.to_string(), format!(".{e}")),
        None => (name.to_string(), String::new()),
    };
    (1..1000).map(|n| dir.join(format!("{stem}-{n}{ext}"))).find(|p| !p.exists()).unwrap_or(first)
}

fn human(bytes: u64) -> String {
    const U: [&str; 4] = ["o", "Kio", "Mio", "Gio"];
    let mut v = bytes as f64;
    let mut i = 0;
    while v >= 1024.0 && i < 3 {
        v /= 1024.0;
        i += 1;
    }
    if i == 0 { format!("{bytes} o") } else { format!("{v:.1} {}", U[i]) }
}

fn is_archive(p: &Path) -> bool {
    let n = p.file_name().map(|s| s.to_string_lossy().to_lowercase()).unwrap_or_default();
    n.ends_with(".tar.gz") || n.ends_with(".tgz") || n.ends_with(".tar.xz") || n.ends_with(".txz") || n.ends_with(".tar.bz2") || n.ends_with(".tbz2") || n.ends_with(".tar.zst")
}

fn is_bundle(p: &Path) -> bool {
    p.extension().is_some_and(|e| e.eq_ignore_ascii_case("flatpak"))
}

fn color_image(bytes: &[u8]) -> Option<egui::ColorImage> {
    let img = image::load_from_memory(bytes).ok()?.to_rgba8();
    let size = [img.width() as usize, img.height() as usize];
    Some(egui::ColorImage::from_rgba_unmultiplied(size, img.as_raw()))
}

impl App {
    pub fn new(cc: &eframe::CreationContext<'_>, initial: Option<PathBuf>) -> Self {
        setup(&cc.egui_ctx);
        let ctx = cc.egui_ctx.clone();
        let logo_img = color_image(LOGO_BYTES).unwrap_or_else(|| {
            let g = manifest::generated_icon("flatpak.creator");
            egui::ColorImage::from_rgba_unmultiplied([256, 256], g.as_raw())
        });
        let logo = ctx.load_texture("logo", logo_img, egui::TextureOptions::LINEAR);
        let g = manifest::generated_icon("local.flatpak.Placeholder");
        let fallback_icon = ctx.load_texture("fallback_icon", egui::ColorImage::from_rgba_unmultiplied([256, 256], g.as_raw()), egui::TextureOptions::LINEAR);

        // Détection de Flatpak et Flatpak Builder en arrière-plan.
        let (tx, rx) = channel();
        let c2 = ctx.clone();
        std::thread::spawn(move || {
            let ok = |p: &str, a: &[&str]| crate::host::cmd(p).args(a).stdin(std::process::Stdio::null()).stdout(std::process::Stdio::null()).stderr(std::process::Stdio::null()).status().map(|s| s.success()).unwrap_or(false);
            let fp = ok("flatpak", &["--version"]);
            let b = fp && (ok("flatpak-builder", &["--version"]) || ok("flatpak", &["info", "org.flatpak.Builder"]));
            let _ = tx.send((fp, b));
            c2.request_repaint();
        });

        let mut app = App {
            ctx,
            logo,
            fallback_icon,
            screen: Screen::Home,
            advanced: false,
            opts: Options::default(),
            history: history::load(),
            error: None,
            flatpak_rx: Some(rx),
            flatpak_state: None,
            picker: None,
            analysis: None,
            info: None,
            cfg: None,
            intent: Intent::Create,
            id_edited: false,
            icon_tex: None,
            icon_rx: None,
            install: None,
            run: None,
            done: None,
            show_log: false,
            tc: Toolchain::default(),
            mg: Manage::default(),
            up: Updates::default(),
            path_input: String::new(),
            toast: None,
        };
        if let Some(p) = initial {
            app.open_path(p);
        }
        app
    }

    fn open_path(&mut self, p: PathBuf) {
        if is_bundle(&p) {
            self.start_install_review(p);
        } else if is_archive(&p) {
            self.start_analysis(p, Intent::Create);
        } else {
            self.error = Some(format!("Fichier non reconnu : {} (attendu : archive .tar.gz, .tar.xz, .tar.zst, .tar.bz2 ou fichier .flatpak).", p.file_name().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default()));
            self.screen = Screen::Home;
        }
    }

    // ------------------------------------------------------------------ tâches
    fn start_pick(&mut self, purpose: Purpose) {
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
            let dl = dirs::download_dir().or_else(dirs::home_dir).unwrap_or_default();
            let started = Instant::now();
            let archive_exts = ["gz", "tgz", "xz", "txz", "zst", "bz2", "tbz2"];
            let mut r = match purpose {
                Purpose::Archive(_) => rfd::FileDialog::new().set_title("Choisir une archive").add_filter("Archives tar compressées", &archive_exts).set_directory(&dl).pick_file(),
                Purpose::Bundle => rfd::FileDialog::new().set_title("Choisir un paquet Flatpak").add_filter("Paquets Flatpak", &["flatpak"]).set_directory(&dl).pick_file(),
                Purpose::Save(_) => rfd::FileDialog::new().set_title("Enregistrer le paquet Flatpak").add_filter("Paquet Flatpak", &["flatpak"]).set_directory(&dl).set_file_name(&default_name).save_file(),
                Purpose::Icon => rfd::FileDialog::new().set_title("Choisir une icône").add_filter("Images", &["png", "svg"]).pick_file(),
            };
            // Réponse quasi immédiate sans fichier : le portail n'est probablement pas disponible
            // (hors KDE notamment). On bascule sur zenity / kdialog.
            if r.is_none() && started.elapsed() < Duration::from_millis(600) {
                r = match purpose {
                    Purpose::Archive(_) => chooser::open("Choisir une archive", &dl, &chooser::Filter { label: "Archives tar compressées", exts: &archive_exts }),
                    Purpose::Bundle => chooser::open("Choisir un paquet Flatpak", &dl, &chooser::Filter { label: "Paquets Flatpak", exts: &["flatpak"] }),
                    Purpose::Icon => chooser::open("Choisir une icône", &dl, &chooser::Filter { label: "Images", exts: &["png", "svg"] }),
                    Purpose::Save(_) => {
                        let f = chooser::Filter { label: "Paquet Flatpak", exts: &["flatpak"] };
                        // Aucun sélecteur du tout : on enregistre dans le dossier Téléchargements.
                        chooser::save("Enregistrer le paquet Flatpak", &dl, &default_name, &f).or_else(|| (!chooser::available()).then(|| unique_path(&dl, &default_name)))
                    }
                };
            }
            let _ = tx.send(r);
            ctx.request_repaint();
        });
        self.picker = Some((rx, purpose));
    }

    fn start_analysis(&mut self, path: PathBuf, intent: Intent) {
        let name = path.file_name().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
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

    fn start_install_review(&mut self, path: PathBuf) {
        let (tx, rx) = channel();
        let ctx = self.ctx.clone();
        let p = path.clone();
        std::thread::spawn(move || {
            let r = pipeline::verify_bundle_only(&p).map_err(|e| format!("{e:#}"));
            let _ = tx.send(r);
            ctx.request_repaint();
        });
        self.install = Some(InstallTarget { path, report: None, rx: Some(rx) });
        self.error = None;
        self.screen = Screen::InstallReview;
    }

    fn tc_scan(&mut self) {
        let Some(src) = self.cfg.as_ref().and_then(|c| c.source.as_ref()) else { return };
        let system = src.system;
        let (tx, rx) = channel();
        let ctx = self.ctx.clone();
        std::thread::spawn(move || {
            let _ = tx.send((toolchains::scan_flatpak(), toolchains::scan_host_tools(system)));
            ctx.request_repaint();
        });
        self.tc.scan_rx = Some(rx);
    }

    /// Composants Flatpak manquants pour la compilation (None si l'état n'est pas encore connu).
    fn tc_missing(&self) -> Option<Vec<toolchains::Component>> {
        let cfg = self.cfg.as_ref()?;
        let src = cfg.source.as_ref()?;
        let state = self.tc.state.as_ref()?;
        Some(toolchains::missing(&toolchains::components(src.system, cfg.runtime, &cfg.runtime_version, state)))
    }

    fn tc_install(&mut self) {
        let Some(miss) = self.tc_missing() else { return };
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

    fn mg_reload(&mut self) {
        let (tx, rx) = channel();
        let ctx = self.ctx.clone();
        std::thread::spawn(move || {
            let _ = tx.send(manager::list_installed());
            ctx.request_repaint();
        });
        self.mg.rx = Some(rx);
    }

    fn mg_run(&mut self, plan: Vec<Vec<String>>) {
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
    fn mg_poll(&mut self) {
        if let Some(rx) = &self.mg.rx
            && let Ok(r) = rx.try_recv() {
                self.mg.rx = None;
                match r {
                    Ok(v) => {
                        if let Some(it) = self.mg.expect_gone.take()
                            && v.iter().any(|x| x.id == it.id && x.branch == it.branch && x.installation == it.installation)
                        {
                            let tail: Vec<&str> = self.mg.lines.iter().rev().take(4).map(String::as_str).collect::<Vec<_>>().into_iter().rev().collect();
                            self.mg.notice = None;
                            self.mg.error = Some(format!(
                                "flatpak a répondu « succès », mais {} est toujours installé. Dernières lignes : {}",
                                it.name,
                                if tail.is_empty() { "(aucune)".to_string() } else { tail.join(" | ") }
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

    fn up_reload(&mut self) {
        let (tx, rx) = channel();
        let ctx = self.ctx.clone();
        self.up.loaded = false;
        std::thread::spawn(move || {
            let _ = tx.send(manager::list_updates());
            ctx.request_repaint();
        });
        self.up.rx = Some(rx);
    }

    fn up_run(&mut self, plan: Vec<Vec<String>>) {
        let ctx = self.ctx.clone();
        let notify: Arc<dyn Fn() + Send + Sync> = Arc::new(move || ctx.request_repaint());
        self.up.install = Some(toolchains::spawn_install(plan, notify));
        self.up.lines.clear();
        self.up.progress = 0.0;
        self.up.error = None;
        self.up.notice = None;
        toast(&self.ctx, "Mise à jour lancée…");
    }

    fn up_poll(&mut self) {
        if let Some(rx) = &self.up.rx
            && let Ok(r) = rx.try_recv() {
                self.up.rx = None;
                self.up.loaded = true;
                match r {
                    Ok(v) => {
                        if std::mem::take(&mut self.up.manual) {
                            let n = v.len();
                            toast(&self.ctx, if n == 0 { "Tout est à jour".to_string() } else { format!("{n} mise(s) à jour disponible(s)") });
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

    fn tc_clean(&mut self) {
        let (Some(cfg), Some(state)) = (&self.cfg, &self.tc.state) else { return };
        let plan = toolchains::cleanup_plan(&toolchains::cleanup_candidates(state, &cfg.runtime_version));
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

    fn load_icon_preview(&mut self) {
        let (Some(info), Some(cfg)) = (&self.info, &self.cfg) else { return };
        let src = cfg.icon.clone();
        let archive = info.path.clone();
        let (tx, rx) = channel();
        let ctx = self.ctx.clone();
        std::thread::spawn(move || {
            let bytes = match src {
                IconSource::FromArchive { raw, is_svg: false } => analyze::read_member(&archive, &raw, 5 << 20).ok(),
                IconSource::File(p) if p.extension().is_some_and(|e| e.eq_ignore_ascii_case("png")) => std::fs::read(p).ok(),
                _ => None,
            };
            let img = bytes.and_then(|b| image::load_from_memory(&b).ok()).map(|i| {
                let i = i.resize(128, 128, image::imageops::FilterType::Lanczos3).to_rgba8();
                egui::ColorImage::from_rgba_unmultiplied([i.width() as usize, i.height() as usize], i.as_raw())
            });
            let _ = tx.send(img);
            ctx.request_repaint();
        });
        self.icon_rx = Some(rx);
        self.icon_tex = None;
    }

    fn start_run(&mut self, intent: Intent, out: PathBuf) {
        let (Some(info), Some(mut cfg)) = (self.info.clone(), self.cfg.clone()) else { return };
        if let (Some(src), Some(st)) = (cfg.source.as_mut(), self.tc.state.as_ref()) {
            src.extension = toolchains::resolve_extension(src.system, &cfg.runtime_version, st);
        }
        let mut opts = self.opts;
        opts.confirm_before_install = self.advanced && opts.confirm_before_install && intent == Intent::CreateInstall;
        let ctx = self.ctx.clone();
        let notify: Arc<dyn Fn() + Send + Sync> = Arc::new(move || ctx.request_repaint());
        let (name, app_id, version) = (cfg.name.clone(), cfg.app_id.clone(), cfg.version.clone());
        let job = match intent {
            Intent::Create => Job::Create { info, cfg: Box::new(cfg), out },
            Intent::CreateInstall => Job::CreateInstall { info, cfg: Box::new(cfg), out },
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
            action: if intent == Intent::Create { Action::Created } else { Action::CreatedAndInstalled },
            started: Instant::now(),
            name,
            app_id,
            version,
            cancelling: false,
        });
        self.screen = Screen::Running;
        self.show_log = false;
    }

    fn start_install(&mut self) {
        let Some(t) = &self.install else { return };
        let handle = {
            let ctx = self.ctx.clone();
            let notify: Arc<dyn Fn() + Send + Sync> = Arc::new(move || ctx.request_repaint());
            pipeline::spawn(Job::Install { bundle: t.path.clone() }, self.opts, notify)
        };
        let name = t.path.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
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

    // ------------------------------------------------------------------ logique
    fn poll(&mut self) {
        if let Some(rx) = &self.flatpak_rx
            && let Ok(s) = rx.try_recv() {
                self.flatpak_state = Some(s);
                self.flatpak_rx = None;
            }
        if let Some((rx, purpose)) = &self.picker
            && let Ok(res) = rx.try_recv() {
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
            && let Ok(r) = rx.try_recv() {
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
            && let Ok((state, tools)) = rx.try_recv() {
                self.tc.scan_rx = None;
                // Première analyse : on s'aligne sur le SDK déjà présent sur la machine.
                if !self.tc.version_set {
                    self.tc.version_set = true;
                    if let Some(c) = &mut self.cfg
                        && c.source.is_some() && c.runtime == RuntimeKind::Freedesktop {
                            c.runtime_version = toolchains::pick_branch(&state, c.source.as_ref().map(|s| s.system.extension_candidates()).unwrap_or(&[]));
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
                        toast(&self.ctx, if self.tc.error.is_none() { "Opération terminée" } else { "Échec : voir le message affiché" });
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
            && let Ok(img) = rx.try_recv() {
                self.icon_rx = None;
                self.icon_tex = img.map(|i| self.ctx.load_texture("app_icon", i, egui::TextureOptions::LINEAR));
            }
        if let Some(t) = &mut self.install
            && let Some(rx) = &t.rx
                && let Ok(r) = rx.try_recv() {
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
            && let Some(run) = self.run.take() {
                let cancelled = result.as_ref().err().is_some_and(|f| f.cancelled);
                if !cancelled {
                    let (success, action, bundle) = match &result {
                        Ok(o) => (
                            true,
                            if run.action == Action::CreatedAndInstalled && !o.installed { Action::Created } else { run.action },
                            o.bundle.clone(),
                        ),
                        Err(_) => (false, run.action, None),
                    };
                    let (name, app_id, version) = match &result {
                        Ok(o) if !o.name.is_empty() => (o.name.clone(), o.app_id.clone(), o.version.clone()),
                        _ => (run.name.clone(), run.app_id.clone(), run.version.clone()),
                    };
                    self.history.insert(0, Entry { timestamp: chrono::Utc::now().timestamp(), action, name, app_id, version, bundle, success });
                    self.history.truncate(200);
                    history::save(&self.history);
                }
                let report = run.report.clone();
                self.done = Some(Done { result, report, log: run.log, action: run.action, elapsed: run.started.elapsed(), plan: run.plan, phase: run.phase });
                self.screen = Screen::Done;
            }
        // Glisser-déposer
        let dropped: Vec<PathBuf> = self.ctx.input(|i| i.raw.dropped_files.iter().map(|f| f.path().to_path_buf()).collect());
        if let Some(p) = dropped.into_iter().next()
            && !matches!(self.screen, Screen::Running | Screen::Analyzing) {
                self.open_path(p);
            }
    }

    // ------------------------------------------------------------------ vues
    fn sidebar(&mut self, ui: &mut Ui) {
        ui.add_space(6.0);
        ui.horizontal(|ui| {
            ui.add(egui::Image::new(&self.logo).fit_to_exact_size(vec2(40.0, 40.0)).corner_radius(10));
            ui.vertical(|ui| {
                ui.spacing_mut().item_spacing.y = 0.0;
                ui.label(RichText::new("Flatpak").strong().size(17.0));
                ui.label(muted("Manager").size(14.0));
            });
        });
        ui.add_space(22.0);
        let busy = matches!(self.screen, Screen::Running);
        if nav_item(ui, "Accueil", !matches!(self.screen, Screen::History | Screen::Manage | Screen::Updates)).clicked() && !busy
            && matches!(self.screen, Screen::History | Screen::Manage | Screen::Updates) {
                self.screen = Screen::Home;
            }
        let ulabel = if self.up.loaded && !self.up.items.is_empty() { format!("Mises à jour ({})", self.up.items.len()) } else { "Mises à jour".to_string() };
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
                        toast(ui.ctx(), if self.advanced { "Mode avancé activé" } else { "Mode avancé désactivé" });
                    }
                });
            });
            ui.add_space(4.0);
            ui.separator();
        });
    }

    fn ui_home(&mut self, ui: &mut Ui) {
        ui.add_space(8.0);
        ui.label(RichText::new("Que voulez-vous faire ?").heading().strong());
        ui.label(muted("Créez ou installez une application Flatpak en quelques clics, sans terminal."));
        ui.add_space(14.0);

        if let Some(id) = crate::host::sandbox_blocked() {
            sandbox_banner(ui, &id);
        } else if let Some((fp, _)) = self.flatpak_state {
            if !fp {
                banner(ui, DANGER, "Flatpak n'est pas détecté sur ce système", "Installez le paquet « flatpak » avec votre gestionnaire de paquets (ex. : sudo pacman -S flatpak, sudo apt install flatpak, sudo dnf install flatpak), puis relancez Flatpak Manager.");
            } else if let Some((_, false)) = self.flatpak_state {
                banner(ui, ACCENT, "Flatpak Builder sera installé automatiquement", "Au premier usage, l'outil de construction (org.flatpak.Builder) sera téléchargé depuis Flathub pour votre utilisateur.");
            }
        }
        let mut dismiss = false;
        if let Some(e) = &self.error {
            ui.horizontal_top(|ui| {
                Frame::new().fill(DANGER.gamma_multiply(0.12)).stroke(Stroke::new(1.0, DANGER.gamma_multiply(0.5))).corner_radius(CornerRadius::same(12)).inner_margin(Margin::same(14)).show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.horizontal(|ui| {
                        ui.vertical(|ui| {
                            ui.set_width(ui.available_width() - 90.0);
                            ui.label(RichText::new("Impossible d'ouvrir ce fichier").strong().color(DANGER));
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
        let w = if stacked { avail } else { (avail - 2.0 * gap) / 3.0 };
        let mut pick: Option<Purpose> = None;
        let mut cards = |ui: &mut Ui| {
            if action_card(ui, w, IconKind::Package, "Créer un Flatpak", "Choisissez une archive (.tar.gz, .tar.xz…) : elle est analysée, puis convertie en fichier .flatpak.", ACCENT, !busy).clicked() {
                pick = Some(Purpose::Archive(Intent::Create));
            }
            if action_card(ui, w, IconKind::Install, "Installer un Flatpak", "Sélectionnez un fichier .flatpak existant pour l'installer sur ce système.", SUCCESS, !busy).clicked() {
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

    fn ui_analyzing(&mut self, ui: &mut Ui) {
        ui.vertical_centered(|ui| {
            ui.add_space(ui.available_height() * 0.32);
            ui.spinner();
            ui.add_space(10.0);
            ui.label(RichText::new("Analyse de l'archive…").size(20.0).strong());
            if let Some((_, name)) = &self.analysis {
                ui.label(muted(name.clone()));
            }
            ui.label(muted("Contrôles de sécurité, recherche de l'exécutable et des dépendances."));
        });
    }

    fn ui_review(&mut self, ui: &mut Ui) {
        let advanced = self.advanced;
        // Les archives de sources ont besoin du SDK Flatpak avant de pouvoir être compilées.
        let tc_block: Option<String> = if self.cfg.as_ref().is_some_and(|c| c.source.is_some()) {
            match (&self.tc.state, self.tc_missing()) {
                _ if self.tc.install.is_some() => Some("Installation en cours…".into()),
                (None, _) => Some("Analyse du système…".into()),
                (Some(st), _) if !st.available => Some("Flatpak est introuvable sur ce système.".into()),
                (_, Some(m)) if !m.is_empty() => Some("Installez d'abord les composants manquants.".into()),
                _ => None,
            }
        } else {
            None
        };
        let Some(info) = self.info.as_deref() else { return };
        let Some(cfg) = self.cfg.as_mut() else { return };
        let problems = manifest::validate(cfg, info);
        let compat = info.is_compatible();
        let warn = info.findings.iter().any(|f| f.severity == Severity::Warning);
        let mut action: Option<&'static str> = None;
        let mut icon_changed = false;

        // Barre d'actions
        egui::Panel::bottom("review_bar").frame(Frame::new().fill(BG).inner_margin(Margin::symmetric(0, 10))).show(ui, |ui| {
            ui.horizontal(|ui| {
                if button(ui, "Retour", BtnKind::Ghost).clicked() {
                    action = Some("back");
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let ok = compat && problems.is_empty() && self.picker.is_none() && tc_block.is_none();
                    let (pk, sk) = if self.intent == Intent::Create { (BtnKind::Primary, BtnKind::Secondary) } else { (BtnKind::Secondary, BtnKind::Primary) };
                    ui.add_enabled_ui(ok, |ui| {
                        // Dans un layout droite→gauche, le dernier ajouté est le plus à gauche.
                        let second_is_install = self.intent == Intent::Create;
                        let (a, b) = if second_is_install { (("Créer et installer", "ci", sk), ("Créer le paquet", "c", pk)) } else { (("Créer le paquet", "c", sk), ("Créer et installer", "ci", pk)) };
                        // a est placé à droite, b à sa gauche : on veut l'action principale à droite.
                        let (right, left) = if pk == BtnKind::Primary { (b, a) } else { (a, b) };
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
                        ui.label(RichText::new("Cette archive ne peut pas être convertie.").color(DANGER).size(13.0));
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
                    && let Some(st) = &self.tc.state {
                        let text = toolchains::install_plan(&m, st.has_flathub).iter().map(|a| toolchains::shell_line(a)).collect::<Vec<_>>().join("\n");
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

    fn ui_install_review(&mut self, ui: &mut Ui) {
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
            ui.label(muted("Par défaut, l'application est installée pour votre utilisateur uniquement.").size(13.0));
            if advanced {
                perm_row(ui, &mut self.opts.system_install, "Installer pour tous les utilisateurs", "Installation système (demande une authentification).", Some(Severity::Info));
            }
            perm_row(ui, &mut self.opts.replace_existing, "Remplacer une installation existante", "Réinstalle l'application ; vos données sont conservées.", None);
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

    fn ui_running(&mut self, ui: &mut Ui) {
        let show_log = &mut self.show_log;
        let Some(run) = &mut self.run else { return };
        let title = match run.action {
            Action::Created => "Création du paquet",
            Action::CreatedAndInstalled => "Création et installation",
            Action::Installed => "Installation",
        };
        ui.add_space(8.0);
        ui.label(RichText::new(format!("{title} en cours…")).heading().strong());
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
                .map(|(i, p)| (p.label(), if i < cur { StepState::Done } else if i == cur { StepState::Active } else { StepState::Pending }))
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
            card(ui, Some("Vérification terminée : confirmer l'installation ?"), |ui| {
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
            });
        }
        ui.add_space(8.0);
        ui.horizontal(|ui| {
            if run.cancelling {
                ui.spinner();
                ui.label(muted("Annulation en cours…"));
            } else if button(ui, "Annuler", BtnKind::Danger).clicked() {
                run.cancelling = true;
                run.handle.cancel.store(true, std::sync::atomic::Ordering::SeqCst);
                let _ = run.handle.confirm_tx.send(false);
            }
            if button(ui, if *show_log { "Masquer les détails" } else { "Afficher les détails" }, BtnKind::Ghost).clicked() {
                *show_log = !*show_log;
            }
        });
        if *show_log {
            log_view(ui, &run.log, 340.0);
        }
    }

    fn ui_done(&mut self, ui: &mut Ui) {
        let Some(d) = &self.done else { return };
        let mut action = None;
        ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            ui.add_space(12.0);
            match &d.result {
                Ok(o) => {
                    ui.horizontal(|ui| {
                        big_status_icon(ui, true);
                        ui.vertical(|ui| {
                            ui.add_space(6.0);
                            let t = if o.installed {
                                if d.action == Action::Installed { "Installation terminée" } else { "Paquet créé et installé" }
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
                            ui.label(RichText::new(format!("{} {}", o.name, o.version)).strong());
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
                        if o.installed && !o.app_id.is_empty() && button(ui, "Lancer l'application", BtnKind::Primary).clicked() {
                            action = Some("launch");
                        }
                        if o.bundle.is_some() && button(ui, "Ouvrir le dossier", BtnKind::Secondary).clicked() {
                            action = Some("folder");
                        }
                        if o.bundle.is_some() && !o.installed && d.action != Action::Installed && button(ui, "Installer maintenant", BtnKind::Secondary).clicked() {
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
                            ui.label(RichText::new(if f.cancelled { "Opération annulée" } else { "Échec de l'opération" }).heading().strong());
                            ui.add(egui::Label::new(muted(f.message.clone())).wrap());
                        });
                    });
                    if !d.plan.is_empty() {
                        ui.add_space(8.0);
                        card(ui, None, |ui| {
                            let cur = d.plan.iter().position(|p| *p == d.phase).unwrap_or(0);
                            let steps: Vec<(&str, StepState)> = d.plan.iter().enumerate().map(|(i, p)| (p.label(), if i < cur { StepState::Done } else if i == cur { StepState::Active } else { StepState::Pending })).collect();
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
                                        ui.add(egui::Label::new(RichText::new(format!("Piste : {fix}")).color(ACCENT).size(13.5)).wrap());
                                    });
                                }
                            }
                        });
                    }
                    ui.add_space(8.0);
                    ui.horizontal(|ui| {
                        if self.info.is_some() && button(ui, "Modifier et réessayer", BtnKind::Primary).clicked() {
                            action = Some("review");
                        }
                        if button(ui, "Accueil", BtnKind::Ghost).clicked() {
                            action = Some("home");
                        }
                    });
                }
            }
            ui.add_space(8.0);
            egui::CollapsingHeader::new("Journal détaillé").show(ui, |ui| log_view(ui, &d.log, 320.0));
        });
        let bundle = d.result.as_ref().ok().and_then(|o| o.bundle.clone());
        let app_id = d.result.as_ref().ok().map(|o| o.app_id.clone()).unwrap_or_default();
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

    fn ui_updates(&mut self, ui: &mut Ui) {
        ui.add_space(8.0);
        ui.label(RichText::new("Mises à jour").heading().strong());
        ui.label(muted("Mettez à jour une application ou tout d'un coup. Les commandes lancées sont affichées dans le journal."));
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
                if button(ui, format!("Tout mettre à jour ({n_all})").as_str(), BtnKind::Primary).clicked() {
                    action = Some(("all", 0));
                }
            });
            if button(ui, "Rechercher", BtnKind::Secondary).clicked() && !busy && self.up.rx.is_none() {
                action = Some(("reload", 0));
            }
            ui.checkbox(&mut self.up.show_runtimes, "Afficher les runtimes");
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
                    ui.add(egui::Label::new(RichText::new(single_line_trunc(l, 90)).monospace().color(MUTED).size(12.0)).truncate());
                }
                if button(ui, "Annuler", BtnKind::Danger).clicked() {
                    action = Some(("stop", 0));
                }
            });
            ui.add_space(6.0);
        }
        if self.up.rx.is_some() {
            ui.horizontal(|ui| {
                ui.spinner();
                ui.label(muted("Recherche des mises à jour…"));
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
                                badge(ui, if u.is_user() { "utilisateur" } else { &u.installation }, if u.is_user() { ACCENT } else { MUTED });
                                if u.runtime {
                                    badge(ui, "runtime", MUTED);
                                }
                            });
                            ui.label(muted(format!("{}//{}  ·  {}", u.id, u.branch, u.size)).size(12.5));
                        });
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            ui.add_enabled_ui(!busy, |ui| {
                                if button(ui, "Mettre à jour", BtnKind::Secondary).clicked() {
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
                        ui.label(RichText::new("Tout est à jour.").color(SUCCESS));
                    } else {
                        ui.label(muted(format!("Aucune application à mettre à jour ({hidden} runtime(s) disponible(s) : cochez « Afficher les runtimes »).")));
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
                toast(&self.ctx, "Annulation demandée…");
                if let Some(t) = &self.up.install {
                    t.cancel.store(true, std::sync::atomic::Ordering::SeqCst);
                }
            }
            _ => {}
        }
    }

    fn ui_manage(&mut self, ui: &mut Ui) {
        ui.add_space(8.0);
        ui.label(RichText::new("Gérer les Flatpak").heading().strong());
        ui.label(muted("Applications et composants installés sur cette machine. La désinstallation est définitive."));
        ui.add_space(10.0);
        if let Some(id) = crate::host::sandbox_blocked() {
            sandbox_banner(ui, &id);
            return;
        }
        let busy = self.mg.install.is_some();
        let own_id = crate::host::sandbox_app_id();
        let mut action: Option<(&'static str, usize)> = None;

        ui.horizontal_wrapped(|ui| {
            ui.add(egui::TextEdit::singleline(&mut self.mg.filter).hint_text("Rechercher…").desired_width(240.0));
            ui.checkbox(&mut self.mg.show_runtimes, "Afficher les runtimes et SDK");
            if button(ui, "Actualiser", BtnKind::Ghost).clicked() && !busy {
                action = Some(("reload", 0));
            }
            if button(ui, "Retirer les composants inutilisés…", BtnKind::Secondary).clicked() && !busy {
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
            Some((format!("Désinstaller {} ?", it.name), manager::uninstall_plan(it, self.mg.delete_data)))
        } else if self.mg.pending_unused {
            Some(("Retirer les runtimes qu'aucune application n'utilise ?".to_string(), manager::unused_plan()))
        } else {
            None
        };
        if let Some((title, cmds)) = plan {
            let offer_wipe = self.mg.pending.as_ref().is_some_and(|i| !i.runtime);
            let system = self.mg.pending.as_ref().is_some_and(|i| !i.is_user());
            card(ui, None, |ui| {
                ui.label(RichText::new(format!("⚠ {title}")).strong().color(WARN));
                ui.add(egui::Label::new(muted("Commande destructive : elle sera exécutée telle quelle.").size(13.0)).wrap());
                if system {
                    ui.add(egui::Label::new(muted("Installation système : une demande d'authentification peut s'afficher.").size(13.0)).wrap());
                }
                ui.add_space(4.0);
                for argv in &cmds {
                    ui.add(egui::Label::new(RichText::new(format!("$ {}", toolchains::shell_line(argv))).monospace().size(12.5).color(WARN)).wrap());
                }
                if offer_wipe {
                    ui.add_space(4.0);
                    ui.checkbox(&mut self.mg.delete_data, "Supprimer aussi les données de l'application (--delete-data)");
                }
                ui.add_space(6.0);
                ui.horizontal_wrapped(|ui| {
                    if button(ui, "Supprimer maintenant", BtnKind::Danger).clicked() {
                        action = Some(("confirm", 0));
                    }
                    if button(ui, "Annuler", BtnKind::Ghost).clicked() {
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
                    ui.add(egui::Label::new(RichText::new(single_line_trunc(l, 90)).monospace().color(MUTED).size(12.0)).truncate());
                }
                if button(ui, "Annuler", BtnKind::Danger).clicked() {
                    action = Some(("stop", 0));
                }
            });
            ui.add_space(6.0);
        }

        if !self.mg.loaded {
            ui.horizontal(|ui| {
                ui.spinner();
                ui.label(muted("Lecture des paquets installés…"));
            });
        }
        let q = self.mg.filter.trim().to_lowercase();
        ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            let mut shown = 0;
            for (i, it) in self.mg.items.iter().enumerate() {
                if (it.runtime && !self.mg.show_runtimes) || (!q.is_empty() && !it.name.to_lowercase().contains(&q) && !it.id.to_lowercase().contains(&q)) {
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
                                badge(ui, if it.is_user() { "utilisateur" } else { &it.installation }, if it.is_user() { ACCENT } else { MUTED });
                                if it.runtime {
                                    badge(ui, "runtime", MUTED);
                                }
                            });
                            ui.label(muted(format!("{}//{}  ·  {}", it.id, it.branch, it.size)).size(12.5));
                        });
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            let is_self = own_id.as_deref() == Some(it.id.as_str());
                            if is_self {
                                ui.label(muted("application en cours").size(12.5));
                            } else if button(ui, "Désinstaller…", BtnKind::Danger).clicked() && !busy {
                                action = Some(("ask", i));
                            }
                        });
                    });
                });
            }
            if self.mg.loaded && shown == 0 {
                card(ui, None, |ui| {
                    ui.label(muted("Aucun paquet à afficher."));
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
                toast(&self.ctx, "Annulation demandée…");
                if let Some(t) = &self.mg.install {
                    t.cancel.store(true, std::sync::atomic::Ordering::SeqCst);
                }
            }
            _ => {}
        }
    }

    fn ui_history(&mut self, ui: &mut Ui) {
        ui.add_space(8.0);
        ui.label(RichText::new("Historique").heading().strong());
        ui.label(muted("Les paquets créés et installés avec Flatpak Manager."));
        ui.add_space(10.0);
        let mut action: Option<(&str, usize)> = None;
        ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            if self.history.is_empty() {
                card(ui, None, |ui| {
                    ui.label(muted("Rien pour l'instant : vos opérations apparaîtront ici."));
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
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if button(ui, "Retirer de la liste", BtnKind::Ghost).on_hover_text("Ne supprime pas le fichier .flatpak").clicked() {
                                action = Some(("remove", i));
                            }
                        });
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

fn nav_item(ui: &mut Ui, label: &str, selected: bool) -> egui::Response {
    let (rect, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 40.0), Sense::click());
    let t = ui.ctx().animate_bool_with_time(resp.id, resp.hovered(), 0.12);
    let p = ui.painter();
    if selected {
        p.rect_filled(rect, CornerRadius::same(10), ACCENT.gamma_multiply(0.16));
        p.rect_filled(egui::Rect::from_min_size(rect.min + vec2(0.0, 9.0), vec2(3.0, 22.0)), CornerRadius::same(2), ACCENT);
    } else if t > 0.0 {
        p.rect_filled(rect, CornerRadius::same(10), Color32::from_white_alpha((14.0 * t) as u8));
    }
    p.text(rect.left_center() + vec2(16.0, 0.0), egui::Align2::LEFT_CENTER, label, egui::FontId::new(15.0, egui::FontFamily::Proportional), if selected { TEXT } else { MUTED });
    resp.on_hover_cursor(egui::CursorIcon::PointingHand)
}

/// Carte « Environnement de compilation » : outils détectés, composants Flatpak requis,
/// commande d'installation affichée avant exécution.
fn toolchain_card(ui: &mut Ui, system: BuildSystem, runtime: RuntimeKind, version: &str, tc: &Toolchain, action: &mut Option<&'static str>) {
    card(ui, Some("Environnement de compilation"), |ui| {
        ui.horizontal(|ui| {
            badge(ui, system.label(), ACCENT);
            ui.label(muted("détecté dans l'archive").size(13.0));
        });
        ui.add_space(4.0);
        let Some(state) = &tc.state else {
            ui.horizontal(|ui| {
                ui.spinner();
                ui.label(muted("Analyse de votre système…"));
            });
            return;
        };
        if let Some(id) = crate::host::sandbox_blocked() {
            sandbox_banner(ui, &id);
            if button(ui, "Revérifier", BtnKind::Secondary).clicked() {
                *action = Some("tc_rescan");
            }
            return;
        }
        if !state.available {
            dot_row(ui, DANGER, "Flatpak est introuvable", "Installez le paquet « flatpak » avec votre gestionnaire de paquets, puis cliquez sur « Revérifier ».");
            if button(ui, "Revérifier", BtnKind::Secondary).clicked() {
                *action = Some("tc_rescan");
            }
            return;
        }
        if !tc.tools.is_empty() {
            ui.label(RichText::new("Sur votre machine").strong());
            for t in &tc.tools {
                let (c, d) = match &t.version {
                    Some(v) => (SUCCESS, v.clone()),
                    None => (MUTED, "non trouvé (sans importance : la compilation utilise le SDK Flatpak)".to_string()),
                };
                dot_row(ui, c, &t.name, &d);
            }
            ui.add_space(6.0);
        }
        ui.label(RichText::new("Composants Flatpak requis").strong());
        let comps = toolchains::components(system, runtime, version, state);
        for c in &comps {
            let d = format!("{} — {}", c.reference(), if c.installed { "installé" } else { "manquant" });
            dot_row(ui, if c.installed { SUCCESS } else { DANGER }, &c.label, &d);
        }
        let miss = toolchains::missing(&comps);
        if miss.is_empty() {
            ui.add_space(4.0);
            ui.label(RichText::new("Tout est prêt pour compiler.").color(SUCCESS));
            cleanup_section(ui, tc, state, version, action);
            return;
        }
        ui.add_space(8.0);
        ui.label(RichText::new("Commande qui sera exécutée").strong());
        Frame::new().fill(INPUT).stroke(Stroke::new(1.0, BORDER)).corner_radius(CornerRadius::same(10)).inner_margin(Margin::same(10)).show(ui, |ui| {
            ui.set_width(ui.available_width());
            ScrollArea::horizontal().id_salt("tc_cmd").auto_shrink([false, true]).show(ui, |ui| {
                for argv in toolchains::install_plan(&miss, state.has_flathub) {
                    ui.add(egui::Label::new(RichText::new(format!("$ {}", toolchains::shell_line(&argv))).monospace().size(12.5).color(ACCENT)).extend());
                }
                ui.add_space(4.0);
            });
        });
        ui.add(egui::Label::new(muted("« -y » évite que Flatpak pose la question dans un terminal. Seul l'utilisateur courant est concerné (--user).").size(12.5)).wrap());
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
                ui.add(egui::Label::new(RichText::new(single_line_trunc(l, 70)).monospace().color(MUTED).size(12.0)).truncate());
            }
            ui.add_space(4.0);
            if button(ui, "Annuler", BtnKind::Danger).clicked() {
                *action = Some("tc_cancel");
            }
        } else {
            ui.horizontal_wrapped(|ui| {
                if button(ui, "Installer maintenant", BtnKind::Primary).clicked() {
                    *action = Some("tc_install");
                }
                if button(ui, "Copier la commande", BtnKind::Secondary).clicked() {
                    *action = Some("tc_copy");
                }
                if button(ui, "Revérifier", BtnKind::Ghost).clicked() {
                    *action = Some("tc_rescan");
                }
            });
            cleanup_section(ui, tc, state, version, action);
        }
    });
}

/// Ménage des anciennes versions de SDK : commande affichée, confirmation explicite avant suppression.
fn cleanup_section(ui: &mut Ui, tc: &Toolchain, state: &FlatpakState, version: &str, action: &mut Option<&'static str>) {
    if tc.install.is_some() {
        // Un ménage (ou une installation) est en cours : on affiche sa progression ici.
        ui.add_space(8.0);
        progress_bar(ui, tc.progress, 8.0);
        let start = tc.lines.len().saturating_sub(5);
        for l in &tc.lines[start..] {
            ui.add(egui::Label::new(RichText::new(single_line_trunc(l, 70)).monospace().color(MUTED).size(12.0)).truncate());
        }
        if button(ui, "Annuler", BtnKind::Danger).clicked() {
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
    ui.label(RichText::new("Ménage des anciens SDK").strong());
    ui.add(egui::Label::new(muted(format!("{} ancienne(s) version(s) installée(s) pour vous, devenues inutiles car une version plus récente est présente.", cands.len())).size(13.0)).wrap());
    if !tc.clean_ask {
        if button(ui, "Faire le ménage…", BtnKind::Secondary).clicked() {
            *action = Some("tc_clean_ask");
        }
        return;
    }
    ui.add_space(4.0);
    ui.add(egui::Label::new(RichText::new("⚠ Ces commandes suppriment des composants (réinstallables depuis Flathub) :").color(WARN).size(13.0)).wrap());
    Frame::new().fill(INPUT).stroke(Stroke::new(1.0, BORDER)).corner_radius(CornerRadius::same(10)).inner_margin(Margin::same(10)).show(ui, |ui| {
        ui.set_width(ui.available_width());
        ScrollArea::horizontal().id_salt("tc_clean_cmd").auto_shrink([false, true]).show(ui, |ui| {
            for argv in toolchains::cleanup_plan(&cands) {
                ui.add(egui::Label::new(RichText::new(format!("$ {}", toolchains::shell_line(&argv))).monospace().size(12.5).color(WARN)).extend());
            }
            ui.add_space(4.0);
        });
    });
    ui.add(egui::Label::new(muted("La 2ᵉ commande retire aussi tout runtime qu'aucune application installée n'utilise. Les applications installées ne sont pas touchées.").size(12.5)).wrap());
    ui.add_space(4.0);
    ui.horizontal_wrapped(|ui| {
        if button(ui, "Supprimer maintenant", BtnKind::Danger).clicked() {
            *action = Some("tc_clean");
        }
        if button(ui, "Annuler", BtnKind::Ghost).clicked() {
            *action = Some("tc_clean_no");
        }
    });
}

/// Application lancée dans un Flatpak sans la permission d'appeler `flatpak` sur l'hôte.
fn sandbox_banner(ui: &mut Ui, app_id: &str) {
    banner(ui, DANGER, "Permission manquante : l'application tourne dans un Flatpak", "Pour gérer les Flatpak de votre machine, elle doit être autorisée à lancer des commandes sur l'hôte. Copiez cette commande dans un terminal, puis relancez l'application :");
    let cmd = crate::host::override_command(app_id);
    ui.horizontal_wrapped(|ui| {
        ui.add(egui::Label::new(RichText::new(format!("$ {cmd}")).monospace().size(12.5).color(ACCENT)).wrap());
        if button(ui, "Copier", BtnKind::Ghost).clicked() {
            ui.ctx().copy_text(cmd.clone());
            toast(ui.ctx(), "Commande copiée dans le presse-papiers");
        }
    });
    ui.add_space(8.0);
}

fn banner(ui: &mut Ui, color: Color32, title: &str, text: &str) {
    Frame::new().fill(color.gamma_multiply(0.1)).stroke(Stroke::new(1.0, color.gamma_multiply(0.45))).corner_radius(CornerRadius::same(12)).inner_margin(Margin::same(14)).show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.label(RichText::new(title).strong().color(color));
        ui.add(egui::Label::new(muted(text).size(13.5)).wrap());
    });
    ui.add_space(8.0);
}

fn history_line(ui: &mut Ui, e: &Entry) {
    let when = chrono::DateTime::from_timestamp(e.timestamp, 0).map(|d| d.with_timezone(&chrono::Local).format("%d/%m/%Y %H:%M").to_string()).unwrap_or_default();
    ui.horizontal(|ui| {
        let (rect, _) = ui.allocate_exact_size(vec2(14.0, 20.0), Sense::hover());
        ui.painter().circle_filled(rect.center(), 4.5, if e.success { SUCCESS } else { DANGER });
        ui.label(RichText::new(format!("{} {}", e.name, e.version)).strong());
        badge(ui, if e.success { e.action.label() } else { "Échec" }, if e.success { ACCENT } else { DANGER });
        ui.label(muted(when).size(12.5));
    });
    if !e.app_id.is_empty() {
        ui.label(muted(e.app_id.clone()).size(12.5));
    }
}

fn log_view(ui: &mut Ui, log: &[(Level, String)], height: f32) {
    Frame::new().fill(INPUT).stroke(Stroke::new(1.0, BORDER)).corner_radius(CornerRadius::same(10)).inner_margin(Margin::same(10)).show(ui, |ui| {
        ui.set_width(ui.available_width());
        ScrollArea::vertical().max_height(height).stick_to_bottom(true).auto_shrink([false, true]).show(ui, |ui| {
            ui.spacing_mut().item_spacing.y = 2.0;
            for (l, s) in log {
                let c = match l {
                    Level::Info => TEXT,
                    Level::Cmd => ACCENT,
                    Level::Warn => WARN,
                    Level::Error => DANGER,
                    Level::Out => MUTED,
                };
                ui.add(egui::Label::new(RichText::new(s.as_str()).monospace().color(c)).wrap());
            }
        });
    });
}

impl eframe::App for App {
    fn logic(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        if let Some(m) = take_toast(ctx) {
            self.toast = Some((m, Instant::now()));
        }
        self.poll();
    }

    fn ui(&mut self, ui: &mut Ui, _frame: &mut eframe::Frame) {
        egui::Panel::left("sidebar").exact_size(232.0).resizable(false).frame(Frame::new().fill(SIDEBAR).inner_margin(Margin::same(16))).show_separator_line(false).show(ui, |ui| self.sidebar(ui));
        egui::CentralPanel::default().frame(Frame::new().fill(BG).inner_margin(Margin::symmetric(28, 18))).show(ui, |ui| match self.screen {
            Screen::Home => self.ui_home(ui),
            Screen::Analyzing => self.ui_analyzing(ui),
            Screen::Review => self.ui_review(ui),
            Screen::InstallReview => self.ui_install_review(ui),
            Screen::Running => self.ui_running(ui),
            Screen::Done => self.ui_done(ui),
            Screen::History => self.ui_history(ui),
            Screen::Manage => self.ui_manage(ui),
            Screen::Updates => self.ui_updates(ui),
        });
        // Indication de dépôt de fichier
        let hovering = ui.ctx().input(|i| !i.raw.hovered_files.is_empty());
        if hovering && !matches!(self.screen, Screen::Running | Screen::Analyzing) {
            let painter = ui.ctx().layer_painter(egui::LayerId::new(egui::Order::Foreground, egui::Id::NULL.with("dnd")));
            let rect = ui.ctx().content_rect();
            painter.rect_filled(rect, CornerRadius::ZERO, Color32::from_rgba_unmultiplied(8, 10, 16, 215));
            painter.rect_stroke(rect.shrink(18.0), CornerRadius::same(18), Stroke::new(2.0, ACCENT), egui::StrokeKind::Inside);
            painter.text(rect.center() - vec2(0.0, 12.0), egui::Align2::CENTER_CENTER, "Déposez un fichier ici", egui::FontId::new(26.0, egui::FontFamily::Proportional), TEXT);
            painter.text(rect.center() + vec2(0.0, 22.0), egui::Align2::CENTER_CENTER, "archive .tar.* pour créer, .flatpak pour installer", egui::FontId::new(15.0, egui::FontFamily::Proportional), MUTED);
        }
        self.draw_toast(ui);
        if matches!(self.screen, Screen::Running | Screen::Analyzing) {
            ui.ctx().request_repaint_after(Duration::from_millis(50));
        }
    }
}

impl App {
    /// Bandeau de confirmation temporaire, centré en bas.
    fn draw_toast(&mut self, ui: &mut Ui) {
        let Some((msg, t0)) = &self.toast else { return };
        let el = t0.elapsed().as_secs_f32();
        if el > 3.0 {
            self.toast = None;
            return;
        }
        let fade = if el > 2.5 { 1.0 - (el - 2.5) / 0.5 } else { (el / 0.12).min(1.0) };
        let ctx = ui.ctx().clone();
        egui::Area::new(egui::Id::NULL.with("fm_toast_area")).order(egui::Order::Tooltip).anchor(egui::Align2::CENTER_BOTTOM, vec2(0.0, -26.0)).interactable(false).show(&ctx, |ui| {
            ui.set_opacity(fade);
            Frame::new().fill(Color32::from_rgb(28, 38, 56)).stroke(Stroke::new(1.0, ACCENT)).corner_radius(CornerRadius::same(12)).inner_margin(Margin::symmetric(18, 11)).show(ui, |ui| {
                ui.label(RichText::new(format!("✓  {msg}")).color(TEXT).strong());
            });
        });
        ctx.request_repaint_after(Duration::from_millis(30));
    }
}
