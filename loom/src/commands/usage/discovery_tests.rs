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
