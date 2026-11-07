//! Types décrivant un projet source : système de build, point d'entrée, fichiers de build.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BuildSystem {
    Cargo,
    CMake,
    Meson,
    Autotools,
    Make,
    Go,
    Python,
    Node,
    /// Application Java déjà empaquetée (.jar exécutable) : rien à compiler, seul le JRE est ajouté.
    Java,
}

impl BuildSystem {
    pub fn label(&self) -> &'static str {
        match self {
            BuildSystem::Cargo => "Rust (Cargo)",
            BuildSystem::CMake => "C / C++ (CMake)",
            BuildSystem::Meson => "C / C++ (Meson)",
            BuildSystem::Autotools => "C / C++ (Autotools)",
            BuildSystem::Make => "C / C++ (Makefile)",
            BuildSystem::Go => "Go",
            BuildSystem::Python => "Python",
            BuildSystem::Node => "Node.js",
            BuildSystem::Java => "Java (JAR)",
        }
    }

    /// Extensions du SDK Freedesktop pouvant fournir l'outil, de la préférée à la moins préférée
    /// (le C/C++ et Python sont déjà dans le SDK). Les identifiants proposés dépendent de la branche.
    pub fn extension_candidates(&self) -> &'static [&'static str] {
        match self {
            BuildSystem::Cargo => &["org.freedesktop.Sdk.Extension.rust-stable"],
            BuildSystem::Go => &["org.freedesktop.Sdk.Extension.golang"],
            BuildSystem::Node => &[
                "org.freedesktop.Sdk.Extension.node24",
                "org.freedesktop.Sdk.Extension.node22",
                "org.freedesktop.Sdk.Extension.node20",
            ],
            BuildSystem::Java => &[
                "org.freedesktop.Sdk.Extension.openjdk25",
                "org.freedesktop.Sdk.Extension.openjdk21",
                "org.freedesktop.Sdk.Extension.openjdk",
            ],
            _ => &[],
        }
    }

    /// Vrai si une extension n'existe que pour le SDK Freedesktop (le runtime est alors imposé).
    pub fn needs_fdo(&self) -> bool {
        !self.extension_candidates().is_empty()
    }

    pub fn extension_label(&self) -> &'static str {
        match self {
            BuildSystem::Cargo => tr!("Extension Rust (rustc, cargo)"),
            BuildSystem::Go => tr!("Extension Go"),
            BuildSystem::Node => tr!("Extension Node.js"),
            BuildSystem::Java => tr!("Extension OpenJDK (Java)"),
            _ => "",
        }
    }

    /// Vrai si le projet est compilé ; faux pour Java (JAR déjà construit).
    pub fn compiles(&self) -> bool {
        !matches!(self, BuildSystem::Java)
    }

    /// Phrase d'introduction affichée dans l'analyse.
    pub fn intro(&self) -> &'static str {
        match self {
            BuildSystem::Python => {
                tr!(
                    "Les dépendances Python seront installées dans le bac à sable Flatpak (pip) puis le programme sera empaqueté avec l'interpréteur du runtime."
                )
            }
            BuildSystem::Node => {
                tr!(
                    "Les dépendances seront installées (npm) dans le bac à sable Flatpak, puis le programme sera empaqueté avec Node.js fourni par l'extension du SDK."
                )
            }
            BuildSystem::Java => {
                tr!(
                    "Ce programme Java est déjà construit : il sera empaqueté tel quel avec un environnement Java (JRE) fourni par l'extension OpenJDK."
                )
            }
            _ => {
                tr!(
                    "L'archive sera compilée dans le bac à sable Flatpak avec le SDK Freedesktop. La première compilation peut durer plusieurs minutes."
                )
            }
        }
    }

    /// Outils recherchés sur la machine (information seulement).
    pub(super) fn host_tools(&self) -> &'static [(&'static str, &'static [&'static str])] {
        match self {
            BuildSystem::Cargo => &[("rustc", &["--version"]), ("cargo", &["--version"])],
            BuildSystem::CMake => &[
                ("gcc", &["--version"]),
                ("g++", &["--version"]),
                ("cmake", &["--version"]),
            ],
            BuildSystem::Meson => &[("gcc", &["--version"]), ("meson", &["--version"])],
            BuildSystem::Autotools => &[("gcc", &["--version"]), ("make", &["--version"])],
            BuildSystem::Make => &[("gcc", &["--version"]), ("make", &["--version"])],
            BuildSystem::Go => &[("go", &["version"])],
            BuildSystem::Python => &[("python3", &["--version"]), ("pip3", &["--version"])],
            BuildSystem::Node => &[("node", &["--version"]), ("npm", &["--version"])],
            BuildSystem::Java => &[("java", &["--version"])],
        }
    }
}

