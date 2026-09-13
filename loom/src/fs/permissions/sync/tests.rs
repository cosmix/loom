use super::*;

#[test]
fn test_is_worktree_specific_permission() {
    assert!(is_worktree_specific_permission(
        "Read(../../../.loom/work/**)"
    ));
    assert!(is_worktree_specific_permission(
        "Write(.worktrees/stage-1/**)"
    ));
    // Single-level ../ must also be treated as worktree-specific (C-18)
    assert!(is_worktree_specific_permission("Read(../doc/**)"));
    assert!(is_worktree_specific_permission("Write(../src/**)"));
    assert!(!is_worktree_specific_permission("Read(.loom/work/**)"));
    assert!(!is_worktree_specific_permission("Bash(cargo:*)"));
}

#[test]
fn test_extract_permissions() {
    let settings = json!({
        "permissions": {
            "allow": ["Read(foo)", "Write(bar)"],
            "deny": ["Bash(rm:*)"]
        }
    });

    let (allow, deny) = extract_permissions(&settings);
    assert_eq!(allow, vec!["Read(foo)", "Write(bar)"]);
    assert_eq!(deny, vec!["Bash(rm:*)"]);
}

#[test]
fn test_extract_permissions_empty() {
    let settings = json!({});
    let (allow, deny) = extract_permissions(&settings);
    assert!(allow.is_empty());
    assert!(deny.is_empty());
}

#[test]
fn test_transform_worktree_path_absolute() {
    // Absolute path with .worktrees/stage-id/ should be transformed to relative
    assert_eq!(
        transform_worktree_path("Read(/home/user/.worktrees/stage-1/loom/src/**)"),
        Some("Read(loom/src/**)".to_string())
    );
    assert_eq!(
        transform_worktree_path("Write(/tmp/project/.worktrees/my-stage/doc/plans/**)"),
        Some("Write(doc/plans/**)".to_string())
    );
}

#[test]
fn test_transform_worktree_path_relative() {
    // Relative path with ../../ should be resolved
    assert_eq!(
        transform_worktree_path("Read(../../../.loom/work/**)"),
        Some("Read(.loom/work/**)".to_string())
    );
    assert_eq!(
        transform_worktree_path("Write(../../doc/plans/**)"),
        Some("Write(doc/plans/**)".to_string())
    );
    // Multiple ../ levels
    assert_eq!(
        transform_worktree_path("Read(../../../foo/bar)"),
        Some("Read(foo/bar)".to_string())
    );
    // Single-level ../ (C-18: previously missed)
    assert_eq!(
        transform_worktree_path("Read(../doc/**)"),
        Some("Read(doc/**)".to_string())
    );
    assert_eq!(
        transform_worktree_path("Write(../src/main.rs)"),
        Some("Write(src/main.rs)".to_string())
    );
}

#[test]
fn test_transform_worktree_path_unchanged() {
    // Normal path without worktree patterns should return None
    assert_eq!(transform_worktree_path("Read(.loom/work/**)"), None);
    assert_eq!(transform_worktree_path("Bash(cargo:*)"), None);
    assert_eq!(transform_worktree_path("Write(src/**)"), None);
}

#[test]
fn test_transform_worktree_path_edge_cases() {
    // Invalid permission format
    assert_eq!(transform_worktree_path("NoParens"), None);
    assert_eq!(transform_worktree_path("BadFormat()"), None);

    // Empty path after transformation
    assert_eq!(transform_worktree_path("Read(.worktrees/stage/)"), None);
    assert_eq!(transform_worktree_path("Read(../../)"), None);

    // Bare glob after stripping ../ — escape prevention rules like ../../**
    // must not become Read(**) / Write(**) which would match everything
    assert_eq!(transform_worktree_path("Read(../../**)"), None);
    assert_eq!(transform_worktree_path("Write(../../**)"), None);

    // Just the stage id with no further path
    assert_eq!(transform_worktree_path("Read(.worktrees/stage-id)"), None);
}

