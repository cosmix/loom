use super::*;
use tempfile::TempDir;

/// Write a `~/.loom/config.toml`-shaped file under a caller-supplied temp
/// directory and return its path, for use with
/// [`crate::user_config::redirect_user_config`]. Does not touch `$HOME` — the
/// redirect is a per-thread seam inside `user_config`, not an env var, so
/// this needs no directory layout beyond "a file somewhere in `temp`".
fn write_user_config(temp: &TempDir, body: &str) -> PathBuf {
    let path = temp.path().join("user-config.toml");
    fs::write(&path, body).unwrap();
    path
}

/// A project root the upward walk cannot escape: the `.git` marker bounds it,
/// so a workspace anywhere above the temp directory can never be adopted.
fn bare_repo(temp: &TempDir) -> PathBuf {
    let root = temp.path().join("repo");
    fs::create_dir_all(root.join(".git")).unwrap();
    root
}

/// Plant a workspace of `layout` under `repo_root` and return its state root.
/// Resolution is keyed on `config.toml`, so the file is what makes it one.
fn plant_workspace(repo_root: &Path, layout: Layout) -> PathBuf {
    let root = match layout {
        Layout::Nested => repo_root.join(".loom").join("work"),
        Layout::Legacy => repo_root.join(".work"),
    };
    fs::create_dir_all(&root).unwrap();
    fs::write(root.join("config.toml"), "[plan]\nplan_id = \"p\"\n").unwrap();
    root
}

#[test]
fn test_main_project_root_non_symlink() {
    // Create a temporary directory structure simulating a main repo
    let temp_dir = TempDir::new().unwrap();
    let project_root = bare_repo(&temp_dir);

    // Create the state directory (not a symlink)
    plant_workspace(&project_root, Layout::Nested);

    let work_dir = WorkDir::new(&project_root).unwrap();

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

    // Create main repo structure: base/repo/.loom/work/
    let main_repo = bare_repo(&temp_dir);
    plant_workspace(&main_repo, Layout::Nested);

    // Create worktree structure: base/repo/.worktrees/my-worktree/.loom/
    let worktree = main_repo.join(".worktrees").join("my-worktree");
    let worktree_loom = worktree.join(".loom");
    fs::create_dir_all(&worktree_loom).unwrap();

    // Create symlink: worktree/.loom/work -> ../../../.loom/work
    let worktree_work = worktree_loom.join("work");
    #[cfg(unix)]
    std::os::unix::fs::symlink("../../../.loom/work", &worktree_work).unwrap();
    #[cfg(windows)]
    std::os::windows::fs::symlink_dir("../../../.loom/work", &worktree_work).unwrap();

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
    // Create a temporary directory without any workspace
    let temp_dir = TempDir::new().unwrap();
    let project_root = bare_repo(&temp_dir);

    let work_dir = WorkDir::new(&project_root).unwrap();

    // The state root doesn't exist, so is_symlink() returns false
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
    let project_root = bare_repo(&temp);

    // Create the workspace at the project root
    let work_dir_path = plant_workspace(&project_root, Layout::Nested);

    // Create a subdirectory (simulates agent cd'ing into loom/)
    let subdir = project_root.join("loom");
    fs::create_dir(&subdir).unwrap();

    // WorkDir::new from subdirectory should find the parent's workspace
    let work_dir = WorkDir::new(&subdir).unwrap();
    assert_eq!(
        work_dir.root().canonicalize().unwrap(),
        work_dir_path.canonicalize().unwrap(),
        "WorkDir should find .loom/work in a parent directory"
    );
}

