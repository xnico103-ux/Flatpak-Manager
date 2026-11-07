use super::paths::guess_name_version;
use super::sniff::jar_has_main_class;
use super::*;
use flate2::{Compression, write::GzEncoder};
use std::fs::File;
use std::io::Write;
use std::path::PathBuf;

fn elf_header(machine: u16) -> Vec<u8> {
    let mut v = vec![0u8; 128];
    v[..4].copy_from_slice(b"\x7fELF");
    v[4] = 2;
    v[5] = 1;
    v[18..20].copy_from_slice(&machine.to_le_bytes());
    v
}

fn build(dir: &Path, name: &str, f: impl FnOnce(&mut tar::Builder<GzEncoder<File>>)) -> PathBuf {
    let p = dir.join(name);
    let enc = GzEncoder::new(File::create(&p).unwrap(), Compression::fast());
    let mut b = tar::Builder::new(enc);
    f(&mut b);
    b.into_inner().unwrap().finish().unwrap().flush().unwrap();
    p
}

fn add(b: &mut tar::Builder<GzEncoder<File>>, path: &str, data: &[u8], mode: u32) {
    let mut h = tar::Header::new_gnu();
    h.set_size(data.len() as u64);
    h.set_mode(mode);
    h.set_entry_type(tar::EntryType::Regular);
    b.append_data(&mut h, path, data).unwrap();
}

fn host_machine() -> u16 {
    if std::env::consts::ARCH == "aarch64" {
        183
    } else {
        62
    }
}

#[test]
fn simple_native_app() {
    let d = tempfile::tempdir().unwrap();
    let p = build(d.path(), "monappli-1.2.3-linux-x64.tar.gz", |b| {
        add(
            b,
            "monappli-1.2.3/monappli",
            &elf_header(host_machine()),
            0o755,
        );
        add(b, "monappli-1.2.3/README.txt", b"hello", 0o644);
        add(b, "monappli-1.2.3/monappli.desktop", b"[Desktop Entry]\nName=Mon Appli\nExec=monappli %U\nIcon=monappli\nCategories=Utility;\n", 0o644);
    });
    let i = analyze(&p).unwrap();
    assert!(i.is_compatible(), "{:?}", i.findings);
    assert_eq!(i.top_dir.as_deref(), Some("monappli-1.2.3"));
    assert_eq!(i.main().unwrap().rel, "monappli");
    assert_eq!(i.guessed_version, "1.2.3");
    assert_eq!(i.kind, AppKind::Native);
    assert_eq!(
        i.desktop.as_ref().unwrap().name.as_deref(),
        Some("Mon Appli")
    );
}

#[test]
fn path_traversal_is_blocked() {
    let d = tempfile::tempdir().unwrap();
    let p = build(d.path(), "evil.tar.gz", |b| {
        let data = elf_header(host_machine());
        let mut h = tar::Header::new_gnu();
        h.set_size(data.len() as u64);
        h.set_mode(0o755);
        h.set_entry_type(tar::EntryType::Regular);
        let name = b"../../evil";
        h.as_old_mut().name[..name.len()].copy_from_slice(name);
        h.set_cksum();
        b.append(&h, &data[..]).unwrap();
    });
    let i = analyze(&p).unwrap();
    assert!(!i.is_compatible());
    assert!(i.blockers().any(|f| f.title.contains("dangereux")));
}

#[test]
fn escaping_symlink_is_blocked() {
    let d = tempfile::tempdir().unwrap();
    let p = build(d.path(), "link.tar.gz", |b| {
        let mut h = tar::Header::new_gnu();
        h.set_entry_type(tar::EntryType::Symlink);
        h.set_size(0);
        h.set_mode(0o777);
        b.append_link(&mut h, "app/evil", "../../etc").unwrap();
        add(b, "app/evil/passwd", b"x", 0o644);
        add(b, "app/run", &elf_header(host_machine()), 0o755);
    });
    let i = analyze(&p).unwrap();
    assert!(i.blockers().any(|f| f.title.contains("dangereux")));
}