#[test]
fn normalize_single_slash_path_rewrites_single_leading_slash() {
    assert_eq!(normalize_single_slash_path("Edit(/src/**)"), "Edit(src/**)");
    assert_eq!(
        normalize_single_slash_path("Read(/docs/**)"),
        "Read(docs/**)"
    );
    assert_eq!(
        normalize_single_slash_path("Write(/build/**)"),
        "Write(build/**)"
    );
    assert_eq!(
        normalize_single_slash_path("NotebookEdit(/nb.ipynb)"),
        "NotebookEdit(nb.ipynb)"
    );
}

#[test]
fn normalize_single_slash_path_leaves_other_shapes_unchanged() {
    // `//path` is already absolute from the filesystem root.
    assert_eq!(
        normalize_single_slash_path("Edit(//abs/x)"),
        "Edit(//abs/x)"
    );
    // `~/path` is home-relative, not settings-file-relative.
    assert_eq!(normalize_single_slash_path("Edit(~/x)"), "Edit(~/x)");
    // Already cwd-relative.
    assert_eq!(normalize_single_slash_path("Edit(src/**)"), "Edit(src/**)");
    // Not a path-argument permission type.
    assert_eq!(
        normalize_single_slash_path("Bash(cargo test:*)"),
        "Bash(cargo test:*)"
    );
}

/// Owner decision: a single-leading-slash path rule resolves relative to the
/// settings file that recorded it (the recording checkout's root), not to
/// wherever the rule is rendered later. Recorded verbatim it would grant the
/// wrong path once rendered into a capsule; normalizing to the cwd-relative
/// form lets a later session apply it to its own checkout. Covers a rule
/// sourced from the main repository's own settings and one sourced from the
/// worktree's, and every shape the normalization must leave alone, run
/// through the control-surface filter after normalizing.
#[test]
fn single_slash_paths_are_normalized_to_cwd_relative_before_recording() {
    let temp = tempfile::TempDir::new().unwrap();
    let repo = temp.path().join("repo");
    let worktree = repo.join(".worktrees").join("s1");
    std::fs::create_dir_all(repo.join(".loom").join("work")).unwrap();
    std::fs::create_dir_all(repo.join(".claude")).unwrap();
    std::fs::create_dir_all(worktree.join(".claude")).unwrap();

    // R's own local settings file.
    let main_settings = json!({ "permissions": { "allow": [
        "Edit(/src/**)",
        "Edit(//abs/x)",
        "Bash(cargo test:*)"
    ] } });
    std::fs::write(
        repo.join(".claude/settings.local.json"),
        main_settings.to_string(),
    )
    .unwrap();

    // The worktree's own local settings file.
    let worktree_settings = json!({ "permissions": { "allow": [
        "Read(/docs/**)",
        "Edit(~/x)",
        "Edit(/.loom/**)",
        "Edit(/.claude/settings.json)"
    ] } });
    std::fs::write(
        worktree.join(".claude/settings.local.json"),
        worktree_settings.to_string(),
    )
    .unwrap();

    sync_worktree_permissions_with_working_dir(&worktree, &repo, None).unwrap();

    let resolved_state_root = repo.join(".loom").join("work").canonicalize().unwrap();
    let list = super::super::approved::approved_path(&resolved_state_root);
    let approved: Value = serde_json::from_str(&std::fs::read_to_string(list).unwrap()).unwrap();
    assert_eq!(
        approved["allow"],
        json!([
            "Bash(cargo test:*)",
            "Edit(//abs/x)",
            "Edit(src/**)",
            "Edit(~/x)",
            "Read(docs/**)"
        ])
    );
}