#[test]
fn test_open_or_initialize_idempotent() {
    let temp = TempDir::new().unwrap();
    let project_root = bare_repo(&temp);
    let state_root = project_root.join(".loom").join("work");

    let work_dir = WorkDir::new(&project_root).unwrap();
    // First call initializes
    work_dir.open_or_initialize().unwrap();
    assert!(state_root.is_dir());

    // Second call must succeed without error
    let work_dir2 = WorkDir::new(&project_root).unwrap();
    work_dir2
        .open_or_initialize()
        .expect("open_or_initialize must be idempotent on an existing workspace");
    // Structure still intact
    assert!(state_root.join("stages").is_dir());
}

#[test]
fn adopt_existing_requires_root_to_exist() {
    let temp = TempDir::new().unwrap();
    let work_dir = WorkDir::new(temp.path().join(".loom").join("work")).unwrap();
    assert!(
        work_dir.adopt_existing().is_err(),
        "adopt_existing must bail when the state directory does not exist"
    );
}

#[test]
fn adopt_existing_fills_in_layout_for_a_phantom_work_dir() {
    let temp = TempDir::new().unwrap();
    let project_root = bare_repo(&temp);

    // Simulate the phantom state directory: only derived caches exist, no
    // orchestration state and none of the layout initialize() creates.
    let work_path = project_root.join(".loom").join("work");
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
    let work_dir = WorkDir::new(bare_repo(&temp)).unwrap();

    work_dir.initialize().unwrap();

    for path in [work_dir.root().to_path_buf(), work_dir.root().join("pids")] {
        let mode = fs::metadata(path).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o700);
    }
}

#[test]
fn disputes_dir_path_shape() {
    let temp = TempDir::new().unwrap();
    let project_root = bare_repo(&temp);
    plant_workspace(&project_root, Layout::Nested);
    let wd = WorkDir::new(&project_root).unwrap();
    assert_eq!(wd.disputes_dir(), wd.root().join("disputes"));
}

#[test]
fn plan_versions_dir_path_shape() {
    let temp = TempDir::new().unwrap();
    let project_root = bare_repo(&temp);
    plant_workspace(&project_root, Layout::Nested);
    let wd = WorkDir::new(&project_root).unwrap();
    assert_eq!(wd.plan_versions_dir(), wd.root().join("plan_versions"));
}

#[test]
fn test_workdir_new_falls_back_when_no_work_dir() {
    let temp = TempDir::new().unwrap();
    let project_root = bare_repo(&temp);

    // No workspace anywhere
    let subdir = project_root.join("some/nested/dir");
    fs::create_dir_all(&subdir).unwrap();

    // WorkDir::new should fall back to subdir/.loom/work
    let work_dir = WorkDir::new(&subdir).unwrap();
    assert_eq!(
        work_dir.root(),
        subdir.join(".loom").join("work"),
        "Without a workspace anywhere, should fall back to base_path/.loom/work"
    );
    assert_eq!(work_dir.layout(), Layout::Nested);
}

#[test]
fn test_workdir_new_legacy_hint_naming_work_dir_resolves_to_itself() {
    let temp = TempDir::new().unwrap();
    let project_root = bare_repo(&temp);

    // No workspace exists anywhere, and the hint itself already names a
    // legacy `.work` directory — mirrors a stale LOOM_WORK_DIR pin left
    // behind after its `.work/` was deleted (the phantom-.work bug). The
    // nested spelling is covered by `resolver::state_root_shaped_base_...`.
    let hint = project_root.join(".work");
    let work_dir = WorkDir::new(&hint).unwrap();
    assert_eq!(
        work_dir.root(),
        hint,
        "A hint naming .work directly must resolve to itself, not <hint>/.work"
    );
    assert_eq!(work_dir.layout(), Layout::Legacy);
}

// ----- Centralized config.toml API tests -----

use crate::plan::schema::SandboxConfig;

