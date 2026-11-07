//! Projets Java (JAR exécutable).

use super::helpers::*;
use super::model::*;
use crate::manifest::slugify;

pub(super) fn java(jars: &[String], fallback: &str) -> SourceProject {
    let slug = slugify(fallback);
    let mut sorted = jars.to_vec();
    sorted.sort_by_key(|p| {
        (
            p.matches('/').count(),
            !p.to_ascii_lowercase().contains(&slug),
            p.len(),
            p.clone(),
        )
    });
    let main = sorted[0].clone();
    let (bin, _) = pick(Some(slug), fallback);
    let mut p = project(BuildSystem::Java, (bin, true), "", &[]);
    // Swing, JavaFX et SWT sont des bibliothèques de fenêtres : l'écran est accordé par défaut, à retirer pour un outil en ligne de commande.
    p.gui = true;
    p.entry = Entry::File(main);
    if sorted.len() > 1 {
        p.alternatives = sorted;
    }
    p.notes.push(tr!("Application graphique supposée : pour un outil en ligne de commande, cochez « Ouvrir dans un terminal » en mode avancé.").into());
    p
}
