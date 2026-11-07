//! Traductions anglaises : manifest et chaînes d'outils.

pub const ENTRIES: &[(&str, &str)] = &[
    // Runtimes
    ("Freedesktop (défaut)", "Freedesktop (default)"),
    // Accès aux fichiers
    (
        "Aucun (via les sélecteurs de fichiers uniquement)",
        "None (through file pickers only)",
    ),
    ("Téléchargements", "Downloads"),
    ("Documents et Téléchargements", "Documents and Downloads"),
    ("Tout le dossier personnel", "Entire home folder"),
    // Risques des permissions
    (
        "Accès à tout le dossier personnel (y compris ~/.ssh et ~/.gnupg). Préférez un accès restreint.",
        "Access to the entire home folder (including ~/.ssh and ~/.gnupg). Prefer restricted access.",
    ),
    (
        "Accès à tous les périphériques : affaiblit fortement le bac à sable.",
        "Access to all devices: greatly weakens the sandbox.",
    ),
    (
        "Réseau et accès aux fichiers combinés : l'application pourrait envoyer vos fichiers sur Internet.",
        "Network and file access combined: the application could send your files over the Internet.",
    ),
    (
        "Accès aux clés USB et disques externes.",
        "Access to USB drives and external disks.",
    ),
    // Catégories
    ("Utilitaire", "Utility"),
    ("Développement", "Development"),
    ("Graphisme", "Graphics"),
    ("Audio et vidéo", "Audio and video"),
    ("Jeu", "Games"),
    ("Internet", "Internet"),
    ("Bureautique", "Office"),
    ("Système", "System"),
    ("Éducation", "Education"),
    // Validation de l'identifiant
    (
        "L'identifiant doit avoir au moins trois segments (ex. : org.exemple.App).",
        "The ID must have at least three segments (e.g. org.example.App).",
    ),
    ("Identifiant trop long.", "ID too long."),
    (
        "Segment vide dans l'identifiant.",
        "Empty segment in the ID.",
    ),
    (
        "Le segment « {p} » ne peut pas commencer par un chiffre.",
        "The segment “{p}” cannot start with a digit.",
    ),
    (
        "Le segment « {p} » contient des caractères non autorisés (lettres, chiffres et _ seulement).",
        "The segment “{p}” contains disallowed characters (letters, digits and _ only).",
    ),
    // Préparation du répertoire de travail
    ("lecture de l'icône {chemin}", "reading icon {chemin}"),
    (
        "copie de l'archive impossible : {erreur}",
        "could not copy the archive: {erreur}",
    ),
    // Validation de la configuration
    (
        "Le nom de l'application est vide.",
        "The application name is empty.",
    ),
    (
        "Nom du programme invalide (lettres, chiffres, . _ + - seulement).",
        "Invalid program name (letters, digits, . _ + - only).",
    ),
    (
        "Aucun point d'entrée à lancer n'a été trouvé.",
        "No entry point to launch was found.",
    ),
    (
        "Confirmez le nom du programme à installer avant de compiler.",
        "Confirm the name of the program to install before building.",
    ),
    ("Aucun exécutable sélectionné.", "No executable selected."),
    (
        "L'exécutable choisi n'existe pas dans l'archive.",
        "The chosen executable does not exist in the archive.",
    ),
    ("La version est vide.", "The version is empty."),
    // Chaînes d'outils : extensions
    (
        "Extension Rust (rustc, cargo)",
        "Rust extension (rustc, cargo)",
    ),
    ("Extension Go", "Go extension"),
    ("Extension Node.js", "Node.js extension"),
    ("Extension OpenJDK (Java)", "OpenJDK extension (Java)"),
    // Introductions par système de compilation
    (
        "Les dépendances Python seront installées dans le bac à sable Flatpak (pip) puis le programme sera empaqueté avec l'interpréteur du runtime.",
        "Python dependencies will be installed in the Flatpak sandbox (pip), then the program will be packaged with the runtime's interpreter.",
    ),
    (
        "Les dépendances seront installées (npm) dans le bac à sable Flatpak, puis le programme sera empaqueté avec Node.js fourni par l'extension du SDK.",
        "Dependencies will be installed (npm) in the Flatpak sandbox, then the program will be packaged with Node.js provided by the SDK extension.",
    ),
    (
        "Ce programme Java est déjà construit : il sera empaqueté tel quel avec un environnement Java (JRE) fourni par l'extension OpenJDK.",
        "This Java program is already built: it will be packaged as is with a Java environment (JRE) provided by the OpenJDK extension.",
    ),
    (
        "L'archive sera compilée dans le bac à sable Flatpak avec le SDK Freedesktop. La première compilation peut durer plusieurs minutes.",
        "The archive will be built in the Flatpak sandbox with the Freedesktop SDK. The first build may take several minutes.",
    ),
    // Langages pris en charge
    (
        "Rust, C/C++, Go, Python, Node.js et Java (JAR)",
        "Rust, C/C++, Go, Python, Node.js and Java (JAR)",
    ),
    // Détection : notes
    (
        "Aucun main.go ni dossier cmd/ : le point d'entrée n'a pas été repéré.",
        "No main.go or cmd/ folder: the entry point was not found.",
    ),
    (
        "Les modules Go sont téléchargés pendant la compilation.",
        "Go modules are downloaded during the build.",
    ),
    (
        "Nom du binaire déduit du nom du projet.",
        "Binary name inferred from the project name.",
    ),
    (
        "Nom du binaire deviné : vérifiez-le.",
        "Binary name guessed: please check it.",
    ),
    // Node.js
    (
        "Les dépendances npm sont téléchargées pendant la création du paquet (accès réseau nécessaire à ce moment-là).",
        "npm dependencies are downloaded while the package is being created (network access is needed at that point).",
    ),
    (
        "Le script « build » de package.json sera exécuté avant l'empaquetage.",
        "The “build” script from package.json will be run before packaging.",
    ),
    (
        "Ce projet utilise Electron : sa construction depuis les sources n'est pas prise en charge. Utilisez plutôt l'archive de l'application déjà compilée (celle que le projet distribue pour Linux).",
        "This project uses Electron: building it from source is not supported. Use the archive of the already built application instead (the one the project distributes for Linux).",
    ),
    (
        "Aucun point d'entrée Node.js trouvé (ni « bin », ni « main », ni « scripts.start », ni index.js) : il n'y a rien à lancer.",
        "No Node.js entry point found (no “bin”, no “main”, no “scripts.start”, no index.js): there is nothing to launch.",
    ),
    (
        "Le fichier « {e} » n'a pas été repéré à la racine de l'archive ; vérifiez-le dans « Point d'entrée ».",
        "The file “{e}” was not found at the root of the archive; check it in “Entry point”.",
    ),
    // Python
    (
        "Ce projet Python n'installe aucune commande (pas de [project.scripts]) et n'a ni __main__.py ni main.py : c'est une bibliothèque, il n'y a rien à lancer.",
        "This Python project installs no command (no [project.scripts]) and has neither __main__.py nor main.py: it is a library, there is nothing to launch.",
    ),
    (
        "Aucun point d'entrée Python trouvé (main.py, __main__.py, app.py ou run.py à la racine).",
        "No Python entry point found (main.py, __main__.py, app.py or run.py at the root).",
    ),
    (
        "Aucun requirements.txt ni pyproject.toml : seules les bibliothèques standard de Python seront disponibles.",
        "No requirements.txt or pyproject.toml: only Python's standard library will be available.",
    ),
    (
        "Les dépendances Python sont téléchargées pendant la création du paquet (accès réseau nécessaire à ce moment-là).",
        "Python dependencies are downloaded while the package is being created (network access is needed at that point).",
    ),
    (
        "Tkinter n'est pas toujours fourni par le runtime : si la fenêtre ne s'ouvre pas, ce projet n'est sans doute pas adapté.",
        "Tkinter is not always provided by the runtime: if the window does not open, this project is probably not suitable.",
    ),
    // Java
    (
        "Application graphique supposée : pour un outil en ligne de commande, cochez « Ouvrir dans un terminal » en mode avancé.",
        "Graphical application assumed: for a command-line tool, tick “Open in a terminal” in advanced mode.",
    ),
    // Cargo
    (
        "Accès direct au matériel (USB/HID) détecté : la permission « tous les périphériques » est accordée.",
        "Direct hardware access (USB/HID) detected: the “all devices” permission is granted.",
    ),
    (
        "Ce projet Rust ne contient pas de programme (ni src/main.rs, ni section [[bin]]) : c'est une bibliothèque, il n'y a rien à lancer.",
        "This Rust project contains no program (no src/main.rs, no [[bin]] section): it is a library, there is nothing to launch.",
    ),
    (
        "Espace de travail Cargo : indiquez le nom du binaire à installer.",
        "Cargo workspace: enter the name of the binary to install.",
    ),
    (
        "Les dépendances Rust sont téléchargées pendant la compilation (crates.io).",
        "Rust dependencies are downloaded during the build (crates.io).",
    ),
    // Composants
    ("Runtime d'exécution", "Execution runtime"),
    (
        "SDK (compilateurs et outils de build)",
        "SDK (compilers and build tools)",
    ),
    // Installation d'outils
    ("{titre} : {explication}", "{titre}: {explication}"),
    ("L'installation a échoué.", "The installation failed."),
    (
        "impossible de lancer {programme} : {erreur}",
        "unable to launch {programme}: {erreur}",
    ),
    ("Opération annulée.", "Operation cancelled."),
];
