//! Génération du manifeste Flatpak et des fichiers annexes (script de
//! lancement, entrée de menu, icône).

use crate::analyze::{ArchiveInfo, Severity, read_member};
use crate::toolchains::{BuildSystem, Entry, extension_dir};
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::path::{Path, PathBuf};

pub const FDO_VERSION: &str = "25.08";
/// Valeurs de secours si la résolution automatique échoue.
pub const GNOME_FALLBACK: &str = "50";
pub const KDE_FALLBACK: &str = "6.10";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RuntimeKind {
    Freedesktop,
    Gnome,
    Kde,
}

impl RuntimeKind {
    pub const ALL: [RuntimeKind; 3] = [
        RuntimeKind::Freedesktop,
        RuntimeKind::Gnome,
        RuntimeKind::Kde,
    ];
    pub fn platform(&self) -> &'static str {
        match self {
            RuntimeKind::Freedesktop => "org.freedesktop.Platform",
            RuntimeKind::Gnome => "org.gnome.Platform",
            RuntimeKind::Kde => "org.kde.Platform",
        }
    }
    pub fn sdk(&self) -> &'static str {
        match self {
            RuntimeKind::Freedesktop => "org.freedesktop.Sdk",
            RuntimeKind::Gnome => "org.gnome.Sdk",
            RuntimeKind::Kde => "org.kde.Sdk",
        }
    }
    pub fn fallback_version(&self) -> &'static str {
        match self {
            RuntimeKind::Freedesktop => FDO_VERSION,
            RuntimeKind::Gnome => GNOME_FALLBACK,
            RuntimeKind::Kde => KDE_FALLBACK,
        }
    }
    pub fn label(&self) -> &'static str {
        match self {
            RuntimeKind::Freedesktop => tr!("Freedesktop (défaut)"),
            RuntimeKind::Gnome => "GNOME (GTK)",
            RuntimeKind::Kde => "KDE (Qt)",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum FileAccess {
    None,
    Downloads,
    DocumentsAndDownloads,
    Home,
}

impl FileAccess {
    pub const ALL: [FileAccess; 4] = [
        FileAccess::None,
        FileAccess::Downloads,
        FileAccess::DocumentsAndDownloads,
        FileAccess::Home,
    ];
    pub fn label(&self) -> &'static str {
        match self {
            FileAccess::None => tr!("Aucun (via les sélecteurs de fichiers uniquement)"),
            FileAccess::Downloads => tr!("Téléchargements"),
            FileAccess::DocumentsAndDownloads => tr!("Documents et Téléchargements"),
            FileAccess::Home => tr!("Tout le dossier personnel"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Permissions {
    pub display: bool,
    pub gpu: bool,
    pub audio: bool,
    pub network: bool,
    pub files: FileAccess,
    pub removable_media: bool,
    pub all_devices: bool,
    pub notifications: bool,
    pub tray: bool,
    pub printing: bool,
}

impl Permissions {
    pub fn finish_args(&self) -> Vec<String> {
        let mut a: Vec<String> = Vec::new();
        if self.display {
            a.extend(
                ["--socket=wayland", "--socket=fallback-x11", "--share=ipc"].map(String::from),
            );
        }
        if self.gpu {
            a.push("--device=dri".into());
        }
        if self.audio {
            a.push("--socket=pulseaudio".into());
        }
        if self.network {
            a.push("--share=network".into());
        }
        match self.files {
            FileAccess::None => {}
            FileAccess::Downloads => a.push("--filesystem=xdg-download".into()),
            FileAccess::DocumentsAndDownloads => {
                a.push("--filesystem=xdg-documents".into());
                a.push("--filesystem=xdg-download".into());
            }
            FileAccess::Home => a.push("--filesystem=home".into()),
        }
        if self.removable_media {
            a.push("--filesystem=/run/media".into());
            a.push("--filesystem=/media".into());
        }
        if self.all_devices {
            a.push("--device=all".into());
        }
        if self.notifications {
            a.push("--talk-name=org.freedesktop.Notifications".into());
        }
        if self.tray {
            a.push("--talk-name=org.kde.StatusNotifierWatcher".into());
        }
        if self.printing {
            a.push("--socket=cups".into());
        }
        a
    }

    /// Avertissements de sécurité sur la configuration choisie.
    pub fn risks(&self) -> Vec<(Severity, String)> {
        let mut r = Vec::new();
        if self.files == FileAccess::Home {
            r.push((Severity::Warning, tr!("Accès à tout le dossier personnel (y compris ~/.ssh et ~/.gnupg). Préférez un accès restreint.").into()));
        }
        if self.all_devices {
            r.push((
                Severity::Warning,
                tr!("Accès à tous les périphériques : affaiblit fortement le bac à sable.").into(),
            ));
        }
        if self.network
            && matches!(
                self.files,
                FileAccess::Home | FileAccess::DocumentsAndDownloads
            )
        {
            r.push((Severity::Info, tr!("Réseau et accès aux fichiers combinés : l'application pourrait envoyer vos fichiers sur Internet.").into()));
        }
        if self.removable_media {
            r.push((
                Severity::Info,
                tr!("Accès aux clés USB et disques externes.").into(),
            ));
        }
        r
    }
}

#[derive(Debug, Clone)]
pub enum IconSource {
    /// Icône trouvée dans l'archive (chemin brut).
    FromArchive { raw: String, is_svg: bool },
    /// Fichier choisi par l'utilisateur.
    File(PathBuf),
    /// Icône générée automatiquement.
    Generated,
}

/// Paramètres d'une compilation depuis des sources.
#[derive(Debug, Clone)]
pub struct SourceBuild {
    pub system: BuildSystem,
    /// Nom du programme installé dans /app/bin.
    pub bin_name: String,
    pub bin_certain: bool,
    /// Extension du SDK retenue (renseignée au lancement, selon ce qui est installé ou proposé).
    pub extension: Option<String>,
    pub entry: Entry,
    pub alternatives: Vec<String>,
    pub installable: bool,
    pub requirements: bool,
    pub build_script: bool,
}

#[derive(Debug, Clone)]
pub struct BuildConfig {
    pub name: String,
    pub app_id: String,
    pub version: String,
    pub summary: String,
    pub category: String,
    pub exec_rel: String,
    pub icon: IconSource,
    pub runtime: RuntimeKind,
    pub runtime_version: String,
    pub perms: Permissions,
    pub terminal: bool,
    pub electron: bool,
    pub lib_dirs: Vec<String>,
    pub desktop_exec_code: String,
    pub source: Option<SourceBuild>,
}

pub const CATEGORIES: [(&str, &str); 9] = [
    ("Utility", "Utilitaire"),
    ("Development", "Développement"),
    ("Graphics", "Graphisme"),
    ("AudioVideo", "Audio et vidéo"),
    ("Game", "Jeu"),
    ("Network", "Internet"),
    ("Office", "Bureautique"),
    ("System", "Système"),
    ("Education", "Éducation"),
];

/// Libellé affiché d'une catégorie freedesktop (l'identifiant, lui, reste tel quel).
/// Les libellés français de `CATEGORIES` ne servent plus que de repli.
#[allow(dead_code)]
pub fn category_label(id: &str) -> &'static str {
    match id {
        "Utility" => tr!("Utilitaire"),
        "Development" => tr!("Développement"),
        "Graphics" => tr!("Graphisme"),
        "AudioVideo" => tr!("Audio et vidéo"),
        "Game" => tr!("Jeu"),
        "Network" => tr!("Internet"),
        "Office" => tr!("Bureautique"),
        "System" => tr!("Système"),
        "Education" => tr!("Éducation"),
        _ => tr!("Utilitaire"),
    }
}

pub fn slugify(name: &str) -> String {
    let s: String = name
        .to_lowercase()
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect();
    let s = s
        .split('-')
        .filter(|p| !p.is_empty())
        .collect::<Vec<_>>()
        .join("-");
    if s.is_empty() { "app".into() } else { s }
}

pub fn default_app_id(name: &str) -> String {
    let mut last: String = name.chars().filter(|c| c.is_ascii_alphanumeric()).collect();
    if last.is_empty() || last.chars().next().unwrap().is_ascii_digit() {
        last = format!("App{last}");
    }
    format!("local.flatpak.{last}")
}

/// Vérifie un identifiant d'application. Renvoie un message d'erreur éventuel.
pub fn validate_app_id(id: &str) -> Option<String> {
    let parts: Vec<&str> = id.split('.').collect();
    if parts.len() < 3 {
        return Some(
            tr!("L'identifiant doit avoir au moins trois segments (ex. : org.exemple.App).").into(),
        );
    }
    if id.len() > 255 {
        return Some(tr!("Identifiant trop long.").into());
    }
    for p in &parts {
        if p.is_empty() {
            return Some(tr!("Segment vide dans l'identifiant.").into());
        }
        if p.chars().next().unwrap().is_ascii_digit() {
            return Some(tr!(
                "Le segment « {p} » ne peut pas commencer par un chiffre.",
                p = p
            ));
        }
        if !p.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
            return Some(tr!(
                "Le segment « {p} » contient des caractères non autorisés (lettres, chiffres et _ seulement).",
                p = p
            ));
        }
    }
    None
}

impl BuildConfig {
    pub fn from_info(info: &ArchiveInfo) -> Self {
        let name = info.guessed_name.clone();
        let hints = info.hints;
        let desktop = info.desktop.as_ref();
        let source = info.source.as_ref().map(|p| SourceBuild {
            system: p.system,
            bin_name: p.bin_name.clone(),
            bin_certain: p.bin_certain,
            extension: p
                .system
                .extension_candidates()
                .first()
                .map(|s| s.to_string()),
            entry: p.entry.clone(),
            alternatives: p.alternatives.clone(),
            installable: p.installable,
            requirements: p.requirements,
            build_script: p.build_script,
        });
        // Les extensions (Rust, Go, Node.js, Java) ne sont fournies que pour le SDK Freedesktop.
        let runtime = if source.as_ref().is_some_and(|s| s.system.needs_fdo()) {
            RuntimeKind::Freedesktop
        } else if hints.qt && !hints.electron {
            RuntimeKind::Kde
        } else if hints.gtk && !hints.electron {
            RuntimeKind::Gnome
        } else {
            RuntimeKind::Freedesktop
        };
        let category = desktop
            .and_then(|d| d.categories.as_ref())
            .and_then(|c| {
                c.split(';')
                    .find(|x| CATEGORIES.iter().any(|(k, _)| k == x))
            })
            .map(String::from)
            .unwrap_or_else(|| "Utility".into());
        let code = desktop
            .and_then(|d| d.exec.as_ref())
            .and_then(|e| ["%U", "%u", "%F", "%f"].into_iter().find(|c| e.contains(c)))
            .unwrap_or("")
            .to_string();
        let icon = match info.best_icon() {
            Some(i) => IconSource::FromArchive {
                raw: i.raw.clone(),
                is_svg: i.is_svg,
            },
            None => IconSource::Generated,
        };
        BuildConfig {
            app_id: default_app_id(&name),
            version: info.guessed_version.clone(),
            summary: desktop.and_then(|d| d.comment.clone()).unwrap_or_default(),
            category,
            exec_rel: info.main().map(|m| m.rel.clone()).unwrap_or_default(),
            icon,
            runtime,
            runtime_version: runtime.fallback_version().into(),
            perms: Permissions {
                display: hints.gui,
                gpu: hints.gui,
                audio: hints.gui,
                network: hints.network_likely || info.source.as_ref().is_some_and(|p| p.network),
                files: FileAccess::None,
                removable_media: false,
                all_devices: hints.devices,
                notifications: hints.gui,
                tray: false,
                printing: false,
            },
            terminal: !hints.gui || desktop.is_some_and(|d| d.terminal),
            electron: hints.electron,
            lib_dirs: info.lib_dirs.clone(),
            desktop_exec_code: code,
            source,
            name,
        }
    }

    pub fn slug(&self) -> String {
        slugify(&self.name)
    }

    /// Commande lancée par le paquet (script de lancement ou binaire compilé).
    pub fn command(&self) -> String {
        match &self.source {
            Some(s) => s.bin_name.clone(),
            None => self.slug(),
        }
    }

    pub fn wrapper_script(&self) -> String {
        let slug = self.slug();
        let base = format!("/app/opt/{slug}");
        let ld: Vec<String> = self
            .lib_dirs
            .iter()
            .map(|d| {
                if d.is_empty() {
                    base.clone()
                } else {
                    format!("{base}/{d}")
                }
            })
            .collect();
        let mut s = String::from("#!/bin/sh\n");
        if !ld.is_empty() {
            // Les chemins viennent de l'archive : on les protège pour le shell.
            s.push_str(&format!(
                "export LD_LIBRARY_PATH={}\"${{LD_LIBRARY_PATH:+:$LD_LIBRARY_PATH}}\"\n",
                sh_quote(&ld.join(":"))
            ));
        }
        let flag = if self.electron { " --no-sandbox" } else { "" };
        s.push_str(&format!(
            "exec {}{} \"$@\"\n",
            sh_quote(&format!("{base}/{}", self.exec_rel)),
            flag
        ));
        s
    }

    /// Script de lancement des projets Python, Node.js et Java (None pour les autres).
    pub fn launcher_script(&self) -> Option<String> {
        let src = self.source.as_ref()?;
        let slug = self.slug();
        let base = format!("/app/opt/{slug}");
        let mut s = String::from("#!/bin/sh\n");
        match (&src.system, &src.entry) {
            (BuildSystem::Python, entry) => {
                s.push_str(&format!(
                    "base={}\npp=\"\"\nfor d in \"$base\"/lib/python3*/site-packages \"$base\"/lib64/python3*/site-packages \"$base/src\"; do\n  [ -d \"$d\" ] && pp=\"$pp${{pp:+:}}$d\"\ndone\n[ -n \"$pp\" ] && export PYTHONPATH=\"$pp${{PYTHONPATH:+:$PYTHONPATH}}\"\n",
                    sh_quote(&base)
                ));
                match entry {
                    Entry::File(f) => s.push_str(&format!(
                        "exec python3 {} \"$@\"\n",
                        sh_quote(&format!("{base}/src/{f}"))
                    )),
                    Entry::Console(c) => s.push_str(&format!(
                        "exec python3 {} \"$@\"\n",
                        sh_quote(&format!("{base}/bin/{c}"))
                    )),
                    Entry::Module(m) => {
                        s.push_str(&format!("exec python3 -m {} \"$@\"\n", sh_quote(m)))
                    }
                    Entry::None => return None,
                }
            }
            (BuildSystem::Node, Entry::File(f)) => s.push_str(&format!(
                "exec /app/node/bin/node {} \"$@\"\n",
                sh_quote(&format!("{base}/{f}"))
            )),
            (BuildSystem::Java, Entry::File(f)) => s.push_str(&format!(
                "exec /app/jre/bin/java -jar {} \"$@\"\n",
                sh_quote(&format!("{base}/{f}"))
            )),
            _ => return None,
        }
        Some(s)
    }

    pub fn desktop_file(&self) -> String {
        let cmd = self.command();
        let exec = if self.desktop_exec_code.is_empty() {
            cmd
        } else {
            format!("{cmd} {}", self.desktop_exec_code)
        };
        let mut s = String::from("[Desktop Entry]\nType=Application\n");
        s.push_str(&format!("Name={}\n", self.name));
        if !self.summary.is_empty() {
            s.push_str(&format!("Comment={}\n", self.summary.replace('\n', " ")));
        }
        s.push_str(&format!(
            "Exec={exec}\nIcon={}\nTerminal={}\nCategories={};\n",
            self.app_id, self.terminal, self.category
        ));
        s
    }

    /// Manifeste complet. `icon_file` : (nom du fichier, dossier d'installation).
    pub fn manifest(
        &self,
        info: &ArchiveInfo,
        icon_file: &str,
        icon_dir: &str,
    ) -> serde_json::Value {
        if let Some(src) = &self.source {
            return self.source_manifest(src, info, icon_file, icon_dir);
        }
        let slug = self.slug();
        let ext = Path::new(icon_file)
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("png");
        let exec_path = format!("/app/opt/{slug}/{}", self.exec_rel);
        let cmds = vec![
            format!("mkdir -p /app/opt/{slug} /app/bin"),
            format!("cp -a app/. /app/opt/{slug}/"),
            format!("chmod +x {}", sh_quote(&exec_path)),
            format!("install -Dm755 wrapper.sh /app/bin/{slug}"),
            format!(
                "install -Dm644 {id}.desktop /app/share/applications/{id}.desktop",
                id = self.app_id
            ),
            format!(
                "install -Dm644 {icon_file} /app/share/icons/hicolor/{icon_dir}/apps/{id}.{ext}",
                id = self.app_id
            ),
        ];
        json!({
            "app-id": self.app_id,
            "runtime": self.runtime.platform(),
            "runtime-version": self.runtime_version,
            "sdk": self.runtime.sdk(),
            "command": slug,
            "finish-args": self.perms.finish_args(),
            "modules": [{
                "name": slug,
                "buildsystem": "simple",
                "build-options": { "strip": false, "no-debuginfo": true },
                "build-commands": cmds,
                "sources": [
                    {
                        "type": "archive",
                        "path": archive_name(info),
                        "sha256": info.sha256,
                        "strip-components": info.strip_components(),
                        "dest": "app"
                    },
                    { "type": "file", "path": "wrapper.sh" },
                    { "type": "file", "path": format!("{}.desktop", self.app_id) },
                    { "type": "file", "path": icon_file }
                ]
            }]
        })
    }
}

impl BuildConfig {
    /// Manifeste d'une compilation depuis les sources : un module pour le programme,
    /// un second pour l'entrée de menu et l'icône.
    fn source_manifest(
        &self,
        src: &SourceBuild,
        info: &ArchiveInfo,
        icon_file: &str,
        icon_dir: &str,
    ) -> serde_json::Value {
        let slug = self.slug();
        let bin = &src.bin_name;
        let ext = Path::new(icon_file)
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("png");
        let archive = json!({
            "type": "archive",
            "path": archive_name(info),
            "sha256": info.sha256,
            "strip-components": info.strip_components()
        });
        let run = format!("/run/build/{slug}");
        let mut m = serde_json::Map::new();
        m.insert("name".into(), json!(slug));
        let mut sources = vec![archive];
        if self.launcher_script().is_some() {
            sources.push(json!({ "type": "file", "path": "launcher.sh" }));
        }
        m.insert("sources".into(), json!(sources));
        let ext_dir = src.extension.as_deref().map(extension_dir);
        let ext_bin = ext_dir
            .as_ref()
            .map(|d| format!("{d}/bin"))
            .unwrap_or_default();
        let opt = format!("/app/opt/{slug}");
        let mut opts = serde_json::Map::new();
        opts.insert("no-debuginfo".into(), json!(true));
        match src.system {
            BuildSystem::Cargo => {
                m.insert("buildsystem".into(), json!("simple"));
                opts.insert("append-path".into(), json!(ext_bin));
                opts.insert(
                    "env".into(),
                    json!({ "CARGO_HOME": format!("{run}/cargo") }),
                );
                opts.insert("build-args".into(), json!(["--share=network"]));
                // Le nom du binaire peut être mal deviné (nom du paquet ≠ nom du programme, cible personnalisée dans
                // .cargo/config.toml…) : si le fichier attendu manque, on prend l'unique exécutable produit.
                let install = format!(
                    "p=target/release/{bin}; if [ ! -f \"$p\" ]; then \
                     all=$(find target -maxdepth 3 -type f -perm -u+x -path '*/release/*' ! -path '*/deps/*' ! -path '*/build/*' ! -path '*/examples/*' ! -path '*/incremental/*' ! -name '*.so' ! -name '*.d' ! -name '*.rlib'); \
                     set -- $all; if [ \"$#\" -gt 1 ]; then set -- $(echo \"$all\" | grep -i gui); fi; \
                     if [ \"$#\" -eq 1 ]; then echo \"Binaire « {bin} » introuvable, utilisation de $1\"; p=\"$1\"; \
                     else echo \"Binaire « {bin} » introuvable après la compilation. Fichiers produits :\"; find target -maxdepth 3 -type f -perm -u+x -path '*/release/*' ! -path '*/deps/*' ! -path '*/build/*'; exit 1; fi; fi; \
                     install -Dm755 \"$p\" /app/bin/{bin}"
                );
                m.insert(
                    "build-commands".into(),
                    json!(["cargo build --release", install]),
                );
            }
            BuildSystem::Go => {
                m.insert("buildsystem".into(), json!("simple"));
                opts.insert("append-path".into(), json!(ext_bin));
                opts.insert(
                    "env".into(),
                    json!({ "GOCACHE": format!("{run}/go-cache"), "GOPATH": format!("{run}/go") }),
                );
                opts.insert("build-args".into(), json!(["--share=network"]));
                // Le paquet principal est soit à la racine, soit dans cmd/<nom>.
                m.insert(
                    "build-commands".into(),
                    json!([format!("if [ ! -f main.go ] && [ -d cmd/{bin} ]; then go build -trimpath -o /app/bin/{bin} ./cmd/{bin}; else go build -trimpath -o /app/bin/{bin} .; fi")]),
                );
            }
            BuildSystem::CMake => {
                m.insert("buildsystem".into(), json!("cmake-ninja"));
                m.insert("config-opts".into(), json!(["-DCMAKE_BUILD_TYPE=Release"]));
            }
            BuildSystem::Meson => {
                m.insert("buildsystem".into(), json!("meson"));
                m.insert("config-opts".into(), json!(["--buildtype=release"]));
            }
            BuildSystem::Autotools => {
                m.insert("buildsystem".into(), json!("autotools"));
            }
            BuildSystem::Python => {
                m.insert("buildsystem".into(), json!("simple"));
                opts.insert("env".into(), json!({ "PIP_DISABLE_PIP_VERSION_CHECK": "1", "PIP_BREAK_SYSTEM_PACKAGES": "1" }));
                opts.insert("build-args".into(), json!(["--share=network"]));
                let mut cmds: Vec<String> = Vec::new();
                if src.installable {
                    cmds.push(format!("pip3 install --prefix={opt} --no-cache-dir ."));
                } else if src.requirements {
                    cmds.push(format!(
                        "pip3 install --prefix={opt} --no-cache-dir -r requirements.txt"
                    ));
                }
                if matches!(src.entry, Entry::File(_)) {
                    cmds.push(format!("mkdir -p {opt}/src && cp -a . {opt}/src/"));
                }
                cmds.push(format!("install -Dm755 launcher.sh /app/bin/{bin}"));
                m.insert("build-commands".into(), json!(cmds));
            }
            BuildSystem::Node => {
                m.insert("buildsystem".into(), json!("simple"));
                let dir = ext_dir.clone().unwrap_or_default();
                opts.insert("append-path".into(), json!(ext_bin));
                opts.insert("env".into(), json!({ "npm_config_cache": format!("{run}/npm-cache"), "npm_config_nodedir": dir, "npm_config_update_notifier": "false" }));
                opts.insert("build-args".into(), json!(["--share=network"]));
                let mut cmds: Vec<String> = vec!["npm install --no-audit --no-fund".into()];
                if src.build_script {
                    cmds.push("npm run build".into());
                    cmds.push("npm prune --omit=dev --no-audit --no-fund".into());
                }
                cmds.push(format!("mkdir -p {opt} && cp -a . {opt}/"));
                cmds.push(format!("if [ -x {dir}/install.sh ]; then {dir}/install.sh; else install -Dm755 {dir}/bin/node /app/node/bin/node; fi"));
                cmds.push("test -x /app/node/bin/node".into());
                cmds.push(format!("install -Dm755 launcher.sh /app/bin/{bin}"));
                m.insert("build-commands".into(), json!(cmds));
            }
            BuildSystem::Java => {
                m.insert("buildsystem".into(), json!("simple"));
                let dir = ext_dir.clone().unwrap_or_default();
                let cmds = vec![
                    format!("mkdir -p {opt} && cp -a . {opt}/"),
                    format!(
                        "if [ -x {dir}/install.sh ]; then {dir}/install.sh; elif [ -x {dir}/installjdk.sh ]; then {dir}/installjdk.sh; else mkdir -p /app/jre && cp -a {dir}/jvm/openjdk-*/. /app/jre/; fi"
                    ),
                    "test -x /app/jre/bin/java".into(),
                    format!("install -Dm755 launcher.sh /app/bin/{bin}"),
                ];
                m.insert("build-commands".into(), json!(cmds));
            }
            BuildSystem::Make => {
                m.insert("buildsystem".into(), json!("simple"));
                m.insert("build-commands".into(), json!(["make -j${FLATPAK_BUILDER_N_JOBS:-2}", format!("make install PREFIX=/app prefix=/app || install -Dm755 {bin} /app/bin/{bin}")]));
            }
        }
        m.insert("build-options".into(), serde_json::Value::Object(opts));

        let meta = json!({
            "name": format!("{slug}-metadata"),
            "buildsystem": "simple",
            "build-commands": [
                format!("install -Dm644 {id}.desktop /app/share/applications/{id}.desktop", id = self.app_id),
                format!("install -Dm644 {icon_file} /app/share/icons/hicolor/{icon_dir}/apps/{id}.{ext}", id = self.app_id),
            ],
            "sources": [
                { "type": "file", "path": format!("{}.desktop", self.app_id) },
                { "type": "file", "path": icon_file }
            ]
        });
        let mut root = json!({
            "app-id": self.app_id,
            "runtime": self.runtime.platform(),
            "runtime-version": self.runtime_version,
            "sdk": self.runtime.sdk(),
            "command": bin,
            "finish-args": self.perms.finish_args(),
            "modules": [serde_json::Value::Object(m), meta]
        });
        if let Some(e) = &src.extension {
            root["sdk-extensions"] = json!([e]);
        }
        root
    }
}

fn sh_quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', "'\\''"))
}

fn icon_dir_for(width: u32) -> String {
    format!("{width}x{width}")
}

/// Prépare l'icône dans le dossier de travail. Renvoie (nom du fichier, dossier d'installation).
fn prepare_icon(cfg: &BuildConfig, info: &ArchiveInfo, work: &Path) -> Result<(String, String)> {
    let bytes_and_svg: Option<(Vec<u8>, bool)> = match &cfg.icon {
        IconSource::FromArchive { raw, is_svg } => {
            Some((read_member(&info.path, raw, 5 << 20)?, *is_svg))
        }
        IconSource::File(p) => {
            let is_svg = p.extension().is_some_and(|e| e.eq_ignore_ascii_case("svg"));
            Some((
                std::fs::read(p)
                    .with_context(|| tr!("lecture de l'icône {chemin}", chemin = p.display()))?,
                is_svg,
            ))
        }
        IconSource::Generated => None,
    };
    if let Some((data, is_svg)) = bytes_and_svg {
        if is_svg {
            std::fs::write(work.join("icon.svg"), &data)?;
            return Ok(("icon.svg".into(), "scalable".into()));
        }
        if let Ok(img) = image::load_from_memory_with_format(&data, image::ImageFormat::Png) {
            let (w, h) = (img.width(), img.height());
            if w == h && (16..=512).contains(&w) {
                std::fs::write(work.join("icon.png"), &data)?;
                return Ok(("icon.png".into(), icon_dir_for(w)));
            }
            // Recadrage carré + redimensionnement à 256
            let side = w.min(h);
            let sq = img.crop_imm((w - side) / 2, (h - side) / 2, side, side);
            let out = sq.resize_exact(256, 256, image::imageops::FilterType::Lanczos3);
            out.save_with_format(work.join("icon.png"), image::ImageFormat::Png)?;
            return Ok(("icon.png".into(), "256x256".into()));
        }
    }
    generated_icon(&cfg.app_id).save_with_format(work.join("icon.png"), image::ImageFormat::Png)?;
    Ok(("icon.png".into(), "256x256".into()))
}

/// Icône par défaut : carré arrondi coloré selon l'identifiant.
pub fn generated_icon(seed: &str) -> image::RgbaImage {
    let mut h: u32 = 2166136261;
    for b in seed.bytes() {
        h = (h ^ b as u32).wrapping_mul(16777619);
    }
    let hue = (h % 360) as f32;
    let (r1, g1, b1) = hsv(hue, 0.65, 0.92);
    let (r2, g2, b2) = hsv((hue + 40.0) % 360.0, 0.75, 0.62);
    let n = 256u32;
    image::RgbaImage::from_fn(n, n, |x, y| {
        let (fx, fy) = (x as f32 + 0.5, y as f32 + 0.5);
        let c = n as f32 / 2.0;
        let (half, radius) = (c - 24.0, 52.0f32);
        let (qx, qy) = (
            (fx - c).abs() - (half - radius),
            (fy - c).abs() - (half - radius),
        );
        let d = (qx.max(0.0).powi(2) + qy.max(0.0).powi(2)).sqrt() + qx.max(qy).min(0.0) - radius;
        if d >= 0.0 {
            return image::Rgba([0, 0, 0, 0]);
        }
        let t = (fx + fy) / (2.0 * n as f32);
        let mix = |a: u8, b: u8| (a as f32 + (b as f32 - a as f32) * t) as u8;
        // Cube stylisé au centre
        let (ux, uy) = (fx - c, fy - c);
        let in_box = ux.abs() < 52.0
            && uy.abs() < 52.0
            && (ux.abs() > 38.0 || uy.abs() > 38.0 || (ux.abs() < 6.0 && uy.abs() < 38.0));
        if in_box {
            return image::Rgba([255, 255, 255, 235]);
        }
        image::Rgba([mix(r1, r2), mix(g1, g2), mix(b1, b2), 255])
    })
}

fn hsv(h: f32, s: f32, v: f32) -> (u8, u8, u8) {
    let c = v * s;
    let x = c * (1.0 - ((h / 60.0) % 2.0 - 1.0).abs());
    let m = v - c;
    let (r, g, b) = match (h / 60.0) as u32 {
        0 => (c, x, 0.0),
        1 => (x, c, 0.0),
        2 => (0.0, c, x),
        3 => (0.0, x, c),
        4 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };
    (
        ((r + m) * 255.0) as u8,
        ((g + m) * 255.0) as u8,
        ((b + m) * 255.0) as u8,
    )
}

/// Nom de l'archive à l'intérieur du dossier de travail (l'extension sert à détecter le format).
fn archive_name(info: &ArchiveInfo) -> String {
    info.path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| "source.tar.gz".into())
}

