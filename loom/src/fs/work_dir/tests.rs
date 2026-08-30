use super::*;
use tempfile::TempDir;

#[test]
fn test_main_project_root_non_symlink() {
    // Create a temporary directory structure simulating a main repo
    let temp_dir = TempDir::new().unwrap();
    let project_root = temp_dir.path();

    // Create .work directory (not a symlink)
    let work_dir_path = project_root.join(".work");
    fs::create_dir(&work_dir_path).unwrap();

    let work_dir = WorkDir::new(project_root).unwrap();

    // main_project_root should return the same as project_root for non-symlink
    let main_root = work_dir.main_project_root();
    assert!(main_root.is_some());
    assert_eq!(
        main_root.unwrap().canonicalize().unwrap(),
        project_root.canonicalize().unwrap()
    );
}

#[test]
fn test_main_project_root_with_symlink() {
    // Create a temporary directory structure simulating main repo and worktree
    let temp_dir = TempDir::new().unwrap();
    let base = temp_dir.path();

    // Create main repo structure: base/main-repo/.work/
    let main_repo = base.join("main-repo");
    let main_work = main_repo.join(".work");
    fs::create_dir_all(&main_work).unwrap();

    // Create worktree structure: base/main-repo/.worktrees/my-worktree/
    let worktree = main_repo.join(".worktrees").join("my-worktree");
    fs::create_dir_all(&worktree).unwrap();

    // Create symlink: worktree/.work -> ../../.work
    let worktree_work = worktree.join(".work");
    #[cfg(unix)]
    std::os::unix::fs::symlink("../../.work", &worktree_work).unwrap();
    #[cfg(windows)]
    std::os::windows::fs::symlink_dir("../../.work", &worktree_work).unwrap();

    // Create WorkDir from worktree perspective
    let work_dir = WorkDir::new(&worktree).unwrap();

    // main_project_root should follow the symlink and return main repo root
    let main_root = work_dir.main_project_root();
    assert!(main_root.is_some());
    assert_eq!(
        main_root.unwrap().canonicalize().unwrap(),
        main_repo.canonicalize().unwrap()
    );
}

#[test]
fn test_main_project_root_missing_work_dir() {
    // Create a temporary directory without .work
    let temp_dir = TempDir::new().unwrap();
    let project_root = temp_dir.path();

    let work_dir = WorkDir::new(project_root).unwrap();

    // .work doesn't exist, so is_symlink() returns false
    // project_root() should still work
    let main_root = work_dir.main_project_root();
    assert!(main_root.is_some());
    assert_eq!(
        main_root.unwrap().canonicalize().unwrap(),
        project_root.canonicalize().unwrap()
    );
}

#[test]
fn test_workdir_new_searches_upward() {
    let temp = TempDir::new().unwrap();
    let project_root = temp.path();

    // Create .work at project root
    let work_dir_path = project_root.join(".work");
    fs::create_dir(&work_dir_path).unwrap();

    // Create a subdirectory (simulates agent cd'ing into loom/)
    let subdir = project_root.join("loom");
    fs::create_dir(&subdir).unwrap();

    // WorkDir::new from subdirectory should find parent's .work
    let work_dir = WorkDir::new(&subdir).unwrap();
    assert_eq!(
        work_dir.root().canonicalize().unwrap(),
        work_dir_path.canonicalize().unwrap(),
        "WorkDir should find .work in parent directory"
    );
}

#[test]
fn test_open_or_initialize_idempotent() {
    let temp = TempDir::new().unwrap();
    let project_root = temp.path();

    let work_dir = WorkDir::new(project_root).unwrap();
    // First call initializes
    work_dir.open_or_initialize().unwrap();
    assert!(project_root.join(".work").is_dir());

    // Second call must succeed without error
    let work_dir2 = WorkDir::new(project_root).unwrap();
    work_dir2
        .open_or_initialize()
        .expect("open_or_initialize must be idempotent on existing .work");
    // Structure still intact
    assert!(project_root.join(".work").join("stages").is_dir());
}

#[test]
fn adopt_existing_requires_root_to_exist() {
    let temp = TempDir::new().unwrap();
    let work_dir = WorkDir::new(temp.path().join(".work")).unwrap();
    assert!(
        work_dir.adopt_existing().is_err(),
        "adopt_existing must bail when .work does not exist"
    );
}

