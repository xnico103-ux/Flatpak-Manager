//! Traductions anglaises : moteur (diagnostics, étapes, journal et vérifications de la construction).

pub const ENTRIES: &[(&str, &str)] = &[
    ("Disque plein", "Disk full"),
    (
        "Il n'y a plus assez d'espace pour construire ou installer le paquet.",
        "There is not enough space left to build or install the package.",
    ),
    (
        "Libérez de la place (le dossier ~/.cache/flatpak-manager peut être vidé) puis recommencez.",
        "Free up some space (the ~/.cache/flatpak-manager folder can be emptied), then try again.",
    ),
    ("Flathub est injoignable", "Flathub is unreachable"),
    (
        "Le runtime Flatpak doit être téléchargé depuis Flathub (https://dl.flathub.org), mais la connexion a échoué.",
        "The Flatpak runtime must be downloaded from Flathub (https://dl.flathub.org), but the connection failed.",
    ),
    (
        "Vérifiez votre connexion Internet (et votre pare-feu ou proxy), puis recommencez.",
        "Check your Internet connection (and your firewall or proxy), then try again.",
    ),
    (
        "Runtime introuvable sur Flathub",
        "Runtime not found on Flathub",
    ),
    (
        "La version de runtime demandée n'existe pas (ou plus) sur Flathub.",
        "The requested runtime version does not exist (or no longer exists) on Flathub.",
    ),
    (
        "Choisissez une autre version de runtime en mode avancé.",
        "Choose a different runtime version in advanced mode.",
    ),
    ("Dépôt Flathub absent", "Flathub repository missing"),
    (
        "Le dépôt Flathub n'est pas configuré sur ce système.",
        "The Flathub repository is not configured on this system.",
    ),
    ("FUSE indisponible", "FUSE unavailable"),
    (
        "flatpak-builder utilise FUSE (rofiles-fuse) qui n'est pas utilisable ici. L'application a réessayé sans.",
        "flatpak-builder uses FUSE (rofiles-fuse), which cannot be used here. The application retried without it.",
    ),
    (
        "Installez fuse3 si le problème persiste.",
        "Install fuse3 if the problem persists.",
    ),
    (
        "Application déjà installée",
        "Application already installed",
    ),
    (
        "Une version de cette application est déjà présente.",
        "A version of this application is already present.",
    ),
    (
        "Utilisez l'option de remplacement dans les paramètres avancés, ou désinstallez-la d'abord.",
        "Use the replace option in the advanced settings, or uninstall it first.",
    ),
    ("Permission refusée", "Permission denied"),
    (
        "Une opération a été refusée (fichier ou autorisation système).",
        "An operation was denied (file or system authorization).",
    ),
    (
        "Pour une installation système, validez la demande d'authentification ; sinon installez pour l'utilisateur courant.",
        "For a system-wide installation, approve the authentication request; otherwise install for the current user.",
    ),
    (
        "Archive modifiée pendant la construction",
        "Archive modified during the build",
    ),
    (
        "L'empreinte de l'archive ne correspond plus à celle analysée.",
        "The archive's checksum no longer matches the one that was analyzed.",
    ),
    (
        "Recommencez sans modifier l'archive.",
        "Try again without modifying the archive.",
    ),
    ("Identifiant invalide", "Invalid identifier"),
    (
        "L'identifiant d'application ne respecte pas le format attendu.",
        "The application ID does not follow the expected format.",
    ),
    (
        "Utilisez un identifiant de type org.exemple.MonApp (lettres, chiffres et _).",
        "Use an ID of the form org.example.MyApp (letters, digits and _).",
    ),
    (
        "Fichier introuvable dans la construction",
        "File not found during the build",
    ),
    (
        "Un fichier attendu n'a pas été trouvé dans l'archive.",
        "An expected file was not found in the archive.",
    ),
    (
        "Vérifiez l'exécutable choisi en mode avancé.",
        "Check the executable chosen in advanced mode.",
    ),
    ("Extension du SDK manquante", "SDK extension missing"),
    (
        "Le compilateur du langage n'est pas disponible dans le SDK Flatpak.",
        "The language compiler is not available in the Flatpak SDK.",
    ),
    (
        "Installez l'extension depuis la carte « Environnement de compilation », par exemple : flatpak install --user flathub org.freedesktop.Sdk.Extension.rust-stable//25.08",
        "Install the extension from the \"Build environment\" card, for example: flatpak install --user flathub org.freedesktop.Sdk.Extension.rust-stable//25.08",
    ),
    ("Compilateur Rust trop ancien", "Rust compiler too old"),
    (
        "Le SDK Flatpak utilisé fournit une version de Rust trop ancienne pour les dépendances du projet.",
        "The Flatpak SDK in use provides a Rust version that is too old for the project's dependencies.",
    ),
    (
        "Choisissez la branche la plus récente du SDK (25.08 ou plus) en mode avancé, puis installez l'extension : flatpak install --user flathub org.freedesktop.Sdk.Extension.rust-stable//25.08",
        "Choose the most recent SDK branch (25.08 or later) in advanced mode, then install the extension: flatpak install --user flathub org.freedesktop.Sdk.Extension.rust-stable//25.08",
    ),
    ("Aucun programme à compiler", "No program to build"),
    (
        "Le projet ne produit pas de programme exécutable (bibliothèque seule ?).",
        "The project does not produce an executable program (library only?).",
    ),
    (
        "Vérifiez que l'archive contient bien une application et non une bibliothèque.",
        "Make sure the archive contains an application and not a library.",
    ),
    ("Runtime manquant", "Runtime missing"),
    (
        "Le runtime nécessaire n'est pas installé.",
        "The required runtime is not installed.",
    ),
    (
        "Il est normalement installé automatiquement ; vérifiez la connexion à Flathub.",
        "It is normally installed automatically; check the connection to Flathub.",
    ),
    ("Flatpak Builder absent", "Flatpak Builder missing"),
    (
        "L'outil flatpak-builder n'est pas disponible.",
        "The flatpak-builder tool is not available.",
    ),
    ("Préparation", "Preparation"),
    ("Construction", "Build"),
    ("Export", "Export"),
    ("Vérification", "Verification"),
    ("Installation", "Installation"),
    ("Runtime", "Runtime"),
    ("Opération annulée.", "Operation cancelled."),
    (
        "Installation non confirmée : le paquet a été créé mais pas installé.",
        "Installation not confirmed: the package was created but not installed.",
    ),
    (
        "Le fichier n'est pas un paquet Flatpak valide.",
        "The file is not a valid Flatpak package.",
    ),
    (
        "Flatpak n'est pas installé sur ce système. Installez le paquet « flatpak » avec votre gestionnaire de paquets, puis relancez l'opération.",
        "Flatpak is not installed on this system. Install the \"flatpak\" package with your package manager, then run the operation again.",
    ),
    (
        "flatpak-builder est absent : installation de org.flatpak.Builder depuis Flathub (pour l'utilisateur courant).",
        "flatpak-builder is missing: installing org.flatpak.Builder from Flathub (for the current user).",
    ),
    (
        "Impossible d'installer Flatpak Builder (code {code}).",
        "Unable to install Flatpak Builder (code {code}).",
    ),
    (
        "Version de runtime résolue : {plateforme} {version}",
        "Runtime version resolved: {plateforme} {version}",
    ),
    (
        "Version de runtime non résolue, valeur par défaut : {version}",
        "Runtime version not resolved, using default: {version}",
    ),
    (
        "L'archive n'est pas convertible (voir l'analyse).",
        "The archive cannot be converted (see the analysis).",
    ),
    (
        "Ajout du dépôt Flathub si nécessaire…",
        "Adding the Flathub repository if needed…",
    ),
    ("dossier de cache introuvable", "cache folder not found"),
    ("Dossier de travail : {chemin}", "Working folder: {chemin}"),
    (
        "Nettoyage incomplet de {chemin} : {erreur}",
        "Incomplete cleanup of {chemin}: {erreur}",
    ),
    (
        "Manifeste généré : {chemin}",
        "Manifest generated: {chemin}",
    ),
    (
        "FUSE indisponible : nouvelle tentative sans rofiles-fuse.",
        "FUSE unavailable: retrying without rofiles-fuse.",
    ),
    (
        "La construction a échoué (flatpak-builder, code {code}).",
        "The build failed (flatpak-builder, code {code}).",
    ),
    (
        "La création du fichier .flatpak a échoué (code {code}).",
        "Creating the .flatpak file failed (code {code}).",
    ),
    (
        "La vérification du paquet a échoué : il n'a pas été enregistré.",
        "Package verification failed: it was not saved.",
    ),
    (
        "écriture de {chemin} impossible : {erreur}",
        "cannot write {chemin}: {erreur}",
    ),
    ("Paquet enregistré : {chemin}", "Package saved: {chemin}"),
    (
        "L'application est déjà installée (already installed). Activez le remplacement dans les paramètres avancés pour la mettre à jour.",
        "The application is already installed (already installed). Enable replacement in the advanced settings to update it.",
    ),
    (
        "Application déjà installée : elle sera réinstallée (vos données dans ~/.var/app sont conservées).",
        "Application already installed: it will be reinstalled (your data in ~/.var/app is kept).",
    ),
    (
        "Déjà installée : nouvelle tentative en réinstallation (données conservées).",
        "Already installed: retrying as a reinstall (data kept).",
    ),
    (
        "L'installation a échoué (code {code}).",
        "The installation failed (code {code}).",
    ),
    (
        "L'installation semble avoir réussi mais l'application est introuvable (flatpak info).",
        "The installation seems to have succeeded but the application cannot be found (flatpak info).",
    ),
    (
        "Application installée et vérifiée : {id}",
        "Application installed and verified: {id}",
    ),
    (
        "impossible de lancer {programme} : {erreur}",
        "unable to launch {programme}: {erreur}",
    ),
    (
        "« {nom} » existe dans /app/bin mais n'est ni un programme compilé ni un script : la compilation n'a pas produit ce qui était attendu.",
        "\"{nom}\" exists in /app/bin but is neither a compiled program nor a script: the build did not produce what was expected.",
    ),
    (
        "Programme compilé présent : /app/bin/{nom}",
        "Compiled program present: /app/bin/{nom}",
    ),
    (
        "Le programme « {nom} » n'a pas été installé dans /app/bin : la compilation a réussi mais ce nom est probablement incorrect. Corrigez le champ « Exécutable » en mode avancé.",
        "The program \"{nom}\" was not installed in /app/bin: the build succeeded but this name is probably wrong. Fix the \"Executable\" field in advanced mode.",
    ),
    (
        "Le point d'entrée « {nom} » est introuvable dans le paquet : choisissez le bon fichier dans « Point d'entrée ».",
        "The entry point \"{nom}\" was not found in the package: choose the right file in \"Entry point\".",
    ),
    (
        "Script de lancement présent : /app/bin/{slug}",
        "Launch script present: /app/bin/{slug}",
    ),
    (
        "Script de lancement manquant ou non exécutable : /app/bin/{slug}",
        "Launch script missing or not executable: /app/bin/{slug}",
    ),
    (
        "Exécutable présent : {chemin}",
        "Executable present: {chemin}",
    ),
    (
        "Exécutable introuvable dans le paquet : {chemin}",
        "Executable not found in the package: {chemin}",
    ),
    ("Entrée de menu exportée", "Menu entry exported"),
    (
        "Entrée de menu valide (desktop-file-validate)",
        "Menu entry valid (desktop-file-validate)",
    ),
    (
        "desktop-file-validate signale des avertissements",
        "desktop-file-validate reports warnings",
    ),
    ("Entrée de menu non exportée", "Menu entry not exported"),
    ("Icône exportée", "Icon exported"),
    ("Icône non exportée", "Icon not exported"),
    ("Le fichier .flatpak est vide", "The .flatpak file is empty"),
    ("Extension .flatpak", "Extension .flatpak"),
    (
        "L'extension n'est pas .flatpak",
        "The extension is not .flatpak",
    ),
    ("Le fichier est vide", "The file is empty"),
    (
        "En-tête de paquet Flatpak reconnu",
        "Flatpak package header recognized",
    ),
    (
        "En-tête non reconnu : Flatpak validera le fichier à l'installation",
        "Header not recognized: Flatpak will validate the file at installation",
    ),
    (
        "Fichier .flatpak créé ({taille} Mio)",
        "File .flatpak created ({taille} MiB)",
    ),
    ("Taille : {taille} Mio", "Size: {taille} MiB"),
    (
        "lecture de {chemin} : {erreur}",
        "reading {chemin}: {erreur}",
    ),
];
