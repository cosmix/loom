use super::tests::has_read_deny;
use super::*;
use crate::models::stage::Implementers;
use crate::plan::schema::{CommandConfinement, FilesystemConfig, LinuxConfig, NetworkConfig};

#[cfg(unix)]
#[test]
fn test_write_settings_adds_resolved_work_symlink_permissions_nested_layout() {
    use tempfile::TempDir;

    let temp_dir = TempDir::new().unwrap();
    let base = temp_dir.path();

    // Simulate the nested layout: repo_root/.loom/work and
    // repo_root/.worktrees/stage/.loom/work (a real .loom/ holding the link).
    let work_dir = base.join(".loom").join("work");
    fs::create_dir_all(&work_dir).unwrap();
    fs::create_dir_all(work_dir.join("signals")).unwrap();

    let worktree_path = base.join(".worktrees").join("my-stage");
    let worktree_loom = worktree_path.join(".loom");
    fs::create_dir_all(&worktree_loom).unwrap();

    // Create the symlink: .worktrees/my-stage/.loom/work -> ../../../.loom/work
    std::os::unix::fs::symlink("../../../.loom/work", worktree_loom.join("work")).unwrap();

    let config = MergedSandboxConfig {
        enabled: true,
        auto_allow: true,
        allow_unsandboxed_escape: false,
        excluded_commands: vec![],
        filesystem: FilesystemConfig::default(),
        network: NetworkConfig::default(),
        linux: LinuxConfig::default(),
        permission_mode: PermissionMode::Auto,
        implementers: Implementers::default(),
        command_confinement: CommandConfinement::default(),
    };

    write_settings(&config, &worktree_path).unwrap();

    let settings_path = worktree_path.join(".claude/settings.local.json");
    let result_content = fs::read_to_string(&settings_path).unwrap();
    let result: Value = serde_json::from_str(&result_content).unwrap();

    let allow = result["permissions"]["allow"].as_array().unwrap();
    let allow_strs: Vec<&str> = allow.iter().filter_map(|v| v.as_str()).collect();

    let resolved_work = work_dir.canonicalize().unwrap();
    let resolved_str = resolved_work.to_string_lossy();

    // The nested `.loom/work` link must be the one resolved: same narrow
    // grants as the legacy arm, no broad `**` allow (S-1).
    let broad_read = format!("Read(/{}/**)", resolved_str);
    let broad_edit = format!("Edit(/{}/**)", resolved_str);
    assert!(!allow_strs.contains(&broad_read.as_str()));
    assert!(!allow_strs.contains(&broad_edit.as_str()));

    let expected_read_signals = format!("Read(/{}/signals/**)", resolved_str);
    let expected_edit_handoffs = format!("Edit(/{}/handoffs/**)", resolved_str);
    assert!(
        allow_strs.contains(&expected_read_signals.as_str()),
        "Should have resolved .loom/work/signals read permission, got: {:?}",
        allow_strs
    );
    assert!(
        allow_strs.contains(&expected_edit_handoffs.as_str()),
        "Should have resolved .loom/work/handoffs edit permission, got: {:?}",
        allow_strs
    );

    assert!(
        !has_read_deny(&result),
        "got: {:?}",
        result["permissions"]["deny"]
    );
    let os_deny = result["sandbox"]["filesystem"]["denyRead"]
        .as_array()
        .unwrap();
    assert!(os_deny
        .iter()
        .any(|value| value == &format!("/{resolved_str}/admin.token")));
    assert!(os_deny
        .iter()
        .any(|value| value == &format!("/{resolved_str}/user.token")));

    assert!(allow_strs.contains(&"Read(.loom/work/signals/**)"));
}

