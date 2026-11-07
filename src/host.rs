//! Exécution des commandes sur la machine hôte.
//!
//! Hors bac à sable, les commandes sont lancées directement. Quand l'application tourne
//! elle-même dans un Flatpak, `flatpak` et `flatpak-builder` ne sont pas visibles : on passe
//! alors par `flatpak-spawn --host`, ce qui exige la permission `org.freedesktop.Flatpak`.

use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::OnceLock;

/// Vrai si l'application s'exécute dans un bac à sable Flatpak.
pub fn in_sandbox() -> bool {
    static S: OnceLock<bool> = OnceLock::new();
    *S.get_or_init(|| Path::new("/.flatpak-info").exists())
}

/// Identifiant de l'application Flatpak courante (lu dans /.flatpak-info).
pub fn sandbox_app_id() -> Option<String> {
    static ID: OnceLock<Option<String>> = OnceLock::new();
    ID.get_or_init(|| {
        let t = std::fs::read_to_string("/.flatpak-info").ok()?;
        t.lines()
            .find_map(|l| l.strip_prefix("name="))
            .map(|s| s.trim().to_string())
    })
    .clone()
}

/// Exécute une commande en lui laissant au plus `secs` secondes (None si elle dépasse ou ne démarre pas).
pub fn output_timeout(mut c: Command, secs: u64) -> Option<std::process::Output> {
    use std::io::Read;
    let mut child = c
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;
    let mut stdout = child.stdout.take()?;
    let reader = std::thread::spawn(move || {
        let mut buf = Vec::new();
        let _ = stdout.read_to_end(&mut buf);
        buf
    });
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(secs);
    let status = loop {
        match child.try_wait() {
            Ok(Some(s)) => break s,
            Ok(None) if std::time::Instant::now() < deadline => {
                std::thread::sleep(std::time::Duration::from_millis(40))
            }
            _ => {
                let _ = child.kill();
                let _ = child.wait();
                return None;
            }
        }
    };
    Some(std::process::Output {
        status,
        stdout: reader.join().unwrap_or_default(),
        stderr: Vec::new(),
    })
}

/// Commande à lancer sur l'hôte.
pub fn cmd(prog: &str) -> Command {
    cmd_inner(prog)
}

fn cmd_inner(prog: &str) -> Command {
    if in_sandbox() {
        let mut c = Command::new("flatpak-spawn");
        // --watch-bus : la commande côté hôte est arrêtée si l'application se ferme ou annule.
        c.args([
            "--host",
            "--watch-bus",
            "--env=LC_ALL=C",
            "--env=LANG=C",
            prog,
        ]);
        c
    } else {
        Command::new(prog)
    }
}

/// Some(identifiant) si l'application est dans un bac à sable qui n'a pas le droit d'appeler l'hôte.
pub fn sandbox_blocked() -> Option<String> {
    static BLOCKED: OnceLock<Option<String>> = OnceLock::new();
    BLOCKED
        .get_or_init(|| {
            if !in_sandbox() {
                return None;
            }
            // Délai maximal : un portail qui ne répond pas ne doit pas empêcher la fenêtre de s'ouvrir.
            let mut probe = Command::new("flatpak-spawn");
            probe.args(["--host", "true"]);
            let ok = output_timeout(probe, 5).is_some_and(|o| o.status.success());
            if ok {
                None
            } else {
                Some(
                    sandbox_app_id()
                        .unwrap_or_else(|| tr!("<identifiant de l'application>").into()),
                )
            }
        })
        .clone()
}

/// Commande à copier pour accorder la permission manquante.
pub fn override_command(app_id: &str) -> String {
    format!("flatpak override --user --talk-name=org.freedesktop.Flatpak {app_id}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn timeout_kills_slow_commands() {
        let mut slow = Command::new("sleep");
        slow.arg("5");
        let t = std::time::Instant::now();
        assert!(output_timeout(slow, 1).is_none());
        assert!(t.elapsed().as_secs() < 3);
        let mut fast = Command::new("echo");
        fast.arg("ok");
        let o = output_timeout(fast, 5).unwrap();
        assert!(o.status.success() && String::from_utf8_lossy(&o.stdout).trim() == "ok");
    }
}
