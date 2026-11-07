//! Traductions anglaises : analyse d'archive (constats, types d'application, erreurs).

pub const ENTRIES: &[(&str, &str)] = &[
    // Architecture
    ("machine {m}", "machine {m}"),
    // Types d'application
    ("Binaire natif précompilé", "Prebuilt native binary"),
    ("Application à base de script", "Script-based application"),
    ("Code source à compiler", "Source code to build"),
    ("Application Java", "Java application"),
    ("Type inconnu", "Unknown type"),
    // Chemins d'archive refusés
    ("contient « .. »", "contains \"..\""),
    ("chemin absolu", "absolute path"),
    // Erreurs de lecture
    ("archive zstd invalide : {e}", "invalid zstd archive: {e}"),
    ("lecture de {path}", "reading {path}"),
    ("lecture de l'archive", "reading the archive"),
    (
        "archive corrompue ou tronquée : {e}",
        "corrupted or truncated archive: {e}",
    ),
    (
        "calcul de l'empreinte interrompu",
        "checksum computation interrupted",
    ),
    (
        "format d'archive non reconnu",
        "unrecognized archive format",
    ),
    (
        "fichier introuvable dans l'archive : {raw}",
        "file not found in the archive: {raw}",
    ),
    // Constats : format et taille
    ("Format non pris en charge", "Unsupported format"),
    (
        "Le fichier n'est pas une archive .tar compressée en gzip, xz, zstd ou bzip2.",
        "The file is not a .tar archive compressed with gzip, xz, zstd or bzip2.",
    ),
    ("Trop de fichiers", "Too many files"),
    (
        "L'archive contient plus de {max} entrées.",
        "The archive contains more than {max} entries.",
    ),
    (
        "{path} (passe par un lien symbolique)",
        "{path} (goes through a symbolic link)",
    ),
    (
        "{path} → {target} (lien hors de l'archive)",
        "{path} → {target} (link outside the archive)",
    ),
    (
        "Archive trop volumineuse une fois décompressée",
        "Archive too large once decompressed",
    ),
    (
        "Plus de {size} Gio décompressés.",
        "More than {size} GiB decompressed.",
    ),
    // Constats : sécurité
    (
        "Chemins dangereux dans l'archive",
        "Dangerous paths in the archive",
    ),
    (
        "{unsafe_paths} entrée(s) tentent d'écrire hors du dossier d'extraction ou à travers un lien symbolique, par exemple : {unsafe_example}. Archive refusée par sécurité.",
        "{unsafe_paths} entry(ies) try to write outside the extraction folder or through a symbolic link, for example: {unsafe_example}. Archive rejected for security reasons.",
    ),
    ("Fichiers spéciaux", "Special files"),
    (
        "{special} fichier(s) de périphérique ou tube nommé : refusés par sécurité.",
        "{special} device file(s) or named pipe(s): rejected for security reasons.",
    ),
    (
        "Taux de compression suspect",
        "Suspicious compression ratio",
    ),
    (
        "Possible « bombe de décompression ».",
        "Possible \"decompression bomb\".",
    ),
    ("Bits setuid/setgid", "setuid/setgid bits"),
    (
        "{setuid} fichier(s) ont des bits spéciaux ; ils sont sans effet dans Flatpak.",
        "{setuid} file(s) have special bits; they have no effect in Flatpak.",
    ),
    // Constats : type de projet
    (
        "Code source à compiler : {system}",
        "Source code to build: {system}",
    ),
    ("Application {system}", "{system} application"),
    ("Aucun programme à compiler", "No program to build"),
    ("Nom du binaire à confirmer", "Binary name to confirm"),
    (
        "Le programme installé s'appellerait « {name} », mais ce nom est deviné. Vous devrez le confirmer (ou le corriger) avant de lancer la compilation.",
        "The installed program would be called \"{name}\", but this name is a guess. You will need to confirm (or correct) it before starting the build.",
    ),
    ("Compilation", "Build"),
    (
        "Projet {lang} non pris en charge",
        "Unsupported {lang} project",
    ),
    (
        "Cette archive contient un projet {lang} ; seuls {supported} sont pris en charge pour l'instant.",
        "This archive contains a {lang} project; only {supported} are supported for now.",
    ),
    ("Archive de sources", "Source archive"),
    (
        "Cette archive contient du code source mais aucun système de build reconnu (Cargo, CMake, Meson, Autotools, Makefile, Go, Python, Node.js, JAR).",
        "This archive contains source code but no recognized build system (Cargo, CMake, Meson, Autotools, Makefile, Go, Python, Node.js, JAR).",
    ),
    ("AppImage détecté", "AppImage detected"),
    (
        "L'archive contient un AppImage. Ce format est déjà autonome ; il ne peut pas être converti directement.",
        "The archive contains an AppImage. This format is already self-contained; it cannot be converted directly.",
    ),
    (
        "Les fichiers .jar de cette archive n'ont pas de classe principale (Main-Class) : ce sont des bibliothèques, pas un programme qu'on peut lancer.",
        "The .jar files in this archive have no main class (Main-Class): they are libraries, not a program that can be run.",
    ),
    ("Aucun exécutable trouvé", "No executable found"),
    (
        "Impossible de repérer un programme exécutable dans cette archive.",
        "Could not find an executable program in this archive.",
    ),
    (
        "Archive non prévue pour être convertie",
        "Archive not meant to be converted",
    ),
    (
        "Le seul programme trouvé est « {name} », qui ressemble à un script d'installation ou de compilation, pas à une application.",
        "The only program found is \"{name}\", which looks like an installation or build script, not an application.",
    ),
    // Constats : exécutable principal
    ("Exécutable principal", "Main executable"),
    ("{file} ({kind})", "{file} ({kind})"),
    (
        "{n} autres exécutables détectés",
        "{n} other executables detected",
    ),
    (
        "Au besoin, choisissez-en un autre en mode avancé (ex. : {others}).",
        "If needed, choose another one in advanced mode (e.g. {others}).",
    ),
    ("Architecture incompatible", "Incompatible architecture"),
    (
        "Le binaire est compilé pour {arch} mais cette machine est en {host}.",
        "The binary is built for {arch} but this machine is {host}.",
    ),
    ("Binaire 32 bits", "32-bit binary"),
    (
        "Les binaires 32 bits nécessitent l'extension de compatibilité i386 ; non prise en charge.",
        "32-bit binaries require the i386 compatibility extension, which is not supported.",
    ),
    ("Dépendances non analysées", "Dependencies not analyzed"),
    (
        "L'en-tête ELF n'a pas pu être lu entièrement.",
        "The ELF header could not be read completely.",
    ),
    ("Interpréteur à vérifier", "Interpreter to check"),
    (
        "Le script utilise « {interpreter} ». Cet interpréteur doit exister dans le runtime ou être embarqué dans l'archive.",
        "The script uses \"{interpreter}\". This interpreter must exist in the runtime or be bundled in the archive.",
    ),
    // Constats : bibliothèques et type d'application
    (
        "Bibliothèques possiblement manquantes",
        "Possibly missing libraries",
    ),
    (
        "Non embarquées et absentes du runtime standard (analyse heuristique) : {libs}.",
        "Not bundled and absent from the standard runtime (heuristic analysis): {libs}.",
    ),
    (
        "Application Electron détectée",
        "Electron application detected",
    ),
    (
        "Le bac à sable Chromium ne fonctionne pas sans composant supplémentaire ; l'application sera lancée avec --no-sandbox (le bac à sable Flatpak reste actif).",
        "The Chromium sandbox does not work without an extra component; the application will be launched with --no-sandbox (the Flatpak sandbox stays active).",
    ),
    ("Application graphique", "Graphical application"),
    (
        "Accès à l'affichage et à l'accélération graphique proposés par défaut.",
        "Access to the display and graphics acceleration is offered by default.",
    ),
    (
        "Application en ligne de commande probable",
        "Probably a command-line application",
    ),
    (
        "Aucune bibliothèque graphique détectée : l'entrée de menu s'ouvrira dans un terminal.",
        "No graphics library detected: the menu entry will open in a terminal.",
    ),
    ("Contenu", "Contents"),
    (
        "{count} fichiers, {size} Mio décompressés.",
        "{count} files, {size} MiB decompressed.",
    ),
];