#[test]
fn makefile_project_is_a_source_build() {
    let d = tempfile::tempdir().unwrap();
    let p = build(d.path(), "src.tar.gz", |b| {
        add(b, "proj/Makefile", b"TARGET = hello\nall:\n", 0o644);
        add(b, "proj/main.c", b"int main(){}", 0o644);
    });
    let i = analyze(&p).unwrap();
    assert_eq!(i.kind, AppKind::Source);
    assert!(i.is_compatible());
    assert!(i.executables.is_empty());
    let s = i.source.unwrap();
    assert_eq!(
        (s.system, s.bin_name.as_str(), s.bin_certain),
        (crate::toolchains::BuildSystem::Make, "hello", true)
    );
}

#[test]
fn rust_project_is_a_source_build() {
    let d = tempfile::tempdir().unwrap();
    let p = build(d.path(), "outil-1.2.0.tar.gz", |b| {
        add(
            b,
            "outil-1.2.0/Cargo.toml",
            b"[package]\nname = \"outil\"\nversion = \"1.2.0\"\n",
            0o644,
        );
        add(b, "outil-1.2.0/src/main.rs", b"fn main(){}", 0o644);
        // un Cargo.toml de sous-dossier ne compte pas
        add(
            b,
            "outil-1.2.0/vendor/x/Cargo.toml",
            b"[package]\nname = \"x\"\n",
            0o644,
        );
    });
    let i = analyze(&p).unwrap();
    assert!(i.is_compatible());
    let s = i.source.as_ref().unwrap();
    assert_eq!(s.bin_name, "outil");
    assert_eq!(i.strip_components(), 1);
    let cfg = crate::manifest::BuildConfig::from_info(&i);
    let m = cfg.manifest(&i, "icon.png", "256x256");
    assert_eq!(
        m["sdk-extensions"][0],
        "org.freedesktop.Sdk.Extension.rust-stable"
    );
    assert_eq!(m["command"], "outil");
    assert_eq!(
        m["modules"][0]["build-options"]["build-args"][0],
        "--share=network"
    );
    assert_eq!(m["modules"].as_array().unwrap().len(), 2);
}

#[test]
fn git_archive_pax_header_is_not_a_root_file() {
    let d = tempfile::tempdir().unwrap();
    let p = build(d.path(), "outil-1.2.0.tar.gz", |b| {
        let mut h = tar::Header::new_ustar();
        h.set_entry_type(tar::EntryType::XGlobalHeader);
        h.set_path("pax_global_header").unwrap();
        h.set_size(0);
        h.set_mode(0o666);
        h.set_cksum();
        b.append(&h, &[][..]).unwrap();
        add(
            b,
            "outil-1.2.0/Cargo.toml",
            b"[package]\nname = \"outil\"\nversion = \"1.2.0\"\n",
            0o644,
        );
        add(b, "outil-1.2.0/src/main.rs", b"fn main(){}", 0o644);
    });
    let i = analyze(&p).unwrap();
    assert_eq!(i.strip_components(), 1);
    assert!(
        i.source.is_some(),
        "l'archive doit être reconnue comme un projet Rust"
    );
}

#[test]
fn unsupported_or_unknown_sources_are_blocked() {
    let d = tempfile::tempdir().unwrap();
    let p = build(d.path(), "js.tar.gz", |b| {
        add(b, "proj/package.json", b"{}", 0o644)
    });
    assert!(!analyze(&p).unwrap().is_compatible());
    let p = build(d.path(), "rb.tar.gz", |b| {
        add(b, "proj/Gemfile", b"source 'x'", 0o644)
    });
    let i = analyze(&p).unwrap();
    assert!(!i.is_compatible());
    assert!(i.blockers().any(|f| f.title.contains("Ruby")));
    let p = build(d.path(), "mvn.tar.gz", |b| {
        add(b, "proj/pom.xml", b"<project/>", 0o644)
    });
    assert!(
        analyze(&p)
            .unwrap()
            .blockers()
            .any(|f| f.title.contains("Maven"))
    );
    let p = build(d.path(), "c.tar.gz", |b| {
        add(b, "proj/main.c", b"int main(){}", 0o644)
    });
    assert!(!analyze(&p).unwrap().is_compatible());
}

#[test]
fn prebuilt_elf_with_cmakelists_stays_native() {
    let d = tempfile::tempdir().unwrap();
    let p = build(d.path(), "mix.tar.gz", |b| {
        add(b, "app/CMakeLists.txt", b"project(x)\n", 0o644);
        add(b, "app/run", &elf_header(host_machine()), 0o755);
    });
    let i = analyze(&p).unwrap();
    assert_eq!(i.kind, AppKind::Native);
    assert!(i.source.is_none());
}

