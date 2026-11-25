//! The timing rules of a standard stage's `loom check`: refusals before a new
//! request, the `--force` override, and the next-step lines.

use std::fs;
use std::path::{Path, PathBuf};

use serde_json::json;
use tempfile::TempDir;

use super::{next_step, refusal, REFUSED_EXIT};
use crate::commands::stage::native_gate::tests::seed_active_worker;
use crate::commands::verify::CheckSelection;
use crate::host_verify::{CriterionOutcome, JobState};
use crate::models::stage::{Stage, StageType};
use crate::relay::emit::RelayContext;
use crate::subagent_lifecycle::{
    LifecycleProducer, LifecycleRecord, LifecycleState, WorkerIdentity, LIFECYCLE_VERSION,
};
use crate::verify::review::store::{write_round, ReviewRound, RECORD_VERSION};

const STAGE: &str = "s1";
const SESSION: &str = "session-1";

fn context(session_type: &str) -> RelayContext {
    RelayContext {
        session_id: SESSION.to_string(),
        scratch_dir: PathBuf::from("scratch"),
        stage_id: Some(STAGE.to_string()),
        session_type: Some(session_type.to_string()),
        worktree_path: None,
        work_dir: None,
    }
}

fn stage(stage_type: StageType) -> Stage {
    let mut stage = Stage::new(STAGE.to_string(), None);
    stage.id = STAGE.to_string();
    stage.stage_type = stage_type;
    stage
}

fn ledger_dir(work: &Path) -> PathBuf {
    let dir = work.join("subagents").join(STAGE);
    fs::create_dir_all(&dir).unwrap();
    dir
}

fn append(path: &Path, row: &serde_json::Value) {
    let mut text = fs::read_to_string(path).unwrap_or_default();
    text.push_str(&format!("{row}\n"));
    fs::write(path, text).unwrap();
}

fn start(work: &Path, agent_type: &str, agent_id: &str, session: &str) {
    let row = json!({
        "stage_id": STAGE,
        "loom_session_id": session,
        "parent_session_id": "parent",
        "agent_id": agent_id,
        "agent_type": agent_type,
    });
    append(&ledger_dir(work).join("starts.jsonl"), &row);
}

fn stop(work: &Path, agent_id: &str) {
    let record = LifecycleRecord {
        version: LIFECYCLE_VERSION,
        event_id: format!("sha256:{agent_id}"),
        producer: LifecycleProducer::ClaudeSubagentStop,
        identity: WorkerIdentity::ClaudeSubagent {
            stage_id: STAGE.to_string(),
            loom_session_id: SESSION.to_string(),
            parent_session_id: "parent".to_string(),
            agent_id: agent_id.to_string(),
            agent_type: "loom-software-engineer".to_string(),
            transcript_path: PathBuf::from("transcript.jsonl"),
        },
        observed_at: chrono::Utc::now(),
        state: LifecycleState::Completed,
        evidence: json!({}),
    };
    let row = serde_json::to_value(record).unwrap();
    append(&ledger_dir(work).join("lifecycle.jsonl"), &row);
}

fn record_round(work: &Path) {
    let round = ReviewRound {
        version: RECORD_VERSION,
        round: 1,
        agent_id: "reviewer-1".to_string(),
        harvested_at: chrono::Utc::now(),
        fingerprint: "sha256:0000".to_string(),
        base: String::new(),
        behaviour: Default::default(),
        files: Default::default(),
        malformed: None,
        findings: Vec::new(),
        resolved: Vec::new(),
        unresolved: Vec::new(),
        suggestion_memory_ids: Vec::new(),
    };
    write_round(work, STAGE, &round).unwrap();
}

/// A standard stage's lead session asking for `selection`, not joining.
fn refused(work: &Path, selection: &CheckSelection, force: bool) -> Option<String> {
    let (context, stage) = (context("stage"), stage(StageType::Standard));
    refusal(&context, &stage, work, (selection, force), || false)
}

