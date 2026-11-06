//! Interface graphique (egui).

mod actions;
mod history_view;
mod home;
mod logo;
mod manage;
mod poll;
mod review;
mod run;
mod settings_view;
mod theme;
mod updates;
mod widgets;

use crate::analyze::{self, ArchiveInfo, Severity};
use crate::chooser;
use crate::history::{self, Action, Entry};
use crate::manager::{self, Installed};
use crate::manifest::{self, BuildConfig, CATEGORIES, FileAccess, IconSource, RuntimeKind};
use crate::pipeline::{
    self, Event, Failure, Handle, Job, Level, Options, Outcome, Phase, VerifyReport,
};
use crate::settings::Settings;
use crate::toolchains::{self, BuildSystem, FlatpakState, HostTool, ToolInstall, ToolMsg};
use egui::{
    Color32, CornerRadius, Frame, Margin, RichText, ScrollArea, Sense, Stroke, TextureHandle, Ui,
    vec2,
};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::mpsc::{Receiver, channel};
use std::time::{Duration, Instant};
use theme::*;
use widgets::*;

pub use logo::window_icon;

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
    Settings,
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
    logo: logo::Logo,
    fallback_icon: TextureHandle,
    screen: Screen,
    advanced: bool,
    opts: Options,
    settings: Settings,
    clear_history_ask: bool,
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
    (1..1000)
        .map(|n| dir.join(format!("{stem}-{n}{ext}")))
        .find(|p| !p.exists())
        .unwrap_or(first)
}

fn human(bytes: u64) -> String {
    const U: [&str; 4] = ["o", "Kio", "Mio", "Gio"];
    let mut v = bytes as f64;
    let mut i = 0;
    while v >= 1024.0 && i < 3 {
        v /= 1024.0;
        i += 1;
    }
    if i == 0 {
        format!("{bytes} o")
    } else {
        format!("{v:.1} {}", U[i])
    }
}

fn is_archive(p: &Path) -> bool {
    let n = p
        .file_name()
        .map(|s| s.to_string_lossy().to_lowercase())
        .unwrap_or_default();
    n.ends_with(".tar.gz")
        || n.ends_with(".tgz")
        || n.ends_with(".tar.xz")
        || n.ends_with(".txz")
        || n.ends_with(".tar.bz2")
        || n.ends_with(".tbz2")
        || n.ends_with(".tar.zst")
}

fn is_bundle(p: &Path) -> bool {
    p.extension()
        .is_some_and(|e| e.eq_ignore_ascii_case("flatpak"))
}