#[test]
fn wrong_arch_is_blocked() {
    let d = tempfile::tempdir().unwrap();
    let other = if host_machine() == 62 { 183 } else { 62 };
    let p = build(d.path(), "arm.tar.gz", |b| {
        add(b, "app/run", &elf_header(other), 0o755)
    });
    let i = analyze(&p).unwrap();
    assert!(i.blockers().any(|f| f.title.contains("Architecture")));
}

#[test]
fn script_app_is_detected() {
    let d = tempfile::tempdir().unwrap();
    let p = build(d.path(), "tool.tar.gz", |b| {
        add(b, "tool/tool", b"#!/usr/bin/env bash\necho hi\n", 0o755)
    });
    let i = analyze(&p).unwrap();
    assert_eq!(i.kind, AppKind::Script);
    assert!(i.is_compatible());
}

fn tar_bytes() -> Vec<u8> {
    let mut b = tar::Builder::new(Vec::new());
    let data = elf_header(host_machine());
    let mut h = tar::Header::new_gnu();
    h.set_size(data.len() as u64);
    h.set_mode(0o755);
    h.set_entry_type(tar::EntryType::Regular);
    b.append_data(&mut h, "app/run", &data[..]).unwrap();
    b.into_inner().unwrap()
}

#[test]
fn root_directory_entry_is_stripped() {
    // Les archives créées par `tar` contiennent une entrée pour le dossier racine.
    let d = tempfile::tempdir().unwrap();
    let p = build(d.path(), "tool-3.1.tar.gz", |b| {
        let mut h = tar::Header::new_gnu();
        h.set_entry_type(tar::EntryType::Directory);
        h.set_size(0);
        h.set_mode(0o755);
        b.append_data(&mut h, "tool-3.1/", &[][..]).unwrap();
        add(b, "tool-3.1/tool", &elf_header(host_machine()), 0o755);
    });
    let i = analyze(&p).unwrap();
    assert_eq!(i.top_dir.as_deref(), Some("tool-3.1"));
    assert_eq!(i.strip_components(), 1);
    assert_eq!(i.main().unwrap().rel, "tool");
}

#[test]
fn xz_and_bzip2_archives_are_supported() {
    let d = tempfile::tempdir().unwrap();
    let raw = tar_bytes();
    let mut x = xz2::write::XzEncoder::new(Vec::new(), 1);
    x.write_all(&raw).unwrap();
    let px = d.path().join("app-1.0.tar.xz");
    std::fs::write(&px, x.finish().unwrap()).unwrap();
    let mut z = bzip2::write::BzEncoder::new(Vec::new(), bzip2::Compression::fast());
    z.write_all(&raw).unwrap();
    let pb = d.path().join("app-1.0.tar.bz2");
    std::fs::write(&pb, z.finish().unwrap()).unwrap();
    for p in [px, pb] {
        let i = analyze(&p).unwrap();
        assert!(i.is_compatible(), "{:?}", i.findings);
        assert_eq!(i.main().unwrap().rel, "run");
        assert_eq!(i.guessed_name, "app");
    }
}

#[test]
fn not_gzip_is_rejected() {
    let d = tempfile::tempdir().unwrap();
    let p = d.path().join("x.tar.gz");
    std::fs::write(&p, b"not a gzip").unwrap();
    let i = analyze(&p).unwrap();
    assert!(!i.is_compatible());
}

#[test]
fn name_version_guess() {
    assert_eq!(
        guess_name_version("Foo-Bar_2.4.1_linux_x64.tar.gz"),
        ("Foo Bar".into(), "2.4.1".into())
    );
    assert_eq!(
        guess_name_version("tool.tgz"),
        ("tool".into(), "1.0".into())
    );
}

#[test]
fn rust_library_archive_is_refused() {
    let d = tempfile::tempdir().unwrap();
    let p = build(d.path(), "lib.tar.gz", |b| {
        add(
            b,
            "mylib/Cargo.toml",
            b"[package]\nname = \"mylib\"\nversion = \"0.1.0\"\n",
            0o644,
        );
        add(b, "mylib/src/lib.rs", b"pub fn f() {}", 0o644);
    });
    let i = analyze(&p).unwrap();
    assert!(!i.is_compatible());
    assert!(i.blockers().any(|f| f.title.contains("Aucun programme")));
}

