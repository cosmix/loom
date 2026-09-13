use super::*;
use chrono::TimeZone as _;
use std::io::Write;
use tempfile::TempDir;

fn delivered(stage_id: &str) -> TelemetryEvent {
    TelemetryEvent::ContextDelivered {
        stage_id: stage_id.to_string(),
        session_id: "session-1".to_string(),
        context_epoch: "abc123".to_string(),
        items: 3,
    }
}

fn record(at: i64, event: TelemetryEvent) -> TelemetryRecord {
    TelemetryRecord {
        at: Utc.timestamp_opt(at, 0).single().unwrap(),
        event,
    }
}

fn prompt_brief() -> TelemetryEvent {
    TelemetryEvent::PromptBrief {
        stage_id: Some("stage-a".to_string()),
        session_id: Some("session-1".to_string()),
        items: 2,
        estimated_tokens: 100,
        omitted: 1,
    }
}

fn prompt_abstained() -> TelemetryEvent {
    TelemetryEvent::PromptAbstained {
        stage_id: Some("stage-a".to_string()),
        session_id: Some("session-1".to_string()),
        reason: "floor".to_string(),
    }
}

fn context_pulled() -> TelemetryEvent {
    TelemetryEvent::ContextPulled {
        stage_id: Some("stage-a".to_string()),
        session_id: Some("session-1".to_string()),
        query_chars: 12,
        budget_tokens: 600,
        items: 4,
        estimated_tokens: 300,
        unmet_required: 1,
    }
}

#[test]
fn events_round_trip_with_timestamps() {
    let temp = TempDir::new().unwrap();
    let expected = record(1_700_000_000, delivered("stage-a"));

    append_record(temp.path(), &expected).unwrap();

    let events = read_events(temp.path()).unwrap();
    assert_eq!(events, vec![expected]);
}

#[cfg(unix)]
#[test]
#[serial_test::serial]
fn emit_falls_back_to_the_spool_when_the_state_root_is_read_only() {
    use std::os::unix::fs::PermissionsExt as _;

    let temp = TempDir::new().unwrap();
    let worktree = temp.path().join(".worktrees").join("stage-a");
    std::fs::create_dir_all(&worktree).unwrap();
    let work_dir = temp.path().join("state");
    let telemetry_dir = work_dir.join("telemetry");
    std::fs::create_dir_all(&telemetry_dir).unwrap();
    std::fs::set_permissions(&telemetry_dir, std::fs::Permissions::from_mode(0o500)).unwrap();
    let original_cwd = std::env::current_dir().unwrap();
    std::env::set_current_dir(&worktree).unwrap();

    let result = emit(&work_dir, &delivered("stage-a"));

    std::env::set_current_dir(original_cwd).unwrap();
    std::fs::set_permissions(&telemetry_dir, std::fs::Permissions::from_mode(0o700)).unwrap();
    assert!(result.is_ok());
    let pending = spool::read_pending(&worktree).unwrap();
    assert_eq!(pending.len(), 1);
    assert_eq!(pending[0].event, delivered("stage-a"));
}

#[test]
fn drain_moves_spooled_lines_and_truncates() {
    let worktree = TempDir::new().unwrap();
    let work_dir = TempDir::new().unwrap();
    let expected = record(10, delivered("stage-a"));
    spool::append_to_spool(worktree.path(), &expected).unwrap();
    let path = spool::spool_path(worktree.path());
    writeln!(
        OpenOptions::new().append(true).open(&path).unwrap(),
        "not-json"
    )
    .unwrap();

    let outcome = spool::drain_into_events(work_dir.path(), worktree.path()).unwrap();

    assert_eq!(outcome.drained, 1);
    assert_eq!(outcome.skipped_malformed, 1);
    assert_eq!(read_events(work_dir.path()).unwrap(), vec![expected]);
    assert_eq!(std::fs::read_to_string(path).unwrap(), "");
}

const VICTIM: &str = "outside the worktree; a drain must never read or truncate this\n";

#[test]
fn drain_refuses_a_spool_symlinked_outside_the_worktree() {
    let worktree = TempDir::new().unwrap();
    let work_dir = TempDir::new().unwrap();
    let outside = TempDir::new().unwrap();
    let victim = outside.path().join("victim.txt");
    std::fs::write(&victim, VICTIM).unwrap();
    std::fs::create_dir_all(worktree.path().join(".loom")).unwrap();
    std::os::unix::fs::symlink(&victim, spool::spool_path(worktree.path())).unwrap();

    let error = spool::drain_into_events(work_dir.path(), worktree.path()).unwrap_err();

    assert!(
        format!("{error:#}").contains("was not drained"),
        "{error:#}"
    );
    assert_eq!(std::fs::read_to_string(&victim).unwrap(), VICTIM);
}

