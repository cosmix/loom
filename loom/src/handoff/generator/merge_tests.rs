use std::fs;
use std::path::{Path, PathBuf};

use tempfile::TempDir;

use super::{merge_session_handoff, HandoffContent, MergeOutcome};
use crate::handoff::schema::{
    CompletionAttemptEvidence, CompletionCheckpoint, CompletionPhase, CriterionResult,
    HandoffOrigin, HandoffV2, ParsedHandoff, VerificationCheckpoint, COMPLETION_EVIDENCE_VERSION,
};
use crate::models::session::Session;
use crate::models::stage::Stage;

fn fixture() -> (TempDir, Session, Stage) {
    let temp = TempDir::new().unwrap();
    let mut session = Session::new();
    session.id = "session-merge".to_string();
    let mut stage = Stage::new("merge".to_string(), None);
    stage.id = "stage-merge".to_string();
    (temp, session, stage)
}

fn content(session: &Session, stage: &Stage) -> HandoffContent {
    HandoffContent::new(session.id.clone(), stage.id.clone())
}

fn evidence(
    session: &Session,
    stage: &Stage,
    commit_digit: char,
    nonce: &str,
    failure_code: &str,
) -> CompletionAttemptEvidence {
    CompletionAttemptEvidence {
        version: COMPLETION_EVIDENCE_VERSION,
        stage_id: stage.id.clone(),
        session_id: session.id.clone(),
        commit: commit_digit.to_string().repeat(40),
        check_definition_hash: "b".repeat(64),
        exact_command: "cargo test --lib".to_string(),
        evidence_nonce: nonce.to_string(),
        verification: VerificationCheckpoint {
            criteria: vec![CriterionResult {
                id: "criterion-1".to_string(),
                passed: true,
            }],
            environment_policy: "trusted-host-v1".to_string(),
            environment: Vec::new(),
        },
        phase: CompletionPhase::VerifiedPendingAck,
        external_failure_code: Some(failure_code.to_string()),
        diagnostic_first_line: Some("daemon did not acknowledge".to_string()),
        observed_at: "2026-09-14T10:00:00Z".to_string(),
        attestation: None,
    }
}

fn checkpoint(evidence: &CompletionAttemptEvidence) -> CompletionCheckpoint {
    let mut checkpoint = CompletionCheckpoint::new(&evidence.stage_id, &evidence.session_id);
    checkpoint.record_attempt(evidence).unwrap();
    checkpoint
}

fn completion_content(
    session: &Session,
    stage: &Stage,
    checkpoint: CompletionCheckpoint,
) -> HandoffContent {
    content(session, stage)
        .with_origin(HandoffOrigin::CompletionEvidence)
        .with_completion_checkpoint(Some(checkpoint))
}

fn parse(path: &Path) -> HandoffV2 {
    let markdown = fs::read_to_string(path).unwrap();
    ParsedHandoff::parse(&markdown).as_v2().unwrap().clone()
}

fn handoff_count(work_dir: &Path) -> usize {
    fs::read_dir(work_dir.join("handoffs")).unwrap().count()
}

fn write_v2(path: PathBuf, handoff: &HandoffV2) {
    fs::write(path, format!("---\n{}---\n", handoff.to_yaml().unwrap())).unwrap();
}

#[test]
fn first_merge_without_prior_artifact_is_created() {
    let (temp, session, stage) = fixture();

    let (path, outcome) = merge_session_handoff(
        &session,
        &stage,
        None,
        content(&session, &stage),
        temp.path(),
    )
    .unwrap();

    assert_eq!(outcome, MergeOutcome::Created);
    assert!(path.ends_with("stage-merge-handoff-001.md"));
}

#[test]
fn duplicate_identical_merge_is_unchanged() {
    let (temp, session, stage) = fixture();
    let first = content(&session, &stage).with_completed_work(vec!["kept".to_string()]);
    let (first_path, _) =
        merge_session_handoff(&session, &stage, None, first.clone(), temp.path()).unwrap();

    let (second_path, outcome) =
        merge_session_handoff(&session, &stage, None, first, temp.path()).unwrap();

    assert_eq!(
        (second_path, outcome),
        (first_path, MergeOutcome::Unchanged)
    );
    assert_eq!(handoff_count(temp.path()), 1);
}

