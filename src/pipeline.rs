//! Exécution des opérations : construction, export, vérification, installation.
//! Tout se passe dans un thread dédié ; l'interface reçoit des `Event`.

use crate::analyze::{ArchiveInfo, Severity, sha256_file};
use crate::diagnose::{Diagnosis, diagnose};
use crate::manifest::{BuildConfig, RuntimeKind, prepare_workdir};
use anyhow::{Result, anyhow, bail};
use std::io::Read;
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::mpsc::{Receiver, Sender, channel};
use std::sync::{Arc, Mutex};
use std::time::Duration;

pub const FLATHUB_URL: &str = "https://dl.flathub.org/repo/flathub.flatpakrepo";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Phase {
    Prepare,
    Runtime,
    Build,
    Export,
    Verify,
    Install,
}

impl Phase {
    pub fn label(&self) -> &'static str {
        match self {
            Phase::Prepare => tr!("Préparation"),
            Phase::Runtime => tr!("Runtime"),
            Phase::Build => tr!("Construction"),
            Phase::Export => tr!("Export"),
            Phase::Verify => tr!("Vérification"),
            Phase::Install => tr!("Installation"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Level {
    Info,
    Cmd,
    Warn,
    Error,
    Out,
}

#[derive(Debug, Clone)]
pub struct VerifyReport {
    pub items: Vec<(Severity, String)>,
    pub sha256: String,
}

#[derive(Debug, Clone)]
pub struct Outcome {
    pub bundle: Option<PathBuf>,
    pub installed: bool,
    pub app_id: String,
    pub name: String,
    pub version: String,
}

#[derive(Debug, Clone)]
pub struct Failure {
    pub message: String,
    pub diagnoses: Vec<Diagnosis>,
    pub cancelled: bool,
}

pub enum Event {
    Plan(Vec<Phase>),
    Phase(Phase),
    Progress(f32),
    Status(String),
    Log(Level, String),
    AwaitConfirm(VerifyReport),
    Verified(VerifyReport),
    Finished(Result<Outcome, Failure>),
}

#[derive(Debug, Clone, Copy, Default)]
pub struct Options {
    pub system_install: bool,
    pub replace_existing: bool,
    pub confirm_before_install: bool,
}

pub enum Job {
    Create {
        info: Box<ArchiveInfo>,
        cfg: Box<BuildConfig>,
        out: PathBuf,
    },
    Install {
        bundle: PathBuf,
    },
    CreateInstall {
        info: Box<ArchiveInfo>,
        cfg: Box<BuildConfig>,
        out: PathBuf,
    },
}

pub struct Handle {
    pub rx: Receiver<Event>,
    pub cancel: Arc<AtomicBool>,
    pub confirm_tx: Sender<bool>,
}

pub fn spawn(job: Job, opts: Options, notify: Arc<dyn Fn() + Send + Sync>) -> Handle {
    let (tx, rx) = channel();
    let (confirm_tx, confirm_rx) = channel();
    let cancel = Arc::new(AtomicBool::new(false));
    let runner = Runner {
        tx,
        cancel: cancel.clone(),
        notify,
        plan: Mutex::new(Vec::new()),
        cur: AtomicUsize::new(0),
        confirm_rx: Mutex::new(confirm_rx),
        tail: Mutex::new(Vec::new()),
        opts,
    };
    std::thread::spawn(move || {
        let res = runner.run(job);
        let ev = match res {
            Ok(o) => Ok(o),
            Err(e) => {
                let cancelled = runner.cancel.load(Ordering::SeqCst);
                let tail = runner
                    .tail
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .clone();
                let mut diagnoses = diagnose(&tail);
                let msg = if cancelled {
                    tr!("Opération annulée.").to_string()
                } else {
                    format!("{e:#}")
                };
                if !cancelled {
                    diagnoses.extend(diagnose(std::slice::from_ref(&msg)));
                }
                Err(Failure {
                    message: msg,
                    diagnoses,
                    cancelled,
                })
            }
        };
        runner.emit(Event::Finished(ev));
    });
    Handle {
        rx,
        cancel,
        confirm_tx,
    }
}

struct Runner {
    tx: Sender<Event>,
    cancel: Arc<AtomicBool>,
    notify: Arc<dyn Fn() + Send + Sync>,
    plan: Mutex<Vec<Phase>>,
    cur: AtomicUsize,
    confirm_rx: Mutex<Receiver<bool>>,
    tail: Mutex<Vec<String>>,
    opts: Options,
}

enum Builder {
    Native,
    Flatpak,
}

impl Runner {
    fn emit(&self, e: Event) {
        let _ = self.tx.send(e);
        (self.notify)();
    }
    fn log(&self, l: Level, s: impl Into<String>) {
        self.emit(Event::Log(l, s.into()));
    }
    fn check_cancel(&self) -> Result<()> {
        if self.cancel.load(Ordering::SeqCst) {
            bail!(tr!("Opération annulée."))
        } else {
            Ok(())
        }
    }
    fn set_plan(&self, p: Vec<Phase>) {
        *self.plan.lock().unwrap_or_else(|e| e.into_inner()) = p.clone();
        self.cur.store(0, Ordering::SeqCst);
        self.emit(Event::Plan(p));
    }
    fn set_phase(&self, ph: Phase) {
        let plan = self.plan.lock().unwrap_or_else(|e| e.into_inner()).clone();
        if let Some(i) = plan.iter().position(|p| *p == ph) {
            // Ne jamais revenir en arrière.
            let prev = self.cur.load(Ordering::SeqCst);
            if i >= prev {
                self.cur.store(i, Ordering::SeqCst);
                self.emit(Event::Phase(ph));
                self.emit(Event::Progress(i as f32 / plan.len() as f32));
            }
        }
    }
    fn frac(&self, f: f32) {
        let n = self
            .plan
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .len()
            .max(1) as f32;
        let i = self.cur.load(Ordering::SeqCst) as f32;
        self.emit(Event::Progress(((i + f.clamp(0.0, 1.0)) / n).min(1.0)));
    }

    /// Lance une commande, relaie sa sortie, gère l'annulation. Renvoie le code de sortie.
    fn run_cmd(&self, mut cmd: Command, cwd: Option<&Path>) -> Result<i32> {
        let shown = format!(
            "$ {} {}",
            cmd.get_program().to_string_lossy(),
            cmd.get_args()
                .map(|a| a.to_string_lossy().into_owned())
                .collect::<Vec<_>>()
                .join(" ")
        );
        self.log(Level::Cmd, shown);
        if let Some(c) = cwd {
            cmd.current_dir(c);
        }
        cmd.env("LC_ALL", "C")
            .env("LANG", "C")
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .process_group(0);
        let mut child = cmd.spawn().map_err(|e| {
            anyhow!(tr!(
                "impossible de lancer {programme} : {erreur}",
                programme = cmd.get_program().to_string_lossy(),
                erreur = e
            ))
        })?;
        let (ltx, lrx) = channel::<String>();
        let mut readers = Vec::new();
        if let Some(o) = child.stdout.take() {
            readers.push(pump(o, ltx.clone()));
        }
        if let Some(e) = child.stderr.take() {
            readers.push(pump(e, ltx.clone()));
        }
        drop(ltx);
        let pid = child.id();
        loop {
            match lrx.recv_timeout(Duration::from_millis(120)) {
                Ok(line) => self.handle_line(&line),
                Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {}
                Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => break,
            }
            if self.cancel.load(Ordering::SeqCst) {
                let _ = Command::new("kill")
                    .args(["-TERM", "--", &format!("-{pid}")])
                    .status();
                let _ = child.kill();
                break;
            }
        }
        for r in readers {
            let _ = r.join();
        }
        let status = child.wait()?;
        self.check_cancel()?;
        Ok(status.code().unwrap_or(-1))
    }

    fn handle_line(&self, line: &str) {
        let t = line.trim_end();
        if t.trim().is_empty() {
            return;
        }
        self.tail
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .push(t.to_string());
        {
            let mut tail = self.tail.lock().unwrap_or_else(|e| e.into_inner());
            if tail.len() > 400 {
                let n = tail.len() - 400;
                tail.drain(..n);
            }
        }
        let low = t.to_lowercase();
        // Lignes de progression : barre de statut, pas de journal.
        if let Some(p) = parse_percent(t) {
            self.emit(Event::Status(t.to_string()));
            self.frac(p);
            return;
        }
        // Détection de phases dans la sortie de flatpak-builder.
        if low.starts_with("downloading")
            || low.starts_with("installing org.")
            || low.starts_with("installing runtime")
        {
            self.set_phase(Phase::Runtime);
        } else if low.starts_with("building module")
            || low.starts_with("starting build")
            || low.starts_with("running:")
        {
            self.set_phase(Phase::Build);
        } else if low.starts_with("committing")
            || low.starts_with("exporting")
            || low.starts_with("pruning")
            || low.starts_with("cleaning up")
        {
            self.set_phase(Phase::Export);
        }
        let level = if low.starts_with("error") || low.contains("error:") {
            Level::Error
        } else if low.starts_with("warning") || low.contains("warning:") {
            Level::Warn
        } else {
            Level::Out
        };
        self.emit(Event::Status(t.to_string()));
        self.log(level, t.to_string());
    }

    fn simple(&self, prog: &str, args: &[&str]) -> bool {
        crate::host::cmd(prog)
            .args(args)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .map(|s| s.success())
            .unwrap_or(false)
    }

    fn scope_flag(&self) -> &'static str {
        if self.opts.system_install {
            "--system"
        } else {
            "--user"
        }
    }

    fn run(&self, job: Job) -> Result<Outcome> {
        match job {
            Job::Create { info, cfg, out } => {
                self.set_plan(vec![
                    Phase::Prepare,
                    Phase::Runtime,
                    Phase::Build,
                    Phase::Export,
                    Phase::Verify,
                ]);
                let (bundle, _) = self.create(&info, &cfg, &out)?;
                Ok(Outcome {
                    bundle: Some(bundle),
                    installed: false,
                    app_id: cfg.app_id.clone(),
                    name: cfg.name.clone(),
                    version: cfg.version.clone(),
                })
            }
            Job::CreateInstall { info, cfg, out } => {
                self.set_plan(vec![
                    Phase::Prepare,
                    Phase::Runtime,
                    Phase::Build,
                    Phase::Export,
                    Phase::Verify,
                    Phase::Install,
                ]);
                let (bundle, report) = self.create(&info, &cfg, &out)?;
                let mut go = true;
                if self.opts.confirm_before_install {
                    self.emit(Event::AwaitConfirm(report));
                    go = self
                        .confirm_rx
                        .lock()
                        .unwrap_or_else(|e| e.into_inner())
                        .recv()
                        .unwrap_or(false);
                    self.check_cancel()?;
                }
                if !go {
                    self.log(
                        Level::Info,
                        tr!("Installation non confirmée : le paquet a été créé mais pas installé."),
                    );
                    return Ok(Outcome {
                        bundle: Some(bundle),
                        installed: false,
                        app_id: cfg.app_id.clone(),
                        name: cfg.name.clone(),
                        version: cfg.version.clone(),
                    });
                }
                self.install_bundle(&bundle, Some(&cfg.app_id))?;
                Ok(Outcome {
                    bundle: Some(bundle),
                    installed: true,
                    app_id: cfg.app_id.clone(),
                    name: cfg.name.clone(),
                    version: cfg.version.clone(),
                })
            }
            Job::Install { bundle } => {
                self.set_plan(vec![Phase::Verify, Phase::Install]);
                self.set_phase(Phase::Verify);
                self.require_flatpak()?;
                let report = verify_bundle_only(&bundle)?;
                self.emit(Event::Verified(report.clone()));
                if report.items.iter().any(|(s, _)| *s == Severity::Blocker) {
                    bail!(tr!("Le fichier n'est pas un paquet Flatpak valide."));
                }
                self.install_bundle(&bundle, None)?;
                let name = bundle
                    .file_stem()
                    .map(|s| s.to_string_lossy().into_owned())
                    .unwrap_or_default();
                Ok(Outcome {
                    bundle: Some(bundle),
                    installed: true,
                    app_id: String::new(),
                    name,
                    version: String::new(),
                })
            }
        }
    }

    fn require_flatpak(&self) -> Result<()> {
        if !self.simple("flatpak", &["--version"]) {
            bail!(tr!(
                "Flatpak n'est pas installé sur ce système. Installez le paquet « flatpak » avec votre gestionnaire de paquets, puis relancez l'opération."
            ));
        }
        Ok(())
    }

    fn find_builder(&self) -> Result<Builder> {
        if self.simple("flatpak-builder", &["--version"]) {
            return Ok(Builder::Native);
        }
        if self.simple("flatpak", &["info", "org.flatpak.Builder"]) {
            return Ok(Builder::Flatpak);
        }
        self.log(Level::Warn, tr!("flatpak-builder est absent : installation de org.flatpak.Builder depuis Flathub (pour l'utilisateur courant)."));
        let mut c = crate::host::cmd("flatpak");
        c.args([
            "install",
            "--user",
            "-y",
            "--noninteractive",
            "flathub",
            "org.flatpak.Builder",
        ]);
        let code = self.run_cmd(c, None)?;
        if code != 0 {
            bail!(tr!(
                "Impossible d'installer Flatpak Builder (code {code}).",
                code = code
            ));
        }
        Ok(Builder::Flatpak)
    }

    fn resolve_runtime_version(&self, cfg: &BuildConfig) -> String {
        if cfg.runtime == RuntimeKind::Freedesktop
            || cfg.runtime_version != cfg.runtime.fallback_version()
        {
            return cfg.runtime_version.clone();
        }
        let out = crate::host::cmd("flatpak")
            .args([
                "remote-ls",
                "--user",
                "--runtime",
                "--columns=application,branch",
                "flathub",
            ])
            .env("LC_ALL", "C")
            .stdin(Stdio::null())
            .stderr(Stdio::null())
            .output();
        let mut best: Option<(Vec<u32>, String)> = None;
        if let Ok(o) = out {
            for line in String::from_utf8_lossy(&o.stdout).lines() {
                let mut it = line.split_whitespace();
                if it.next() != Some(cfg.runtime.platform()) {
                    continue;
                }
                if let Some(b) = it.next() {
                    let key: Option<Vec<u32>> = b.split('.').map(|x| x.parse().ok()).collect();
                    if let Some(k) = key
                        && best.as_ref().is_none_or(|(bk, _)| k > *bk)
                    {
                        best = Some((k, b.to_string()));
                    }
                }
            }
        }
        match best {
            Some((_, v)) => {
                self.log(
                    Level::Info,
                    tr!(
                        "Version de runtime résolue : {plateforme} {version}",
                        plateforme = cfg.runtime.platform(),
                        version = v
                    ),
                );
                v
            }
            None => {
                self.log(
                    Level::Warn,
                    tr!(
                        "Version de runtime non résolue, valeur par défaut : {version}",
                        version = cfg.runtime_version
                    ),
                );
                cfg.runtime_version.clone()
            }
        }
    }

    fn create(
        &self,
        info: &ArchiveInfo,
        cfg: &BuildConfig,
        out: &Path,
    ) -> Result<(PathBuf, VerifyReport)> {
        self.set_phase(Phase::Prepare);
        self.require_flatpak()?;
        if !info.is_compatible() {
            bail!(tr!("L'archive n'est pas convertible (voir l'analyse)."));
        }
        let mut cfg = cfg.clone();
        let builder = self.find_builder()?;
        self.log(Level::Info, tr!("Ajout du dépôt Flathub si nécessaire…"));
        let mut c = crate::host::cmd("flatpak");
        c.args([
            "remote-add",
            "--user",
            "--if-not-exists",
            "flathub",
            FLATHUB_URL,
        ]);
        let _ = self.run_cmd(c, None)?;
        cfg.runtime_version = self.resolve_runtime_version(&cfg);

        let cache = dirs::cache_dir()
            .ok_or_else(|| anyhow!(tr!("dossier de cache introuvable")))?
            .join("flatpak-manager")
            .join("build");
        let ts = chrono::Local::now().format("%Y%m%d-%H%M%S");
        let work = cache.join(format!("{}-{ts}", cfg.slug()));
        self.log(
            Level::Info,
            tr!("Dossier de travail : {chemin}", chemin = work.display()),
        );
        let result = self.create_in(&builder, info, &cfg, &work, out);
        // Nettoyage du dossier de travail (nous l'avons créé nous-mêmes).
        if let Err(e) = std::fs::remove_dir_all(&work) {
            self.log(
                Level::Warn,
                tr!(
                    "Nettoyage incomplet de {chemin} : {erreur}",
                    chemin = work.display(),
                    erreur = e
                ),
            );
        }
        result
    }

    fn create_in(
        &self,
        builder: &Builder,
        info: &ArchiveInfo,
        cfg: &BuildConfig,
        work: &Path,
        out: &Path,
    ) -> Result<(PathBuf, VerifyReport)> {
        let manifest = prepare_workdir(cfg, info, work)?;
        self.log(
            Level::Info,
            tr!("Manifeste généré : {chemin}", chemin = manifest.display()),
        );
        self.check_cancel()?;
        self.set_phase(Phase::Runtime);

        let build = |extra: &[&str]| -> Result<i32> {
            let mut c = match builder {
                Builder::Native => crate::host::cmd("flatpak-builder"),
                Builder::Flatpak => {
                    let mut c = crate::host::cmd("flatpak");
                    c.arg("run");
                    c.arg(format!("--filesystem={}", work.display()));
                    c.arg("org.flatpak.Builder");
                    c
                }
            };
            c.args(["--user", "--force-clean", "--install-deps-from=flathub"]);
            c.args(extra);
            c.arg("--repo=repo")
                .arg("builddir")
                .arg(manifest.file_name().unwrap_or_default());
            self.run_cmd(c, Some(work))
        };
        let mut code = build(&[])?;
        if code != 0 {
            let t = self
                .tail
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .join("\n")
                .to_lowercase();
            if t.contains("rofiles-fuse") || t.contains("fuse") {
                self.log(
                    Level::Warn,
                    tr!("FUSE indisponible : nouvelle tentative sans rofiles-fuse."),
                );
                code = build(&["--disable-rofiles-fuse"])?;
            }
        }
        if code != 0 {
            bail!(tr!(
                "La construction a échoué (flatpak-builder, code {code}).",
                code = code
            ));
        }

        self.set_phase(Phase::Export);
        let tmp_bundle = work.join("out.flatpak");
        let mut c = crate::host::cmd("flatpak");
        c.arg("build-bundle")
            .arg(format!("--runtime-repo={FLATHUB_URL}"))
            .arg("repo")
            .arg(&tmp_bundle)
            .arg(&cfg.app_id);
        let code = self.run_cmd(c, Some(work))?;
        if code != 0 {
            bail!(tr!(
                "La création du fichier .flatpak a échoué (code {code}).",
                code = code
            ));
        }

        self.set_phase(Phase::Verify);
        let report = verify_build(work, cfg, &tmp_bundle, self)?;
        self.emit(Event::Verified(report.clone()));
        if report.items.iter().any(|(s, _)| *s == Severity::Blocker) {
            bail!(tr!(
                "La vérification du paquet a échoué : il n'a pas été enregistré."
            ));
        }
        if let Some(dir) = out.parent() {
            std::fs::create_dir_all(dir)?;
        }
        std::fs::copy(&tmp_bundle, out).map_err(|e| {
            anyhow!(tr!(
                "écriture de {chemin} impossible : {erreur}",
                chemin = out.display(),
                erreur = e
            ))
        })?;
        self.log(
            Level::Info,
            tr!("Paquet enregistré : {chemin}", chemin = out.display()),
        );
        Ok((out.to_path_buf(), report))
    }

    fn install_bundle(&self, bundle: &Path, app_id: Option<&str>) -> Result<()> {
        self.set_phase(Phase::Install);
        let args = |reinstall: bool| {
            let mut c = crate::host::cmd("flatpak");
            c.args([
                "install",
                self.scope_flag(),
                "--bundle",
                "-y",
                "--noninteractive",
            ]);
            if reinstall {
                c.arg("--reinstall");
            }
            c.arg(bundle);
            c
        };
        let already =
            app_id.is_some_and(|id| self.simple("flatpak", &["info", self.scope_flag(), id]));
        if already && !self.opts.replace_existing {
            bail!(tr!(
                "L'application est déjà installée (already installed). Activez le remplacement dans les paramètres avancés pour la mettre à jour."
            ));
        }
        if already {
            self.log(Level::Warn, tr!("Application déjà installée : elle sera réinstallée (vos données dans ~/.var/app sont conservées)."));
        }
        let mut code = self.run_cmd(args(already), None)?;
        if code != 0 && self.opts.replace_existing && !already {
            let t = self
                .tail
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .join("\n")
                .to_lowercase();
            if t.contains("already installed") {
                self.log(
                    Level::Warn,
                    tr!("Déjà installée : nouvelle tentative en réinstallation (données conservées)."),
                );
                code = self.run_cmd(args(true), None)?;
            }
        }
        if code != 0 {
            bail!(tr!("L'installation a échoué (code {code}).", code = code));
        }
        if let Some(id) = app_id {
            if !self.simple("flatpak", &["info", self.scope_flag(), id]) {
                bail!(tr!(
                    "L'installation semble avoir réussi mais l'application est introuvable (flatpak info)."
                ));
            }
            self.log(
                Level::Info,
                tr!("Application installée et vérifiée : {id}", id = id),
            );
        }
        self.frac(1.0);
        Ok(())
    }
}

pub(crate) fn pump<R: Read + Send + 'static>(
    mut r: R,
    tx: Sender<String>,
) -> std::thread::JoinHandle<()> {
    std::thread::spawn(move || {
        let mut buf = [0u8; 4096];
        let mut cur: Vec<u8> = Vec::new();
        loop {
            match r.read(&mut buf) {
                Ok(0) | Err(_) => break,
                Ok(n) => {
                    for &b in &buf[..n] {
                        if b == b'\n' || b == b'\r' {
                            if !cur.is_empty() {
                                let _ = tx.send(String::from_utf8_lossy(&cur).into_owned());
                                cur.clear();
                            }
                        } else {
                            cur.push(b);
                        }
                    }
                }
            }
        }
        if !cur.is_empty() {
            let _ = tx.send(String::from_utf8_lossy(&cur).into_owned());
        }
    })
}

/// Extrait un pourcentage d'une ligne de progression de flatpak (« Installing… 42% »).
pub fn parse_percent(line: &str) -> Option<f32> {
    let l = line.to_lowercase();
    if !(l.contains("installing")
        || l.contains("updating")
        || l.contains("downloading")
        || l.contains("fetching")
        || l.contains("pulling"))
    {
        return None;
    }
    let idx = line.find('%')?;
    let digits: String = line[..idx]
        .chars()
        .rev()
        .take_while(|c| c.is_ascii_digit())
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect();
    digits.parse::<f32>().ok().map(|p| p / 100.0)
}

fn on_path(prog: &str) -> bool {
    std::env::var_os("PATH")
        .is_some_and(|p| std::env::split_paths(&p).any(|d| d.join(prog).is_file()))
}

/// Vrai si le fichier commence par l'en-tête ELF ou par « #! ».
fn is_elf_or_script(p: &Path) -> bool {
    use std::io::Read;
    let mut h = [0u8; 4];
    std::fs::File::open(p)
        .and_then(|mut f| f.read_exact(&mut h))
        .is_ok()
        && (h == [0x7f, b'E', b'L', b'F'] || h[..2] == *b"#!")
}

fn verify_build(work: &Path, cfg: &BuildConfig, bundle: &Path, r: &Runner) -> Result<VerifyReport> {
    use std::os::unix::fs::PermissionsExt;
    let mut items = Vec::new();
    let slug = cfg.slug();
    let files = work.join("builddir/files");
    if let Some(src) = &cfg.source {
        let bin = files.join("bin").join(&src.bin_name);
        match std::fs::metadata(&bin) {
            Ok(_) if !is_elf_or_script(&bin) => items.push((Severity::Blocker, tr!("« {nom} » existe dans /app/bin mais n'est ni un programme compilé ni un script : la compilation n'a pas produit ce qui était attendu.", nom = src.bin_name))),
            Ok(m) if m.permissions().mode() & 0o111 != 0 => items.push((Severity::Info, tr!("Programme compilé présent : /app/bin/{nom}", nom = src.bin_name))),
            _ => items.push((Severity::Blocker, tr!("Le programme « {nom} » n'a pas été installé dans /app/bin : la compilation a réussi mais ce nom est probablement incorrect. Corrigez le champ « Exécutable » en mode avancé.", nom = src.bin_name))),
        }
        let entry_path = match (&src.system, &src.entry) {
            (crate::toolchains::BuildSystem::Python, crate::toolchains::Entry::File(f)) => {
                Some(files.join("opt").join(&slug).join("src").join(f))
            }
            (
                crate::toolchains::BuildSystem::Node | crate::toolchains::BuildSystem::Java,
                crate::toolchains::Entry::File(f),
            ) => Some(files.join("opt").join(&slug).join(f)),
            _ => None,
        };
        if let Some(e) = entry_path
            && !e.exists()
            && e.extension().is_none_or(|x| {
                x != "js" && x != "mjs" && x != "cjs" || !e.with_extension("js").exists()
            })
        {
            items.push((Severity::Blocker, tr!("Le point d'entrée « {nom} » est introuvable dans le paquet : choisissez le bon fichier dans « Point d'entrée ».", nom = e.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default())));
        }
    } else {
        let wrapper = files.join("bin").join(&slug);
        match std::fs::metadata(&wrapper) {
            Ok(m) if m.permissions().mode() & 0o111 != 0 => items.push((
                Severity::Info,
                tr!("Script de lancement présent : /app/bin/{slug}", slug = slug),
            )),
            _ => items.push((
                Severity::Blocker,
                tr!(
                    "Script de lancement manquant ou non exécutable : /app/bin/{slug}",
                    slug = slug
                ),
            )),
        }
        let exec = files.join("opt").join(&slug).join(&cfg.exec_rel);
        if exec.is_file() {
            items.push((
                Severity::Info,
                tr!("Exécutable présent : {chemin}", chemin = cfg.exec_rel),
            ));
        } else {
            items.push((
                Severity::Blocker,
                tr!(
                    "Exécutable introuvable dans le paquet : {chemin}",
                    chemin = cfg.exec_rel
                ),
            ));
        }
    }
    let desktop = work
        .join("builddir/export/share/applications")
        .join(format!("{}.desktop", cfg.app_id));
    if desktop.is_file() {
        items.push((Severity::Info, tr!("Entrée de menu exportée").into()));
        if on_path("desktop-file-validate") {
            let ok = r.simple("desktop-file-validate", &[&desktop.to_string_lossy()]);
            items.push((
                if ok {
                    Severity::Info
                } else {
                    Severity::Warning
                },
                if ok {
                    tr!("Entrée de menu valide (desktop-file-validate)").into()
                } else {
                    tr!("desktop-file-validate signale des avertissements").into()
                },
            ));
        }
    } else {
        items.push((Severity::Blocker, tr!("Entrée de menu non exportée").into()));
    }
    let icons = work.join("builddir/export/share/icons");
    let has_icon = walk_has_prefix(&icons, &cfg.app_id, 5);
    items.push((
        if has_icon {
            Severity::Info
        } else {
            Severity::Warning
        },
        if has_icon {
            tr!("Icône exportée").into()
        } else {
            tr!("Icône non exportée").into()
        },
    ));
    let size = std::fs::metadata(bundle).map(|m| m.len()).unwrap_or(0);
    if size == 0 {
        items.push((
            Severity::Blocker,
            tr!("Le fichier .flatpak est vide").into(),
        ));
    } else {
        items.push((
            Severity::Info,
            tr!(
                "Fichier .flatpak créé ({taille} Mio)",
                taille = (size >> 20).max(1)
            ),
        ));
    }
    let sha256 = sha256_file(bundle).unwrap_or_default();
    Ok(VerifyReport { items, sha256 })
}

fn walk_has_prefix(dir: &Path, prefix: &str, depth: u32) -> bool {
    if depth == 0 {
        return false;
    }
    let Ok(rd) = std::fs::read_dir(dir) else {
        return false;
    };
    for e in rd.flatten() {
        let p = e.path();
        if p.is_dir() {
            if walk_has_prefix(&p, prefix, depth - 1) {
                return true;
            }
        } else if e.file_name().to_string_lossy().starts_with(prefix) {
            return true;
        }
    }
    false
}

pub fn verify_bundle_only(bundle: &Path) -> Result<VerifyReport> {
    let mut items = Vec::new();
    let meta = std::fs::metadata(bundle).map_err(|e| {
        anyhow!(tr!(
            "lecture de {chemin} : {erreur}",
            chemin = bundle.display(),
            erreur = e
        ))
    })?;
    let is_ext = bundle
        .extension()
        .is_some_and(|e| e.eq_ignore_ascii_case("flatpak"));
    items.push((
        if is_ext {
            Severity::Info
        } else {
            Severity::Warning
        },
        if is_ext {
            tr!("Extension .flatpak").into()
        } else {
            tr!("L'extension n'est pas .flatpak").into()
        },
    ));
    if meta.len() == 0 {
        items.push((Severity::Blocker, tr!("Le fichier est vide").into()));
    } else {
        items.push((
            Severity::Info,
            tr!("Taille : {taille} Mio", taille = (meta.len() >> 20).max(1)),
        ));
    }
    // Un bundle OSTree commence par l'en-tête GVariant ; on vérifie la présence de la chaîne « flatpak » au début.
    let mut head = vec![0u8; 4096];
    let n = std::fs::File::open(bundle)?.read(&mut head)?;
    head.truncate(n);
    let looks = head.windows(7).any(|w| w == b"flatpak") || head.windows(5).any(|w| w == b"ostree");
    items.push((
        if looks {
            Severity::Info
        } else {
            Severity::Warning
        },
        if looks {
            tr!("En-tête de paquet Flatpak reconnu").into()
        } else {
            tr!("En-tête non reconnu : Flatpak validera le fichier à l'installation").into()
        },
    ));
    Ok(VerifyReport {
        items,
        sha256: sha256_file(bundle)?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn percent() {
        assert_eq!(parse_percent("Installing… ████ 42% 3.1 MB/s"), Some(0.42));
        assert_eq!(parse_percent("Installing 7%"), Some(0.07));
        assert_eq!(parse_percent("Building module foo"), None);
    }
}