#[test]
fn a_started_worker_without_a_stop_refuses_and_is_named() {
    let work = TempDir::new().unwrap();
    start(work.path(), "loom-software-engineer", "agent-7", SESSION);

    let text = refused(work.path(), &CheckSelection::All, false).expect("refused");

    assert!(text.contains("loom-software-engineer agent-7"), "{text}");
    assert!(text.contains("loom subagents watch"), "{text}");
    assert!(text.contains("--force"), "{text}");
}

#[test]
fn a_reviewer_out_does_not_refuse() {
    let work = TempDir::new().unwrap();
    start(work.path(), "loom-code-reviewer", "reviewer-1", SESSION);

    assert_eq!(refused(work.path(), &CheckSelection::All, false), None);
}

#[test]
fn a_recorded_stop_returns_the_worker() {
    let work = TempDir::new().unwrap();
    start(work.path(), "loom-software-engineer", "agent-7", SESSION);
    stop(work.path(), "agent-7");

    assert_eq!(refused(work.path(), &CheckSelection::All, false), None);
}

#[test]
fn a_worker_of_another_session_is_ignored() {
    let work = TempDir::new().unwrap();
    start(
        work.path(),
        "loom-software-engineer",
        "agent-7",
        "session-2",
    );

    assert_eq!(refused(work.path(), &CheckSelection::All, false), None);
}

#[test]
fn a_worker_out_refuses_every_selection_but_at_base() {
    let work = TempDir::new().unwrap();
    start(work.path(), "loom-software-engineer", "agent-7", SESSION);

    for selection in [
        CheckSelection::Criteria(vec![0]),
        CheckSelection::Contracts,
        CheckSelection::Trial(Vec::new()),
    ] {
        assert!(refused(work.path(), &selection, false).is_some());
    }
    assert_eq!(
        refused(work.path(), &CheckSelection::AtBase(vec![0]), false),
        None
    );
}

#[test]
fn an_active_native_codex_worker_refuses() {
    let work = TempDir::new().unwrap();
    seed_active_worker(work.path(), STAGE, "unit");

    let text = refused(work.path(), &CheckSelection::Criteria(vec![0]), false).expect("refused");

    assert!(text.contains("native Codex workers"), "{text}");
}

#[test]
fn a_recorded_round_refuses_a_full_check_only() {
    let work = TempDir::new().unwrap();
    assert_eq!(refused(work.path(), &CheckSelection::All, false), None);
    record_round(work.path());

    let text = refused(work.path(), &CheckSelection::All, false).expect("refused");

    assert!(text.contains("loom stage complete s1"), "{text}");
    assert!(text.contains("loom check s1 --criterion <n>"), "{text}");
    for selection in [
        CheckSelection::Criteria(vec![1]),
        CheckSelection::Contracts,
        CheckSelection::AtBase(vec![1]),
    ] {
        assert_eq!(refused(work.path(), &selection, false), None);
    }
}

#[test]
fn force_passes_both_refusals() {
    let work = TempDir::new().unwrap();
    start(work.path(), "loom-software-engineer", "agent-7", SESSION);
    record_round(work.path());

    assert!(refused(work.path(), &CheckSelection::All, false).is_some());
    assert_eq!(refused(work.path(), &CheckSelection::All, true), None);
}

#[test]
fn a_call_joining_its_pending_request_is_never_refused() {
    let work = TempDir::new().unwrap();
    start(work.path(), "loom-software-engineer", "agent-7", SESSION);
    record_round(work.path());
    let (context, stage) = (context("stage"), stage(StageType::Standard));

    let joined = refusal(
        &context,
        &stage,
        work.path(),
        (&CheckSelection::All, false),
        || true,
    );

    assert_eq!(joined, None);
}

