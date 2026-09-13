//! Unit tests for `session_denies.rs`: the capsule's write denies and the
//! ancestor-of-a-writable-root skip, split out of that module (CLAUDE.md
//! Rule 17 keeps a file under the 400-line ceiling).

use super::*;
use tempfile::TempDir;

const NESTED: &str = "/repo/.loom/work";

fn denies_for(
    state_root: &str,
    executable_dirs: &[PathBuf],
    plugin_entries: Option<&[String]>,
) -> Result<SessionDenies> {
    session_denies(&DenyInputs {
        repo_root: Path::new("/repo"),
        state_root: Path::new(state_root),
        worktree: None,
        executable_dirs,
        plugin_entries,
        writable_roots: &[],
    })
}

fn has(list: &[String], item: &str) -> bool {
    list.iter().any(|entry| entry == item)
}

#[test]
fn the_legacy_layout_adds_the_work_spellings_in_both_layers() {
    let legacy = denies_for("/repo/.work", &[], None).unwrap();
    assert!(has(&legacy.deny_write, "/repo/.work"), "{legacy:?}");
    assert!(has(&legacy.edit, "Edit(//repo/.work/**)"), "{legacy:?}");
    assert!(has(&legacy.edit, "Edit(.work/**)"), "{legacy:?}");

    let nested = denies_for(NESTED, &[], None).unwrap();
    assert!(!has(&nested.deny_write, "/repo/.work"), "{nested:?}");
    assert!(!has(&nested.edit, "Edit(//repo/.work/**)"), "{nested:?}");
    assert!(!has(&nested.edit, "Edit(.work/**)"), "{nested:?}");
}

#[test]
fn codex_plugin_entries_deny_everything_beside_the_codex_grant() {
    let home = TempDir::new().unwrap();
    let plugins = home.path().join(PLUGINS_DIR);
    for dir in [
        "cache",
        "marketplaces",
        "data/codex-openai-codex",
        "data/other-plugin",
    ] {
        std::fs::create_dir_all(plugins.join(dir)).unwrap();
    }
    std::fs::write(plugins.join("installed_plugins.json"), "{}").unwrap();
    std::fs::write(plugins.join("data").join("stray.json"), "{}").unwrap();

    assert_eq!(
        codex_plugin_entries(home.path()).unwrap(),
        [
            ".claude/plugins/cache/**",
            ".claude/plugins/data/other-plugin/**",
            ".claude/plugins/data/stray.json",
            ".claude/plugins/installed_plugins.json",
            ".claude/plugins/marketplaces/**",
        ]
    );
}

#[test]
fn a_home_without_plugins_lists_nothing() {
    let home = TempDir::new().unwrap();
    assert!(codex_plugin_entries(home.path()).unwrap().is_empty());
}

#[test]
fn plugin_entries_replace_the_whole_plugins_deny_in_both_layers() {
    let entries = [
        ".claude/plugins/cache/**".to_string(),
        ".claude/plugins/installed_plugins.json".to_string(),
    ];
    let lane = denies_for(NESTED, &[], Some(&entries)).unwrap();
    assert!(!has(&lane.deny_write, "~/.claude/plugins"), "{lane:?}");
    assert!(!has(&lane.edit, "Edit(~/.claude/plugins/**)"), "{lane:?}");
    for path in [
        "~/.claude/plugins/cache",
        "~/.claude/plugins/installed_plugins.json",
    ] {
        assert!(has(&lane.deny_write, path), "{path}: {lane:?}");
    }
    for rule in [
        "Edit(~/.claude/plugins/cache/**)",
        "Edit(~/.claude/plugins/installed_plugins.json)",
    ] {
        assert!(has(&lane.edit, rule), "{rule}: {lane:?}");
    }

    let claude_only = denies_for(NESTED, &[], None).unwrap();
    assert!(has(&claude_only.deny_write, "~/.claude/plugins"));
    assert!(has(&claude_only.edit, "Edit(~/.claude/plugins/**)"));
}

#[test]
fn executable_dirs_are_denied_once_in_both_layers() {
    let dirs = [
        PathBuf::from("/opt/hooks"),
        PathBuf::from("/home/op/.local/bin"),
        PathBuf::from("/opt/hooks"),
    ];
    let denies = denies_for(NESTED, &dirs, None).unwrap();
    let count = |path: &str| denies.deny_write.iter().filter(|p| *p == path).count();
    assert_eq!(count("/opt/hooks"), 1, "{denies:?}");
    assert_eq!(count("/home/op/.local/bin"), 1, "{denies:?}");
    assert!(has(&denies.edit, "Edit(//opt/hooks/**)"), "{denies:?}");
    assert!(has(&denies.edit, "Edit(//home/op/.local/bin/**)"));
}

#[test]
fn an_executable_dir_that_is_an_ancestor_of_the_repo_root_is_not_denied() {
    // `~/src` on `PATH` with the repo checked out at `~/src/loom` is
    // exactly this shape: a PATH coincidence must not deny the whole
    // ancestor tree, the checkout included.
    let ancestor_of_repo = PathBuf::from("/home/op/src");
    let repo_root = PathBuf::from("/home/op/src/repo");
    let unrelated = PathBuf::from("/opt/hooks");

    let denies = session_denies(&DenyInputs {
        repo_root: &repo_root,
        state_root: &repo_root.join(".loom").join("work"),
        worktree: None,
        executable_dirs: &[ancestor_of_repo.clone(), unrelated.clone()],
        plugin_entries: None,
        writable_roots: &[],
    })
    .unwrap();

    assert!(
        !has(&denies.deny_write, ancestor_of_repo.to_str().unwrap()),
        "{denies:?}"
    );
    assert!(
        !has(&denies.edit, "Edit(//home/op/src/**)"),
        "an ancestor of the repo root must not become a deny, {denies:?}"
    );
    assert!(
        has(&denies.deny_write, unrelated.to_str().unwrap()),
        "an unrelated operator directory must still be denied, {denies:?}"
    );
    assert!(has(&denies.edit, "Edit(//opt/hooks/**)"), "{denies:?}");
}