#[test]
fn empty_session_end_cannot_erase_rich_stalled_handoff() {
    let (temp, session, stage) = fixture();
    let attempt = evidence(
        &session,
        &stage,
        'a',
        "nonce_attempt_0000000001",
        "DAEMON_OFFLINE",
    );
    let rich = content(&session, &stage)
        .with_origin(HandoffOrigin::Stalled)
        .with_completed_work(vec!["implemented recovery".to_string()])
        .with_completion_checkpoint(Some(checkpoint(&attempt)));
    let (rich_path, _) = merge_session_handoff(
        &session,
        &stage,
        Some(HandoffOrigin::Stalled),
        rich,
        temp.path(),
    )
    .unwrap();

    let (path, outcome) = merge_session_handoff(
        &session,
        &stage,
        None,
        content(&session, &stage),
        temp.path(),
    )
    .unwrap();

    assert_eq!((path, outcome), (rich_path, MergeOutcome::Unchanged));
    assert_eq!(handoff_count(temp.path()), 1);
}

#[test]
fn new_blocker_keeps_prior_completed_work() {
    let (temp, session, stage, update) = new_blocker_fixture();

    let (path, outcome) = merge_session_handoff(
        &session,
        &stage,
        Some(HandoffOrigin::CompletionEvidence),
        update,
        temp.path(),
    )
    .unwrap();
    let parsed = parse(&path);

    assert_eq!(outcome, MergeOutcome::Created);
    assert_eq!(
        parsed.completed_tasks[0].description,
        "implemented recovery"
    );
    assert_eq!(
        parsed
            .completion_checkpoint
            .unwrap()
            .blocker
            .unwrap()
            .external_failure_code,
        "ACK_REJECTED"
    );
}

fn new_blocker_fixture() -> (TempDir, Session, Stage, HandoffContent) {
    let (temp, session, stage) = fixture();
    let first = evidence(
        &session,
        &stage,
        'a',
        "nonce_attempt_0000000001",
        "DAEMON_OFFLINE",
    );
    let rich = completion_content(&session, &stage, checkpoint(&first))
        .with_completed_work(vec!["implemented recovery".to_string()]);
    merge_session_handoff(
        &session,
        &stage,
        Some(HandoffOrigin::CompletionEvidence),
        rich,
        temp.path(),
    )
    .unwrap();
    let second = evidence(
        &session,
        &stage,
        'c',
        "nonce_attempt_0000000002",
        "ACK_REJECTED",
    );
    let update = completion_content(&session, &stage, checkpoint(&second));
    (temp, session, stage, update)
}

#[test]
fn wrong_session_and_malformed_artifacts_are_ignored() {
    let (temp, session, stage) = fixture();
    let dir = temp.path().join("handoffs");
    fs::create_dir_all(&dir).unwrap();
    write_v2(
        dir.join("stage-merge-handoff-001.md"),
        &HandoffV2::new("other-session", &stage.id)
            .with_completed_tasks(vec![crate::handoff::schema::CompletedTask::new("foreign")]),
    );
    fs::write(dir.join("stage-merge-handoff-002.md"), "not a handoff").unwrap();
    let incoming = content(&session, &stage).with_completed_work(vec!["local".to_string()]);

    let (path, outcome) =
        merge_session_handoff(&session, &stage, None, incoming, temp.path()).unwrap();

    assert_eq!(outcome, MergeOutcome::Created);
    assert!(path.ends_with("stage-merge-handoff-003.md"));
    assert_eq!(parse(&path).completed_tasks[0].description, "local");
}

#[test]
fn agent_ceiling_origin_after_equal_content_is_created() {
    let (temp, session, stage) = fixture();
    let prior = content(&session, &stage).with_completed_work(vec!["done".to_string()]);
    merge_session_handoff(&session, &stage, None, prior, temp.path()).unwrap();
    let ceiling = content(&session, &stage)
        .with_origin(HandoffOrigin::AgentCeiling)
        .with_completed_work(vec!["done".to_string()]);

    let (_, outcome) = merge_session_handoff(
        &session,
        &stage,
        Some(HandoffOrigin::AgentCeiling),
        ceiling,
        temp.path(),
    )
    .unwrap();

    assert_eq!(outcome, MergeOutcome::Created);
    assert_eq!(handoff_count(temp.path()), 2);
}