fn init_work(temp: &TempDir) -> PathBuf {
    let work = temp.path().join(".loom").join("work");
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
            model_window_tokens: None,
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
        crate::models::constants::DEFAULT_CONTEXT_CEILING_TOKENS
    );

    write_context_config(
        &work,
        &ContextConfig {
            ceiling_tokens: 250_000,
            subagent_ceiling_tokens: 100_000,
            model_window_tokens: None,
        },
    )
    .unwrap();

    // Config tier wins over the default, stage tier over both.
    assert_eq!(resolve_context_ceiling_tokens(&work, None), 250_000);
    assert_eq!(resolve_context_ceiling_tokens(&work, Some(80_000)), 80_000);
}

#[test]
fn global_config_tier_terminal_backend_falls_through_to_user_config() {
    let temp = TempDir::new().unwrap();
    let path = write_user_config(&temp, "[terminal]\nbackend = \"tmux\"\n");
    let _redirect = crate::user_config::redirect_user_config(path);

    let work = init_work(&temp);

    // Workspace [terminal] absent: falls through to the user config.
    let config = read_terminal_config(&work).unwrap();
    assert_eq!(
        config.backend,
        crate::models::session::SessionBackendKind::Tmux
    );

    // A present workspace [terminal] wins whole, regardless of the user config.
    write_terminal_config(
        &work,
        &TerminalConfig {
            backend: crate::models::session::SessionBackendKind::Native,
        },
    )
    .unwrap();
    let config = read_terminal_config(&work).unwrap();
    assert_eq!(
        config.backend,
        crate::models::session::SessionBackendKind::Native
    );
}

#[test]
fn global_config_tier_context_ceiling_falls_through_to_user_config() {
    let temp = TempDir::new().unwrap();
    let path = write_user_config(&temp, "[context]\nceiling_tokens = 123456\n");
    let _redirect = crate::user_config::redirect_user_config(path);

    let work = init_work(&temp);

    // Workspace [context] absent: ceiling_tokens falls through to the user
    // config; subagent_ceiling_tokens keeps deriving from the built-in.
    let config = read_context_config(&work).unwrap();
    assert_eq!(config.ceiling_tokens, 123_456);
    assert_eq!(
        config.subagent_ceiling_tokens,
        crate::models::constants::DEFAULT_SUBAGENT_CEILING_TOKENS
    );

    // A present workspace [context] wins whole.
    write_context_config(
        &work,
        &ContextConfig {
            ceiling_tokens: 999_999,
            subagent_ceiling_tokens: 111_111,
            model_window_tokens: None,
        },
    )
    .unwrap();
    let config = read_context_config(&work).unwrap();
    assert_eq!(config.ceiling_tokens, 999_999);
    assert_eq!(config.subagent_ceiling_tokens, 111_111);
}

/// Regression guard for the developer's-own-home hazard: with no redirect
/// installed, `UserConfig::load` must never fall through to a real
/// `~/.loom/config.toml` — `loom config` is the tool that creates that file,
/// so any lib test reaching this path without a redirect would otherwise be
/// hermetic only on a machine that had never run it.
#[test]
fn global_config_tier_context_ceiling_ignores_the_real_home_when_no_redirect_is_installed() {
    let temp = TempDir::new().unwrap();
    let work = init_work(&temp);

    let config = read_context_config(&work).unwrap();
    assert_eq!(config, ContextConfig::default());
}

/// A malformed workspace `[context]` section must still fall through to the
/// user config tier, not skip straight past it to the built-in default —
/// `resolve_context_ceiling_tokens`'s documented resolution order names the
/// user config as a middle tier, not a tier only reachable when the
/// workspace section is cleanly absent.
#[test]
fn global_config_tier_context_ceiling_survives_a_malformed_workspace_section() {
    let temp = TempDir::new().unwrap();
    let path = write_user_config(&temp, "[context]\nceiling_tokens = 654321\n");
    let _redirect = crate::user_config::redirect_user_config(path);

    let work = init_work(&temp);
    fs::write(
        work.join("config.toml"),
        "[context]\nceiling_tokens = \"not-a-number\"\n",
    )
    .unwrap();

    assert_eq!(resolve_context_ceiling_tokens(&work, None), 654_321);
}

