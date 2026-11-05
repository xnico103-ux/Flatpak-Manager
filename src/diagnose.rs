//! Explication des erreurs courantes de Flatpak / flatpak-builder.

#[derive(Debug, Clone)]
pub struct Diagnosis {
    pub title: String,
    pub explanation: String,
    pub fix: Option<String>,
}

fn d(title: &str, explanation: &str, fix: Option<&str>) -> Diagnosis {
    Diagnosis {
        title: title.into(),
        explanation: explanation.into(),
        fix: fix.map(String::from),
    }
}

/// Analyse les dernières lignes d'un journal et renvoie les diagnostics reconnus.
pub fn diagnose(lines: &[String]) -> Vec<Diagnosis> {
    let text = lines.join("\n").to_lowercase();
    let has = |s: &str| text.contains(s);
    let mut out = Vec::new();

    if has("no space left on device") {
        out.push(d("Disque plein", "Il n'y a plus assez d'espace pour construire ou installer le paquet.", Some("Libérez de la place (le dossier ~/.cache/flatpak-manager peut être vidé) puis recommencez.")));
    }
    let network = has("could not resolve host")
        || has("unable to connect")
        || has("network is unreachable")
        || has("temporary failure in name resolution")
        || has("can't load uri")
        || has("while fetching")
        || has("failure when receiving data")
        || has("connect tunnel failed");
    if network {
        out.push(d("Flathub est injoignable", "Le runtime Flatpak doit être téléchargé depuis Flathub (https://dl.flathub.org), mais la connexion a échoué.", Some("Vérifiez votre connexion Internet (et votre pare-feu ou proxy), puis recommencez.")));
    } else if has("no remote refs found") || has("nothing matches") && has("flathub") {
        out.push(d(
            "Runtime introuvable sur Flathub",
            "La version de runtime demandée n'existe pas (ou plus) sur Flathub.",
            Some("Choisissez une autre version de runtime en mode avancé."),
        ));
    }
    if has("no such remote 'flathub'")
        || has("remote \"flathub\" not found")
        || has("no remote named")
    {
        out.push(d("Dépôt Flathub absent", "Le dépôt Flathub n'est pas configuré sur ce système.", Some("flatpak remote-add --user --if-not-exists flathub https://dl.flathub.org/repo/flathub.flatpakrepo")));
    }
    if has("rofiles-fuse") || has("fusermount") || has("fuse: device not found") {
        out.push(d("FUSE indisponible", "flatpak-builder utilise FUSE (rofiles-fuse) qui n'est pas utilisable ici. L'application a réessayé sans.", Some("Installez fuse3 si le problème persiste.")));
    }
    if has("already installed") {
        out.push(d("Application déjà installée", "Une version de cette application est déjà présente.", Some("Utilisez l'option de remplacement dans les paramètres avancés, ou désinstallez-la d'abord.")));
    }
    if has("permission denied") || has("not allowed") && has("polkit") {
        out.push(d("Permission refusée", "Une opération a été refusée (fichier ou autorisation système).", Some("Pour une installation système, validez la demande d'authentification ; sinon installez pour l'utilisateur courant.")));
    }
    if has("sha256") && (has("mismatch") || has("not match") || has("invalid checksum")) {
        out.push(d(
            "Archive modifiée pendant la construction",
            "L'empreinte de l'archive ne correspond plus à celle analysée.",
            Some("Recommencez sans modifier l'archive."),
        ));
    }
    if has("is not a valid") && has("name")
        || has("invalid id")
        || has("not a valid application id")
    {
        out.push(d(
            "Identifiant invalide",
            "L'identifiant d'application ne respecte pas le format attendu.",
            Some("Utilisez un identifiant de type org.exemple.MonApp (lettres, chiffres et _)."),
        ));
    }
    if has("command not found") || has("no such file or directory") && has("build-commands") {
        out.push(d(
            "Fichier introuvable dans la construction",
            "Un fichier attendu n'a pas été trouvé dans l'archive.",
            Some("Vérifiez l'exécutable choisi en mode avancé."),
        ));
    }
    if (has("cargo: command not found")
        || has("go: command not found")
        || has("cmake: command not found")
        || has("meson: command not found")
        || has("rustc: command not found"))
        || has("sdk-extension") && has("not installed")
    {
        out.push(d("Extension du SDK manquante", "Le compilateur du langage n'est pas disponible dans le SDK Flatpak.", Some("Installez l'extension depuis la carte « Environnement de compilation », par exemple : flatpak install --user flathub org.freedesktop.Sdk.Extension.rust-stable//25.08")));
    }
    if has("is not supported by the following packages") || has("requires rustc") {
        out.push(d("Compilateur Rust trop ancien", "Le SDK Flatpak utilisé fournit une version de Rust trop ancienne pour les dépendances du projet.", Some("Choisissez la branche la plus récente du SDK (25.08 ou plus) en mode avancé, puis installez l'extension : flatpak install --user flathub org.freedesktop.Sdk.Extension.rust-stable//25.08")));
    }
    if has("could not find a bin") || has("a bin target must be available") {
        out.push(d(
            "Aucun programme à compiler",
            "Le projet ne produit pas de programme exécutable (bibliothèque seule ?).",
            Some("Vérifiez que l'archive contient bien une application et non une bibliothèque."),
        ));
    }
    if has("runtime") && has("not installed") {
        out.push(d(
            "Runtime manquant",
            "Le runtime nécessaire n'est pas installé.",
            Some("Il est normalement installé automatiquement ; vérifiez la connexion à Flathub."),
        ));
    }
    if has("flatpak-builder: ") && has("not found") {
        out.push(d(
            "Flatpak Builder absent",
            "L'outil flatpak-builder n'est pas disponible.",
            Some("flatpak install --user flathub org.flatpak.Builder"),
        ));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn recognises_disk_full() {
        let l = vec!["error: Writing file: No space left on device".to_string()];
        assert_eq!(diagnose(&l)[0].title, "Disque plein");
    }
    #[test]
    fn unreachable_flathub_is_not_reported_as_missing_runtime() {
        // Lignes réelles observées avec flatpak 1.14 sans accès réseau.
        let l = vec![
            "error: Can't load uri https://dl.flathub.org/repo/flathub.flatpakrepo: While fetching https://dl.flathub.org/repo/flathub.flatpakrepo: [56] Failure when receiving data from the peer".to_string(),
            "error: No remote refs found for ?flathub?".to_string(),
        ];
        let d = diagnose(&l);
        assert_eq!(d.len(), 1);
        assert_eq!(d[0].title, "Flathub est injoignable");
    }
    #[test]
    fn nothing_for_unknown() {
        assert!(diagnose(&["tout va bien".to_string()]).is_empty());
    }
}
