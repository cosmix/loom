use super::super::model::{MessageState, Outcome, NATIVE_DEADLINE_SECS};
use super::super::store::{create_invocation, EventLog};
use super::*;
use chrono::TimeZone;
use std::path::PathBuf;
use tempfile::TempDir;

pub(super) fn manifest(unit: &str, serial: u8, at: i64) -> NativeManifest {
    NativeManifest {
        v: 1,
        stage_id: "stage".into(),
        unit_id: unit.into(),
        invocation_id: format!("inv-{serial:032x}"),
        owner_session_id: "session".into(),
        workspace_root: PathBuf::from("/workspace"),
        brief_path: PathBuf::from("/workspace/brief.md"),
        brief_sha256: crate::utils::sha256_hex(b"brief"),
        model: "gpt-5.4".into(),
        effort: "high".into(),
        deadline_secs: NATIVE_DEADLINE_SECS,
        created_at: Utc.timestamp_opt(at, 0).unwrap(),
    }
}

fn records(events: &[NativeEvent]) -> Vec<EventRecord> {
    events
        .iter()
        .enumerate()
        .map(|(index, event)| EventRecord {
            seq: index as u64 + 1,
            at: Utc.timestamp_opt(index as i64 + 1, 0).unwrap(),
            event: event.clone(),
        })
        .collect()
}

fn folded(events: &[NativeEvent]) -> NativeUnitState {
    fold(manifest("unit", 1, 0), &records(events))
}

fn running() -> NativeEvent {
    NativeEvent::TurnStarted {
        turn_id: "turn".into(),
    }
}

fn terminal() -> NativeEvent {
    NativeEvent::Terminal {
        outcome: Outcome::Succeeded,
        reason: "done".into(),
    }
}

fn released(by: &str) -> NativeEvent {
    NativeEvent::OwnershipReleased { by: by.into() }
}

pub(super) fn unverified() -> NativeEvent {
    NativeEvent::OwnershipUnverified {
        detail: "missing".into(),
    }
}

fn message(id: &str, state: MessageState, turn: Option<&str>) -> NativeEvent {
    NativeEvent::Message {
        message_id: id.into(),
        state,
        turn_id: turn.map(str::to_string),
        detail: None,
    }
}

pub(super) fn admit(
    work: &Path,
    unit: &str,
    serial: u8,
    at: i64,
    events: &[NativeEvent],
) -> PathBuf {
    let dir = create_invocation(work, &manifest(unit, serial, at), b"brief").unwrap();
    let mut log = EventLog::open(&dir).unwrap();
    for event in events {
        log.append(event.clone()).unwrap();
    }
    dir
}

#[test]
fn fold_phase_follows_events_and_never_moves_backward() {
    let steps = [
        (NativeEvent::Admitted, Phase::Admitted),
        (NativeEvent::BoundaryCreated, Phase::Starting),
        (running(), Phase::Running),
        (NativeEvent::Admitted, Phase::Running),
        (
            NativeEvent::CancelRequested {
                reason: "cancel".into(),
            },
            Phase::Finalizing,
        ),
        (running(), Phase::Finalizing),
        (terminal(), Phase::Terminal),
        (NativeEvent::DeadlineExpired, Phase::Terminal),
    ];
    let mut events = Vec::new();
    for (event, phase) in steps {
        events.push(event);
        assert_eq!(folded(&events).phase, phase);
    }
}

#[test]
fn fold_first_terminal_wins() {
    let later = NativeEvent::Terminal {
        outcome: Outcome::Failed,
        reason: "later".into(),
    };
    let state = folded(&[terminal(), later]);
    assert_eq!(state.outcome, Some(Outcome::Succeeded));
    assert_eq!(state.outcome_reason.as_deref(), Some("done"));
}

#[test]
fn fold_ownership_unverified_then_operator_release() {
    let state = folded(&[unverified(), released("operator"), unverified()]);
    assert_eq!(state.ownership, Ownership::Released);
}

#[test]
fn fold_ownership_release_by_daemon_does_not_lift_unverified() {
    let state = folded(&[unverified(), released("daemon"), released("supervisor")]);
    assert_eq!(state.ownership, Ownership::Unverified);
}

#[test]
fn fold_message_keeps_latest_state_and_first_seen_order() {
    let state = folded(&[
        message("b", MessageState::Queued, None),
        message("a", MessageState::Dispatching, Some("turn")),
        message("b", MessageState::Delivered, Some("new")),
        message("a", MessageState::Rejected, None),
    ]);
    let expected = [
        MessageInfo {
            message_id: "b".into(),
            state: MessageState::Delivered,
            turn_id: Some("new".into()),
        },
        MessageInfo {
            message_id: "a".into(),
            state: MessageState::Rejected,
            turn_id: Some("turn".into()),
        },
    ];
    assert_eq!(state.messages, expected);
}