/// Regression guard against a future key-level merge: a workspace `[context]`
/// section that sets only `subagent_ceiling_tokens` must still win WHOLE over
/// the user config's `ceiling_tokens` — the section, not the individual key,
/// is what "present" means for this fallback tier.
#[test]
fn global_config_tier_a_partial_workspace_context_section_still_wins_whole() {
    let temp = TempDir::new().unwrap();
    let path = write_user_config(&temp, "[context]\nceiling_tokens = 123456\n");
    let _redirect = crate::user_config::redirect_user_config(path);

    let work = init_work(&temp);
    fs::write(
        work.join("config.toml"),
        "[context]\nsubagent_ceiling_tokens = 111111\n",
    )
    .unwrap();

    let config = read_context_config(&work).unwrap();
    assert_eq!(
        config.ceiling_tokens,
        crate::models::constants::DEFAULT_CONTEXT_CEILING_TOKENS
    );
    assert_eq!(config.subagent_ceiling_tokens, 111_111);
}

/// Same regression guard for `[terminal]`: an EMPTY workspace section is
/// still a present section, and must win WHOLE over the user config's
/// `terminal.backend`.
#[test]
fn global_config_tier_an_empty_workspace_terminal_section_still_wins_whole() {
    let temp = TempDir::new().unwrap();
    let path = write_user_config(&temp, "[terminal]\nbackend = \"tmux\"\n");
    let _redirect = crate::user_config::redirect_user_config(path);

    let work = init_work(&temp);
    fs::write(work.join("config.toml"), "[terminal]\n").unwrap();

    let config = read_terminal_config(&work).unwrap();
    assert_eq!(
        config.backend,
        crate::models::session::SessionBackendKind::Native
    );
}

/// Resolution order, layout, and the hop count that follows from it.
///
/// Every assertion on a path that exists canonicalizes BOTH sides: on macOS a
/// `TempDir` lives under a symlinked `/var`, so a one-sided comparison fails
/// there for reasons that have nothing to do with resolution.
mod resolver {
    use super::*;

    #[test]
    fn nested_config_wins_over_a_sibling_legacy_config() {
        let temp = TempDir::new().unwrap();
        let repo = bare_repo(&temp);
        let nested = plant_workspace(&repo, Layout::Nested);
        plant_workspace(&repo, Layout::Legacy);

        let wd = WorkDir::new(&repo).unwrap();
        assert_eq!(
            wd.root().canonicalize().unwrap(),
            nested.canonicalize().unwrap()
        );
        assert_eq!(wd.layout(), Layout::Nested);
    }

    #[test]
    fn a_lone_legacy_config_is_the_resolved_root() {
        let temp = TempDir::new().unwrap();
        let repo = bare_repo(&temp);
        let legacy = plant_workspace(&repo, Layout::Legacy);

        let wd = WorkDir::new(&repo).unwrap();
        assert_eq!(
            wd.root().canonicalize().unwrap(),
            legacy.canonicalize().unwrap()
        );
        assert_eq!(wd.layout(), Layout::Legacy);
    }

    /// `.loom/cache/` exists in any project that has run `loom map`, and
    /// `~/.loom/config.toml` at the user level. Neither marks a workspace:
    /// resolution is keyed on `.loom/work/config.toml`, not on `.loom/`.
    #[test]
    fn a_bare_loom_cache_is_not_a_workspace() {
        let temp = TempDir::new().unwrap();
        let repo = bare_repo(&temp);
        fs::create_dir_all(repo.join(".loom").join("cache").join("context-v1")).unwrap();

        let wd = WorkDir::new(&repo).unwrap();
        assert_eq!(wd.root(), repo.join(".loom").join("work"));
        assert_eq!(wd.layout(), Layout::Nested);
    }

