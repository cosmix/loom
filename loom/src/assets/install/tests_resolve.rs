//! The install-root resolver: a relative home directory is an error, and
//! every root resolves canonical.

use std::fs;
use std::path::Path;

use serial_test::serial;
use tempfile::TempDir;

use super::*;
use crate::assets::install::test_env::InstallRootsEnv;

/// A temporary directory spelled canonical, so expectations compare alike
/// where the temp root is itself a symlink (macOS `/var`).
fn canonical_temp() -> (TempDir, PathBuf) {
    let temp = TempDir::new().unwrap();
    let path = temp.path().canonicalize().unwrap();
    (temp, path)
}

#[test]
#[serial]
fn the_roots_are_the_home_defaults() {
    let (_temp, home) = canonical_temp();
    let _env = InstallRootsEnv::isolate(&home);

    let paths = default_paths().unwrap();
    assert_eq!(paths.claude_dir, home.join(".claude"));
    assert_eq!(paths.codex_dir, home.join(".codex"));
    assert_eq!(claude_hooks_dir(), Some(home.join(".claude/hooks/loom")));
}

#[test]
#[serial]
fn a_relative_home_is_an_error() {
    let (_temp, home) = canonical_temp();
    let mut env = InstallRootsEnv::isolate(&home);
    env.set("HOME", "relative/home");

    let error = format!("{:#}", default_paths().unwrap_err());
    assert!(
        error.contains("$HOME") && error.contains("must be absolute"),
        "{error}"
    );
    assert_eq!(claude_hooks_dir(), None);
}

#[cfg(unix)]
#[test]
#[serial]
fn a_symlinked_default_root_resolves_to_its_target() {
    let (_temp, base) = canonical_temp();
    let home = base.join("home");
    let dotfiles = base.join("dotfiles-claude");
    fs::create_dir_all(dotfiles.join("hooks/loom")).unwrap();
    fs::create_dir_all(&home).unwrap();
    std::os::unix::fs::symlink(&dotfiles, home.join(".claude")).unwrap();
    let _env = InstallRootsEnv::isolate(&home);

    assert_eq!(default_paths().unwrap().claude_dir, dotfiles);
    assert_eq!(claude_hooks_dir(), Some(dotfiles.join("hooks/loom")));
}

#[test]
#[serial]
fn the_preflight_refusal_names_why_the_root_does_not_resolve() {
    let (_temp, home) = canonical_temp();
    let mut env = InstallRootsEnv::isolate(&home);
    env.set("HOME", "./home");
    let facts = crate::sandbox::preflight::HostFacts {
        writable_roots: Vec::new(),
        hooks_dir: None,
        loom_bin: home.join("loom"),
        hook_path: Vec::new(),
        python3: None,
        python_hooks: Vec::new(),
    };

    let refusals = crate::sandbox::preflight::host_refusals(&facts);
    assert!(
        refusals.iter().any(|refusal| {
            refusal.starts_with("no verified loom hooks directory")
                && refusal.contains("$HOME")
                && refusal.contains("must be absolute")
        }),
        "{refusals:?}"
    );
}

#[test]
fn a_missing_path_keeps_its_unresolved_tail() {
    let (_temp, base) = canonical_temp();
    let missing = base.join("not/yet/here");
    assert_eq!(canonical_or_lexical(&missing), missing);
    assert_eq!(
        canonical_or_lexical(Path::new("relative/x")),
        Path::new("relative/x")
    );
}
