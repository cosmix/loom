//! Contract: a final token scan that fails before it registers the run's
//! totals publishes no plan statistics, so the stale checkpoint is not
//! archived as the permanent record.

use super::finish_foreground;
use crate::fs::work_dir::WorkDir;
use crate::models::session::Session;
use crate::models::stage::{Stage, StageStatus};
use crate::orchestrator::OrchestratorResult;
use crate::user_config::redirect_user_config;
use serial_test::serial;
use std::fs;
use std::path::Path;

/// A run whose one stage finished and merged, with a session record naming a
/// transcript of one assistant line.
fn merged_run_with_transcript(root: &Path) -> (WorkDir, OrchestratorResult) {
    let work = WorkDir::new(root).expect("work dir");
    work.initialize().expect("initialize");
    let plan = root.join("IN_PROGRESS-PLAN-test.md");
    fs::write(&plan, "# Test plan\n").expect("plan file");
    let config = format!(
        "[plan]\nsource_path = {:?}\nplan_id = \"test\"\n",
        plan.to_str().expect("utf-8 path")
    );
    fs::write(work.root().join("config.toml"), config).expect("config");
    fs::create_dir_all(work.root().join("stages")).expect("stages dir");
    let stage = Stage {
        id: "test".into(),
        name: "Test".into(),
        status: StageStatus::Completed,
        merged: true,
        ..Default::default()
    };
    let text = crate::verify::transitions::serialize_stage_to_markdown(&stage).expect("stage");
    fs::write(work.root().join("stages/test.md"), text).expect("stage file");
    fs::create_dir_all(work.root().join("memory")).expect("memory dir");
    fs::write(work.root().join("memory/test.md"), "journal").expect("journal");
    save_transcript_session(&work, root);
    let now = chrono::Utc::now();
    let result = OrchestratorResult {
        completed_stages: vec!["test".into()],
        failed_stages: vec![],
        unfinished_stages: vec![],
        needs_handoff: vec![],
        total_sessions_spawned: 1,
        started_at: now,
        completed_at: now,
    };
    (work, result)
}

fn save_transcript_session(work: &WorkDir, root: &Path) {
    let at = chrono::Utc::now() - chrono::Duration::seconds(60);
    let line = serde_json::json!({
        "type": "assistant", "timestamp": at.to_rfc3339(), "isSidechain": false,
        "sessionId": "01ed9f4c-608e-4e2c-8edf-05d26adcd4d6",
        "message": {"id": "m1", "model": "claude-opus-5-5", "usage": {
            "input_tokens": 0, "cache_creation_input_tokens": 100,
            "cache_read_input_tokens": 0, "output_tokens": 0}}
    });
    let transcript = root.join("transcript.jsonl");
    fs::write(&transcript, format!("{line}\n")).expect("transcript");
    let mut session = Session::new();
    session.stage_id = Some("test".into());
    session.transcript_path = Some(transcript.to_string_lossy().into_owned());
    crate::fs::session_files::save_session(&session, work.root()).expect("session");
}

/// Puts a file where the sessions directory is, so session discovery, which
/// runs after the tokens directory is opened and before the final scan
/// registers any totals, fails. The symlinked tokens directory fails the
/// plan-totals reader too, so it cannot show a stale checkpoint reaching the
/// archive.
fn break_session_discovery(work: &WorkDir) {
    let sessions = work.root().join("sessions");
    fs::remove_dir_all(&sessions).expect("remove sessions");
    fs::write(&sessions, "not a directory").expect("sessions blocker");
}

#[test]
#[serial]
fn a_failed_final_scan_publishes_no_plan_statistics() {
    let temp = tempfile::tempdir().expect("tempdir");
    let home = temp.path().join("home");
    fs::create_dir_all(&home).expect("home");
    let _redirect = redirect_user_config(home.join("config.toml"));
    let (work, result) = merged_run_with_transcript(temp.path());
    let now = chrono::Utc::now().timestamp();
    crate::tokens::scan_pass(work.root(), None, now).expect("tracker pass");
    break_session_discovery(&work);
    assert!(
        crate::tokens::final_scan(work.root(), None, now).is_err(),
        "the scenario: the final scan fails before it registers anything"
    );

    finish_foreground(&work, &result).expect("the stages are done whatever the statistics say");

    assert!(
        !work.root().join("plan-stats.json").exists(),
        "an incomplete capture exposes no current copy"
    );
    let archived = fs::read_dir(home.join("stats/plans")).map_or(0, Iterator::count);
    assert_eq!(archived, 0, "an incomplete capture archives nothing");
}