#[test]
fn adopt_existing_fills_in_layout_for_a_phantom_work_dir() {
    let temp = TempDir::new().unwrap();
    let project_root = temp.path();

    // Simulate the phantom .work/: only derived caches exist, no
    // orchestration state and none of the layout initialize() creates.
    let work_path = project_root.join(".work");
    fs::create_dir_all(work_path.join("context")).unwrap();

    let work_dir = WorkDir::new(&work_path).unwrap();
    work_dir.adopt_existing().unwrap();

    assert!(work_path.join("stages").is_dir());
    assert!(work_path.join("sessions").is_dir());
    assert!(work_path.join("README.md").is_file());
    // The pre-existing derived cache is left untouched.
    assert!(work_path.join("context").is_dir());

    // Idempotent: a second call over the now-complete layout succeeds.
    work_dir.adopt_existing().unwrap();
}

#[test]
fn initialize_creates_private_control_directories() {
    let temp = TempDir::new().unwrap();
    let work_dir = WorkDir::new(temp.path()).unwrap();

    work_dir.initialize().unwrap();

    for path in [work_dir.root().to_path_buf(), work_dir.root().join("pids")] {
        let mode = fs::metadata(path).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o700);
    }
}

#[test]
fn disputes_dir_path_shape() {
    let temp = TempDir::new().unwrap();
    let project_root = temp.path();
    fs::create_dir_all(project_root.join(".work")).unwrap();
    let wd = WorkDir::new(project_root).unwrap();
    assert_eq!(wd.disputes_dir(), wd.root().join("disputes"));
}

#[test]
fn plan_versions_dir_path_shape() {
    let temp = TempDir::new().unwrap();
    let project_root = temp.path();
    fs::create_dir_all(project_root.join(".work")).unwrap();
    let wd = WorkDir::new(project_root).unwrap();
    assert_eq!(wd.plan_versions_dir(), wd.root().join("plan_versions"));
}

#[test]
fn test_workdir_new_falls_back_when_no_work_dir() {
    let temp = TempDir::new().unwrap();
    let project_root = temp.path();

    // No .work anywhere
    let subdir = project_root.join("some/nested/dir");
    fs::create_dir_all(&subdir).unwrap();

    // WorkDir::new should fall back to subdir/.work
    let work_dir = WorkDir::new(&subdir).unwrap();
    assert_eq!(
        work_dir.root(),
        subdir.join(".work"),
        "Without .work anywhere, should fall back to base_path/.work"
    );
}

#[test]
fn test_workdir_new_hint_naming_work_dir_resolves_to_itself() {
    let temp = TempDir::new().unwrap();
    let project_root = temp.path();

    // No .work exists anywhere, and the hint itself already names the
    // `.work` directory — mirrors a stale LOOM_WORK_DIR pin left behind
    // after its .work/ was deleted (the phantom-.work bug).
    let hint = project_root.join(".work");
    let work_dir = WorkDir::new(&hint).unwrap();
    assert_eq!(
        work_dir.root(),
        hint,
        "A hint naming .work directly must resolve to itself, not <hint>/.work"
    );
}

// ----- Centralized config.toml API tests -----

use crate::plan::schema::SandboxConfig;

fn init_work(temp: &TempDir) -> PathBuf {
    let work = temp.path().join(".work");
    fs::create_dir_all(&work).unwrap();
    work
}

#[test]
fn read_config_returns_empty_when_missing() {
    let temp = TempDir::new().unwrap();
    let work = init_work(&temp);
    let doc = read_config(&work).unwrap();
    assert!(doc.iter().next().is_none());
}

#[test]
fn read_config_preserves_comments_and_unknown_keys() {
    let temp = TempDir::new().unwrap();
    let work = init_work(&temp);
    let original = "# Top comment\n\n[plan]\n# inner comment\nsource_path = \"docs/plans/PLAN-x.md\"\nplan_id = \"x\"\nplan_name = \"X\"\nbase_branch = \"main\"\nunknown_key = \"keep me\"\n";
    fs::write(work.join("config.toml"), original).unwrap();

    let doc = read_config(&work).unwrap();
    write_config(&work, &doc).unwrap();
    let after = fs::read_to_string(work.join("config.toml")).unwrap();
    assert!(after.contains("# Top comment"));
    assert!(after.contains("# inner comment"));
    assert!(after.contains("unknown_key = \"keep me\""));
}

