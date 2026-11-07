//! Verdict : type d'application, exécutable principal, bibliothèques, et les constats qui en découlent.

use super::io::read_member;
use super::model::*;
use super::scan::Scan;
use super::sniff::{is_gui_lib, is_runtime_lib};
use crate::toolchains::{self, SourceProject};
use std::collections::HashSet;
use std::path::Path;

/// Premières indications, avant l'analyse de l'exécutable.
pub(super) fn initial_hints(scan: &Scan, source: &Option<SourceProject>) -> Hints {
    let (has_asar, has_chrome_sandbox, qt_bundled, gtk_bundled) = (
        scan.has_asar,
        scan.has_chrome_sandbox,
        scan.qt_bundled,
        scan.gtk_bundled,
    );
    let lib_names = &scan.lib_names;
    Hints {
        electron: has_asar || (has_chrome_sandbox && lib_names.contains("libffmpeg.so")),
        qt: qt_bundled || source.as_ref().is_some_and(|s| s.qt),
        gtk: gtk_bundled || source.as_ref().is_some_and(|s| s.gtk),
        gui: source.as_ref().is_some_and(|s| s.gui),
        devices: source.as_ref().is_some_and(|s| s.devices),
        ..Default::default()
    }
}

/// Type d'application d'après l'exécutable principal et le contenu de l'archive.
pub(super) fn classify(
    main_kind: Option<&ExecKind>,
    scan: &Scan,
    has_source: bool,
    has_unsupported: bool,
) -> AppKind {
    let (has_appimage, has_jar, has_makefile) =
        (scan.has_appimage, scan.has_jar, scan.has_makefile);
    match main_kind {
        _ if has_source => AppKind::Source,
        Some(ExecKind::Elf { .. }) => AppKind::Native,
        Some(ExecKind::Script { .. }) => AppKind::Script,
        None if has_appimage => AppKind::AppImage,
        None if has_jar => AppKind::Java,
        None if has_makefile || has_unsupported => AppKind::Source,
        None => AppKind::Unknown,
    }
}

/// Constats propres au type d'application (source à compiler, AppImage, JAR sans classe principale…).
pub(super) fn report_kind(
    findings: &mut Vec<Finding>,
    kind: AppKind,
    source: &Option<SourceProject>,
    unsupported_source: Option<&'static str>,
) {
    match kind {
        AppKind::Source => match (&source, unsupported_source) {
            (Some(p), _) => {
                let title = if p.system.compiles() {
                    tr!(
                        "Code source à compiler : {system}",
                        system = p.system.label()
                    )
                } else {
                    tr!("Application {system}", system = p.system.label())
                };
                push(findings, Severity::Info, &title, p.system.intro());
                if let Some(why) = &p.blocker {
                    push(
                        findings,
                        Severity::Blocker,
                        tr!("Aucun programme à compiler"),
                        why.clone(),
                    );
                } else if !p.bin_certain {
                    push(
                        findings,
                        Severity::Warning,
                        tr!("Nom du binaire à confirmer"),
                        tr!(
                            "Le programme installé s'appellerait « {name} », mais ce nom est deviné. Vous devrez le confirmer (ou le corriger) avant de lancer la compilation.",
                            name = p.bin_name
                        ),
                    );
                }
                for n in &p.notes {
                    push(findings, Severity::Info, tr!("Compilation"), n.clone());
                }
            }
            (None, Some(lang)) => push(
                findings,
                Severity::Blocker,
                &tr!("Projet {lang} non pris en charge", lang = lang),
                tr!(
                    "Cette archive contient un projet {lang} ; seuls {supported} sont pris en charge pour l'instant.",
                    lang = lang,
                    supported = toolchains::supported_langs()
                ),
            ),
            (None, None) => push(
                findings,
                Severity::Blocker,
                tr!("Archive de sources"),
                tr!(
                    "Cette archive contient du code source mais aucun système de build reconnu (Cargo, CMake, Meson, Autotools, Makefile, Go, Python, Node.js, JAR)."
                ),
            ),
        },
        AppKind::AppImage => push(
            findings,
            Severity::Blocker,
            tr!("AppImage détecté"),
            tr!(
                "L'archive contient un AppImage. Ce format est déjà autonome ; il ne peut pas être converti directement."
            ),
        ),
        AppKind::Java => push(
            findings,
            Severity::Blocker,
            tr!("Application Java"),
            tr!(
                "Les fichiers .jar de cette archive n'ont pas de classe principale (Main-Class) : ce sont des bibliothèques, pas un programme qu'on peut lancer."
            ),
        ),
        AppKind::Unknown => push(
            findings,
            Severity::Blocker,
            tr!("Aucun exécutable trouvé"),
            tr!("Impossible de repérer un programme exécutable dans cette archive."),
        ),
        _ => {}
    }
}