    /// The upward walk stops at the repo root, so an unrelated workspace above
    /// the project — `~/.loom/work` being the live hazard — is never adopted.
    #[test]
    fn the_upward_walk_stops_at_the_git_repo_root() {
        let temp = TempDir::new().unwrap();
        plant_workspace(temp.path(), Layout::Nested);

        let repo = bare_repo(&temp);
        let inner = repo.join("loom").join("src");
        fs::create_dir_all(&inner).unwrap();

        let wd = WorkDir::new(&inner).unwrap();
        assert_eq!(
            wd.root(),
            inner.join(".loom").join("work"),
            "a workspace above the repo root must not be adopted from inside it"
        );
    }

    /// With no `.git` anywhere in the ancestry there is no repository to bound
    /// the walk at, so the walk adopts nothing — the `~/.loom/work` hazard:
    /// one `loom init` in `$HOME` must not claim every later command issued
    /// from a non-git directory beneath it.
    ///
    /// Deliberately does NOT use `bare_repo`: planting a `.git` is exactly what
    /// hides this case, which is why none of the other resolver tests reaches
    /// it. The direct check at `base` itself is unaffected by the bound and is
    /// asserted here too, since it is what a hook's `LOOM_WORK_DIR` and a
    /// non-git project root both rely on.
    #[test]
    fn a_workspace_above_a_git_free_directory_is_never_adopted() {
        let temp = TempDir::new().unwrap();
        let above = plant_workspace(temp.path(), Layout::Nested);

        let inner = temp.path().join("not-a-repo").join("deeper");
        fs::create_dir_all(&inner).unwrap();

        let wd = WorkDir::new(&inner).unwrap();
        assert_eq!(
            wd.root(),
            inner.join(".loom").join("work"),
            "a workspace above a directory with no .git must not be adopted"
        );
        assert_eq!(wd.layout(), Layout::Nested);

        // Still resolved when it IS the base, git marker or not.
        let wd = WorkDir::new(temp.path()).unwrap();
        assert_eq!(
            wd.root().canonicalize().unwrap(),
            above.canonicalize().unwrap()
        );
    }

    /// Every hook is handed `LOOM_WORK_DIR`, which names the state directory
    /// ITSELF. Appending a second state root under it is the phantom-directory
    /// bug; the nested spelling's final component is the unremarkable `work`,
    /// so the check has to look at the parent too.
    #[test]
    fn state_root_shaped_base_resolves_to_itself() {
        let temp = TempDir::new().unwrap();
        let repo = bare_repo(&temp);
        let state = plant_workspace(&repo, Layout::Nested);

        let wd = WorkDir::new(&state).unwrap();
        assert_eq!(
            wd.root().canonicalize().unwrap(),
            state.canonicalize().unwrap()
        );
        assert_eq!(wd.layout(), Layout::Nested);

        // The same shape for a pin whose directory has since been deleted:
        // it must resolve back to that missing path, not nest under it.
        let stale = temp.path().join("gone").join(".loom").join("work");
        let wd = WorkDir::new(&stale).unwrap();
        assert_eq!(wd.root(), stale);
        assert_eq!(wd.layout(), Layout::Nested);
    }

    #[test]
    fn project_root_applies_the_layouts_hop_count() {
        let temp = TempDir::new().unwrap();

        let nested_repo = bare_repo(&temp);
        plant_workspace(&nested_repo, Layout::Nested);
        let wd = WorkDir::new(&nested_repo).unwrap();
        assert_eq!(
            wd.project_root().unwrap().canonicalize().unwrap(),
            nested_repo.canonicalize().unwrap()
        );
        assert_eq!(
            wd.main_project_root().unwrap().canonicalize().unwrap(),
            nested_repo.canonicalize().unwrap()
        );

        let legacy_repo = temp.path().join("legacy");
        fs::create_dir_all(legacy_repo.join(".git")).unwrap();
        plant_workspace(&legacy_repo, Layout::Legacy);
        let wd = WorkDir::new(&legacy_repo).unwrap();
        assert_eq!(
            wd.project_root().unwrap().canonicalize().unwrap(),
            legacy_repo.canonicalize().unwrap()
        );
        assert_eq!(
            wd.main_project_root().unwrap().canonicalize().unwrap(),
            legacy_repo.canonicalize().unwrap()
        );
    }