#[test]
fn write_then_read_plan_sandbox_round_trip() {
    let temp = TempDir::new().unwrap();
    let work = init_work(&temp);
    let mut sandbox = SandboxConfig::default();
    sandbox.network.allowed_domains = vec!["github.com".to_string()];
    write_plan_sandbox(&work, &sandbox).unwrap();
    let back = read_plan_sandbox(&work).unwrap().unwrap();
    assert_eq!(back.network.allowed_domains, vec!["github.com".to_string()]);
    assert_eq!(back.enabled, sandbox.enabled);
}

#[test]
fn writes_preserve_unrelated_sections_and_comments() {
    let temp = TempDir::new().unwrap();
    let work = init_work(&temp);
    let original = "# Header\n[plan]\nsource_path = \"a\"\nplan_id = \"id\"\nplan_name = \"n\"\nbase_branch = \"main\"\n# trailing comment\n";
    fs::write(work.join("config.toml"), original).unwrap();

    let mut sandbox = SandboxConfig::default();
    sandbox.network.allowed_domains = vec!["github.com".to_string()];
    write_plan_sandbox(&work, &sandbox).unwrap();

    let after = fs::read_to_string(work.join("config.toml")).unwrap();
    assert!(after.contains("[plan]"));
    assert!(after.contains("source_path = \"a\""));
    assert!(after.contains("# Header"));
    assert!(after.contains("[plan_sandbox]"));
}

#[test]
fn write_then_read_terminal_config_round_trip() {
    let temp = TempDir::new().unwrap();
    let work = init_work(&temp);
    let config = TerminalConfig {
        backend: crate::models::session::SessionBackendKind::Tmux,
    };
    write_terminal_config(&work, &config).unwrap();
    let back = read_terminal_config(&work).unwrap();
    assert_eq!(back, config);
}

#[test]
fn read_terminal_config_defaults_when_section_missing() {
    let temp = TempDir::new().unwrap();
    let work = init_work(&temp);
    let config = read_terminal_config(&work).unwrap();
    assert_eq!(config, TerminalConfig::default());
    assert_eq!(
        config.backend,
        crate::models::session::SessionBackendKind::Native
    );
}

#[test]
fn read_returns_none_when_section_absent() {
    let temp = TempDir::new().unwrap();
    let work = init_work(&temp);
    fs::write(
        work.join("config.toml"),
        "[plan]\nsource_path = \"x\"\nplan_id = \"id\"\nplan_name = \"n\"\nbase_branch = \"main\"\n",
    )
    .unwrap();
    assert!(read_plan_sandbox(&work).unwrap().is_none());
}

#[test]
fn update_config_round_trips_under_lock() {
    let temp = TempDir::new().unwrap();
    let work = init_work(&temp);
    update_config(&work, |doc| {
        let plan = doc.entry("plan").or_insert(toml_edit::table());
        if let Some(t) = plan.as_table_mut() {
            t["plan_id"] = toml_edit::value("abc");
        }
        Ok(())
    })
    .unwrap();
    let doc = read_config(&work).unwrap();
    assert_eq!(doc["plan"]["plan_id"].as_str(), Some("abc"));
    // No stray temp file left behind by the atomic write.
    assert!(!work.join("config.toml.tmp").exists());
}

