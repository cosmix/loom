use anyhow::Result;
use chrono::{Duration, Utc};

use super::*;

#[test]
fn parse_since_accepts_durations_and_dates() -> Result<()> {
    let before = Utc::now();
    for (spec, duration) in [
        ("7d", Duration::days(7)),
        ("24h", Duration::hours(24)),
        ("30m", Duration::minutes(30)),
    ] {
        let parsed = parse_since(spec)?;
        assert!(parsed <= Utc::now() - duration + Duration::seconds(1));
        assert!(parsed >= before - duration - Duration::seconds(1));
    }
    assert_eq!(
        parse_since("2026-08-01")?.to_rfc3339(),
        "2026-08-01T00:00:00+00:00"
    );
    assert!(parse_since("laterish").is_err());
    Ok(())
}

#[test]
fn project_slug_preserves_worktree_double_dash() {
    assert_eq!(
        project_slug(Path::new("/repo/.worktrees/measure")),
        "-repo--worktrees-measure"
    );
}

fn file(slug: &str) -> DiscoveredFile {
    DiscoveredFile {
        path: PathBuf::from(format!("/{slug}.jsonl")),
        project_slug: slug.to_owned(),
        scope: super::super::transcript::Scope::Main,
        session_id: "id".to_owned(),
        agent_id: None,
    }
}

#[test]
fn stage_filter_matches_only_worktree_suffix() {
    let files = filter_stage(
        vec![
            file("-repo--worktrees-measure"),
            file("-repo--worktrees-other"),
            file("-repo"),
        ],
        "measure",
    );
    assert_eq!(files.len(), 1);
    assert_eq!(files[0].project_slug, "-repo--worktrees-measure");
}

#[test]
fn normalize_plan_strips_status_prefix_before_plan_prefix() {
    // A live plan's source_path carries a status prefix in front of `PLAN-`;
    // both sides of the `--plan` comparison must collapse to the same stem.
    for path in [
        "doc/plans/IN_PROGRESS-PLAN-foo.md",
        "doc/plans/PLAN-foo.md",
        "foo",
        "foo.md",
    ] {
        assert_eq!(normalize_plan(Path::new(path)), "foo", "path: {path}");
    }
    assert_eq!(
        normalize_plan(Path::new("doc/plans/DONE-PLAN-foo.md")),
        "foo"
    );
}

#[test]
fn since_duration_overflow_errors_instead_of_panicking() {
    assert!(parse_since("999999999999h").is_err());
    assert!(parse_since("999999999999d").is_err());
}

#[test]
fn worktree_repository_root_resolves_gitdir_file_to_the_main_repo() {
    let temp = tempfile::tempdir().unwrap();
    let git_file = temp.path().join(".git");
    std::fs::write(
        &git_file,
        "gitdir: /home/user/repo/.git/worktrees/measure-and-govern\n",
    )
    .unwrap();
    assert_eq!(
        worktree_repository_root(&git_file),
        Some(PathBuf::from("/home/user/repo"))
    );
}

#[test]
fn worktree_repository_root_is_none_for_unparseable_contents() {
    let temp = tempfile::tempdir().unwrap();
    let git_file = temp.path().join(".git");
    std::fs::write(&git_file, "not a gitdir line\n").unwrap();
    assert_eq!(worktree_repository_root(&git_file), None);
}

/// Shared ceremony for the repo's unreadable-file/directory probes (see
/// `tests_source_graph.rs`, `tests_wiring_detection.rs`,
/// `tests_duplicate_detection.rs`): `still_readable` is whether a 0o000 path
/// stayed readable anyway (root, or a sandbox that ignores mode bits).
/// Returns whether the caller must skip - loudly, on stderr - rather than
/// asserting against a path it never actually exercised. Set
/// `LOOM_TEST_REQUIRE_UNREADABLE_FILE=1` to turn that skip into a panic
/// instead, for environments that must enforce the permission.
fn skip_if_permissions_ignored(test_name: &str, still_readable: bool) -> bool {
    if !still_readable {
        return false;
    }
    if std::env::var("LOOM_TEST_REQUIRE_UNREADABLE_FILE").as_deref() == Ok("1") {
        panic!(
            "{test_name}: this environment does not enforce 0o000 permissions (running as \
             root, or a sandbox that ignores mode bits), so the unreadable path was never \
             exercised (LOOM_TEST_REQUIRE_UNREADABLE_FILE=1 demands a real run)"
        );
    }
    eprintln!(
        "SKIP {test_name}: this environment does not enforce 0o000 permissions (running as \
         root, or a sandbox that ignores mode bits), so the unreadable path was never \
         exercised (set LOOM_TEST_REQUIRE_UNREADABLE_FILE=1 to fail instead)"
    );
    true
}

/// Under a sweep (`--all`, no explicit `--project`), one project directory
/// that can't be read must not sink the whole report: it's skipped with a
/// warning while the other project's transcript is still discovered.
#[test]
fn collect_projects_skips_unreadable_project_directory_under_sweep() {
    use std::os::unix::fs::PermissionsExt;

    let root = tempfile::tempdir().unwrap();

    let readable = root.path().join("good-project");
    std::fs::create_dir(&readable).unwrap();
    std::fs::write(readable.join("session.jsonl"), "{}\n").unwrap();

    let blocked = root.path().join("bad-project");
    std::fs::create_dir(&blocked).unwrap();
    let original_perms = std::fs::metadata(&blocked).unwrap().permissions();
    std::fs::set_permissions(&blocked, std::fs::Permissions::from_mode(0o000)).unwrap();
    let still_readable = std::fs::read_dir(&blocked).is_ok();

    let options = DiscoveryOptions {
        since: Utc::now() - Duration::days(1),
        project: None,
        all: true,
        stage: None,
        plan: None,
    };
    let result = collect_projects(root.path(), &options);

    std::fs::set_permissions(&blocked, original_perms).unwrap();

    if skip_if_permissions_ignored(
        "collect_projects_skips_unreadable_project_directory_under_sweep",
        still_readable,
    ) {
        return;
    }

    let files =
        result.expect("an unreadable project directory should be skipped, not fail the sweep");
    assert_eq!(files.len(), 1);
    assert_eq!(files[0].project_slug, "good-project");
}
