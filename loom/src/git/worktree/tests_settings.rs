//! Tests for `git/worktree/settings.rs`.

use super::*;
use crate::fs::permissions::state_root::{is_parent_glob_token_deny, token_read_denies};
use tempfile::TempDir;

#[test]
fn test_is_worktree_scaffold_path() {
    // Everything create_worktree plants, in the shapes git reports it.
    for scaffold in [
        ".work",
        ".work/",
        ".loom",
        ".loom/",
        ".loom/work",
        ".loom/work/",
        ".claude",
        ".claude/",
        ".claude/settings.local.json",
        ".claude/CLAUDE.md",
        "CLAUDE.md",
        ".loom/memory-spool.jsonl",
        ".loom/stage-request-spool.jsonl",
        ".loom/cache",
        ".loom/cache/",
        ".loom/cache/context-v1/catalog.json",
    ] {
        assert!(
            is_worktree_scaffold_path(scaffold),
            "{scaffold} should be recognized as scaffolding"
        );
    }

    // Agent work must never be discounted as scaffolding. In particular,
    // the bare `.loom` entry is narrower than `.loom/` as a whole: a project
    // may legitimately track `.loom/config.toml`, so that specific path must
    // still read as agent-visible content even though `.loom` alone (git's
    // report for an otherwise entirely-untracked `.loom/`) is scaffold.
    for work in [
        "src/feature.rs",
        "docs/CLAUDE.md",
        ".workflows/ci.yml",
        "claude.md",
        ".loom/config.toml",
    ] {
        assert!(
            !is_worktree_scaffold_path(work),
            "{work} should NOT be treated as scaffolding"
        );
    }
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
fn test_merge_permission_vecs() {
    let a = vec!["Read(foo)".to_string(), "Write(bar)".to_string()];
    let b = vec!["Write(bar)".to_string(), "Bash(cargo:*)".to_string()];

    let merged = merge_permission_vecs(a, b);
    assert_eq!(merged.len(), 3);
    assert!(merged.contains(&"Read(foo)".to_string()));
    assert!(merged.contains(&"Write(bar)".to_string()));
    assert!(merged.contains(&"Bash(cargo:*)".to_string()));
}

#[test]
fn test_merge_permission_vecs_empty() {
    let a: Vec<String> = vec![];
    let b = vec!["Read(foo)".to_string()];

    let merged = merge_permission_vecs(a, b);
    assert_eq!(merged, vec!["Read(foo)"]);
}

#[test]
fn test_refresh_worktree_settings_local_merges_permissions() {
    let temp_dir = TempDir::new().unwrap();
    let repo_root = temp_dir.path().join("repo");
    let worktree = temp_dir.path().join("worktree");

    // Setup main repo with permission A
    let main_claude = repo_root.join(".claude");
    std::fs::create_dir_all(&main_claude).unwrap();
    let main_settings = json!({
        "permissions": {
            "allow": ["Read(main_perm)"]
        }
    });
    std::fs::write(
        main_claude.join("settings.local.json"),
        serde_json::to_string_pretty(&main_settings).unwrap(),
    )
    .unwrap();

    // Setup worktree with permission B
    let wt_claude = worktree.join(".claude");
    std::fs::create_dir_all(&wt_claude).unwrap();
    let wt_settings = json!({
        "permissions": {
            "allow": ["Write(worktree_perm)"]
        }
    });
    std::fs::write(
        wt_claude.join("settings.local.json"),
        serde_json::to_string_pretty(&wt_settings).unwrap(),
    )
    .unwrap();

    // Refresh should merge, not overwrite
    let result = refresh_worktree_settings_local(&worktree, &repo_root).unwrap();
    assert!(result);

    // Verify merged result
    let merged_content = std::fs::read_to_string(wt_claude.join("settings.local.json")).unwrap();
    let merged: Value = serde_json::from_str(&merged_content).unwrap();

    let (allow, _deny) = extract_permissions(&merged);
    assert!(allow.contains(&"Read(main_perm)".to_string()));
    assert!(allow.contains(&"Write(worktree_perm)".to_string()));
}

#[test]
fn test_refresh_worktree_settings_local_no_existing_worktree_settings() {
    let temp_dir = TempDir::new().unwrap();
    let repo_root = temp_dir.path().join("repo");
    let worktree = temp_dir.path().join("worktree");

    // Setup main repo with permission
    let main_claude = repo_root.join(".claude");
    std::fs::create_dir_all(&main_claude).unwrap();
    let main_settings = json!({
        "permissions": {
            "allow": ["Read(main_perm)"],
            "deny": ["Bash(rm:*)"]
        }
    });
    std::fs::write(
        main_claude.join("settings.local.json"),
        serde_json::to_string_pretty(&main_settings).unwrap(),
    )
    .unwrap();

    // Worktree has no existing settings
    std::fs::create_dir_all(&worktree).unwrap();

    // Refresh should create new settings
    let result = refresh_worktree_settings_local(&worktree, &repo_root).unwrap();
    assert!(result);

    // Verify result contains main permissions
    let wt_settings_path = worktree.join(".claude/settings.local.json");
    assert!(wt_settings_path.exists());

    let content = std::fs::read_to_string(&wt_settings_path).unwrap();
    let settings: Value = serde_json::from_str(&content).unwrap();

    let (allow, deny) = extract_permissions(&settings);
    assert_eq!(allow, vec!["Read(main_perm)"]);
    assert_eq!(deny, vec!["Bash(rm:*)"]);
}

#[test]
fn test_refresh_worktree_settings_local_no_main_settings() {
    let temp_dir = TempDir::new().unwrap();
    let repo_root = temp_dir.path().join("repo");
    let worktree = temp_dir.path().join("worktree");

    // Setup repo without settings.local.json
    let main_claude = repo_root.join(".claude");
    std::fs::create_dir_all(&main_claude).unwrap();

    std::fs::create_dir_all(&worktree).unwrap();

    // Should return false when no main settings exist
    let result = refresh_worktree_settings_local(&worktree, &repo_root).unwrap();
    assert!(!result);
}

#[test]
fn test_create_worktree_settings_adds_resolved_work_permissions_legacy_layout() {
    let temp_dir = TempDir::new().unwrap();
    let repo_root = temp_dir.path().join("repo");
    let worktree = temp_dir.path().join("worktree");

    // Create the main .work directory (simulates a legacy .work state dir)
    let main_work = repo_root.join(".work");
    std::fs::create_dir_all(&main_work).unwrap();

    // Create worktree directory and .work symlink pointing to main .work
    std::fs::create_dir_all(&worktree).unwrap();
    let worktree_work_link = worktree.join(".work");
    #[cfg(unix)]
    std::os::unix::fs::symlink(&main_work, &worktree_work_link).unwrap();

    // Create main repo settings.json with an existing permission
    let main_claude = repo_root.join(".claude");
    std::fs::create_dir_all(&main_claude).unwrap();
    let main_settings_json = json!({
        "permissions": {
            "allow": ["Read(.work/**)"]
        }
    });
    let main_settings_path = main_claude.join("settings.json");
    std::fs::write(
        &main_settings_path,
        serde_json::to_string_pretty(&main_settings_json).unwrap(),
    )
    .unwrap();

    // Run create_worktree_settings
    let worktree_settings_path = worktree.join("settings.json");
    create_worktree_settings(&main_settings_path, &worktree_settings_path, &worktree).unwrap();

    // Read and parse the generated settings
    let content = std::fs::read_to_string(&worktree_settings_path).unwrap();
    let settings: Value = serde_json::from_str(&content).unwrap();

    // Extract the allow list
    let (allow, _deny) = extract_permissions(&settings);

    // The original relative permission should still be there
    assert!(
        allow.contains(&"Read(.work/**)".to_string()),
        "Original relative permission should be preserved"
    );

    // Resolve the symlink to get the expected absolute path
    let resolved_work = worktree_work_link.canonicalize().unwrap();
    let resolved_str = resolved_work.to_string_lossy();

    // All resolved absolute-path permissions should be present
    // Note: // prefix is required for absolute paths in Claude Code
    //
    // No blanket read over the resolved `.work` root: it would expose
    // `admin.token` and `user.token` just as a broad `Edit`/`Write` would
    // have exposed them to write (S-1).
    assert!(
        !allow.contains(&format!("Read(/{}/**)", resolved_str)),
        "Should NOT contain a blanket Read(//resolved/**)"
    );
    // No broad write grant over the resolved `.work` root, in either spelling:
    // `Edit(` would re-expose the daemon tokens (S-1); `Write(` granted nothing.
    assert!(!allow.contains(&format!("Write(/{}/**)", resolved_str)));
    assert!(!allow.contains(&format!("Edit(/{}/**)", resolved_str)));
    assert!(
        allow.contains(&format!("Read(/{}/signals/**)", resolved_str)),
        "Should contain Read(//resolved/signals/**)"
    );
    assert!(
        allow.contains(&format!("Read(/{}/config.toml)", resolved_str)),
        "Should contain Read(//resolved/config.toml)"
    );
    assert!(
        allow.contains(&format!("Read(/{}/handoffs/**)", resolved_str)),
        "Should contain Read(//resolved/handoffs/**)"
    );

    // Trust is set; defaultMode is intentionally NOT written here —
    // it lives in settings.local.json via sandbox::apply_default_mode.
    assert_eq!(settings["hasTrustDialogAccepted"], json!(true));
    assert!(
        settings["permissions"].get("defaultMode").is_none(),
        "Base settings.json must not write defaultMode (finding #5)"
    );
}

#[test]
fn test_create_worktree_settings_adds_resolved_work_permissions_nested_layout() {
    let temp_dir = TempDir::new().unwrap();
    let repo_root = temp_dir.path().join("repo");
    let worktree = temp_dir.path().join("worktree");

    // Create the main .loom/work directory (simulates the nested state dir).
    let main_work = repo_root.join(".loom").join("work");
    std::fs::create_dir_all(&main_work).unwrap();

    // Create the worktree's real .loom/ dir and its work symlink pointing at
    // the main repo's .loom/work — the shape `ensure_work_symlink` plants.
    let worktree_loom = worktree.join(".loom");
    std::fs::create_dir_all(&worktree_loom).unwrap();
    let worktree_work_link = worktree_loom.join("work");
    #[cfg(unix)]
    std::os::unix::fs::symlink(&main_work, &worktree_work_link).unwrap();

    let main_claude = repo_root.join(".claude");
    std::fs::create_dir_all(&main_claude).unwrap();
    let main_settings_path = main_claude.join("settings.json");
    std::fs::write(&main_settings_path, "{}").unwrap();

    let worktree_settings_path = worktree.join("settings.json");
    create_worktree_settings(&main_settings_path, &worktree_settings_path, &worktree).unwrap();

    let content = std::fs::read_to_string(&worktree_settings_path).unwrap();
    let settings: Value = serde_json::from_str(&content).unwrap();
    let (allow, _deny) = extract_permissions(&settings);

    // The nested `.loom/work` link must be the one resolved — never the
    // absent legacy `.work` — and grant the same narrow permission set.
    let resolved_work = worktree_work_link.canonicalize().unwrap();
    let resolved_str = resolved_work.to_string_lossy();
    assert!(
        !allow.contains(&format!("Read(/{}/**)", resolved_str)),
        "Should NOT contain a blanket Read(//resolved/**)"
    );
    assert!(allow.contains(&format!("Read(/{}/signals/**)", resolved_str)));
    assert!(allow.contains(&format!("Read(/{}/config.toml)", resolved_str)));
    assert!(allow.contains(&format!("Read(/{}/handoffs/**)", resolved_str)));
    assert!(!allow.contains(&format!("Edit(/{}/**)", resolved_str)));
}

#[test]
fn create_worktree_settings_never_grants_a_blanket_read_over_the_state_root() {
    let temp_dir = TempDir::new().unwrap();
    let repo_root = temp_dir.path().join("repo");
    let worktree = temp_dir.path().join("worktree");

    // Nested layout: worktree/.loom/work -> repo/.loom/work.
    let main_work = repo_root.join(".loom").join("work");
    std::fs::create_dir_all(&main_work).unwrap();
    let worktree_loom = worktree.join(".loom");
    std::fs::create_dir_all(&worktree_loom).unwrap();
    let worktree_work_link = worktree_loom.join("work");
    #[cfg(unix)]
    std::os::unix::fs::symlink(&main_work, &worktree_work_link).unwrap();

    let main_claude = repo_root.join(".claude");
    std::fs::create_dir_all(&main_claude).unwrap();
    let main_settings_path = main_claude.join("settings.json");
    std::fs::write(&main_settings_path, "{}").unwrap();

    let worktree_settings_path = worktree.join("settings.json");
    create_worktree_settings(&main_settings_path, &worktree_settings_path, &worktree).unwrap();

    let content = std::fs::read_to_string(&worktree_settings_path).unwrap();
    let settings: Value = serde_json::from_str(&content).unwrap();
    let (allow, deny) = extract_permissions(&settings);

    let resolved = worktree_work_link.canonicalize().unwrap();
    let resolved_str = resolved.to_string_lossy();
    assert!(
        !allow.contains(&format!("Read(/{}/**)", resolved_str)),
        "create_worktree_settings must never grant a blanket read over the state root \
         (S-1: exposes admin.token / user.token)"
    );

    // `.claude/settings.json` is written by a different code path than
    // settings.local.json, so it must be safe standalone: explicit denies
    // for the resolved-absolute token paths, not just a narrowed allow.
    for deny_perm in token_read_denies(&resolved_str) {
        assert!(is_parent_glob_token_deny(&deny_perm), "{deny_perm}");
        let occurrences = deny.iter().filter(|p| *p == &deny_perm).count();
        assert_eq!(
            occurrences, 1,
            "create_worktree_settings must deny {deny_perm} exactly once, got {occurrences}"
        );
    }
}

/// Set up a scratch repo/worktree with a `.loom/work` symlink and the given
/// main-repo `.claude/settings.json` body, run `create_worktree_settings`,
/// and return the resulting `(resolved_state_root, allow, deny)`.
fn run_create_worktree_settings_with_main_json(
    main_settings_json: &str,
) -> (String, Vec<String>, Vec<String>) {
    let temp_dir = TempDir::new().unwrap();
    let repo_root = temp_dir.path().join("repo");
    let worktree = temp_dir.path().join("worktree");

    let main_work = repo_root.join(".loom").join("work");
    std::fs::create_dir_all(&main_work).unwrap();
    let worktree_loom = worktree.join(".loom");
    std::fs::create_dir_all(&worktree_loom).unwrap();
    let worktree_work_link = worktree_loom.join("work");
    #[cfg(unix)]
    std::os::unix::fs::symlink(&main_work, &worktree_work_link).unwrap();

    let main_claude = repo_root.join(".claude");
    std::fs::create_dir_all(&main_claude).unwrap();
    let main_settings_path = main_claude.join("settings.json");
    std::fs::write(&main_settings_path, main_settings_json).unwrap();

    let worktree_settings_path = worktree.join("settings.json");
    create_worktree_settings(&main_settings_path, &worktree_settings_path, &worktree).unwrap();

    let content = std::fs::read_to_string(&worktree_settings_path).unwrap();
    let settings: Value = serde_json::from_str(&content).unwrap();
    let (allow, deny) = extract_permissions(&settings);

    let resolved = worktree_work_link.canonicalize().unwrap();
    (resolved.to_string_lossy().into_owned(), allow, deny)
}

#[test]
fn create_worktree_settings_normalizes_a_non_array_deny_instead_of_dropping_token_denies() {
    let (resolved_str, allow, deny) = run_create_worktree_settings_with_main_json(
        r#"{"permissions": {"deny": null, "allow": []}}"#,
    );

    for deny_perm in token_read_denies(&resolved_str) {
        assert!(
            deny.contains(&deny_perm) && is_parent_glob_token_deny(&deny_perm),
            "a non-array `deny` must be normalized rather than skip the token deny {deny_perm}"
        );
    }
    assert!(allow.contains(&format!("Read(/{}/signals/**)", resolved_str)));
    assert!(allow.contains(&format!("Read(/{}/config.toml)", resolved_str)));
    assert!(allow.contains(&format!("Read(/{}/handoffs/**)", resolved_str)));
}

#[test]
fn create_worktree_settings_normalizes_a_non_object_permissions_instead_of_dropping_token_denies() {
    let (resolved_str, allow, deny) =
        run_create_worktree_settings_with_main_json(r#"{"permissions": "nonsense"}"#);

    for deny_perm in token_read_denies(&resolved_str) {
        assert!(
            deny.contains(&deny_perm) && is_parent_glob_token_deny(&deny_perm),
            "a non-object `permissions` must be normalized rather than skip the token deny {deny_perm}"
        );
    }
    assert!(allow.contains(&format!("Read(/{}/signals/**)", resolved_str)));
    assert!(allow.contains(&format!("Read(/{}/config.toml)", resolved_str)));
    assert!(allow.contains(&format!("Read(/{}/handoffs/**)", resolved_str)));
}

#[test]
fn create_worktree_settings_normalization_preserves_pre_existing_deny_entries() {
    let (resolved_str, _allow, deny) = run_create_worktree_settings_with_main_json(
        r#"{"permissions": {"deny": ["Read(/etc/shadow)"]}}"#,
    );

    assert!(
        deny.contains(&"Read(/etc/shadow)".to_string()),
        "normalization must not discard a pre-existing valid deny entry"
    );
    for deny_perm in token_read_denies(&resolved_str) {
        assert!(
            deny.contains(&deny_perm) && is_parent_glob_token_deny(&deny_perm),
            "the token deny {deny_perm} must still be added alongside the existing entry"
        );
    }
}

#[test]
fn test_create_worktree_settings_no_work_symlink() {
    let temp_dir = TempDir::new().unwrap();
    let worktree = temp_dir.path().join("worktree");
    std::fs::create_dir_all(&worktree).unwrap();

    // No .work symlink exists -- function should still succeed
    let main_settings_path = temp_dir.path().join("nonexistent_settings.json");
    let worktree_settings_path = worktree.join("settings.json");
    create_worktree_settings(&main_settings_path, &worktree_settings_path, &worktree).unwrap();

    let content = std::fs::read_to_string(&worktree_settings_path).unwrap();
    let settings: Value = serde_json::from_str(&content).unwrap();

    assert_eq!(settings["hasTrustDialogAccepted"], json!(true));
    assert!(
        settings["permissions"].get("defaultMode").is_none(),
        "Base settings.json must not write defaultMode"
    );

    // Allow array should not exist (no permissions were added)
    let allow = settings
        .get("permissions")
        .and_then(|p| p.get("allow"))
        .and_then(|a| a.as_array());
    assert!(
        allow.is_none(),
        "No allow array should exist when there is no .work symlink"
    );
}

#[test]
fn test_add_settings_local_to_worktree_gitignore_creates_exclude() {
    let temp_dir = TempDir::new().unwrap();
    let repo_root = temp_dir.path().join("repo");

    let gitdir = repo_root.join(".git");
    std::fs::create_dir_all(&gitdir).unwrap();

    add_settings_local_to_worktree_gitignore(&repo_root).unwrap();

    // The common git dir, NOT .git/worktrees/<stage-id>/ — see the
    // function's doc comment for why a per-worktree info/exclude is
    // inert.
    let exclude_path = gitdir.join("info/exclude");
    assert!(exclude_path.exists(), "exclude file should be created");

    let content = std::fs::read_to_string(&exclude_path).unwrap();
    for pattern in WORKTREE_EXCLUDE_PATTERNS {
        assert!(
            content.contains(pattern),
            "exclude should contain pattern {pattern:?}, got: {content}"
        );
    }
}

#[test]
fn test_add_settings_local_to_worktree_gitignore_idempotent() {
    let temp_dir = TempDir::new().unwrap();
    let repo_root = temp_dir.path().join("repo");

    let gitdir = repo_root.join(".git");
    std::fs::create_dir_all(&gitdir).unwrap();

    add_settings_local_to_worktree_gitignore(&repo_root).unwrap();
    add_settings_local_to_worktree_gitignore(&repo_root).unwrap();

    let exclude_path = gitdir.join("info/exclude");
    let content = std::fs::read_to_string(&exclude_path).unwrap();

    for pattern in WORKTREE_EXCLUDE_PATTERNS {
        let count = content.lines().filter(|l| l.trim() == *pattern).count();
        assert_eq!(
            count, 1,
            "pattern {pattern:?} should appear exactly once after two calls, got: {content}"
        );
    }
}

#[test]
fn test_add_settings_local_to_main_gitignore_creates_exclude() {
    let temp_dir = TempDir::new().unwrap();
    let repo_root = temp_dir.path().join("repo");

    let gitdir = repo_root.join(".git");
    std::fs::create_dir_all(&gitdir).unwrap();

    add_settings_local_to_main_gitignore(&repo_root).unwrap();

    let exclude_path = gitdir.join("info/exclude");
    assert!(exclude_path.exists(), "exclude file should be created");

    let content = std::fs::read_to_string(&exclude_path).unwrap();
    for pattern in WORKTREE_EXCLUDE_PATTERNS {
        assert!(
            content.contains(pattern),
            "exclude should contain pattern {pattern:?}, got: {content}"
        );
    }
}

#[test]
fn test_add_settings_local_appends_to_existing_exclude() {
    let temp_dir = TempDir::new().unwrap();
    let repo_root = temp_dir.path().join("repo");

    let gitdir = repo_root.join(".git");
    let info_dir = gitdir.join("info");
    std::fs::create_dir_all(&info_dir).unwrap();
    std::fs::write(info_dir.join("exclude"), "# existing patterns\n*.log\n").unwrap();

    add_settings_local_to_main_gitignore(&repo_root).unwrap();

    let content = std::fs::read_to_string(info_dir.join("exclude")).unwrap();
    assert!(content.contains("*.log"), "existing patterns preserved");
    assert!(
        content.contains(".claude/settings.local.json"),
        "new pattern appended"
    );
}
