//! Capsule contents that depend on the sandbox configuration.

use super::*;
use serial_test::serial;

#[test]
fn a_config_validate_config_refuses_is_refused_for_every_kind() {
    let mut config = sandbox(false);
    config.permission_mode = PermissionMode::BypassPermissions;
    for kind in ALL_KINDS {
        for worktree_rooted in [true, false] {
            let error = try_build(kind, &config, &[], None, worktree_rooted).unwrap_err();
            assert!(
                format!("{error:#}").contains("bypass-permissions"),
                "{kind}: {error:#}"
            );
        }
    }
}

/// Reads the install roots here and again inside `build`: both pinned to one
/// scratch home (`InstallRootsEnv`, `#[serial]`).
#[test]
#[serial]
fn the_home_expanded_forward_entry_is_allowed_only_with_the_codex_lane() {
    let home = tempfile::tempdir().unwrap();
    let _env = crate::assets::install::test_env::InstallRootsEnv::isolate(home.path());
    let entry = crate::fs::permissions::settings::codex_forward_home_allow_entry()
        .expect("an isolated home resolves the hooks directory");
    for kind in ALL_KINDS {
        let with_codex = build(kind, &sandbox(true), &[], None, true);
        assert_has(&with_codex, "/permissions/allow", &entry, kind);
        let without = strings(
            &build(kind, &sandbox(false), &[], None, true),
            "/permissions/allow",
        );
        assert!(!without.contains(&entry), "{kind}: {without:?}");
    }
}

const SPACED_HOOKS_DIR: &str = "/home/my user/.claude/hooks/loom";

fn spaced_capsule(kind: SessionType, python3: Option<&Path>, python_hooks: &[PathBuf]) -> Value {
    let config = sandbox(false);
    let denies = denies(true, false);
    capsule_settings(&CapsuleInputs {
        kind,
        sandbox: &config,
        worktree_rooted: true,
        cwd: Path::new("/repo/.worktrees/s1"),
        state_root: Path::new("/repo/.loom/work"),
        repo_root: Path::new("/repo"),
        hooks_dir: Path::new(SPACED_HOOKS_DIR),
        scratch_dir: Path::new("/scratch/session-1"),
        approved: &[],
        checkout_settings: None,
        denies: &denies,
        python3,
        python_hooks,
        hidden_sockets: &[],
        workflow_licensed: false,
    })
    .unwrap()
}

/// A hooks directory with a space: `hooks/config.rs` and `session_event_rules`
/// quote their commands, so each stays one word, quoted once, under `/bin/bash`.
#[test]
fn a_hooks_dir_with_a_space_quotes_every_command_exactly_once() {
    let prefix = format!("/bin/bash '{SPACED_HOOKS_DIR}/");
    for kind in ALL_KINDS {
        for (_, _, command) in hooks(&spaced_capsule(kind, None, &[])) {
            let script = command
                .strip_prefix(&prefix)
                .and_then(|rest| rest.strip_suffix('\''))
                .unwrap_or_else(|| panic!("{kind}: {command}"));
            assert!(!script.contains(['/', '\'', ' ']), "{kind}: {command}");
        }
    }
    let registered = |kind, script: &str| {
        hooks(&spaced_capsule(kind, None, &[]))
            .iter()
            .any(|(_, _, command)| command == &format!("{prefix}{script}'"))
    };
    assert!(registered(SessionType::Stage, "loom-relay.sh"));
    assert!(registered(SessionType::Stage, "loom-control-complete.sh"));
    assert!(
        !registered(SessionType::Merge, "loom-control-complete.sh"),
        "a quoted command must still be found and dropped"
    );
}

/// A Python hook under such a directory is still recognised: it runs under the
/// interpreter when there is one and is dropped when there is none.
#[test]
fn a_python_hook_under_a_spaced_dir_is_recognised_through_its_quoting() {
    let script = PathBuf::from(format!("{SPACED_HOOKS_DIR}/commit-guard.sh"));
    let python_hooks = [script];
    let quoted = format!("'{SPACED_HOOKS_DIR}/commit-guard.sh'");

    let settings = spaced_capsule(
        SessionType::Stage,
        Some(Path::new("/opt/py thon/python3")),
        &python_hooks,
    );
    let interpreted = format!("'/opt/py thon/python3' {quoted}");
    assert!(hooks(&settings)
        .iter()
        .any(|(_, _, command)| *command == interpreted));

    let without = spaced_capsule(SessionType::Stage, None, &python_hooks);
    assert!(!hooks(&without)
        .iter()
        .any(|(_, _, command)| command.ends_with(&quoted)));
}
