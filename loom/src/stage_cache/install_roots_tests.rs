//! Toolchains installed below HOME: a `PATH` entry's install root and its
//! symlink targets are read roots, on the shared computation every launcher
//! uses, and the generated policy grants them. Only recognised install
//! layouts count; projects, hub directories, HOME and its direct children,
//! credential directories and anything a writable root can redirect stay out.

use super::contract_tests::policy::{granted, policy_for};
use super::contract_tests::{stage, temp, write_file, HostEnvGuard};
use super::roots::toolchain_read_roots_in;
use std::collections::BTreeMap;
use std::ffi::OsString;
use std::os::unix::fs::symlink;
use std::path::{Path, PathBuf};

/// The roots below `home` alone, no system-wide install root passed, with
/// `writable` as the command's writable roots.
fn roots_beside(home: &Path, path: &[PathBuf], writable: &[&Path]) -> Vec<PathBuf> {
    let path = std::env::join_paths(path).unwrap();
    let env = BTreeMap::from([
        ("HOME".to_string(), OsString::from(home)),
        ("PATH".to_string(), path),
    ]);
    toolchain_read_roots_in(&env, &[], writable)
}

fn roots_for(home: &Path, path: &[PathBuf]) -> Vec<PathBuf> {
    roots_beside(home, path, &[])
}

/// A Go `GOROOT` at `root`: `bin/go`, `VERSION`, `src/runtime`, `src/testing`.
fn go_layout(root: &Path) {
    write_file(&root.join("bin/go"), "#!/bin/sh\n");
    write_file(&root.join("VERSION"), "go1.0.0\n");
    write_file(&root.join("src/runtime/runtime.go"), "package runtime\n");
    write_file(&root.join("src/testing/testing.go"), "package testing\n");
}

/// A Go install in an `actions/tool-cache` layout below HOME.
fn go_install(home: &Path) -> PathBuf {
    let root = home.join("hostedtoolcache/go/1.0.0/arm64");
    go_layout(&root);
    root
}

#[test]
fn a_bin_entry_grants_its_install_root() {
    let (_h, home) = temp();
    let root = go_install(&home);
    let roots = roots_for(&home, &[root.join("bin")]);
    assert_eq!(roots, vec![root], "the entry lies inside its GOROOT");
}

/// Each recognised layout: a JDK (the macOS `Contents/Home` shape), a Node
/// install, a Gradle distribution and a bare `actions/tool-cache` entry.
#[test]
fn recognised_install_layouts_are_install_roots() {
    let (_h, home) = temp();
    let jdk = home.join("work/_temp/jdk-21.jdk/Contents/Home");
    write_file(&jdk.join("bin/java"), "");
    write_file(&jdk.join("release"), "JAVA_VERSION=\"21\"\n");
    write_file(&jdk.join("lib/modules"), "");
    let node = home.join("apps/node-22");
    write_file(&node.join("bin/node"), "");
    write_file(&node.join("lib/node_modules/npm/package.json"), "{}\n");
    let gradle = home.join("work/_temp/gradle-8.9");
    write_file(&gradle.join("bin/gradle"), "");
    write_file(&gradle.join("lib/gradle-launcher-8.9.jar"), "");
    let cached = home.join("hostedtoolcache/Python/3.12.0/x64");
    write_file(&cached.join("bin/python3"), "");
    write_file(&home.join("hostedtoolcache/Python/3.12.0/x64.complete"), "");
    for install in [jdk, node, gradle, cached] {
        let roots = roots_for(&home, &[install.join("bin")]);
        assert_eq!(roots, vec![install.clone()], "{install:?}");
    }
}

/// A bare tool-cache entry `<cache>/<tool>/<version>/<arch>` with its marker
/// and `bin`, below `home`.
fn tool_cache_entry(home: &Path, cache: &str, arch: &str) -> PathBuf {
    let version = home.join(cache).join("node/20.0.0");
    write_file(&version.join(arch).join("bin/tool"), "");
    write_file(&version.join(format!("{arch}.complete")), "");
    version.join(arch)
}

