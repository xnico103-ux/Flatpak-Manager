use super::*;
use crate::manifest::{FDO_VERSION, RuntimeKind};

fn bf(name: &str, content: &str) -> BuildFile {
    BuildFile {
        name: name.into(),
        content: content.into(),
    }
}

#[test]
fn cargo_package_and_bin() {
    let f = bf(
        "cargo.toml",
        "[package]\nname = \"mon-outil\"\nversion = \"1.0.0\"\n[dependencies]\neframe = \"0.30\"\n",
    );
    let Some(Detection::Supported(p)) = detect(
        &[f],
        &[],
        false,
        "x",
        &["src/main.rs".to_string(), "main.go".to_string()],
    ) else {
        panic!()
    };
    assert_eq!(p.system, BuildSystem::Cargo);
    assert_eq!(p.bin_name, "mon-outil");
    assert!(p.bin_certain && p.gui);
    let f = bf(
        "cargo.toml",
        "[package]\nname = \"lib-name\"\n[[bin]]\nname = \"cli\"\npath = \"src/main.rs\"\n",
    );
    let Some(Detection::Supported(p)) = detect(
        &[f],
        &[],
        false,
        "x",
        &["src/main.rs".to_string(), "main.go".to_string()],
    ) else {
        panic!()
    };
    assert_eq!(p.bin_name, "cli");
}

#[test]
fn cargo_workspace_is_uncertain() {
    let f = bf("cargo.toml", "[workspace]\nmembers = [\"a\", \"b\"]\n");
    let Some(Detection::Supported(p)) = detect(&[f], &[], false, "Mon Projet", &[]) else {
        panic!()
    };
    assert!(!p.bin_certain);
    assert_eq!(p.bin_name, "mon-projet");
    assert!(!p.notes.is_empty());
}

#[test]
fn cargo_workspace_finds_gui_member_and_usb() {
    let root = bf(
        "cargo.toml",
        "[workspace]\nmembers = [\"proto\", \"gui\"]\n",
    );
    let gui = bf(
        "member:cargo.toml",
        "[package]\nname = \"xblades-gui\"\n[[bin]]\nname = \"xblades-gui\"\npath = \"src/main.rs\"\n[dependencies]\neframe = \"0.29\"\n",
    );
    let src = [
        "proto/src/hidraw.rs".to_string(),
        "gui/src/main.rs".to_string(),
    ];
    let Some(Detection::Supported(p)) = detect(&[root, gui], &[], false, "x", &src) else {
        panic!()
    };
    assert!(p.gui && p.devices && p.bin_certain);
    assert_eq!(p.bin_name, "xblades-gui");
}

#[test]
fn cmake_and_meson_names() {
    let f = bf(
        "cmakelists.txt",
        "cmake_minimum_required(VERSION 3.10)\nproject(Demo)\nfind_package(Qt6 REQUIRED)\nadd_executable(demo main.cpp)\n",
    );
    let Some(Detection::Supported(p)) = detect(
        &[f],
        &[],
        false,
        "x",
        &["src/main.rs".to_string(), "main.go".to_string()],
    ) else {
        panic!()
    };
    assert_eq!(
        (p.system, p.bin_name.as_str(), p.bin_certain, p.qt),
        (BuildSystem::CMake, "demo", true, true)
    );
    let f = bf(
        "cmakelists.txt",
        "project(Demo)\nadd_executable(${PROJECT_NAME} main.cpp)\n",
    );
    let Some(Detection::Supported(p)) = detect(
        &[f],
        &[],
        false,
        "x",
        &["src/main.rs".to_string(), "main.go".to_string()],
    ) else {
        panic!()
    };
    assert_eq!((p.bin_name.as_str(), p.bin_certain), ("Demo", false));
    let f = bf(
        "meson.build",
        "project('viewer', 'c')\nexecutable('viewer-bin', 'main.c', dependencies: dependency('gtk4'))\n",
    );
    let Some(Detection::Supported(p)) = detect(
        &[f],
        &[],
        false,
        "x",
        &["src/main.rs".to_string(), "main.go".to_string()],
    ) else {
        panic!()
    };
    assert_eq!(
        (p.system, p.bin_name.as_str(), p.gtk),
        (BuildSystem::Meson, "viewer-bin", true)
    );
}

#[test]
fn autotools_make_go() {
    let f = bf("configure.ac", "AC_INIT([hello], [1.0])\n");
    let Some(Detection::Supported(p)) = detect(&[f], &["autogen.sh".into()], false, "x", &[])
    else {
        panic!()
    };
    assert_eq!(
        (p.system, p.bin_name.as_str()),
        (BuildSystem::Autotools, "hello")
    );
    let f = bf("makefile", "CC=gcc\nTARGET = tool\nall: $(TARGET)\n");
    let Some(Detection::Supported(p)) = detect(
        &[f],
        &[],
        false,
        "x",
        &["src/main.rs".to_string(), "main.go".to_string()],
    ) else {
        panic!()
    };
    assert_eq!(
        (p.system, p.bin_name.as_str(), p.bin_certain),
        (BuildSystem::Make, "tool", true)
    );
    let f = bf("go.mod", "module github.com/me/gotool/v2\n\ngo 1.22\n");
    let Some(Detection::Supported(p)) = detect(
        &[f],
        &[],
        false,
        "x",
        &["src/main.rs".to_string(), "main.go".to_string()],
    ) else {
        panic!()
    };
    assert_eq!((p.system, p.bin_name.as_str()), (BuildSystem::Go, "gotool"));
}

