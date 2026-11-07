//! Archives de sources : détection du langage et du système de build, composants
//! Flatpak nécessaires (runtime, SDK, extensions) et installation de ceux qui manquent.
//!
//! La compilation se fait toujours dans le bac à sable de flatpak-builder avec le
//! SDK Flatpak : les compilateurs installés sur la machine (rustc, gcc…) ne sont
//! affichés qu'à titre d'information.
//!
//! - `model` : types (système de build, point d'entrée, projet détecté)
//! - `detect` : choix du système de build ; `node`, `python`, `java`, `rust` : un fichier par langage
//! - `host_state` : ce qui est installé sur la machine ; `components` : ce qu'il faut ajouter
//! - `install` : installation des composants manquants

mod components;
mod detect;
mod helpers;
mod host_state;
mod install;
mod java;
mod model;
mod node;
mod python;
mod rust;
#[cfg(test)]
mod tests;

pub use components::*;
pub use detect::*;
pub use host_state::*;
pub use install::*;
pub use model::*;