#[test]
fn installer_script_archive_is_refused() {
    let d = tempfile::tempdir().unwrap();
    let p = build(d.path(), "inst.tar.gz", |b| {
        add(b, "pkg/install.sh", b"#!/bin/sh\necho hi\n", 0o755);
        add(b, "pkg/data.bin", b"zzzz", 0o644);
    });
    let i = analyze(&p).unwrap();
    assert!(!i.is_compatible());
}

/// Petit JAR (méthode « stockée ») contenant seulement un MANIFEST.MF.
fn tiny_jar(manifest: &str) -> Vec<u8> {
    let name = b"META-INF/MANIFEST.MF";
    let data = manifest.as_bytes();
    let mut z: Vec<u8> = Vec::new();
    let le16 = |v: usize| (v as u16).to_le_bytes();
    let le32 = |v: usize| (v as u32).to_le_bytes();
    z.extend([0x50, 0x4b, 3, 4]);
    z.extend([20, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]);
    z.extend(le32(data.len()));
    z.extend(le32(data.len()));
    z.extend(le16(name.len()));
    z.extend([0, 0]);
    z.extend(name);
    z.extend(data);
    let cd = z.len();
    z.extend([
        0x50, 0x4b, 1, 2, 20, 0, 20, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
    ]);
    z.extend(le32(data.len()));
    z.extend(le32(data.len()));
    z.extend(le16(name.len()));
    z.extend([0; 12]);
    z.extend(le32(0));
    z.extend(name);
    let cd_len = z.len() - cd;
    z.extend([0x50, 0x4b, 5, 6, 0, 0, 0, 0, 1, 0, 1, 0]);
    z.extend(le32(cd_len));
    z.extend(le32(cd));
    z.extend([0, 0]);
    z
}

#[test]
fn jar_main_class_detection() {
    assert_eq!(
        jar_has_main_class(&tiny_jar("Manifest-Version: 1.0\nMain-Class: app.Main\n")),
        Some(true)
    );
    assert_eq!(
        jar_has_main_class(&tiny_jar("Manifest-Version: 1.0\n")),
        Some(false)
    );
    assert_eq!(jar_has_main_class(b"pas un zip"), None);
}

#[test]
fn java_jar_with_main_class_is_supported() {
    let d = tempfile::tempdir().unwrap();
    let p = build(d.path(), "monjeu-1.0.tar.gz", |b| {
        add(
            b,
            "monjeu-1.0/monjeu.jar",
            &tiny_jar("Main-Class: a.B\n"),
            0o644,
        );
        add(
            b,
            "monjeu-1.0/lib/dep.jar",
            &tiny_jar("Manifest-Version: 1.0\n"),
            0o644,
        );
    });
    let i = analyze(&p).unwrap();
    assert!(i.is_compatible(), "{:?}", i.findings);
    let s = i.source.as_ref().unwrap();
    assert_eq!(s.system, crate::toolchains::BuildSystem::Java);
    assert_eq!(s.entry, crate::toolchains::Entry::File("monjeu.jar".into()));
    let cfg = crate::manifest::BuildConfig::from_info(&i);
    let launcher = cfg.launcher_script().unwrap();
    assert!(launcher.contains("/app/jre/bin/java -jar '/app/opt/monjeu/monjeu.jar'"));
    let m = cfg.manifest(&i, "icon.png", "256x256");
    assert_eq!(
        m["sdk-extensions"][0],
        "org.freedesktop.Sdk.Extension.openjdk25"
    );
}

#[test]
fn java_library_jar_is_blocked() {
    let d = tempfile::tempdir().unwrap();
    let p = build(d.path(), "lib.tar.gz", |b| {
        add(
            b,
            "lib/lib.jar",
            &tiny_jar("Manifest-Version: 1.0\n"),
            0o644,
        );
    });
    let i = analyze(&p).unwrap();
    assert!(!i.is_compatible());
}