/// Une archive qui ne contient que des scripts d'installation n'est pas une application.
pub(super) fn report_install_script(
    findings: &mut Vec<Finding>,
    kind: AppKind,
    desktop: &Option<DesktopInfo>,
    main_exec: Option<usize>,
    executables: &[ExecCandidate],
) {
    if kind == AppKind::Script
        && desktop.is_none()
        && let Some(m) = main_exec.and_then(|i| executables.get(i))
    {
        let f = m.rel.rsplit('/').next().unwrap_or("").to_lowercase();
        if [
            "install",
            "setup",
            "uninstall",
            "configure",
            "bootstrap",
            "autogen",
            "build",
            "update",
            "make",
        ]
        .iter()
        .any(|w| f.contains(w))
        {
            push(
                findings,
                Severity::Blocker,
                tr!("Archive non prévue pour être convertie"),
                tr!(
                    "Le seul programme trouvé est « {name} », qui ressemble à un script d'installation ou de compilation, pas à une application.",
                    name = m.rel
                ),
            );
        }
    }
}

/// Analyse de l'exécutable principal. Renvoie ses bibliothèques déclarées (ELF) et
/// si des bibliothèques graphiques y sont chargées à l'exécution.
pub(super) fn inspect_main_exec(
    path: &Path,
    findings: &mut Vec<Finding>,
    kind: AppKind,
    main_exec: Option<usize>,
    executables: &[ExecCandidate],
) -> (Vec<String>, bool) {
    let mut needed_libs: Vec<String> = Vec::new();
    let mut dlopen_gui = false;
    if let Some(m) = main_exec.map(|i| executables[i].clone()) {
        push(
            findings,
            Severity::Info,
            tr!("Exécutable principal"),
            tr!("{file} ({kind})", file = m.rel, kind = kind.label()),
        );
        if executables.len() > 1 {
            let others = executables
                .iter()
                .skip(1)
                .take(3)
                .map(|e| e.rel.clone())
                .collect::<Vec<_>>()
                .join(", ");
            push(
                findings,
                Severity::Info,
                &tr!("{n} autres exécutables détectés", n = executables.len() - 1),
                tr!(
                    "Au besoin, choisissez-en un autre en mode avancé (ex. : {others}).",
                    others = others
                ),
            );
        }
        match &m.kind {
            ExecKind::Elf { arch, bits } => {
                if !arch.matches_host() {
                    push(
                        findings,
                        Severity::Blocker,
                        tr!("Architecture incompatible"),
                        tr!(
                            "Le binaire est compilé pour {arch} mais cette machine est en {host}.",
                            arch = arch.label(),
                            host = std::env::consts::ARCH
                        ),
                    );
                }
                if *bits == 32 {
                    push(
                        findings,
                        Severity::Blocker,
                        tr!("Binaire 32 bits"),
                        tr!(
                            "Les binaires 32 bits nécessitent l'extension de compatibilité i386 ; non prise en charge."
                        ),
                    );
                }
                if m.size <= MAX_ELF_PARSE {
                    match read_member(path, &m.raw, MAX_ELF_PARSE) {
                        Ok(data) => {
                            // Bibliothèques graphiques chargées à l'exécution (dlopen), invisibles dans les dépendances ELF
                            // (winit, SDL, GLFW… sous Wayland ou X11).
                            dlopen_gui = [
                                &b"libwayland-client.so"[..],
                                b"libxkbcommon.so",
                                b"libX11.so",
                                b"libxcb.so",
                                b"libvulkan.so",
                                b"libEGL.so",
                            ]
                            .iter()
                            .any(|pat| data.windows(pat.len()).any(|w| w == *pat));
                            if let Ok(elf) = goblin::elf::Elf::parse(&data) {
                                needed_libs = elf.libraries.iter().map(|s| s.to_string()).collect();
                            } else {
                                push(
                                    findings,
                                    Severity::Warning,
                                    tr!("Dépendances non analysées"),
                                    tr!("L'en-tête ELF n'a pas pu être lu entièrement."),
                                );
                            }
                        }
                        Err(e) => push(
                            findings,
                            Severity::Warning,
                            tr!("Dépendances non analysées"),
                            e.to_string(),
                        ),
                    }
                }
            }
            ExecKind::Script { interpreter } => {
                let base = Path::new(interpreter)
                    .file_name()
                    .map(|s| s.to_string_lossy().into_owned())
                    .unwrap_or_default();
                if !matches!(base.as_str(), "sh" | "bash" | "dash") {
                    push(
                        findings,
                        Severity::Warning,
                        tr!("Interpréteur à vérifier"),
                        tr!(
                            "Le script utilise « {interpreter} ». Cet interpréteur doit exister dans le runtime ou être embarqué dans l'archive.",
                            interpreter = interpreter
                        ),
                    );
                }
            }
        }
    }
    (needed_libs, dlopen_gui)
}