#[test]
fn write_section_preserves_concurrently_written_plan_section() {
    // Simulates the daemon plan-rename ([plan]) and a CLI section write
    // ([plan_sandbox]) not clobbering each other. Sequential here, but the
    // point is that write_section reads-modifies-writes under the lock and
    // therefore preserves the pre-existing [plan] section.
    let temp = TempDir::new().unwrap();
    let work = init_work(&temp);

    // Daemon writes the plan section first.
    update_config(&work, |doc| {
        let plan = doc.entry("plan").or_insert(toml_edit::table());
        if let Some(t) = plan.as_table_mut() {
            t["source_path"] = toml_edit::value("doc/plans/PLAN-x.md");
            t["plan_id"] = toml_edit::value("x");
        }
        Ok(())
    })
    .unwrap();

    // CLI writes a different section.
    let mut sandbox = SandboxConfig::default();
    sandbox.network.allowed_domains = vec!["github.com".to_string()];
    write_plan_sandbox(&work, &sandbox).unwrap();

    // Both sections survive.
    let doc = read_config(&work).unwrap();
    assert_eq!(
        doc["plan"]["source_path"].as_str(),
        Some("doc/plans/PLAN-x.md")
    );
    assert!(doc.get("plan_sandbox").is_some());
}

/// Regression: a stale `[project_execution]` table left over from the
/// removed multi-backend scaffolding must not break config reads. The
/// table is an unknown section now — `toml_edit` round-trips it
/// harmlessly and the normal read path is unaffected.
#[test]
fn stale_project_execution_section_is_harmless() {
    let temp = TempDir::new().unwrap();
    let work = init_work(&temp);
    let original = "[plan]\nsource_path = \"x\"\nplan_id = \"id\"\nplan_name = \"n\"\nbase_branch = \"main\"\n\n[project_execution]\nbackend = \"native\"\n";
    fs::write(work.join("config.toml"), original).unwrap();

    // Normal config read path succeeds despite the stale table.
    let doc = read_config(&work).unwrap();
    assert!(doc.get("plan").is_some());

    // The same path used by other section readers also succeeds and the
    // stale table has no runtime effect (no known section consumes it).
    assert!(read_plan_sandbox(&work).unwrap().is_none());

    // Writing an unrelated section preserves the stale table verbatim —
    // harmless, no behavior change.
    let sandbox = SandboxConfig::default();
    write_plan_sandbox(&work, &sandbox).unwrap();
    let after = fs::read_to_string(work.join("config.toml")).unwrap();
    assert!(after.contains("[project_execution]"));
    assert!(after.contains("[plan_sandbox]"));
}

/// Regression: `[context]` has a second owner. `prompt_cache_split` is read
/// straight from the document by `native::launch::prompt_cache_split_enabled`
/// and belongs to no struct, so a re-init writing the ceilings must not take
/// it out — that would silently switch prompt cache splitting back off.
#[test]
fn write_context_config_preserves_prompt_cache_split() {
    let temp = TempDir::new().unwrap();
    let work = init_work(&temp);

    update_config(&work, |doc| {
        let context = doc.entry(CONTEXT_SECTION).or_insert(toml_edit::table());
        if let Some(table) = context.as_table_mut() {
            table["prompt_cache_split"] = toml_edit::value(true);
            table["ceiling_tokens"] = toml_edit::value(90_000_i64);
        }
        Ok(())
    })
    .unwrap();

    write_context_config(
        &work,
        &ContextConfig {
            ceiling_tokens: 200_000,
            subagent_ceiling_tokens: 100_000,
        },
    )
    .unwrap();

    let doc = read_config(&work).unwrap();
    assert_eq!(
        doc[CONTEXT_SECTION]["prompt_cache_split"].as_bool(),
        Some(true)
    );
    // The keys ContextConfig owns are still overwritten.
    let config = read_context_config(&work).unwrap();
    assert_eq!(config.ceiling_tokens, 200_000);
    assert_eq!(config.subagent_ceiling_tokens, 100_000);
}

#[test]
fn resolve_context_ceiling_walks_stage_then_config_then_default() {
    let temp = TempDir::new().unwrap();
    let work = init_work(&temp);

    // No config: the built-in default.
    assert_eq!(
        resolve_context_ceiling_tokens(&work, None),
        DEFAULT_CONTEXT_CEILING_TOKENS
    );

    write_context_config(
        &work,
        &ContextConfig {
            ceiling_tokens: 250_000,
            subagent_ceiling_tokens: 100_000,
        },
    )
    .unwrap();

    // Config tier wins over the default, stage tier over both.
    assert_eq!(resolve_context_ceiling_tokens(&work, None), 250_000);
    assert_eq!(resolve_context_ceiling_tokens(&work, Some(80_000)), 80_000);
}
