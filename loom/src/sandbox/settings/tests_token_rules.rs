use super::tests::default_config;
use super::*;
use crate::fs::permissions::state_root::CREDENTIAL_DENY_READ_PATHS;

/// `permissions.deny` entries of a settings value, tolerating an absent key —
/// with no `Read(...)` rules left to emit, a config whose `deny_write` is all
/// parent-traversal produces no `deny` array at all.
fn deny_entries(settings: &Value) -> Vec<String> {
    settings["permissions"]["deny"]
        .as_array()
        .map(|entries| {
            entries
                .iter()
                .filter_map(|value| value.as_str().map(String::from))
                .collect()
        })
        .unwrap_or_default()
}

fn read_settings_local(project_root: &Path) -> Value {
    let content = fs::read_to_string(project_root.join(".claude/settings.local.json")).unwrap();
    serde_json::from_str(&content).unwrap()
}

/// The relative token paths stay in `sandbox.filesystem.denyRead` for OS
/// enforcement, but must never be written as `permissions.deny` rules: Claude
/// Code refuses every relative-path `rg`/`grep`/`diff`/`git`/`cp`/`mv` issued
/// after a `cd` while ANY `Read(` deny rule exists, whatever its path.
#[test]
fn token_denies_are_os_rules_only_never_project_relative_permission_rules() {
    use tempfile::TempDir;

    let temp_dir = TempDir::new().unwrap();
    let repo_root = temp_dir.path();
    write_settings(&default_config(), repo_root).unwrap();

    let settings = read_settings_local(repo_root);
    let deny = deny_entries(&settings);
    assert!(
        !deny.iter().any(|entry| entry.starts_with("Read(")),
        "a repo must carry no read permission rule at all, got: {deny:?}"
    );

    let os_deny = settings["sandbox"]["filesystem"]["denyRead"]
        .as_array()
        .unwrap();
    for relative in [".work/admin.token", ".loom/work/user.token"] {
        assert!(
            os_deny.iter().any(|value| value == relative),
            "the OS deny list must keep {relative}, got: {os_deny:?}"
        );
    }
}

/// Regression guard for the property Claude Code actually enforces: one
/// `Read(...)` entry under `permissions.deny` in ANY settings file makes it
/// refuse every relative-path `rg`, `grep`, `diff`, `git`, `cp` and `mv`
/// issued after a `cd` in the same compound command — bypass-immune, not
/// classifier-approvable, and independent of the rule's path shape. So the
/// generated files carry none, at the repo root and inside a worktree alike,
/// while the OS-level `denyRead` list (which is not a permission rule and does
/// not feed that check) keeps every credential path.
///
/// The sibling `.claude/settings.json` writer is covered by
/// `git::worktree::tests_settings`
/// (`create_worktree_settings_never_grants_a_blanket_read_over_the_state_root`
/// and the normalization tests beside it) — `create_worktree_settings` is
/// private to that module, so it cannot be driven from here.
#[cfg(unix)]
#[test]
fn generated_settings_carry_no_read_deny_rules() {
    use tempfile::TempDir;

    let temp_dir = TempDir::new().unwrap();
    let base = temp_dir.path();
    let work_dir = base.join(".work");
    fs::create_dir_all(work_dir.join("signals")).unwrap();
    let worktree_path = base.join(".worktrees").join("my-stage");
    fs::create_dir_all(&worktree_path).unwrap();
    std::os::unix::fs::symlink(&work_dir, worktree_path.join(".work")).unwrap();

    for project_root in [base, worktree_path.as_path()] {
        write_settings(&default_config(), project_root).unwrap();
        let settings = read_settings_local(project_root);

        let deny = deny_entries(&settings);
        assert!(
            !deny.iter().any(|entry| entry.starts_with("Read(")),
            "{} must carry no Read( deny rule, got: {deny:?}",
            project_root.display()
        );

        let os_deny: Vec<&str> = settings["sandbox"]["filesystem"]["denyRead"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(|value| value.as_str())
            .collect();
        for relative in [".work/admin.token", ".work/user.token"] {
            assert!(
                os_deny.contains(&relative),
                "{} lost the OS deny for {relative}, got: {os_deny:?}",
                project_root.display()
            );
        }
        for credential in CREDENTIAL_DENY_READ_PATHS {
            assert!(
                os_deny.contains(&credential),
                "{} lost the OS deny for {credential}, got: {os_deny:?}",
                project_root.display()
            );
        }
    }
}
