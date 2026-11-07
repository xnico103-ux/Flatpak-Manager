//! Reconnaissance du type d'un fichier d'après ses premiers octets, et des bibliothèques.

use super::model::{Arch, ExecKind};
use std::io::Read;

pub(super) fn sniff_exec(head: &[u8]) -> Option<ExecKind> {
    if head.len() >= 20 && &head[..4] == b"\x7fELF" {
        let bits = if head[4] == 2 { 64 } else { 32 };
        let machine = if head[5] == 2 {
            u16::from_be_bytes([head[18], head[19]])
        } else {
            u16::from_le_bytes([head[18], head[19]])
        };
        let arch = match machine {
            62 => Arch::X86_64,
            183 => Arch::Aarch64,
            m => Arch::Other(m),
        };
        // Les objets partagés (ET_DYN) peuvent être des exécutables PIE : on les garde.
        return Some(ExecKind::Elf { arch, bits });
    }
    if head.starts_with(b"#!") {
        let line = String::from_utf8_lossy(&head[2..]);
        let line = line.lines().next().unwrap_or("").trim();
        let mut it = line.split_whitespace();
        let first = it.next()?;
        let interp = if first.ends_with("/env") {
            it.next().unwrap_or(first)
        } else {
            first
        };
        return Some(ExecKind::Script {
            interpreter: interp.to_string(),
        });
    }
    None
}

/// Lit un membre précis de l'archive en mémoire (borné).
/// Vrai si le JAR déclare un `Main-Class` dans META-INF/MANIFEST.MF ; None si le fichier n'a pas pu être lu.
pub(super) fn jar_has_main_class(data: &[u8]) -> Option<bool> {
    let u16le = |o: usize| {
        data.get(o..o + 2)
            .map(|b| u16::from_le_bytes([b[0], b[1]]) as usize)
    };
    let u32le = |o: usize| {
        data.get(o..o + 4)
            .map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]) as usize)
    };
    // Fin du répertoire central : signature PK\x05\x06 dans les 64 Kio finaux.
    let tail_start = data.len().saturating_sub(65_557);
    let eocd = (tail_start..data.len().checked_sub(22)? + 1)
        .rev()
        .find(|&i| data[i..].starts_with(&[0x50, 0x4b, 5, 6]))?;
    let entries = u16le(eocd + 10)?;
    let mut off = u32le(eocd + 16)?;
    for _ in 0..entries {
        if data.get(off..off + 4)? != [0x50, 0x4b, 1, 2] {
            return None;
        }
        let (method, csize) = (u16le(off + 10)?, u32le(off + 20)?);
        let (nlen, elen, clen) = (u16le(off + 28)?, u16le(off + 30)?, u16le(off + 32)?);
        let lho = u32le(off + 42)?;
        let name = data.get(off + 46..off + 46 + nlen)?;
        if name.eq_ignore_ascii_case(b"META-INF/MANIFEST.MF") {
            if data.get(lho..lho + 4)? != [0x50, 0x4b, 3, 4] {
                return None;
            }
            let start = lho + 30 + u16le(lho + 26)? + u16le(lho + 28)?;
            let comp = data.get(start..start + csize)?;
            let mut text = String::new();
            match method {
                0 => text = String::from_utf8_lossy(comp).into_owned(),
                8 => {
                    flate2::read::DeflateDecoder::new(comp)
                        .take(1 << 20)
                        .read_to_string(&mut text)
                        .ok()?;
                }
                _ => return None,
            }
            return Some(text.lines().any(|l| {
                l.trim_start()
                    .to_ascii_lowercase()
                    .starts_with("main-class:")
                    && l.split(':').nth(1).is_some_and(|v| !v.trim().is_empty())
            }));
        }
        off += 46 + nlen + elen + clen;
    }
    Some(false)
}

pub(super) fn is_gui_lib(l: &str) -> bool {
    [
        "libX11",
        "libxcb",
        "libwayland-client",
        "libGL",
        "libEGL",
        "libgtk",
        "libQt5Gui",
        "libQt6Gui",
        "libSDL2",
        "libglfw",
        "libvulkan",
    ]
    .iter()
    .any(|p| l.starts_with(p))
}

/// Bibliothèques présentes dans org.freedesktop.Platform (liste heuristique).
pub(super) fn is_runtime_lib(l: &str) -> bool {
    const P: &[&str] = &[
        "libc.so",
        "libm.so",
        "libdl.so",
        "libpthread.so",
        "librt.so",
        "libutil.so",
        "libresolv.so",
        "libnsl.so",
        "libgcc_s",
        "libstdc++",
        "ld-linux",
        "libz.so",
        "libbz2",
        "liblzma",
        "libX11",
        "libXext",
        "libXrender",
        "libXi.so",
        "libXcursor",
        "libXrandr",
        "libXfixes",
        "libXcomposite",
        "libXdamage",
        "libXtst",
        "libXss",
        "libXinerama",
        "libxcb",
        "libGL",
        "libEGL",
        "libOpenGL",
        "libgbm",
        "libdrm",
        "libasound",
        "libpulse",
        "libfontconfig",
        "libfreetype",
        "libharfbuzz",
        "libglib-2.0",
        "libgobject-2.0",
        "libgio-2.0",
        "libgmodule-2.0",
        "libgthread-2.0",
        "libdbus-1",
        "libwayland-",
        "libxkbcommon",
        "libssl",
        "libcrypto",
        "libcups",
        "libnss3",
        "libnspr4",
        "libnssutil3",
        "libsmime3",
        "libexpat",
        "libpng16",
        "libjpeg",
        "libuuid",
        "libffi",
        "libpcre2",
        "libudev",
        "libsystemd",
        "libatomic",
        "libcurl",
        "libvulkan",
        "libxshmfence",
        "libxkbfile",
        "libcairo",
        "libpango",
        "libpangocairo",
        "libpangoft2",
        "libgdk_pixbuf",
        "libatk-1.0",
        "libatk-bridge",
        "libatspi",
        "libepoxy",
        "libsndfile",
        "libogg",
        "libvorbis",
        "libopus",
        "libSDL2",
    ];
    P.iter().any(|p| l.starts_with(p))
}
