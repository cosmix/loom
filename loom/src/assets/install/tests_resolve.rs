//! The install-root resolver: a relative root is an error naming its source,
//! and every root resolves canonical.

use std::fs;
use std::path::Path;

use serial_test::serial;
use tempfile::TempDir;

use super::*;
use crate::assets::install::test_env::InstallRootsEnv;
use crate::sandbox::control_surfaces::AgentRoots;

/// A temporary directory spelled canonical, so expectations compare alike
/// where the temp root is itself a symlink (macOS `/var`).
fn canonical_temp() -> (TempDir, PathBuf) {
    let temp = TempDir::new().unwrap();
    let path = temp.path().canonicalize().unwrap();
    (temp, path)
}

#[test]
#[serial]
fn a_relative_override_is_an_error_naming_the_variable() {
    let (_temp, home) = canonical_temp();
    for variable in [CLAUDE_OVERRIDE, CODEX_OVERRIDE] {
        let mut env = InstallRootsEnv::isolate(&home);
        env.set(variable, "agents/root");
        let error = format!("{:#}", default_paths().unwrap_err());
        assert!(
            error.contains(variable) && error.contains("must be an absolute path"),
            "{error}"
        );
        assert_eq!(claude_hooks_dir(), None);
    }
}

#[test]
#[serial]
fn a_relative_recorded_root_is_an_error_naming_the_record() {
    let (_temp, home) = canonical_temp();
    let _env = InstallRootsEnv::isolate(&home);
    let record = home.join(roots::RECORD_PATH);
    fs::create_dir_all(record.parent().unwrap()).unwrap();
    fs::write(
        &record,
        "claude_dir = \"rel/claude\"\ncodex_dir = \"/srv/codex\"\n",
    )
    .unwrap();

    let error = format!("{:#}", try_claude_hooks_dir().unwrap_err());
    assert!(
        error.contains("the claude_dir entry of")
            && error.contains(&record.display().to_string())
            && error.contains("must be an absolute path"),
        "{error}"
    );
}

#[cfg(unix)]
#[test]
#[serial]
fn a_root_reached_through_a_symlink_resolves_to_its_target() {
    let (_temp, base) = canonical_temp();
    let home = base.join("home");
    let real = base.join("real");
    fs::create_dir_all(real.join("claude")).unwrap();
    fs::create_dir_all(&home).unwrap();
    std::os::unix::fs::symlink(&real, base.join("link")).unwrap();
    let mut env = InstallRootsEnv::isolate(&home);
    env.set(CLAUDE_OVERRIDE, base.join("link/claude"));
    // Not created yet: the existing symlinked parent still resolves.
    env.set(CODEX_OVERRIDE, base.join("link/codex"));

    let paths = default_paths().unwrap();
    assert_eq!(paths.claude_dir, real.join("claude"));
    assert_eq!(paths.codex_dir, real.join("codex"));
    assert_eq!(claude_hooks_dir(), Some(real.join("claude/hooks/loom")));
    // The capsule's relocated-root denies name the real paths too.
    let roots = AgentRoots::resolve(Some(&home)).unwrap();
    assert_eq!(roots.claude(), Some(real.join("claude").as_path()));
    assert_eq!(roots.codex(), Some(real.join("codex").as_path()));
}

#[cfg(unix)]
#[test]
#[serial]
fn a_symlinked_default_root_is_still_the_default() {
    let (_temp, base) = canonical_temp();
    let home = base.join("home");
    let dotfiles = base.join("dotfiles-claude");
    fs::create_dir_all(dotfiles.join("hooks/loom")).unwrap();
    fs::create_dir_all(&home).unwrap();
    std::os::unix::fs::symlink(&dotfiles, home.join(".claude")).unwrap();
    let _env = InstallRootsEnv::isolate(&home);

    let hooks = claude_hooks_dir().unwrap();
    assert_eq!(hooks, dotfiles.join("hooks/loom"));
    assert!(is_default_claude_hooks_dir(&hooks, &home));
    assert_eq!(AgentRoots::resolve(Some(&home)).unwrap().claude(), None);
}

#[test]
#[serial]
fn the_preflight_refusal_names_why_the_root_does_not_resolve() {
    let (_temp, home) = canonical_temp();
    let mut env = InstallRootsEnv::isolate(&home);
    env.set(CODEX_OVERRIDE, "./codex");
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
                && refusal.contains(CODEX_OVERRIDE)
                && refusal.contains("must be an absolute path")
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