/// The install roots of `entry`'s `bin` with `RUNNER_TOOL_CACHE` set to `cache`.
fn roots_with_tool_cache(home: &Path, entry: &Path, cache: &Path) -> Vec<PathBuf> {
    let env = BTreeMap::from([
        ("HOME".to_string(), OsString::from(home)),
        ("PATH".to_string(), OsString::from(entry.join("bin"))),
        ("RUNNER_TOOL_CACHE".to_string(), OsString::from(cache)),
    ]);
    toolchain_read_roots_in(&env, &[], &[])
}

/// The `.complete` marker alone is no evidence: the entry must sit below
/// `RUNNER_TOOL_CACHE` when it is set, else below a `hostedtoolcache`
/// directory, under a known `<arch>`.
#[test]
fn a_tool_cache_entry_needs_its_cache_and_a_known_arch() {
    let (_h, home) = temp();
    let hosted = tool_cache_entry(&home, "hostedtoolcache", "aarch64");
    assert_eq!(roots_for(&home, &[hosted.join("bin")]), vec![hosted]);
    let runner = tool_cache_entry(&home, "runner/_work/_tool", "x86");
    let cache = home.join("runner/_work/_tool");
    assert_eq!(
        roots_with_tool_cache(&home, &runner, &cache),
        vec![runner.clone()]
    );
    let refused = [
        roots_for(&home, &[runner.join("bin")]),
        roots_with_tool_cache(&home, &runner, &home.join("elsewhere")),
        roots_for(
            &home,
            &[tool_cache_entry(&home, "hostedtoolcache", "docs").join("bin")],
        ),
        roots_for(
            &home,
            &[tool_cache_entry(&home, "Documents", "x64").join("bin")],
        ),
    ];
    let entries = [
        runner.join("bin"),
        runner.join("bin"),
        home.join("hostedtoolcache/node/20.0.0/docs/bin"),
        home.join("Documents/node/20.0.0/x64/bin"),
    ];
    for (roots, entry) in refused.iter().zip(&entries) {
        assert_eq!(roots, &vec![entry.clone()], "{entry:?}");
    }
}

/// A project's `bin` beside `lib/` (a Rails app) grants only the entry, and a
/// toolchain-shaped directory holding a project marker is refused too.
#[test]
fn a_project_is_never_an_install_root() {
    let (_h, home) = temp();
    let app = home.join("src/app");
    write_file(&app.join("bin/rails"), "#!/bin/sh\n");
    write_file(&app.join("lib/tasks/a.rake"), "");
    write_file(&app.join("libexec/helper"), "");
    write_file(&app.join(".env"), "SECRET=1\n");
    write_file(&app.join("config/master.key"), "SECRET\n");
    let roots = roots_for(&home, &[app.join("bin")]);
    assert_eq!(roots, vec![app.join("bin")]);
    let checkout = home.join("src/go");
    go_layout(&checkout);
    std::fs::create_dir_all(checkout.join(".git")).unwrap();
    let roots = roots_for(&home, &[checkout.join("bin")]);
    assert_eq!(roots, vec![checkout.join("bin")]);
}

#[test]
fn a_hidden_home_directory_is_never_an_install_root() {
    let (_h, home) = temp();
    go_layout(&home.join(".local"));
    write_file(&home.join(".local/share/keyrings/login"), "SECRET\n");
    let roots = roots_for(&home, &[home.join(".local/bin")]);
    assert!(roots.contains(&home.join(".local/bin")), "{roots:?}");
    assert!(!roots.contains(&home.join(".local")), "{roots:?}");
}

#[test]
fn a_symlinked_executable_grants_its_target_install() {
    let (_h, home) = temp();
    let install = home.join("apps/node-22");
    let cli = install.join("lib/node_modules/npm/bin/npm-cli.js");
    write_file(&cli, "#!/usr/bin/env node\n");
    write_file(&install.join("bin/node"), "#!/bin/sh\n");
    std::fs::create_dir_all(home.join("shims")).unwrap();
    symlink(install.join("bin/node"), home.join("shims/node")).unwrap();
    symlink(&cli, install.join("bin/npm")).unwrap();
    let roots = roots_for(&home, &[home.join("shims")]);
    assert_eq!(roots, vec![home.join("shims"), install.clone()]);
    let roots = roots_for(&home, &[install.join("bin")]);
    assert_eq!(roots, vec![install]);
}