#[test]
fn parsed_merged_artifact_preserves_repeat_count() {
    let (temp, session, stage) = fixture();
    let first = evidence(
        &session,
        &stage,
        'a',
        "nonce_attempt_0000000001",
        "DAEMON_OFFLINE",
    );
    let mut second = first.clone();
    second.evidence_nonce = "nonce_attempt_0000000002".to_string();
    second.observed_at = "2026-09-14T10:01:00Z".to_string();
    let mut checkpoint = checkpoint(&first);
    checkpoint.record_attempt(&second).unwrap();
    let incoming = completion_content(&session, &stage, checkpoint);

    let (path, _) = merge_session_handoff(
        &session,
        &stage,
        Some(HandoffOrigin::CompletionEvidence),
        incoming,
        temp.path(),
    )
    .unwrap();

    assert_eq!(
        parse(&path).completion_checkpoint.unwrap().repeat_count(),
        2
    );
}

#[test]
fn checkpoint_merge_preserves_full_structured_context() {
    use crate::handoff::schema::{CompletedTask, FileRef};
    let (temp, session, stage) = fixture();
    let dir = temp.path().join("handoffs");
    fs::create_dir_all(&dir).unwrap();
    let mut prior = HandoffV2::new(&session.id, &stage.id);
    prior.completed_tasks = vec![CompletedTask::with_files(
        "implemented",
        vec!["src/lib.rs".into()],
    )];
    prior.discovered_facts = vec!["fact".into()];
    prior.open_questions = vec!["question".into()];
    prior.files_read = vec![FileRef::with_lines("src/lib.rs", 4, 9, "context")];
    prior.goals = "goal".into();
    prior.memory_content = Some("journal".into());
    write_v2(dir.join("stage-merge-handoff-001.md"), &prior);
    let attempt = evidence(
        &session,
        &stage,
        'a',
        "nonce_attempt_0000000001",
        "ACK_REJECTED",
    );
    let (path, _) = merge_session_handoff(
        &session,
        &stage,
        Some(HandoffOrigin::CompletionEvidence),
        completion_content(&session, &stage, checkpoint(&attempt)),
        temp.path(),
    )
    .unwrap();
    let merged = parse(&path);
    assert_eq!(merged.completed_tasks, prior.completed_tasks);
    assert_eq!(merged.discovered_facts, prior.discovered_facts);
    assert_eq!(merged.open_questions, prior.open_questions);
    assert_eq!(merged.files_read, prior.files_read);
    assert_eq!(merged.goals, prior.goals);
    assert_eq!(merged.memory_content, prior.memory_content);
}

#[test]
fn memory_branch_and_clean_snapshot_are_semantic_changes() {
    let (temp, session, stage) = fixture();
    let first = snapshot_content(&session, &stage, "old", "first", true);
    let (_, created) = merge_session_handoff(&session, &stage, None, first, temp.path()).unwrap();
    assert_eq!(created, MergeOutcome::Created);
    let memory_only = snapshot_content(&session, &stage, "old", "second", true);
    let (memory_path, outcome) =
        merge_session_handoff(&session, &stage, None, memory_only, temp.path()).unwrap();
    assert_eq!(outcome, MergeOutcome::Created);
    assert_eq!(
        parse(&memory_path).memory_content.as_deref(),
        Some("second")
    );
    let update = snapshot_content(&session, &stage, "new", "second", true);
    let (path, outcome) =
        merge_session_handoff(&session, &stage, None, update.clone(), temp.path()).unwrap();
    assert_eq!(outcome, MergeOutcome::Created);
    let merged = parse(&path);
    assert_eq!(merged.branch.as_deref(), Some("new"));
    assert_eq!(merged.memory_content.as_deref(), Some("second"));
    assert_eq!(merged.uncommitted_files, vec!["dirty.rs"]);
    let clean = snapshot_content(&session, &stage, "new", "second", false);
    let (path, outcome) =
        merge_session_handoff(&session, &stage, None, clean.clone(), temp.path()).unwrap();
    assert_eq!(outcome, MergeOutcome::Created);
    assert!(parse(&path).uncommitted_files.is_empty());
    let (again, outcome) =
        merge_session_handoff(&session, &stage, None, clean, temp.path()).unwrap();
    assert_eq!((again, outcome), (path, MergeOutcome::Unchanged));
}

fn snapshot_content(
    session: &Session,
    stage: &Stage,
    branch: &str,
    memory: &str,
    dirty: bool,
) -> HandoffContent {
    use crate::handoff::git_handoff::GitHistory;
    content(session, stage)
        .with_current_branch(Some(branch.into()))
        .with_memory_content(Some(memory.into()))
        .with_git_history(Some(GitHistory {
            branch: branch.into(),
            base_branch: String::new(),
            commits: Vec::new(),
            uncommitted_changes: if dirty {
                vec!["dirty.rs".into()]
            } else {
                Vec::new()
            },
        }))
}