#[test]
fn python_project_with_console_script() {
    let d = tempfile::tempdir().unwrap();
    let p = build(d.path(), "monoutil-2.0.tar.gz", |b| {
        add(b, "monoutil-2.0/pyproject.toml", b"[build-system]\nrequires=[\"setuptools\"]\n[project]\nname=\"monoutil\"\ndependencies=[\"requests\"]\n[project.scripts]\nmonoutil-cli = \"monoutil.cli:main\"\n", 0o644);
        add(
            b,
            "monoutil-2.0/monoutil/cli.py",
            b"def main(): pass",
            0o644,
        );
    });
    let i = analyze(&p).unwrap();
    assert!(i.is_compatible(), "{:?}", i.findings);
    let s = i.source.as_ref().unwrap();
    assert_eq!(
        s.entry,
        crate::toolchains::Entry::Console("monoutil-cli".into())
    );
    assert!(s.installable && s.network);
    let cfg = crate::manifest::BuildConfig::from_info(&i);
    assert!(cfg.perms.network);
    let m = cfg.manifest(&i, "icon.png", "256x256");
    let cmds = m["modules"][0]["build-commands"].to_string();
    assert!(
        cmds.contains("pip3 install --prefix=/app/opt/monoutil"),
        "{cmds}"
    );
    assert!(
        cfg.launcher_script()
            .unwrap()
            .contains("/app/opt/monoutil/bin/monoutil-cli")
    );
    assert!(m.get("sdk-extensions").is_none());
}

#[test]
fn python_loose_script_and_library() {
    let d = tempfile::tempdir().unwrap();
    let p = build(d.path(), "tool.tar.gz", |b| {
        add(b, "tool/main.py", b"print('hi')", 0o644);
        add(b, "tool/requirements.txt", b"pygobject\n", 0o644);
        add(b, "tool/Makefile", b"test:\n", 0o644);
    });
    let i = analyze(&p).unwrap();
    assert!(i.is_compatible(), "{:?}", i.findings);
    let s = i.source.as_ref().unwrap();
    assert_eq!(
        (s.system, s.entry.clone(), s.requirements, s.gtk),
        (
            crate::toolchains::BuildSystem::Python,
            crate::toolchains::Entry::File("main.py".into()),
            true,
            true
        )
    );
    let cfg = crate::manifest::BuildConfig::from_info(&i);
    assert_eq!(cfg.runtime, crate::manifest::RuntimeKind::Gnome);
    let p = build(d.path(), "lib.tar.gz", |b| {
        add(b, "lib/pyproject.toml", b"[project]\nname=\"lib\"\n", 0o644);
        add(b, "lib/lib/__init__.py", b"", 0o644);
    });
    assert!(!analyze(&p).unwrap().is_compatible());
}

#[test]
fn node_project_entry_and_electron() {
    let d = tempfile::tempdir().unwrap();
    let p = build(d.path(), "srv.tar.gz", |b| {
        add(b, "srv/package.json", b"{\"name\":\"@me/srv\",\"bin\":{\"srv\":\"./cli.js\"},\"scripts\":{\"build\":\"tsc\"},\"dependencies\":{\"express\":\"4\"}}", 0o644);
        add(b, "srv/cli.js", b"#!/usr/bin/env node", 0o755);
    });
    let i = analyze(&p).unwrap();
    assert!(i.is_compatible(), "{:?}", i.findings);
    let s = i.source.as_ref().unwrap();
    assert_eq!(
        (
            s.bin_name.as_str(),
            s.entry.clone(),
            s.build_script,
            s.network
        ),
        (
            "srv",
            crate::toolchains::Entry::File("cli.js".into()),
            true,
            true
        )
    );
    let cfg = crate::manifest::BuildConfig::from_info(&i);
    let m = cfg.manifest(&i, "icon.png", "256x256");
    let cmds = m["modules"][0]["build-commands"].to_string();
    assert!(
        cmds.contains("npm run build") && cmds.contains("install.sh"),
        "{cmds}"
    );
    assert_eq!(
        m["sdk-extensions"][0],
        "org.freedesktop.Sdk.Extension.node24"
    );
    let p = build(d.path(), "el.tar.gz", |b| {
        add(
            b,
            "el/package.json",
            b"{\"name\":\"el\",\"main\":\"main.js\",\"devDependencies\":{\"electron\":\"30\"}}",
            0o644,
        );
        add(b, "el/main.js", b"", 0o644);
    });
    assert!(!analyze(&p).unwrap().is_compatible());
}