#[test]
fn drain_refuses_a_spool_under_a_symlinked_loom_directory() {
    let worktree = TempDir::new().unwrap();
    let work_dir = TempDir::new().unwrap();
    let outside = TempDir::new().unwrap();
    let victim = outside.path().join("telemetry-spool.jsonl");
    std::fs::write(&victim, VICTIM).unwrap();
    std::os::unix::fs::symlink(outside.path(), worktree.path().join(".loom")).unwrap();

    assert!(spool::drain_into_events(work_dir.path(), worktree.path()).is_err());
    assert_eq!(std::fs::read_to_string(&victim).unwrap(), VICTIM);
}

#[test]
fn summary_counts_briefs_abstentions_and_pulls_per_stage() {
    let events = vec![
        record(1, delivered("stage-a")),
        record(2, prompt_brief()),
        record(3, prompt_abstained()),
        record(4, context_pulled()),
    ];

    let summaries = summary::summarize(&events);

    assert_eq!(summaries.len(), 1);
    let summary = &summaries[0];
    assert_eq!(summary.stage_id, "stage-a");
    assert_eq!((summary.spawn_briefs, summary.spawn_items), (1, 3));
    assert_eq!(summary.prompt_briefs, 1);
    assert_eq!(summary.prompt_abstained.total, 1);
    assert_eq!(summary.prompt_abstained.by_reason["floor"], 1);
    assert_eq!(summary.pulls, 1);
    assert_eq!((summary.pull_avg_budget, summary.pull_avg_items), (600, 4));
    assert_eq!(summary.pull_unmet, 1);
    assert_eq!(summary.last_at, Utc.timestamp_opt(4, 0).single().unwrap());
}

#[derive(Default)]
struct VecSink {
    stdout: Vec<u8>,
    stderr: Vec<u8>,
}

impl RelaySink for VecSink {
    fn stdout(&mut self) -> &mut dyn Write {
        &mut self.stdout
    }
    fn stderr(&mut self) -> &mut dyn Write {
        &mut self.stderr
    }
}

fn stage_context(scratch: &std::path::Path, worktree: &std::path::Path) -> RelayContext {
    RelayContext {
        session_id: "session-1".to_string(),
        scratch_dir: scratch.to_path_buf(),
        stage_id: Some("stage-a".to_string()),
        session_type: Some("stage".to_string()),
        worktree_path: Some(worktree.to_path_buf()),
        work_dir: None,
    }
}

#[test]
fn relay_mode_relays_a_context_pulled_event_and_writes_nothing_to_the_events_file() {
    use std::os::unix::fs::PermissionsExt;

    let scratch_root = TempDir::new().unwrap();
    let scratch = scratch_root.path().join("session-1");
    std::fs::create_dir(&scratch).unwrap();
    std::fs::set_permissions(&scratch, std::fs::Permissions::from_mode(0o700)).unwrap();
    let worktree = TempDir::new().unwrap();
    let work_dir = TempDir::new().unwrap();
    let context = stage_context(&scratch, worktree.path());
    let mut sink = VecSink::default();
    let event = context_pulled();

    emit_with_mode(
        work_dir.path(),
        &event,
        RelayMode::Relay(context),
        worktree.path(),
        &mut sink,
    )
    .unwrap();

    let tickets: Vec<_> = std::fs::read_dir(&scratch)
        .unwrap()
        .map(|e| e.unwrap().path())
        .collect();
    assert_eq!(tickets.len(), 1);
    let bytes = std::fs::read(&tickets[0]).unwrap();
    let ticket = crate::relay::Ticket::decode(&bytes).unwrap();
    assert_eq!(ticket.kind, RequestKind::Telemetry);
    let decoded: TelemetryEvent = serde_json::from_value(ticket.payload).unwrap();
    assert_eq!(decoded, event);

    assert!(read_events(work_dir.path()).unwrap().is_empty());
    assert!(sink.stderr.is_empty(), "telemetry relay must not print");
}

#[test]
fn relay_mode_refusal_is_silent_and_never_fails_the_caller() {
    let scratch_root = TempDir::new().unwrap();
    let worktree = TempDir::new().unwrap();
    let work_dir = TempDir::new().unwrap();
    // No scratch directory created at all: `check` refuses before any ticket
    // is written, and `emit_with_mode` must still return `Ok`.
    let mut context = stage_context(&scratch_root.path().join("session-1"), worktree.path());
    context.session_type = Some("adjudication".to_string());
    let mut sink = VecSink::default();

    let result = emit_with_mode(
        work_dir.path(),
        &context_pulled(),
        RelayMode::Relay(context),
        worktree.path(),
        &mut sink,
    );

    assert!(result.is_ok());
    assert!(sink.stdout.is_empty());
    assert!(sink.stderr.is_empty());
}