/// Écrit tous les fichiers nécessaires dans `work` et renvoie le chemin du manifeste.
pub fn prepare_workdir(cfg: &BuildConfig, info: &ArchiveInfo, work: &Path) -> Result<PathBuf> {
    std::fs::create_dir_all(work)?;
    // L'archive est copiée dans le dossier de travail : son dossier d'origine peut être
    // inaccessible au builder (portail de documents /run/user/…/doc en Flatpak).
    let dest = work.join(archive_name(info));
    std::fs::copy(&info.path, &dest).map_err(|e| {
        anyhow::anyhow!(tr!("copie de l'archive impossible : {erreur}", erreur = e))
    })?;
    let (icon_file, icon_dir) = prepare_icon(cfg, info, work)?;
    match cfg.launcher_script() {
        Some(l) => std::fs::write(work.join("launcher.sh"), l)?,
        None if cfg.source.is_none() => {
            std::fs::write(work.join("wrapper.sh"), cfg.wrapper_script())?
        }
        None => {}
    }
    std::fs::write(
        work.join(format!("{}.desktop", cfg.app_id)),
        cfg.desktop_file(),
    )?;
    let m = cfg.manifest(info, &icon_file, &icon_dir);
    let path = work.join(format!("{}.json", cfg.app_id));
    std::fs::write(&path, serde_json::to_string_pretty(&m)?)?;
    Ok(path)
}

