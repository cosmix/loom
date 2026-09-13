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
fn test_merge_permissions_with_lock_scrubs_identity_env() {
    let temp_dir = tempfile::TempDir::new().unwrap();
    let settings_path = temp_dir.path().join("settings.local.json");

    // A LIVE work dir must survive alongside the identity heal.
    let live_work_dir = temp_dir.path().join(".loom").join("work");
    std::fs::create_dir_all(&live_work_dir).unwrap();
    let live_work_dir_str = live_work_dir.to_string_lossy().to_string();

    // Pre-fix binaries left per-session identity in the main repo's
    // settings.local.json; the fold-back must heal it on every rewrite.
    let polluted = json!({
        "env": {
            "LOOM_STAGE_ID": "knowledge-bootstrap",
            "LOOM_SESSION_ID": "session-stale",
            "LOOM_WORK_DIR": live_work_dir_str
        },
        "permissions": { "allow": ["Read(.loom/work/**)"] }
    });
    std::fs::write(
        &settings_path,
        serde_json::to_string_pretty(&polluted).unwrap(),
    )
    .unwrap();

    let result =
        merge_permissions_with_lock(&settings_path, &["Bash(cargo:*)".to_string()], &[]).unwrap();
    assert_eq!(result.allow_added, 1);

    let content = std::fs::read_to_string(&settings_path).unwrap();
    let settings: Value = serde_json::from_str(&content).unwrap();
    let env = settings["env"].as_object().unwrap();
    assert!(!env.contains_key("LOOM_STAGE_ID"));
    assert!(!env.contains_key("LOOM_SESSION_ID"));
    assert_eq!(env["LOOM_WORK_DIR"], live_work_dir_str);
    let allow = settings["permissions"]["allow"].as_array().unwrap();
    assert!(allow.iter().any(|v| v == "Read(.loom/work/**)"));
    assert!(allow.iter().any(|v| v == "Bash(cargo:*)"));
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

/// The fold-back feeds the loom-owned approved list from both sources a
/// session's approvals can land in (the worktree's local settings and the
/// main repository's), and nothing that names a control surface gets in.
#[test]
fn fold_back_records_approvals_through_the_control_surface_filter() {
    let temp = tempfile::TempDir::new().unwrap();
    let repo = temp.path().join("repo");
    let worktree = repo.join(".worktrees").join("s1");
    std::fs::create_dir_all(repo.join(".loom").join("work")).unwrap();
    std::fs::create_dir_all(repo.join(".claude")).unwrap();
    std::fs::create_dir_all(worktree.join(".claude")).unwrap();
    let worktree_settings = json!({ "permissions": { "allow": [
        "Bash(cargo test:*)",
        "Edit(.claude/settings.json)",
        "Edit(.loom/work/handoffs/**)"
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

    let work_dir = repo.join(".loom").join("work").canonicalize().unwrap();
    let list = super::super::approved::approved_path(&work_dir);
    let approved: Value = serde_json::from_str(&std::fs::read_to_string(list).unwrap()).unwrap();
    assert_eq!(
        approved["allow"],
        json!(["Bash(cargo test:*)", "WebFetch(domain:docs.rs)"])
    );
}