#[test]
fn an_executable_dir_equal_to_the_repo_root_or_an_ancestor_of_the_worktree_is_not_denied() {
    let repo_root = PathBuf::from("/home/op/src/repo");
    let worktree = repo_root.join(".worktrees").join("stage-1");
    let ancestor_of_worktree = repo_root.join(".worktrees");

    let denies = session_denies(&DenyInputs {
        repo_root: &repo_root,
        state_root: &repo_root.join(".loom").join("work"),
        worktree: Some(&worktree),
        executable_dirs: &[repo_root.clone(), ancestor_of_worktree.clone()],
        plugin_entries: None,
        writable_roots: &[],
    })
    .unwrap();

    for skipped in [&repo_root, &ancestor_of_worktree] {
        assert!(
            !has(&denies.deny_write, skipped.to_str().unwrap()),
            "{skipped:?} must not become a deny, got: {denies:?}"
        );
    }
}

#[test]
fn an_executable_dir_that_is_an_ancestor_of_the_scratch_root_is_not_denied() {
    // `~/.cache/loom-scratch` on `PATH` with the scratch root nested
    // under it is the scratch-root analogue of the repo-root PATH
    // coincidence: it must not deny the whole ancestor tree either.
    let repo_root = PathBuf::from("/home/op/src/repo");
    let scratch_parent = PathBuf::from("/home/op/.cache/loom-scratch");
    let scratch_root = scratch_parent.join("session-1");
    let unrelated = PathBuf::from("/opt/hooks");

    let denies = session_denies(&DenyInputs {
        repo_root: &repo_root,
        state_root: &repo_root.join(".loom").join("work"),
        worktree: None,
        executable_dirs: &[scratch_parent.clone(), unrelated.clone()],
        plugin_entries: None,
        writable_roots: &[scratch_root],
    })
    .unwrap();

    assert!(
        !has(&denies.deny_write, scratch_parent.to_str().unwrap()),
        "an ancestor of the scratch root must not become a deny, {denies:?}"
    );
    assert!(
        has(&denies.deny_write, unrelated.to_str().unwrap()),
        "an unrelated operator directory must still be denied, {denies:?}"
    );
}

#[test]
fn an_executable_dir_that_is_an_ancestor_of_an_allow_write_grant_is_not_denied() {
    // `~/.bun` on `PATH` with a granted `~/.bun/install/cache` must not
    // deny the grant it sits above.
    let repo_root = PathBuf::from("/home/op/src/repo");
    let grant_parent = PathBuf::from("/home/op/.bun");
    let grant = grant_parent.join("install").join("cache");
    let unrelated = PathBuf::from("/opt/hooks");

    let denies = session_denies(&DenyInputs {
        repo_root: &repo_root,
        state_root: &repo_root.join(".loom").join("work"),
        worktree: None,
        executable_dirs: &[grant_parent.clone(), unrelated.clone()],
        plugin_entries: None,
        writable_roots: &[grant],
    })
    .unwrap();

    assert!(
        !has(&denies.deny_write, grant_parent.to_str().unwrap()),
        "an ancestor of an allowWrite grant must not become a deny, {denies:?}"
    );
    assert!(
        has(&denies.deny_write, unrelated.to_str().unwrap()),
        "an unrelated operator directory must still be denied, {denies:?}"
    );
}

#[test]
fn the_writable_root_ancestor_check_compares_canonicalized_paths() {
    // The macOS shape this guards: `/tmp` symlinks to `/private/tmp`, so
    // a writable root reported through the alias must still match an
    // executable dir named by its real, canonical path.
    let real = TempDir::new().unwrap();
    let real_root = real.path().canonicalize().unwrap();
    // `canonicalize` requires the path to exist, so the scratch session
    // directory reached through the alias must be real too.
    std::fs::create_dir_all(real_root.join("session-1")).unwrap();
    let alias_parent = TempDir::new().unwrap();
    let alias = alias_parent.path().join("alias");
    std::os::unix::fs::symlink(&real_root, &alias).unwrap();
    let scratch_root = alias.join("session-1");
    let repo_root = PathBuf::from("/home/op/src/repo");

    let denies = session_denies(&DenyInputs {
        repo_root: &repo_root,
        state_root: &repo_root.join(".loom").join("work"),
        worktree: None,
        executable_dirs: std::slice::from_ref(&real_root),
        plugin_entries: None,
        writable_roots: &[scratch_root],
    })
    .unwrap();

    assert!(
        !has(&denies.deny_write, real_root.to_str().unwrap()),
        "a writable root reached through a symlinked alias must still match \
         its canonical ancestor, {denies:?}"
    );
}

#[test]
fn a_path_no_rule_can_name_literally_refuses_the_spawn() {
    let error = session_denies(&DenyInputs {
        repo_root: Path::new("/src/repo[1]"),
        state_root: Path::new("/src/repo[1]/.loom/work"),
        worktree: None,
        executable_dirs: &[],
        plugin_entries: None,
        writable_roots: &[],
    })
    .unwrap_err();
    assert!(format!("{error:#}").contains("repo[1]"), "{error:#}");
}