#[test]
fn test_write_settings_main_repo_strips_worktree_escape_denies() {
    use tempfile::TempDir;

    // A plain repo root (not under .worktrees, no `.work` symlink) is the main
    // repo: worktree-relative escape rules must be stripped, because `../..`
    // resolves to `$HOME` there and would deny the entire home directory.
    let temp_dir = TempDir::new().unwrap();
    let repo_root = temp_dir.path();

    let config = MergedSandboxConfig {
        enabled: true,
        auto_allow: true,
        allow_unsandboxed_escape: false,
        excluded_commands: vec![],
        // FilesystemConfig::default() includes ../../** (deny_read also has ../.worktrees/**). An
        // explicit non-traversal entry stands in for a plan-authored deny_write path, to prove
        // non-traversal entries survive alongside the stripped traversal ones.
        filesystem: FilesystemConfig {
            deny_write: {
                let mut deny_write = FilesystemConfig::default().deny_write;
                deny_write.push("some/plan/path/**".to_string());
                deny_write
            },
            ..FilesystemConfig::default()
        },
        network: NetworkConfig::default(),
        linux: LinuxConfig::default(),
        permission_mode: PermissionMode::Auto,
        implementers: Implementers::default(),
        command_confinement: CommandConfinement::default(),
    };

    write_settings(&config, repo_root).unwrap();

    let result: Value = serde_json::from_str(
        &fs::read_to_string(repo_root.join(".claude/settings.local.json")).unwrap(),
    )
    .unwrap();
    let deny = result["permissions"]["deny"].as_array().unwrap();
    let deny_strs: Vec<&str> = deny.iter().filter_map(|v| v.as_str()).collect();

    assert!(
        !deny_strs.iter().any(|p| p.contains("../")),
        "main repo deny must not contain parent-traversal rules, got: {deny_strs:?}"
    );
    assert!(
        !deny_strs.iter().any(|p| p.contains(".worktrees")),
        "main repo deny must not reference .worktrees, got: {deny_strs:?}"
    );
    assert!(deny_strs.contains(&"Edit(some/plan/path/**)"));
    assert!(!has_read_deny(&result), "got: {deny_strs:?}");
    let os_deny = result["sandbox"]["filesystem"]["denyRead"]
        .as_array()
        .unwrap();
    assert!(os_deny.iter().any(|value| value == "~/.ssh/**"));
}

#[test]
fn test_write_settings_main_repo_drops_stale_escape_from_existing() {
    use tempfile::TempDir;

    // Simulate a main-repo settings.local.json written by an OLDER loom version that leaked
    // worktree-relative escape rules. Re-running the generator on the main repo must scrub them
    // (both Read and Write sides), even though the merge preserves other user-approved permissions.
    //
    // `Write(~/.bashrc)` stands in for a legitimate user-authored deny entry unrelated to
    // loom's own rules — its INTENT must survive the merge, pinning that loom does not silently
    // discard rules it inherits (mirrors the sibling fixture in
    // `test_write_settings_preserves_existing_deny_but_not_allow`). What it survives as is
    // `Edit(~/.bashrc)`: the `Write(...)` spelling is inert at the tool layer, so carrying it
    // verbatim would keep the startup warning and none of the protection. A stale
    // `Write(doc/loom/knowledge/**)` is deliberately NOT used here: that specific entry is
    // dropped rather than migrated — see `merge_existing_permissions`'s knowledge-dir
    // carve-out, exercised separately below.
    let temp_dir = TempDir::new().unwrap();
    let repo_root = temp_dir.path();
    let claude_dir = repo_root.join(".claude");
    fs::create_dir_all(&claude_dir).unwrap();

    // `Read(~/.gnupg/**)` stands in for a non-traversal read deny; every `Read(` entry drops now.
    let stale = json!({
        "permissions": {
            "deny": [
                "Read(../../**)",
                "Read(../.worktrees/**)",
                "Read(~/.gnupg/**)",
                "Write(../../**)",
                "Write(~/.bashrc)"
            ]
        }
    });
    fs::write(
        claude_dir.join("settings.local.json"),
        serde_json::to_string_pretty(&stale).unwrap(),
    )
    .unwrap();

    let config = MergedSandboxConfig {
        enabled: true,
        auto_allow: true,
        allow_unsandboxed_escape: false,
        excluded_commands: vec![],
        filesystem: FilesystemConfig::default(),
        network: NetworkConfig::default(),
        linux: LinuxConfig::default(),
        permission_mode: PermissionMode::Auto,
        implementers: Implementers::default(),
        command_confinement: CommandConfinement::default(),
    };

    write_settings(&config, repo_root).unwrap();

    let result: Value =
        serde_json::from_str(&fs::read_to_string(claude_dir.join("settings.local.json")).unwrap())
            .unwrap();
    let deny = result["permissions"]["deny"].as_array().unwrap();
    let deny_strs: Vec<&str> = deny.iter().filter_map(|v| v.as_str()).collect();

    assert!(
        !deny_strs
            .iter()
            .any(|p| p.contains("../") || p.contains(".worktrees")),
        "stale escape rules must be scrubbed from the main repo file, got: {deny_strs:?}"
    );
    // A legitimate, unrelated user-authored deny entry is preserved — in the enforceable spelling.
    assert!(deny_strs.contains(&"Edit(~/.bashrc)"), "got: {deny_strs:?}");
    assert!(
        !deny_strs.iter().any(|p| p.starts_with("Write(")),
        "no inert Write(...) deny may survive regeneration, got: {deny_strs:?}"
    );
    assert!(
        !has_read_deny(&result),
        "Read deny must not survive regeneration, got: {deny_strs:?}"
    );
    let os_deny = result["sandbox"]["filesystem"]["denyRead"]
        .as_array()
        .unwrap();
    assert!(os_deny.iter().any(|value| value == "~/.gnupg/**"));
}