/// Classe les bibliothèques, complète les indications et ajoute les constats finaux
/// (bibliothèques manquantes, Electron, graphique ou ligne de commande).
pub(super) fn classify_libs(
    findings: &mut Vec<Finding>,
    kind: AppKind,
    hints: &mut Hints,
    lib_names: &HashSet<String>,
    needed_libs: Vec<String>,
    dlopen_gui: bool,
) {
    let mut missing_libs: Vec<String> = Vec::new();
    for lib in &needed_libs {
        if lib_names.contains(lib) || is_runtime_lib(lib) {
            if lib.starts_with("libssl") || lib.starts_with("libcurl") {
                hints.network_likely = true;
            }
            if is_gui_lib(lib) {
                hints.gui = true;
            }
            continue;
        }
        if lib.starts_with("libgtk-") || lib.starts_with("libgdk-") || lib.starts_with("libadwaita")
        {
            hints.gtk = true;
            hints.gui = true;
        } else if lib.starts_with("libQt5") || lib.starts_with("libQt6") {
            hints.qt = true;
            hints.gui = true;
        } else {
            missing_libs.push(lib.clone());
        }
    }
    if dlopen_gui
        || lib_names.iter().any(|l| is_gui_lib(l))
        || hints.electron
        || hints.qt
        || hints.gtk
    {
        hints.gui = true;
    }
    if hints.electron {
        hints.network_likely = true;
    }
    if !missing_libs.is_empty() {
        push(
            findings,
            Severity::Warning,
            tr!("Bibliothèques possiblement manquantes"),
            tr!(
                "Non embarquées et absentes du runtime standard (analyse heuristique) : {libs}.",
                libs = missing_libs.join(", ")
            ),
        );
    }
    if hints.electron {
        push(
            findings,
            Severity::Warning,
            tr!("Application Electron détectée"),
            tr!(
                "Le bac à sable Chromium ne fonctionne pas sans composant supplémentaire ; l'application sera lancée avec --no-sandbox (le bac à sable Flatpak reste actif)."
            ),
        );
    }
    if matches!(kind, AppKind::Native | AppKind::Script) {
        if hints.gui {
            push(
                findings,
                Severity::Info,
                tr!("Application graphique"),
                tr!("Accès à l'affichage et à l'accélération graphique proposés par défaut."),
            );
        } else {
            push(
                findings,
                Severity::Info,
                tr!("Application en ligne de commande probable"),
                tr!(
                    "Aucune bibliothèque graphique détectée : l'entrée de menu s'ouvrira dans un terminal."
                ),
            );
        }
    }
}