/// A plain `bin` target directory deeper below HOME is granted by itself.
#[test]
fn a_symlink_target_bin_directory_is_granted() {
    let (_h, home) = temp();
    let target = home.join("opt/tool/1.0/bin");
    write_file(&target.join("tool"), "#!/bin/sh\n");
    write_file(&home.join("opt/tool/1.0/notes.txt"), "PRIVATE\n");
    std::fs::create_dir_all(home.join("tools/bin")).unwrap();
    symlink(target.join("tool"), home.join("tools/bin/tool")).unwrap();
    let roots = roots_for(&home, &[home.join("tools/bin")]);
    assert_eq!(roots, vec![home.join("tools/bin"), target]);
}

/// Targets in HOME, a direct child of HOME, a hidden directory in HOME, a
/// directory not named `bin`, or a credential directory grant nothing.
#[test]
fn symlink_targets_outside_toolchain_directories_stay_hidden() {
    let (_h, home) = temp();
    let targets = [
        home.join("script.sh"),
        home.join("Downloads/tool"),
        home.join("bin/tool"),
        home.join(".local/share/keyrings/bin/tool"),
        home.join("Documents/tax/2025.pdf"),
        home.join(".aws/bin/helper"),
    ];
    write_file(&home.join(".aws/credentials"), "SECRET\n");
    std::fs::create_dir_all(home.join("tools/bin")).unwrap();
    for (index, target) in targets.iter().enumerate() {
        write_file(target, "#!/bin/sh\n");
        symlink(target, home.join(format!("tools/bin/t{index}"))).unwrap();
    }
    symlink(home.join("missing"), home.join("tools/bin/dangling")).unwrap();
    let roots = roots_for(&home, &[home.join("tools/bin")]);
    assert_eq!(roots, vec![home.join("tools/bin")]);
}

/// A `PATH` entry in the writable tree, one containing it, and a symlink whose
/// resolution passes through it are never followed: the command could plant
/// `node_modules/.bin/x -> ~/Documents/tax/bin/x`, or retarget a link in the
/// tree, and choose what the next command reads.
#[test]
fn a_writable_root_never_redirects_a_read_root() {
    let (_h, home) = temp();
    let tree = home.join("work/tree");
    let private = home.join("Documents/tax/bin");
    write_file(&private.join("x"), "#!/bin/sh\n");
    let planted = tree.join("node_modules/.bin");
    std::fs::create_dir_all(&planted).unwrap();
    symlink(private.join("x"), planted.join("x")).unwrap();
    go_layout(&tree.join("node_modules"));
    symlink(&private, tree.join("link")).unwrap();
    std::fs::create_dir_all(home.join("tools/bin")).unwrap();
    symlink(tree.join("link/x"), home.join("tools/bin/x")).unwrap();
    symlink(tree.join("link"), home.join("via")).unwrap();
    let path = [
        planted.clone(),
        tree.join("node_modules/bin"),
        home.join("work"),
        home.join("via"),
        home.join("tools/bin"),
    ];
    let roots = roots_beside(&home, &path, &[tree.as_path()]);
    assert_eq!(roots, vec![home.join("tools/bin")], "writable {tree:?}");
    let unconfined = roots_for(&home, &path[4..]);
    assert!(unconfined.contains(&private), "control: {unconfined:?}");
}

/// The generated policy (Seatbelt or Bubblewrap) grants the install root of a
/// toolchain on `PATH` below HOME, as it grants the entry itself.
#[test]
#[serial_test::serial]
fn the_policy_grants_a_home_install_root() {
    let ((_r, repo), (_h, home), (_t, tree)) = (temp(), temp(), temp());
    let root = go_install(&home);
    let cache = stage(&repo, &home);
    let _env = HostEnvGuard::pin(&home, &[root.join("bin")]);
    let Some(policy) = policy_for("the_policy_grants_a_home_install_root", &tree, &cache) else {
        return;
    };
    let readable = granted(&policy);
    assert!(readable.contains(&root), "{root:?} not in {readable:?}");
}