impl App {
    pub fn new(cc: &eframe::CreationContext<'_>, initial: Option<PathBuf>) -> Self {
        setup(&cc.egui_ctx);
        let ctx = cc.egui_ctx.clone();
        let settings = Settings::load();
        ctx.set_zoom_factor(settings.ui_scale);
        let g = manifest::generated_icon("local.flatpak.Placeholder");
        let fallback_icon = ctx.load_texture(
            "fallback_icon",
            egui::ColorImage::from_rgba_unmultiplied([256, 256], g.as_raw()),
            egui::TextureOptions::LINEAR,
        );

        // Détection de Flatpak et Flatpak Builder en arrière-plan.
        let (tx, rx) = channel();
        let c2 = ctx.clone();
        std::thread::spawn(move || {
            let ok = |p: &str, a: &[&str]| {
                crate::host::cmd(p)
                    .args(a)
                    .stdin(std::process::Stdio::null())
                    .stdout(std::process::Stdio::null())
                    .stderr(std::process::Stdio::null())
                    .status()
                    .map(|s| s.success())
                    .unwrap_or(false)
            };
            let fp = ok("flatpak", &["--version"]);
            let b = fp
                && (ok("flatpak-builder", &["--version"])
                    || ok("flatpak", &["info", "org.flatpak.Builder"]));
            let _ = tx.send((fp, b));
            c2.request_repaint();
        });

        let mut app = App {
            ctx,
            logo: logo::Logo::new(),
            fallback_icon,
            screen: Screen::Home,
            advanced: settings.advanced,
            opts: Options {
                system_install: settings.system_install,
                replace_existing: settings.replace_existing,
                confirm_before_install: settings.confirm_before_install,
            },
            settings,
            clear_history_ask: false,
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
}
impl eframe::App for App {
    fn logic(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        if let Some(m) = take_toast(ctx) {
            self.toast = Some((m, Instant::now()));
        }
        self.poll();
    }

    fn ui(&mut self, ui: &mut Ui, _frame: &mut eframe::Frame) {
        egui::Panel::left("sidebar")
            .exact_size(232.0)
            .resizable(false)
            .frame(Frame::new().fill(SIDEBAR).inner_margin(Margin::same(16)))
            .show_separator_line(false)
            .show(ui, |ui| self.sidebar(ui));
        egui::CentralPanel::default()
            .frame(
                Frame::new()
                    .fill(BG)
                    .inner_margin(Margin::symmetric(28, 18)),
            )
            .show(ui, |ui| match self.screen {
                Screen::Home => self.ui_home(ui),
                Screen::Analyzing => self.ui_analyzing(ui),
                Screen::Review => self.ui_review(ui),
                Screen::InstallReview => self.ui_install_review(ui),
                Screen::Running => self.ui_running(ui),
                Screen::Done => self.ui_done(ui),
                Screen::History => self.ui_history(ui),
                Screen::Manage => self.ui_manage(ui),
                Screen::Updates => self.ui_updates(ui),
                Screen::Settings => self.ui_settings(ui),
            });
        // Indication de dépôt de fichier
        let hovering = ui.ctx().input(|i| !i.raw.hovered_files.is_empty());
        if hovering && !matches!(self.screen, Screen::Running | Screen::Analyzing) {
            let painter = ui.ctx().layer_painter(egui::LayerId::new(
                egui::Order::Foreground,
                egui::Id::NULL.with("dnd"),
            ));
            let rect = ui.ctx().content_rect();
            painter.rect_filled(
                rect,
                CornerRadius::ZERO,
                Color32::from_rgba_unmultiplied(8, 10, 16, 215),
            );
            painter.rect_stroke(
                rect.shrink(18.0),
                CornerRadius::same(18),
                Stroke::new(2.0, ACCENT),
                egui::StrokeKind::Inside,
            );
            painter.text(
                rect.center() - vec2(0.0, 12.0),
                egui::Align2::CENTER_CENTER,
                "Déposez un fichier ici",
                egui::FontId::new(26.0, egui::FontFamily::Proportional),
                TEXT,
            );
            painter.text(
                rect.center() + vec2(0.0, 22.0),
                egui::Align2::CENTER_CENTER,
                "archive .tar.* pour créer, .flatpak pour installer",
                egui::FontId::new(15.0, egui::FontFamily::Proportional),
                MUTED,
            );
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
        let fade = if el > 2.5 {
            1.0 - (el - 2.5) / 0.5
        } else {
            (el / 0.12).min(1.0)
        };
        let ctx = ui.ctx().clone();
        egui::Area::new(egui::Id::NULL.with("fm_toast_area"))
            .order(egui::Order::Tooltip)
            .anchor(egui::Align2::CENTER_BOTTOM, vec2(0.0, -26.0))
            .interactable(false)
            .show(&ctx, |ui| {
                ui.set_opacity(fade);
                Frame::new()
                    .fill(Color32::from_rgb(28, 38, 56))
                    .stroke(Stroke::new(1.0, ACCENT))
                    .corner_radius(CornerRadius::same(12))
                    .inner_margin(Margin::symmetric(18, 11))
                    .show(ui, |ui| {
                        ui.label(RichText::new(format!("✓  {msg}")).color(TEXT).strong());
                    });
            });
        ctx.request_repaint_after(Duration::from_millis(30));
    }
}