#[test]
fn fold_last_progress_is_newest_event_time() {
    let manifest = manifest("unit", 1, 0);
    assert_eq!(
        fold(manifest.clone(), &[]).last_progress_at,
        manifest.created_at
    );
    let events = records(&[NativeEvent::Admitted, running(), NativeEvent::Progress]);
    assert_eq!(fold(manifest, &events).last_progress_at, events[2].at);
}

#[test]
fn fold_records_boundary_thread_cancel_and_report() {
    let state = folded(&[
        NativeEvent::BoundaryCreated,
        NativeEvent::ThreadStarted {
            thread_id: "thread".into(),
        },
        NativeEvent::CancelRequested {
            reason: "cancel".into(),
        },
        NativeEvent::ReportStored {
            bytes: 3,
            sha256: "hash".into(),
            complete: true,
        },
    ]);
    assert_eq!(state.phase, Phase::Finalizing);
    assert_eq!(state.thread_id.as_deref(), Some("thread"));
    assert!(state.cancel_requested);
    let report = state.report.unwrap();
    assert_eq!(report.bytes, 3);
    assert_eq!(report.sha256, "hash");
    assert!(report.complete);
}

#[test]
fn disposition_none_without_stage_dir() {
    let work = TempDir::new().unwrap();
    assert_eq!(
        stage_disposition(work.path(), "stage"),
        NativeDisposition::None
    );
}

#[test]
fn disposition_active_for_running_unit() {
    let work = TempDir::new().unwrap();
    admit(work.path(), "unit", 1, 0, &[running()]);
    let active = NativeDisposition::Active(vec!["unit".into()]);
    assert_eq!(stage_disposition(work.path(), "stage"), active);
}

#[test]
fn disposition_active_for_terminal_unit_still_held() {
    let work = TempDir::new().unwrap();
    admit(work.path(), "unit", 1, 0, &[terminal()]);
    let active = NativeDisposition::Active(vec!["unit".into()]);
    assert_eq!(stage_disposition(work.path(), "stage"), active);
}

#[test]
fn disposition_none_after_terminal_and_release() {
    let work = TempDir::new().unwrap();
    admit(
        work.path(),
        "unit",
        1,
        0,
        &[terminal(), released("supervisor")],
    );
    assert_eq!(
        stage_disposition(work.path(), "stage"),
        NativeDisposition::None
    );
}

#[test]
fn disposition_unverified_beats_active() {
    let work = TempDir::new().unwrap();
    admit(work.path(), "unknown", 1, 0, &[unverified()]);
    admit(work.path(), "active", 2, 1, &[running()]);
    let expected = format!("inv-{:032x}: processes not proven stopped", 1);
    let disposition = stage_disposition(work.path(), "stage");
    assert_eq!(disposition, NativeDisposition::Unverified(expected));
}

#[test]
fn disposition_ignores_dot_directories() {
    let work = TempDir::new().unwrap();
    fs::create_dir_all(stage_dir(work.path(), "stage").join(".inv-incomplete.tmp")).unwrap();
    assert_eq!(
        stage_disposition(work.path(), "stage"),
        NativeDisposition::None
    );
}

#[test]
fn disposition_counts_worker_from_any_session() {
    let work = TempDir::new().unwrap();
    let mut prior = manifest("prior", 1, 0);
    prior.owner_session_id = "previous-session".into();
    create_invocation(work.path(), &prior, b"brief").unwrap();
    let active = NativeDisposition::Active(vec!["prior".into()]);
    assert_eq!(stage_disposition(work.path(), "stage"), active);
}

#[test]
fn disposition_deduplicates_units_in_newest_order() {
    let work = TempDir::new().unwrap();
    admit(work.path(), "a", 1, 0, &[running()]);
    admit(work.path(), "b", 2, 1, &[running()]);
    admit(work.path(), "a", 3, 2, &[running()]);
    let active = NativeDisposition::Active(vec!["a".into(), "b".into()]);
    assert_eq!(stage_disposition(work.path(), "stage"), active);
}