    /// loom never creates a `.work/`: a repo with no config.toml anywhere
    /// initializes at `.loom/work`, whatever spelling once existed elsewhere.
    #[test]
    fn a_fresh_repo_never_gets_a_legacy_work_dir() {
        let temp = TempDir::new().unwrap();
        let repo = bare_repo(&temp);

        let wd = WorkDir::new(&repo).unwrap();
        assert_eq!(wd.root(), repo.join(".loom").join("work"));
        assert_eq!(wd.layout(), Layout::Nested);

        wd.initialize().unwrap();
        assert!(repo.join(".loom").join("work").join("stages").is_dir());
        assert!(!repo.join(".work").exists());
    }

    /// The back-compat policy: whatever root resolves is the workspace for
    /// WRITES as well as reads. A project mid-plan on `.work/` keeps getting
    /// its config writes there, and never grows a second `.loom/work`.
    #[test]
    fn a_legacy_root_is_written_through_not_migrated() {
        let temp = TempDir::new().unwrap();
        let repo = bare_repo(&temp);
        let legacy = plant_workspace(&repo, Layout::Legacy);

        let wd = WorkDir::new(&repo).unwrap();
        assert_eq!(wd.layout(), Layout::Legacy);

        let mut doc = read_config(wd.root()).unwrap();
        doc["plan"]["plan_id"] = toml_edit::value("legacy-stays-put");
        write_config(wd.root(), &doc).unwrap();

        let written = fs::read_to_string(legacy.join("config.toml")).unwrap();
        assert!(
            written.contains("legacy-stays-put"),
            "the write must land in the resolved legacy root"
        );
        assert!(
            !repo.join(".loom").exists(),
            "a legacy workspace must not sprout a nested one"
        );
    }

    /// The worktree symlink follows the resolved layout: one component for a
    /// legacy workspace, `.loom/work` under a REAL `.loom/` for a nested one.
    #[test]
    fn the_worktree_symlink_follows_the_resolved_layout() {
        let temp = TempDir::new().unwrap();

        let legacy_repo = temp.path().join("legacy");
        fs::create_dir_all(legacy_repo.join(".git")).unwrap();
        plant_workspace(&legacy_repo, Layout::Legacy);
        let legacy_worktree = legacy_repo.join(".worktrees").join("stage");
        fs::create_dir_all(&legacy_worktree).unwrap();
        crate::git::worktree::ensure_work_symlink(&legacy_worktree, &legacy_repo).unwrap();

        let legacy_link = legacy_worktree.join(".work");
        assert!(legacy_link.is_symlink());
        assert_eq!(
            fs::read_link(&legacy_link).unwrap(),
            Path::new("../../.work")
        );

        let repo = bare_repo(&temp);
        let state = plant_workspace(&repo, Layout::Nested);
        let worktree = repo.join(".worktrees").join("stage");
        fs::create_dir_all(&worktree).unwrap();
        crate::git::worktree::ensure_work_symlink(&worktree, &repo).unwrap();

        let worktree_loom = worktree.join(".loom");
        assert!(
            worktree_loom.is_dir() && !worktree_loom.is_symlink(),
            "the worktree's .loom/ must be a real directory, not a link"
        );
        let link = worktree_loom.join("work");
        assert!(link.is_symlink());
        assert_eq!(
            fs::read_link(&link).unwrap(),
            Path::new("../../../.loom/work")
        );
        assert_eq!(
            link.canonicalize().unwrap(),
            state.canonicalize().unwrap(),
            "the link must resolve to the main repo's state root"
        );
    }
}
