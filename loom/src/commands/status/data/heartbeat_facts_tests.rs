//! How a running session's heartbeat silence classifies: Working, Quiet,
//! Busy (a tool call is open in a transcript) or Stale (past the stage's
//! budget with nothing open).

use std::fs;
use std::path::{Path, PathBuf};

use super::*;
use crate::models::session::SessionStatus;
use crate::orchestrator::monitor::heartbeat::write_heartbeat;

const SESSION: &str = "11111111-2222-3333-4444-555555555555";

const OPEN_CALL: &str = r#"{"type":"assistant","message":{"content":[{"type":"tool_use","id":"t1","name":"Bash","input":{}}]}}"#;
const CALL_RESULT: &str =
    r#"{"type":"user","message":{"content":[{"type":"tool_result","tool_use_id":"t1"}]}}"#;
const BOOKKEEPING: &str = r#"{"type":"last-prompt","lastPrompt":"go"}"#;

/// A temp `.loom/work` plus a fake Claude projects root holding one project
/// directory; nothing here touches the real HOME or live state.
struct Fixture {
    _tmp: tempfile::TempDir,
    work_dir: WorkDir,
    projects: PathBuf,
}

impl Fixture {
    fn new() -> Self {
        let tmp = tempfile::TempDir::new().unwrap();
        let work_dir = WorkDir::new(tmp.path().join("work")).unwrap();
        work_dir.initialize().unwrap();
        let projects = tmp.path().join("projects");
        fs::create_dir_all(projects.join("proj")).unwrap();
        Self {
            _tmp: tmp,
            work_dir,
            projects,
        }
    }

    fn main_transcript(&self) -> PathBuf {
        self.projects.join("proj").join(format!("{SESSION}.jsonl"))
    }

    fn subagent_transcript(&self) -> PathBuf {
        let dir = self.projects.join("proj").join(SESSION).join("subagents");
        fs::create_dir_all(&dir).unwrap();
        dir.join("agent-a1.jsonl")
    }

    /// A running session on `stage` whose heartbeat is `silence_secs` old and
    /// names `transcript`.
    fn facts(&self, stage: &Stage, silence_secs: i64, transcript: &Path) -> HeartbeatFacts {
        let mut session = Session::new();
        session.id = "s1".to_string();
        session.stage_id = Some(stage.id.clone());
        session.status = SessionStatus::Running;
        let then = Utc::now() - chrono::Duration::seconds(silence_secs);
        let mut heartbeat = Heartbeat::new(stage.id.clone(), session.id.clone())
            .with_transcript_path(transcript.to_string_lossy())
            .with_last_tool("Read".to_string());
        heartbeat.timestamp = then;
        heartbeat.progress_at = Some(then);
        write_heartbeat(self.work_dir.root(), &heartbeat).unwrap();
        heartbeat_facts_under(stage, Some(&session), &self.work_dir, Some(&self.projects))
    }
}

fn stage(budget: Option<u64>) -> Stage {
    Stage {
        id: "stage-1".to_string(),
        name: "stage-1".to_string(),
        status: StageStatus::Executing,
        subagent_timeout_secs: budget,
        ..Stage::default()
    }
}

fn write_lines(path: &Path, lines: &[&str]) {
    fs::write(path, lines.join("\n") + "\n").unwrap();
}

#[test]
fn a_larger_budget_makes_the_same_silence_quiet_not_stale() {
    let fixture = Fixture::new();
    let transcript = fixture.main_transcript();
    write_lines(&transcript, &[OPEN_CALL, CALL_RESULT]);

    let default_budget = fixture.facts(&stage(None), 400, &transcript);
    let long_budget = fixture.facts(&stage(Some(900)), 400, &transcript);

    assert_eq!(default_budget.activity_status, ActivityStatus::Stale);
    assert_eq!(long_budget.activity_status, ActivityStatus::Quiet);
}

#[test]
fn silence_past_the_budget_with_nothing_open_is_stale() {
    let fixture = Fixture::new();
    let transcript = fixture.main_transcript();
    write_lines(&transcript, &[OPEN_CALL, CALL_RESULT, BOOKKEEPING]);

    let facts = fixture.facts(&stage(Some(900)), 1000, &transcript);

    assert_eq!(facts.activity_status, ActivityStatus::Stale);
    assert_eq!(facts.last_tool.as_deref(), Some("Read"));
}

#[test]
fn an_open_main_tool_call_is_busy_and_names_the_tool() {
    let fixture = Fixture::new();
    let transcript = fixture.main_transcript();
    // Bookkeeping rows after the call do not close it.
    write_lines(&transcript, &[CALL_RESULT, OPEN_CALL, BOOKKEEPING]);

    let facts = fixture.facts(&stage(None), 400, &transcript);

    assert_eq!(facts.activity_status, ActivityStatus::Busy);
    assert_eq!(facts.last_tool.as_deref(), Some("Bash"));
    assert_eq!(facts.staleness_secs, Some(400));
}

#[test]
fn an_open_subagent_tool_call_is_busy() {
    let fixture = Fixture::new();
    let transcript = fixture.main_transcript();
    write_lines(&transcript, &[OPEN_CALL, CALL_RESULT]);
    write_lines(&fixture.subagent_transcript(), &[OPEN_CALL]);

    let facts = fixture.facts(&stage(None), 400, &transcript);

    assert_eq!(facts.activity_status, ActivityStatus::Busy);
}