#[test]
fn unit_state_returns_newest_invocation() {
    let work = TempDir::new().unwrap();
    admit(work.path(), "unit", 1, 1, &[]);
    admit(work.path(), "unit", 2, 2, &[]);
    admit(work.path(), "unit", 3, 2, &[]);
    let state = unit_state(work.path(), "stage", "unit").unwrap().unwrap();
    assert_eq!(state.manifest.invocation_id, format!("inv-{:032x}", 3));
    assert_eq!(unit_state(work.path(), "stage", "absent").unwrap(), None);
}

#[test]
fn unit_state_errors_when_a_sibling_record_is_corrupt() {
    let work = TempDir::new().unwrap();
    let bad = admit(work.path(), "other", 1, 0, &[]);
    admit(work.path(), "unit", 2, 1, &[]);
    fs::remove_file(bad.join("manifest.json")).unwrap();
    let units = stage_units(work.path(), "stage");
    assert!(units[0].is_ok());
    assert!(units[1].is_err());
    assert!(unit_state(work.path(), "stage", "unit").is_err());
}

#[test]
fn blocker_texts_are_exact() {
    assert_eq!(NativeDisposition::None.blocker("stage"), None);
    assert_eq!(NativeDisposition::Active(vec!["a".into(), "b".into()]).blocker("stage").unwrap(),
        "stage 'stage' has running native Codex workers (a, b); cancel each with 'loom subagents cancel --stage stage --worker codex:<unit> --reason <text>' and wait for it to stop");
    let id = format!("inv-{:032x}", 1);
    assert_eq!(NativeDisposition::Unverified(format!("{id}: processes not proven stopped")).blocker("stage").unwrap(),
        format!("stage 'stage' has a native Codex worker whose processes could not be proven stopped ({id}: processes not proven stopped); inspect the worktree, then run 'loom subagents release --stage stage --invocation <invocation id>'"));
    assert_eq!(NativeDisposition::Unverified(format!("{id}: line 1: bad")).blocker("stage").unwrap(),
        format!("stage 'stage' has a damaged native Codex worker record ({id}: line 1: bad); confirm no process of that worker still runs, then remove the damaged path under .loom/work/native-workers/stage/"));
}

#[test]
fn stage_threads_reports_thread_and_start_time() {
    let work = TempDir::new().unwrap();
    let events = [NativeEvent::ThreadStarted {
        thread_id: "thread".into(),
    }];
    let dir = admit(work.path(), "unit", 1, 0, &events);
    let bad = admit(work.path(), "corrupt", 2, 1, &[]);
    fs::write(bad.join("events.jsonl"), b"broken").unwrap();
    let threads = stage_threads(work.path(), "stage");
    let expected = NativeThread {
        thread_id: "thread".into(),
        unit_id: "unit".into(),
        invocation_id: format!("inv-{:032x}", 1),
        requested_model: "gpt-5.4".into(),
        requested_effort: "high".into(),
        started_at: read_events(&dir).unwrap()[1].at,
    };
    assert_eq!(threads, [expected]);
}

#[test]
fn stages_lists_only_safe_directories_in_sorted_order() {
    let work = TempDir::new().unwrap();
    assert!(stages(work.path()).is_empty());
    let root = work.path().join("native-workers");
    for name in ["z-stage", "a-stage", "_leading", ".hidden", "bad.name"] {
        fs::create_dir_all(root.join(name)).unwrap();
    }
    fs::write(root.join("file"), b"").unwrap();
    std::os::unix::fs::symlink(root.join("a-stage"), root.join("link")).unwrap();
    assert_eq!(stages(work.path()), ["_leading", "a-stage", "z-stage"]);
}

#[test]
fn stage_units_rejects_unsafe_stage_and_symlinked_invocations() {
    let work = TempDir::new().unwrap();
    let invalid = stage_units(work.path(), "../escape");
    assert!(matches!(&invalid[0], Err(StoreError::UnsafePath(_))));
    let dir = admit(work.path(), "unit", 1, 0, &[]);
    let link = stage_dir(work.path(), "stage").join(format!("inv-{:032x}", 2));
    std::os::unix::fs::symlink(dir, link).unwrap();
    assert!(matches!(
        stage_disposition(work.path(), "stage"),
        NativeDisposition::Unverified(_)
    ));
}

#[test]
fn stage_units_requires_initial_admitted_event() {
    let work = TempDir::new().unwrap();
    let dir = admit(work.path(), "unit", 1, 0, &[]);
    let log = records(&[NativeEvent::Progress]);
    let line = format!("{}\n", serde_json::to_string(&log[0]).unwrap());
    fs::write(dir.join("events.jsonl"), line).unwrap();
    assert!(matches!(
        stage_disposition(work.path(), "stage"),
        NativeDisposition::Unverified(_)
    ));
}