/// Raccourci de validation : renvoie la liste des problèmes bloquants de la configuration.
pub fn validate(cfg: &BuildConfig, info: &ArchiveInfo) -> Vec<String> {
    let mut v = Vec::new();
    if cfg.name.trim().is_empty() {
        v.push(tr!("Le nom de l'application est vide.").into());
    }
    if let Some(e) = validate_app_id(&cfg.app_id) {
        v.push(e);
    }
    if let Some(src) = &cfg.source {
        let b = &src.bin_name;
        if b.is_empty()
            || b.starts_with('-')
            || !b
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '+' | '-'))
        {
            v.push(tr!("Nom du programme invalide (lettres, chiffres, . _ + - seulement).").into());
        }
        if matches!(
            src.system,
            BuildSystem::Python | BuildSystem::Node | BuildSystem::Java
        ) && src.entry == Entry::None
        {
            v.push(tr!("Aucun point d'entrée à lancer n'a été trouvé.").into());
        }
        if !src.bin_certain {
            v.push(tr!("Confirmez le nom du programme à installer avant de compiler.").into());
        }
    } else if cfg.exec_rel.is_empty() {
        v.push(tr!("Aucun exécutable sélectionné.").into());
    } else if !info.executables.iter().any(|e| e.rel == cfg.exec_rel) {
        v.push(tr!("L'exécutable choisi n'existe pas dans l'archive.").into());
    }
    if cfg.version.trim().is_empty() {
        v.push(tr!("La version est vide.").into());
    }
    v
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids() {
        assert!(validate_app_id("org.exemple.App").is_none());
        assert!(validate_app_id("App").is_some());
        assert!(validate_app_id("org.1bad.App").is_some());
        assert!(validate_app_id("org.ba-d.App").is_some());
        assert_eq!(default_app_id("Mon Appli"), "local.flatpak.MonAppli");
        assert_eq!(default_app_id("3D Tool"), "local.flatpak.App3DTool");
        assert_eq!(slugify("Mon Appli!"), "mon-appli");
    }

    #[test]
    fn finish_args() {
        let p = Permissions {
            display: true,
            gpu: true,
            audio: false,
            network: true,
            files: FileAccess::Downloads,
            removable_media: false,
            all_devices: false,
            notifications: false,
            tray: false,
            printing: false,
        };
        let a = p.finish_args();
        assert!(a.contains(&"--share=network".to_string()));
        assert!(a.contains(&"--filesystem=xdg-download".to_string()));
        assert!(!a.iter().any(|x| x.contains("home")));
    }

    /// Écrit un dossier de travail réel pour inspection (ignoré sans FC_DUMP_DIR).
    #[test]
    fn dump_workdir() {
        let (Ok(dir), Ok(arch)) = (
            std::env::var("FC_DUMP_DIR"),
            std::env::var("FC_DUMP_ARCHIVE"),
        ) else {
            return;
        };
        let info = crate::analyze::analyze(Path::new(&arch)).unwrap();
        let cfg = BuildConfig::from_info(&info);
        let m = prepare_workdir(&cfg, &info, Path::new(&dir)).unwrap();
        println!("manifest: {}", m.display());
    }

    #[test]
    fn wrapper_protects_hostile_paths() {
        let mut cfg = test_config();
        cfg.lib_dirs = vec!["lib\"; rm -rf ~; \"".into(), String::new()];
        cfg.exec_rel = "bin/it's".into();
        let w = cfg.wrapper_script();
        assert!(
            w.contains("export LD_LIBRARY_PATH='/app/opt/demo/lib\"; rm -rf ~; \":/app/opt/demo'")
        );
        assert!(w.contains("exec '/app/opt/demo/bin/it'\\''s'"));
    }

    fn test_config() -> BuildConfig {
        BuildConfig {
            name: "Demo".into(),
            app_id: "local.flatpak.Demo".into(),
            version: "1".into(),
            summary: String::new(),
            category: "Utility".into(),
            exec_rel: "demo".into(),
            icon: IconSource::Generated,
            runtime: RuntimeKind::Freedesktop,
            runtime_version: FDO_VERSION.into(),
            perms: Permissions {
                display: true,
                gpu: true,
                audio: false,
                network: false,
                files: FileAccess::None,
                removable_media: false,
                all_devices: false,
                notifications: false,
                tray: false,
                printing: false,
            },
            terminal: false,
            electron: false,
            lib_dirs: vec![],
            desktop_exec_code: String::new(),
            source: None,
        }
    }

    #[test]
    fn generated_icon_has_alpha_corners() {
        let i = generated_icon("x.y.z");
        assert_eq!(i.get_pixel(0, 0)[3], 0);
        assert_eq!(i.get_pixel(60, 60)[3], 255);
    }
}