#[test]
fn a_subagent_last_written_before_the_last_progress_is_finished() {
    let fixture = Fixture::new();
    let transcript = fixture.main_transcript();
    write_lines(&transcript, &[OPEN_CALL, CALL_RESULT]);
    let subagent = fixture.subagent_transcript();
    write_lines(&subagent, &[OPEN_CALL]);
    let long_ago = std::time::SystemTime::now() - std::time::Duration::from_secs(3600);
    fs::File::options()
        .write(true)
        .open(&subagent)
        .unwrap()
        .set_modified(long_ago)
        .unwrap();

    let facts = fixture.facts(&stage(None), 400, &transcript);

    assert_eq!(facts.activity_status, ActivityStatus::Stale);
}

#[test]
fn a_fresh_heartbeat_is_working_even_with_a_call_open() {
    let fixture = Fixture::new();
    let transcript = fixture.main_transcript();
    write_lines(&transcript, &[OPEN_CALL]);

    let facts = fixture.facts(&stage(None), 10, &transcript);

    assert_eq!(facts.activity_status, ActivityStatus::Working);
    assert_eq!(facts.last_tool.as_deref(), Some("Read"));
}

#[test]
fn a_zero_budget_falls_back_to_the_default_threshold() {
    let fixture = Fixture::new();
    let transcript = fixture.main_transcript();
    write_lines(&transcript, &[OPEN_CALL, CALL_RESULT]);

    let first_poll = fixture.facts(&stage(Some(0)), 5, &transcript);
    let silent = fixture.facts(&stage(Some(0)), 400, &transcript);

    assert_eq!(first_poll.activity_status, ActivityStatus::Working);
    assert_eq!(silent.activity_status, ActivityStatus::Stale);
}

#[test]
fn a_budget_under_the_default_threshold_goes_stale_at_the_budget() {
    let fixture = Fixture::new();
    let transcript = fixture.main_transcript();
    write_lines(&transcript, &[OPEN_CALL, CALL_RESULT]);

    let facts = fixture.facts(&stage(Some(120)), 200, &transcript);

    assert_eq!(facts.activity_status, ActivityStatus::Stale);
}

#[test]
fn a_transcript_outside_the_projects_root_is_not_followed() {
    let fixture = Fixture::new();
    let outside = fixture.work_dir.root().join(format!("{SESSION}.jsonl"));
    write_lines(&outside, &[OPEN_CALL]);

    let facts = fixture.facts(&stage(None), 400, &outside);

    assert_eq!(facts.activity_status, ActivityStatus::Stale);
}

#[test]
fn a_dot_dot_path_that_escapes_the_projects_root_is_not_followed() {
    let fixture = Fixture::new();
    let outside = fixture.work_dir.root().join(format!("{SESSION}.jsonl"));
    write_lines(&outside, &[OPEN_CALL]);
    let sneaky = fixture
        .projects
        .join("proj")
        .join("..")
        .join("..")
        .join("work")
        .join(format!("{SESSION}.jsonl"));

    let facts = fixture.facts(&stage(None), 400, &sneaky);

    assert_eq!(facts.activity_status, ActivityStatus::Stale);
}

#[test]
fn a_symlinked_transcript_is_not_followed() {
    let fixture = Fixture::new();
    let real = fixture.projects.join("proj").join("real.jsonl");
    write_lines(&real, &[OPEN_CALL]);
    let link = fixture.main_transcript();
    std::os::unix::fs::symlink(&real, &link).unwrap();

    let facts = fixture.facts(&stage(None), 400, &link);

    assert_eq!(facts.activity_status, ActivityStatus::Stale);
}

#[test]
fn a_symlinked_subagent_transcript_is_not_followed() {
    let fixture = Fixture::new();
    let transcript = fixture.main_transcript();
    write_lines(&transcript, &[OPEN_CALL, CALL_RESULT]);
    let real = fixture.projects.join("proj").join("elsewhere.jsonl");
    write_lines(&real, &[OPEN_CALL]);
    std::os::unix::fs::symlink(&real, fixture.subagent_transcript()).unwrap();

    let facts = fixture.facts(&stage(None), 400, &transcript);

    assert_eq!(facts.activity_status, ActivityStatus::Stale);
}

#[test]
fn a_transcript_without_the_jsonl_extension_is_not_read() {
    let fixture = Fixture::new();
    let other = fixture.projects.join("proj").join(format!("{SESSION}.txt"));
    write_lines(&other, &[OPEN_CALL]);

    let facts = fixture.facts(&stage(None), 400, &other);

    assert_eq!(facts.activity_status, ActivityStatus::Stale);
}

#[test]
fn a_long_transcript_is_judged_by_its_tail() {
    let fixture = Fixture::new();
    let transcript = fixture.main_transcript();
    let filler = format!(
        r#"{{"type":"user","message":{{"content":"{}"}}}}"#,
        "x".repeat(1000)
    );
    let mut lines = vec![filler.as_str(); 1500];
    lines.push(OPEN_CALL);
    write_lines(&transcript, &lines);

    let facts = fixture.facts(&stage(None), 400, &transcript);

    assert_eq!(facts.activity_status, ActivityStatus::Busy);
}