#[test]
fn other_stage_and_session_types_are_never_refused() {
    let work = TempDir::new().unwrap();
    start(work.path(), "loom-software-engineer", "agent-7", SESSION);
    record_round(work.path());
    let selection = CheckSelection::All;
    let refused_by = |context: &RelayContext, stage: &Stage| {
        refusal(context, stage, work.path(), (&selection, false), || false)
    };

    for stage_type in [
        StageType::IntegrationVerify,
        StageType::Knowledge,
        StageType::KnowledgeDistill,
    ] {
        assert_eq!(refused_by(&context("stage"), &stage(stage_type)), None);
    }
    for session_type in ["merge", "base_conflict", "knowledge"] {
        let standard = stage(StageType::Standard);
        assert_eq!(refused_by(&context(session_type), &standard), None);
    }
    assert!(refused_by(&context("stage"), &stage(StageType::Standard)).is_some());
}

#[test]
fn the_refusal_exit_code_is_not_one_the_check_uses() {
    assert!(![0, 1, 4].contains(&REFUSED_EXIT));
}

fn outcome(label: &str, passed: bool) -> CriterionOutcome {
    CriterionOutcome {
        label: label.to_string(),
        passed,
        summary: String::new(),
        log: None,
    }
}

fn step(
    selection: &CheckSelection,
    state: JobState,
    outcomes: &[CriterionOutcome],
) -> Option<String> {
    let (context, stage) = (context("stage"), stage(StageType::Standard));
    next_step((&context, &stage), selection, state, outcomes)
}

#[test]
fn a_passed_full_check_says_not_to_run_it_again() {
    let line = step(&CheckSelection::All, JobState::Passed, &[]).unwrap();

    assert!(line.starts_with("Next: these passes are stored for this tree."));
    assert!(line.contains("Do not run loom check again."), "{line}");
    assert!(line.contains("loom stage complete"), "{line}");
}

#[test]
fn a_failed_check_names_the_failed_criteria_for_one_fix_worker() {
    let outcomes = [
        outcome("acceptance-0", true),
        outcome("acceptance-3", false),
        outcome("acceptance-5", false),
        outcome("goal-backward", true),
    ];

    for selection in [CheckSelection::All, CheckSelection::Criteria(vec![0, 3, 5])] {
        let line = step(&selection, JobState::Failed, &outcomes).unwrap();
        assert!(line.contains("to ONE fix worker"), "{line}");
        assert!(
            line.ends_with("run: loom check s1 --criterion 3 --criterion 5"),
            "{line}"
        );
    }
}

#[test]
fn a_failed_selection_without_outcomes_rechecks_the_selection() {
    let line = step(&CheckSelection::Criteria(vec![1, 2]), JobState::Failed, &[]).unwrap();

    assert!(
        line.ends_with("loom check s1 --criterion 1 --criterion 2"),
        "{line}"
    );
}

#[test]
fn a_failed_contract_names_the_contracts_check() {
    let outcomes = [outcome("acceptance-0", true), outcome("contract:c1", false)];

    let line = step(&CheckSelection::All, JobState::Failed, &outcomes).unwrap();

    assert!(line.ends_with("run: loom check s1 --contracts"), "{line}");
}

#[test]
fn a_passed_selection_says_not_to_run_a_full_check() {
    let line = step(&CheckSelection::Criteria(vec![2]), JobState::Passed, &[]).unwrap();

    assert!(line.starts_with("Next: selected criteria pass."), "{line}");
    assert!(line.contains("Do not run a full loom check"), "{line}");
}

#[test]
fn no_next_step_for_other_runs_undecided_jobs_or_other_stages() {
    assert_eq!(
        step(&CheckSelection::Contracts, JobState::Passed, &[]),
        None
    );
    assert_eq!(step(&CheckSelection::All, JobState::Cancelled, &[]), None);
    let integration = stage(StageType::IntegrationVerify);
    let line = next_step(
        (&context("stage"), &integration),
        &CheckSelection::All,
        JobState::Passed,
        &[],
    );
    assert_eq!(line, None);
}
