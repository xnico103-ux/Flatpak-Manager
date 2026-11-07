//! Traductions anglaises : ui_chrome (accueil, barre latérale, composants communs,
//! gestion, mises à jour, historique, thème, démarrage, messages du gestionnaire).

pub const ENTRIES: &[(&str, &str)] = &[
    // Barre latérale
    ("Accueil", "Home"),
    ("Mises à jour ({n})", "Updates ({n})"),
    ("Mises à jour", "Updates"),
    ("Gérer les Flatpak", "Manage Flatpaks"),
    ("Historique ({n})", "History ({n})"),
    ("Paramètres", "Settings"),
    ("Version {version}", "Version {version}"),
    ("Mode avancé", "Advanced mode"),
    (
        "Détails et réglages fins",
        "Details and fine-grained options",
    ),
    ("Mode avancé activé", "Advanced mode on"),
    ("Mode avancé désactivé", "Advanced mode off"),
    // Accueil
    ("Que voulez-vous faire ?", "What would you like to do?"),
    (
        "Créez ou installez une application Flatpak en quelques clics, sans terminal.",
        "Create or install a Flatpak application in a few clicks, no terminal needed.",
    ),
    (
        "Flatpak n'est pas détecté sur ce système",
        "Flatpak was not detected on this system",
    ),
    (
        "Installez le paquet « flatpak » avec votre gestionnaire de paquets (ex. : sudo pacman -S flatpak, sudo apt install flatpak, sudo dnf install flatpak), puis relancez Flatpak Manager.",
        "Install the \"flatpak\" package with your package manager (e.g. sudo pacman -S flatpak, sudo apt install flatpak, sudo dnf install flatpak), then restart Flatpak Manager.",
    ),
    (
        "Flatpak Builder sera installé automatiquement",
        "Flatpak Builder will be installed automatically",
    ),
    (
        "Au premier usage, l'outil de construction (org.flatpak.Builder) sera téléchargé depuis Flathub pour votre utilisateur.",
        "On first use, the build tool (org.flatpak.Builder) will be downloaded from Flathub for your user.",
    ),
    ("Impossible d'ouvrir ce fichier", "Unable to open this file"),
    ("Fermer", "Close"),
    ("Créer un Flatpak", "Create a Flatpak"),
    (
        "Choisissez une archive (.tar.gz, .tar.xz…) : elle est analysée, puis convertie en fichier .flatpak.",
        "Choose an archive (.tar.gz, .tar.xz…): it is analyzed, then converted into a .flatpak file.",
    ),
    ("Installer un Flatpak", "Install a Flatpak"),
    (
        "Sélectionnez un fichier .flatpak existant pour l'installer sur ce système.",
        "Select an existing .flatpak file to install it on this system.",
    ),
    ("Créer et installer", "Create and install"),
    (
        "Tout en une fois : création du paquet depuis l'archive, puis installation automatique.",
        "All in one go: the package is built from the archive, then installed automatically.",
    ),
    ("Ce qui peut être converti", "What can be converted"),
    (
        "Binaires précompilés et scripts",
        "Precompiled binaries and scripts",
    ),
    (
        "Archives contenant un programme déjà prêt à l'emploi (ELF ou script), avec ou sans bibliothèques embarquées.",
        "Archives containing a ready-to-use program (ELF or script), with or without bundled libraries.",
    ),
    (
        "Projets Rust, C/C++, Go, Python, Node.js et Java",
        "Rust, C/C++, Go, Python, Node.js and Java projects",
    ),
    (
        "Compilés ou préparés dans le SDK Flatpak, dépendances comprises. Si le projet ne produit aucun programme, ou si son nom est incertain, l'analyse vous le dit avant toute compilation.",
        "Built or prepared in the Flatpak SDK, dependencies included. If the project produces no program, or if its name is uncertain, the analysis tells you before any build.",
    ),
    (
        "AppImage, Electron, Maven/Gradle, Ruby, PHP",
        "AppImage, Electron, Maven/Gradle, Ruby, PHP",
    ),
    (
        "Non pris en charge pour l'instant : l'analyse l'indiquera clairement avant toute construction.",
        "Not supported yet: the analysis will say so clearly before any build.",
    ),
    (
        "Le glisser-déposer n'est pas disponible sous Wayland natif : utilisez « Parcourir » ou collez le chemin ci-dessous.",
        "Drag and drop is not available on native Wayland: use \"Browse\" or paste the path below.",
    ),
    ("Astuce", "Tip"),
    ("Ou saisir un chemin", "Or enter a path"),
    ("/chemin/vers/archive.tar.gz", "/path/to/archive.tar.gz"),
    ("Ouvrir", "Open"),
    ("Fichier introuvable : {path}", "File not found: {path}"),
    (
        "Utile si le sélecteur de fichiers ne s'ouvre pas sur votre environnement de bureau.",
        "Useful if the file picker does not open on your desktop environment.",
    ),
    ("Activité récente", "Recent activity"),
    ("Analyse de l'archive…", "Analyzing the archive…"),
    (
        "Contrôles de sécurité, recherche de l'exécutable et des dépendances.",
        "Security checks, looking for the executable and its dependencies.",
    ),
    // Composants communs (widgets)
    ("Environnement de compilation", "Build environment"),
    ("détecté dans l'archive", "detected in the archive"),
    ("Analyse de votre système…", "Analyzing your system…"),
    ("Revérifier", "Check again"),
    ("Flatpak est introuvable", "Flatpak not found"),
    (
        "Installez le paquet « flatpak » avec votre gestionnaire de paquets, puis cliquez sur « Revérifier ».",
        "Install the \"flatpak\" package with your package manager, then click \"Check again\".",
    ),
    ("Sur votre machine", "On your machine"),
    (
        "non trouvé (sans importance : la compilation utilise le SDK Flatpak)",
        "not found (not important: the build uses the Flatpak SDK)",
    ),
    ("Composants Flatpak requis", "Required Flatpak components"),
    ("{reference} — installé", "{reference} — installed"),
    ("{reference} — manquant", "{reference} — missing"),
    (
        "Tout est prêt pour compiler.",
        "Everything is ready to build.",
    ),
    ("Commande qui sera exécutée", "Command that will be run"),
    (
        "« -y » évite que Flatpak pose la question dans un terminal. Seul l'utilisateur courant est concerné (--user).",
        "\"-y\" stops Flatpak from asking the question in a terminal. Only the current user is affected (--user).",
    ),
    ("Annuler", "Cancel"),
    ("Installer maintenant", "Install now"),
    ("Copier la commande", "Copy command"),
    ("Ménage des anciens SDK", "Clean up old SDKs"),
    (
        "{n} ancienne(s) version(s) installée(s) pour vous, devenues inutiles car une version plus récente est présente.",
        "{n} old version(s) installed for you, no longer needed because a newer version is present.",
    ),
    ("Faire le ménage…", "Clean up…"),
    (
        "⚠ Ces commandes suppriment des composants (réinstallables depuis Flathub) :",
        "⚠ These commands remove components (they can be reinstalled from Flathub):",
    ),
    (
        "La 2ᵉ commande retire aussi tout runtime qu'aucune application installée n'utilise. Les applications installées ne sont pas touchées.",
        "The 2nd command also removes any runtime that no installed application uses. Installed applications are not affected.",
    ),
    ("Supprimer maintenant", "Delete now"),
    (
        "Permission manquante : l'application tourne dans un Flatpak",
        "Missing permission: the application is running inside a Flatpak",
    ),
    (
        "Pour gérer les Flatpak de votre machine, elle doit être autorisée à lancer des commandes sur l'hôte. Copiez cette commande dans un terminal, puis relancez l'application :",
        "To manage the Flatpaks on your machine, it must be allowed to run commands on the host. Copy this command into a terminal, then restart the application:",
    ),
    ("Copier", "Copy"),
    (
        "Commande copiée dans le presse-papiers",
        "Command copied to the clipboard",
    ),
    ("Échec", "Failed"),
    // Gérer les Flatpak
    (
        "Applications et composants installés sur cette machine. La désinstallation est définitive.",
        "Applications and components installed on this machine. Uninstalling is permanent.",
    ),
    ("Rechercher…", "Search…"),
    ("Afficher les runtimes et SDK", "Show runtimes and SDKs"),
    ("Actualiser", "Refresh"),
    (
        "Retirer les composants inutilisés…",
        "Remove unused components…",
    ),
    ("Désinstaller {name} ?", "Uninstall {name}?"),
    (
        "Retirer les runtimes qu'aucune application n'utilise ?",
        "Remove the runtimes that no application uses?",
    ),
    (
        "Commande destructive : elle sera exécutée telle quelle.",
        "Destructive command: it will be run as is.",
    ),
    (
        "Installation système : une demande d'authentification peut s'afficher.",
        "System-wide installation: an authentication prompt may appear.",
    ),
    (
        "Supprimer aussi les données de l'application (--delete-data)",
        "Also delete the application's data (--delete-data)",
    ),
    (
        "Lecture des paquets installés…",
        "Reading installed packages…",
    ),
    ("utilisateur", "user"),
    ("application en cours", "application running"),
    ("Désinstaller…", "Uninstall…"),
    ("Aucun paquet à afficher.", "No packages to show."),
    ("Annulation demandée…", "Cancellation requested…"),
    // Mises à jour
    (
        "Mettez à jour une application ou tout d'un coup. Les commandes lancées sont affichées dans le journal.",
        "Update a single application or everything at once. The commands that are run are shown in the log.",
    ),
    ("Tout mettre à jour ({n})", "Update all ({n})"),
    ("Rechercher", "Search"),
    ("Afficher les runtimes", "Show runtimes"),
    ("Recherche des mises à jour…", "Checking for updates…"),
    ("Mettre à jour", "Update"),
    ("Tout est à jour.", "Everything is up to date."),
    (
        "Aucune application à mettre à jour ({hidden} runtime(s) disponible(s) : cochez « Afficher les runtimes »).",
        "No applications to update ({hidden} runtime(s) available: tick \"Show runtimes\").",
    ),
    // Historique
    ("Historique", "History"),
    (
        "Les paquets créés et installés avec Flatpak Manager.",
        "The packages created and installed with Flatpak Manager.",
    ),
    (
        "Rien pour l'instant : vos opérations apparaîtront ici.",
        "Nothing yet: your operations will appear here.",
    ),
    ("Installer", "Install"),
    ("Ouvrir le dossier", "Open folder"),
    ("Fichier introuvable", "File not found"),
    ("Retirer de la liste", "Remove from list"),
    (
        "Ne supprime pas le fichier .flatpak",
        "Does not delete the .flatpak file",
    ),
    ("Retiré de l'historique", "Removed from history"),
    ("Créé", "Created"),
    ("Installé", "Installed"),
    ("Créé et installé", "Created and installed"),
    // Thème
    ("sensible", "sensitive"),
    ("à noter", "notable"),
    // Démarrage et sandbox
    (
        "Flatpak Manager fonctionne uniquement sous Wayland (WAYLAND_DISPLAY est absent). Si l'application tourne en Flatpak, il lui manque la permission --socket=wayland.",
        "Flatpak Manager only works on Wayland (WAYLAND_DISPLAY is not set). If the application is running as a Flatpak, it is missing the --socket=wayland permission.",
    ),
    (
        "Impossible d'ouvrir la fenêtre : {e}",
        "Unable to open the window: {e}",
    ),
    ("<identifiant de l'application>", "<application id>"),
    // Gestionnaire
    (
        "impossible de lancer flatpak : {e}",
        "unable to run flatpak: {e}",
    ),
    (
        "flatpak n'a pas pu lister les applications installées.",
        "flatpak could not list the installed applications.",
    ),
    (
        "Impossible de chercher les mises à jour (connexion à Flathub ?).",
        "Unable to check for updates (connection to Flathub?).",
    ),
];