#[test]
fn prebuilt_or_script_archives_are_not_sources() {
    let f = bf("makefile", "all:\n");
    assert!(detect(std::slice::from_ref(&f), &[], true, "x", &[]).is_none());
    assert!(detect(&[f], &["run.sh".into()], false, "x", &[]).is_none());
    let f = bf("package.json", "{}");
    // package.json sans aucun point d'entrée : projet Node.js reconnu mais sans rien à lancer.
    let Some(Detection::Supported(p)) = detect(std::slice::from_ref(&f), &[], false, "x", &[])
    else {
        panic!()
    };
    assert!(p.blocker.is_some());
    assert!(detect(&[f], &["start.sh".into()], false, "x", &[]).is_none());
}

#[test]
fn branch_and_components() {
    let refs = parse_refs(
        "org.freedesktop.Platform\t24.08\norg.freedesktop.Sdk\t24.08\norg.freedesktop.Sdk\t23.08\norg.gnome.Platform\t49\n",
    );
    let st = FlatpakState {
        available: true,
        refs,
        has_flathub: true,
        remote: vec![],
        user_refs: vec![],
    };
    assert_eq!(pick_branch(&st, &[]), FDO_VERSION);
    let comps = components(BuildSystem::Cargo, RuntimeKind::Freedesktop, "24.08", &st);
    assert_eq!(comps.len(), 3);
    let miss = missing(&comps);
    assert_eq!(miss.len(), 1);
    assert_eq!(
        miss[0].reference(),
        "org.freedesktop.Sdk.Extension.rust-stable//24.08"
    );
    let plan = install_plan(&miss, true);
    assert_eq!(plan.len(), 1);
    assert_eq!(
        shell_line(&plan[0]),
        "flatpak install --user -y flathub org.freedesktop.Sdk.Extension.rust-stable//24.08"
    );
    let plan = install_plan(&miss, false);
    assert_eq!(plan.len(), 2);
    assert!(
        shell_line(&plan[0])
            .starts_with("flatpak remote-add --user --if-not-exists flathub https://")
    );
    assert_eq!(pick_branch(&FlatpakState::default(), &[]), FDO_VERSION);
    // C/C++ : pas d'extension
    assert_eq!(
        components(BuildSystem::CMake, RuntimeKind::Freedesktop, "25.08", &st).len(),
        2
    );
}

#[test]
fn newest_remote_branch_with_extension() {
    let mut st = FlatpakState {
        refs: parse_refs("org.freedesktop.Sdk\t24.08\n"),
        ..Default::default()
    };
    st.remote = parse_refs(
        "org.freedesktop.Platform\t26.08\norg.freedesktop.Sdk\t26.08\norg.freedesktop.Platform\t25.08\norg.freedesktop.Sdk\t25.08\norg.freedesktop.Sdk.Extension.rust-stable\t25.08\norg.freedesktop.Sdk\t25.08beta\n",
    );
    // 26.08 n'a pas l'extension Rust : on reste sur 25.08.
    assert_eq!(
        pick_branch(&st, &["org.freedesktop.Sdk.Extension.rust-stable"]),
        "25.08"
    );
    // Sans extension requise, la plus récente gagne.
    assert_eq!(pick_branch(&st, &[]), "26.08");
}

#[test]
fn cleanup_keeps_newest_and_selected() {
    let st = FlatpakState {
        user_refs: parse_refs(
            "org.freedesktop.Sdk\t23.08\norg.freedesktop.Sdk\t24.08\norg.freedesktop.Sdk\t25.08\norg.freedesktop.Sdk.Extension.rust-stable\t24.08\norg.freedesktop.Sdk.Extension.rust-stable\t25.08\norg.freedesktop.Platform\t24.08\norg.gnome.Sdk\t47\n",
        ),
        ..Default::default()
    };
    let c = cleanup_candidates(&st, "25.08");
    let refs: Vec<String> = c.iter().map(|(i, b)| format!("{i}//{b}")).collect();
    assert_eq!(
        refs,
        [
            "org.freedesktop.Sdk//23.08",
            "org.freedesktop.Sdk//24.08",
            "org.freedesktop.Sdk.Extension.rust-stable//24.08"
        ]
    );
    assert_eq!(cleanup_plan(&c).len(), 2);
    assert!(cleanup_candidates(&FlatpakState::default(), "25.08").is_empty());
}

#[test]
fn rust_library_is_blocked_and_go_without_main_is_uncertain() {
    let lib = bf(
        "cargo.toml",
        "[package]\nname = \"mylib\"\nversion = \"1.0.0\"\n",
    );
    let Some(Detection::Supported(p)) =
        detect(&[lib], &[], false, "x", &["src/lib.rs".to_string()])
    else {
        panic!()
    };
    assert!(p.blocker.is_some());
    let go = bf("go.mod", "module example.com/tool\n");
    let Some(Detection::Supported(p)) = detect(&[go], &[], false, "x", &["util.go".to_string()])
    else {
        panic!()
    };
    assert!(!p.bin_certain && p.blocker.is_none());
}
