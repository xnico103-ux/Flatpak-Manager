//! Lecture d'une archive : décompression, empreinte SHA-256, extraction d'un membre.

use anyhow::{Result, anyhow};
use flate2::read::GzDecoder;
use sha2::{Digest, Sha256};
use std::fs::File;
use std::io::{BufReader, Read};
use std::path::Path;

/// Ouvre un flux décompressé selon les octets magiques (gzip, xz, zstd, bzip2).
pub fn open_decoder(path: &Path) -> Result<Option<Box<dyn Read>>> {
    let mut magic = [0u8; 6];
    let n = File::open(path)?.read(&mut magic)?;
    let m = &magic[..n];
    let file =
        || -> Result<BufReader<File>> { Ok(BufReader::with_capacity(1 << 20, File::open(path)?)) };
    Ok(if m.starts_with(&[0x1f, 0x8b]) {
        Some(Box::new(GzDecoder::new(file()?)))
    } else if m.starts_with(&[0xfd, b'7', b'z', b'X', b'Z', 0x00]) {
        Some(Box::new(xz2::read::XzDecoder::new(file()?)))
    } else if m.starts_with(&[0x28, 0xb5, 0x2f, 0xfd]) {
        let d = ruzstd::decoding::StreamingDecoder::new(file()?)
            .map_err(|e| anyhow!(tr!("archive zstd invalide : {e}", e = e)))?;
        Some(Box::new(d))
    } else if m.starts_with(b"BZh") {
        Some(Box::new(bzip2::read::BzDecoder::new(file()?)))
    } else {
        None
    })
}

pub fn sha256_file(path: &Path) -> Result<String> {
    let mut f = BufReader::with_capacity(1 << 20, File::open(path)?);
    let mut h = Sha256::new();
    let mut buf = vec![0u8; 1 << 20];
    loop {
        let n = f.read(&mut buf)?;
        if n == 0 {
            break;
        }
        h.update(&buf[..n]);
    }
    Ok(h.finalize().iter().map(|b| format!("{b:02x}")).collect())
}

pub fn read_member(archive: &Path, raw: &str, max: u64) -> Result<Vec<u8>> {
    let mut ar = tar::Archive::new(
        open_decoder(archive)?.ok_or_else(|| anyhow!(tr!("format d'archive non reconnu")))?,
    );
    for e in ar.entries()? {
        let mut e = e?;
        let p = e
            .path()?
            .to_string_lossy()
            .trim_start_matches("./")
            .to_string();
        if p == raw && e.header().entry_type().is_file() {
            let mut v = Vec::new();
            (&mut e).take(max).read_to_end(&mut v)?;
            return Ok(v);
        }
    }
    Err(anyhow!(tr!(
        "fichier introuvable dans l'archive : {raw}",
        raw = raw
    )))
}