/// Dossier d'une extension du SDK : `/usr/lib/sdk/<fin de l'identifiant>`.
pub fn extension_dir(id: &str) -> String {
    format!("/usr/lib/sdk/{}", id.rsplit('.').next().unwrap_or(id))
}

/// Point d'entrée d'un projet (ce que le script de lancement exécute).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum Entry {
    #[default]
    None,
    /// Fichier du projet (chemin relatif à la racine) : script Python, point d'entrée Node.js ou JAR.
    File(String),
    /// Commande installée par pip (`[project.scripts]`).
    Console(String),
    /// Module Python lancé par `python3 -m`.
    Module(String),
}

#[derive(Debug, Clone)]
pub struct SourceProject {
    pub system: BuildSystem,
    pub bin_name: String,
    /// Vrai si le nom du binaire a été lu dans les fichiers du projet (et non deviné).
    pub bin_certain: bool,
    pub gui: bool,
    /// Accès direct au matériel (USB, HID).
    pub devices: bool,
    /// Accès réseau probablement nécessaire à l'exécution.
    pub network: bool,
    pub qt: bool,
    pub gtk: bool,
    pub notes: Vec<String>,
    /// Raison pour laquelle le projet ne produit visiblement aucun programme (bibliothèque seule…).
    pub blocker: Option<String>,
    /// Python, Node.js, Java : ce que le script de lancement exécute.
    pub entry: Entry,
    /// Autres fichiers possibles pour le point d'entrée (choix proposé à l'utilisateur).
    pub alternatives: Vec<String>,
    /// Python : le projet s'installe avec pip (pyproject.toml, setup.py).
    pub installable: bool,
    /// Python : un fichier requirements.txt est présent.
    pub requirements: bool,
    /// Node.js : package.json définit un script « build » à lancer.
    pub build_script: bool,
}

#[derive(Debug, Clone)]
pub struct BuildFile {
    /// Nom du fichier en minuscules.
    pub name: String,
    pub content: String,
}

/// Noms de fichiers (en minuscules) dont le contenu est lu pendant l'analyse.
pub const BUILD_FILE_NAMES: &[&str] = &[
    "cargo.toml",
    "go.mod",
    "meson.build",
    "cmakelists.txt",
    "configure.ac",
    "configure.in",
    "makefile.am",
    "configure",
    "makefile",
    "gnumakefile",
    "package.json",
    "setup.py",
    "setup.cfg",
    "pyproject.toml",
    "requirements.txt",
    "pom.xml",
    "build.gradle",
    "build.gradle.kts",
    "gemfile",
    "composer.json",
    "build.zig",
    "mix.exs",
];

/// Projets reconnus mais pas encore pris en charge : (fichier, langage).
pub(super) const UNSUPPORTED: &[(&str, &str)] = &[
    ("pom.xml", "Java (Maven)"),
    ("build.gradle", "Java (Gradle)"),
    ("build.gradle.kts", "Java (Gradle)"),
    ("gemfile", "Ruby"),
    ("composer.json", "PHP"),
    ("build.zig", "Zig"),
    ("mix.exs", "Elixir"),
];

/// Langages pris en charge, pour les messages (dans la langue active).
pub fn supported_langs() -> &'static str {
    tr!("Rust, C/C++, Go, Python, Node.js et Java (JAR)")
}

#[derive(Debug, Clone)]
pub enum Detection {
    Supported(SourceProject),
    Unsupported(&'static str),
}
