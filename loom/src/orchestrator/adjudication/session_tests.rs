use super::*;

#[test]
fn attempts_start_at_zero_and_accumulate() {
    let tmp = tempfile::tempdir().unwrap();
    let work = tmp.path();
    assert_eq!(attempt_count(work, "s1", 1), 0);
    assert_eq!(record_attempt(work, "s1", 1), 1);
    assert_eq!(record_attempt(work, "s1", 1), 2);
    assert_eq!(attempt_count(work, "s1", 1), 2);
    // Counted per dispute, not per stage.
    assert_eq!(attempt_count(work, "s1", 2), 0);
}

#[test]
fn attempt_count_survives_a_reread() {
    let tmp = tempfile::tempdir().unwrap();
    let work = tmp.path();
    record_attempt(work, "s1", 1);
    // A fresh read is what a restarted daemon does.
    assert_eq!(attempt_count(work, "s1", 1), 1);
}

#[test]
fn draft_and_attempt_paths_live_in_the_dispute_directory() {
    let tmp = tempfile::tempdir().unwrap();
    let work = tmp.path();
    let dir = dispute_dir(&work.join("disputes"), "s1", 3);
    assert_eq!(verdict_draft_file(work, "s1", 3), dir.join("verdict.json"));
    assert_eq!(attempts_file(work, "s1", 3), dir.join("attempts"));
}

#[test]
fn scratch_draft_is_named_for_the_dispute_inside_the_scratch_directory() {
    let dir = Path::new("/scratch/session-1");
    assert_eq!(scratch_verdict_draft(dir, 4), dir.join("verdict-4.json"));
}

#[test]
fn the_judge_works_in_the_disputed_worktree_when_it_exists() {
    let tmp = tempfile::tempdir().unwrap();
    let repo = tmp.path();
    let stage = Stage {
        id: "s1".to_string(),
        ..Stage::default()
    };
    assert_eq!(judge_cwd(repo, &stage), repo);

    std::fs::create_dir_all(repo.join(".worktrees").join("s1")).unwrap();
    assert_eq!(judge_cwd(repo, &stage), repo.join(".worktrees").join("s1"));

    let recorded = Stage {
        id: "s1".to_string(),
        worktree: Some("other".to_string()),
        ..Stage::default()
    };
    assert_eq!(judge_cwd(repo, &recorded), repo);
}

#[test]
fn resolve_model_falls_back_to_default_when_unset() {
    let tmp = tempfile::tempdir().unwrap();
    assert_eq!(resolve_model(tmp.path()), DEFAULT_ADJUDICATION_MODEL);
}

#[test]
fn resolve_model_reads_config_override() {
    let tmp = tempfile::tempdir().unwrap();
    std::fs::write(
        tmp.path().join("config.toml"),
        "[adjudication]\nmodel = \"claude-haiku-test\"\n",
    )
    .unwrap();
    assert_eq!(resolve_model(tmp.path()), "claude-haiku-test");
}

#[test]
fn persist_verdict_writes_a_parseable_record() {
    let tmp = tempfile::tempdir().unwrap();
    let work = tmp.path();
    persist_verdict(
        work,
        "s1",
        1,
        &DisputeVerdict::NeedsMoreEvidence {
            questions: vec!["why?".to_string()],
        },
        "opus",
        2,
        Some("session-xyz".to_string()),
    )
    .unwrap();
    let path = verdict_file(&work.join("disputes"), "s1", 1);
    let content = std::fs::read_to_string(&path).unwrap();
    let record: DisputeVerdictRecord = super::super::scan::parse_yaml_frontmatter(&content)
        .expect("verdict.md must parse back as a record");
    assert_eq!(record.adjudicator_attempt_count, 2);
    assert_eq!(record.model, "opus");
    assert_eq!(record.session_id.as_deref(), Some("session-xyz"));
}

#[test]
fn no_live_session_when_none_recorded() {
    let tmp = tempfile::tempdir().unwrap();
    assert!(live_adjudication_session(tmp.path(), "s1").is_none());
}
