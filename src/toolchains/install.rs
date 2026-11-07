//! Installation des composants manquants, en tâche de fond.

use super::components::shell_line;
use crate::diagnose::diagnose;
use crate::pipeline::{parse_percent, pump};
use std::os::unix::process::CommandExt;
use std::process::{Command, Stdio};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, channel};
use std::time::Duration;

pub enum ToolMsg {
    Line(String),
    Progress(f32),
    Done(Result<(), String>),
}

pub struct ToolInstall {
    pub rx: Receiver<ToolMsg>,
    pub cancel: Arc<AtomicBool>,
}

fn explain(tail: &[String]) -> String {
    if let Some(d) = diagnose(tail).into_iter().next() {
        return tr!(
            "{titre} : {explication}",
            titre = d.title,
            explication = d.explanation
        );
    }
    tail.iter()
        .rev()
        .find(|l| !l.trim().is_empty())
        .cloned()
        .unwrap_or_else(|| tr!("L'installation a échoué.").into())
}

/// Exécute les commandes l'une après l'autre dans un thread.
pub fn spawn_install(cmds: Vec<Vec<String>>, notify: Arc<dyn Fn() + Send + Sync>) -> ToolInstall {
    let (tx, rx) = channel();
    let cancel = Arc::new(AtomicBool::new(false));
    let flag = cancel.clone();
    std::thread::spawn(move || {
        let mut tail: Vec<String> = Vec::new();
        let res = (|| -> Result<(), String> {
            for argv in &cmds {
                let _ = tx.send(ToolMsg::Line(format!("$ {}", shell_line(argv))));
                notify();
                let mut cmd = crate::host::cmd(&argv[0]);
                cmd.args(&argv[1..])
                    .env("LC_ALL", "C")
                    .env("LANG", "C")
                    .stdin(Stdio::null())
                    .stdout(Stdio::piped())
                    .stderr(Stdio::piped())
                    .process_group(0);
                let mut child = cmd.spawn().map_err(|e| {
                    tr!(
                        "impossible de lancer {programme} : {erreur}",
                        programme = argv[0],
                        erreur = e
                    )
                })?;
                let (ltx, lrx) = channel::<String>();
                let mut readers = Vec::new();
                if let Some(o) = child.stdout.take() {
                    readers.push(pump(o, ltx.clone()));
                }
                if let Some(e) = child.stderr.take() {
                    readers.push(pump(e, ltx.clone()));
                }
                drop(ltx);
                let pid = child.id();
                loop {
                    match lrx.recv_timeout(Duration::from_millis(120)) {
                        Ok(line) => {
                            tail.push(line.clone());
                            if let Some(p) = parse_percent(&line) {
                                let _ = tx.send(ToolMsg::Progress(p));
                            } else {
                                let _ = tx.send(ToolMsg::Line(line));
                            }
                            notify();
                        }
                        Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {}
                        Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => break,
                    }
                    if flag.load(Ordering::SeqCst) {
                        let _ = Command::new("kill")
                            .args(["-TERM", "--", &format!("-{pid}")])
                            .status();
                        let _ = child.kill();
                        break;
                    }
                }
                for r in readers {
                    let _ = r.join();
                }
                let status = child.wait().map_err(|e| e.to_string())?;
                if flag.load(Ordering::SeqCst) {
                    return Err(tr!("Opération annulée.").into());
                }
                if !status.success() {
                    return Err(explain(&tail));
                }
            }
            Ok(())
        })();
        let _ = tx.send(ToolMsg::Done(res));
        notify();
    });
    ToolInstall { rx, cancel }
}