/// The fold-back feeds the loom-owned approved list from both sources a
/// session's approvals can land in (the worktree's local settings and the
/// main repository's), and nothing that names a control surface gets in.
///
/// Covers every control-surface spelling that is checkable without an
/// injected environment (`.loom/`, `.work/`, the resolved state root,
/// `.worktrees/` and `.claude/`); the scratch root and a hooks directory are
/// covered directly in `approved.rs`'s own tests, which do not depend on the
/// daemon's environment.
#[test]
fn fold_back_records_approvals_through_the_control_surface_filter() {
    let temp = tempfile::TempDir::new().unwrap();
    let repo = temp.path().join("repo");
    let worktree = repo.join(".worktrees").join("s1");
    std::fs::create_dir_all(repo.join(".loom").join("work")).unwrap();
    std::fs::create_dir_all(repo.join(".claude")).unwrap();
    std::fs::create_dir_all(worktree.join(".claude")).unwrap();
    let resolved_state_root = repo.join(".loom").join("work").canonicalize().unwrap();
    let worktree_settings = json!({ "permissions": { "allow": [
        "Bash(cargo test:*)",
        "Edit(.claude/settings.json)",
        "Edit(.loom/work/handoffs/**)",
        "Edit(.work/memory/**)",
        "Edit(.worktrees/s1/**)",
        format!("Edit(/{}/signals/**)", resolved_state_root.display())
    ] } });
    std::fs::write(
        worktree.join(".claude/settings.local.json"),
        worktree_settings.to_string(),
    )
    .unwrap();
    let main_settings = json!({ "permissions": { "allow": ["WebFetch(domain:docs.rs)"] } });
    std::fs::write(
        repo.join(".claude/settings.local.json"),
        main_settings.to_string(),
    )
    .unwrap();

    sync_worktree_permissions_with_working_dir(&worktree, &repo, None).unwrap();

    let list = super::super::approved::approved_path(&resolved_state_root);
    let approved: Value = serde_json::from_str(&std::fs::read_to_string(list).unwrap()).unwrap();
    assert_eq!(
        approved["allow"],
        json!(["Bash(cargo test:*)", "WebFetch(domain:docs.rs)"])
    );
}

/// Acceptance: spawn-setup and the fold-back never write either local
/// settings file. Both files are made read-only before the fold-back runs;
/// a write attempt would fail loudly (`std::fs::write` on a 0444 file
/// returns `EACCES`), so an `Ok` result and untouched bytes together prove
/// no write was attempted.
#[test]
fn fold_back_leaves_both_local_settings_files_untouched() {
    use std::os::unix::fs::PermissionsExt;

    let temp = tempfile::TempDir::new().unwrap();
    let repo = temp.path().join("repo");
    let worktree = repo.join(".worktrees").join("s1");
    std::fs::create_dir_all(repo.join(".loom").join("work")).unwrap();
    std::fs::create_dir_all(repo.join(".claude")).unwrap();
    std::fs::create_dir_all(worktree.join(".claude")).unwrap();

    let main_settings_path = repo.join(".claude/settings.local.json");
    let worktree_settings_path = worktree.join(".claude/settings.local.json");
    let main_body = json!({ "permissions": { "allow": ["WebFetch(domain:docs.rs)"] } }).to_string();
    let worktree_body = json!({ "permissions": { "allow": ["Bash(cargo test:*)"] } }).to_string();
    std::fs::write(&main_settings_path, &main_body).unwrap();
    std::fs::write(&worktree_settings_path, &worktree_body).unwrap();
    std::fs::set_permissions(&main_settings_path, std::fs::Permissions::from_mode(0o444)).unwrap();
    std::fs::set_permissions(
        &worktree_settings_path,
        std::fs::Permissions::from_mode(0o444),
    )
    .unwrap();

    let result = sync_worktree_permissions_with_working_dir(&worktree, &repo, None);

    // Restore write permission before asserting, so a failing assertion
    // does not leave a read-only fixture behind for TempDir's cleanup.
    std::fs::set_permissions(&main_settings_path, std::fs::Permissions::from_mode(0o644)).unwrap();
    std::fs::set_permissions(
        &worktree_settings_path,
        std::fs::Permissions::from_mode(0o644),
    )
    .unwrap();

    assert!(result.is_ok(), "fold-back must not error: {result:?}");
    assert_eq!(
        std::fs::read_to_string(&main_settings_path).unwrap(),
        main_body
    );
    assert_eq!(
        std::fs::read_to_string(&worktree_settings_path).unwrap(),
        worktree_body
    );
}
