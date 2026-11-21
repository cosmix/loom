use std::path::{Path, PathBuf};
use std::time::Duration;

use chrono::{TimeZone, Utc};

use super::engine::{wait_for_workers, EvidenceSource, POLL_INTERVAL};
use super::evidence::LifecycleEvidence;
use super::lease::deadline_after;
use super::model::{
    exit_code, BoundWorker, TerminalOutcome, WaitLease, WorkerKind, WorkerSpec, EXIT_UNKNOWN,
};
use super::native::{active_progress, NativeSet};
use super::output;
use super::tests::{acquire_owner, fixture, identity, FakeClock, FakeSleeper};
use crate::native_worker::ids::NATIVE_DEADLINE_SECS;
use crate::native_worker::model::{MessageState, NativeEvent, NativeManifest, Outcome};
use crate::native_worker::store::{create_invocation, EventLog};
use crate::subagent_lifecycle::WorkerOutcome;

const STAGE: &str = "stage-a";

/// Invocation id of the `serial`-th seeded record.
pub(super) fn invocation_id(serial: u8) -> String {
    format!("inv-{serial:032x}")
}

/// Create the record of one invocation, then append `events` after `Admitted`.
/// A larger `serial` is a newer invocation of the unit.
pub(super) fn seed(
    work_dir: &Path,
    workspace: &Path,
    unit: &str,
    serial: u8,
    events: &[NativeEvent],
) -> PathBuf {
    let manifest = NativeManifest {
        v: 1,
        stage_id: STAGE.into(),
        unit_id: unit.into(),
        invocation_id: invocation_id(serial),
        owner_session_id: "loom-a".into(),
        workspace_root: workspace.to_path_buf(),
        brief_path: workspace.join("brief.md"),
        brief_sha256: crate::utils::sha256_hex(b"brief"),
        model: "gpt-6.1-terra".into(),
        effort: "xhigh".into(),
        deadline_secs: NATIVE_DEADLINE_SECS,
        created_at: Utc
            .timestamp_opt(1_700_000_000 + i64::from(serial), 0)
            .unwrap(),
    };
    let dir = create_invocation(work_dir, &manifest, b"brief").unwrap();
    let mut log = EventLog::open(&dir).unwrap();
    for event in events {
        log.append(event.clone()).unwrap();
    }
    dir
}

pub(super) fn terminal(outcome: Outcome) -> NativeEvent {
    NativeEvent::Terminal {
        outcome,
        reason: "turn completed".into(),
    }
}

pub(super) fn released() -> NativeEvent {
    NativeEvent::OwnershipReleased {
        by: "supervisor".into(),
    }
}

fn native_worker(unit: &str, serial: u8) -> BoundWorker {
    BoundWorker {
        worker: WorkerSpec {
            kind: WorkerKind::Codex,
            id: unit.into(),
        },
        lifecycle_identity: None,
        native_invocation: Some(format!("{STAGE}/{}", invocation_id(serial))),
        authority: None,
        evidence: Vec::new(),
    }
}

fn evidence(work_dir: &Path) -> LifecycleEvidence {
    LifecycleEvidence {
        work_dir: work_dir.to_path_buf(),
        stall_budget: Duration::from_secs(600),
    }
}

#[test]
fn native_poll_maps_each_state() {
    let temp = tempfile::tempdir().unwrap();
    let work = temp.path();
    seed(work, work, "admitted", 1, &[]);
    seed(
        work,
        work,
        "done",
        2,
        &[terminal(Outcome::Succeeded), released()],
    );
    seed(
        work,
        work,
        "unverified",
        3,
        &[
            terminal(Outcome::Succeeded),
            NativeEvent::OwnershipUnverified {
                detail: "pid reuse".into(),
            },
        ],
    );
    let evidence = evidence(work);

    let polled = |unit: &str, serial: u8| evidence.poll(&[native_worker(unit, serial)]).remove(0);
    assert_eq!(polled("admitted", 1), (WorkerOutcome::Active, false));
    assert_eq!(polled("done", 2), (WorkerOutcome::Succeeded, false));
    let (outcome, settled) = polled("unverified", 3);
    assert!(matches!(outcome, WorkerOutcome::Unknown(_)), "{outcome:?}");
    assert!(settled);
}

#[test]
fn restarted_or_missing_record_is_unknown() {
    let temp = tempfile::tempdir().unwrap();
    let work = temp.path();
    seed(work, work, "unit-a", 1, &[]);
    seed(work, work, "unit-a", 2, &[]);
    let evidence = evidence(work);

    let (outcome, settled) = evidence.poll(&[native_worker("unit-a", 1)]).remove(0);
    let WorkerOutcome::Unknown(reason) = outcome else {
        panic!("expected Unknown, got {outcome:?}");
    };
    assert!(reason.contains("restarted"), "{reason}");
    assert!(settled);

    let mut elsewhere = native_worker("unit-a", 2);
    elsewhere.native_invocation = Some(format!("no-such-stage/{}", invocation_id(2)));
    let (outcome, _) = evidence.poll(&[elsewhere]).remove(0);
    let WorkerOutcome::Unknown(reason) = outcome else {
        panic!("expected Unknown, got {outcome:?}");
    };
    assert!(reason.contains("missing"), "{reason}");
}

#[test]
fn unverified_worker_ends_the_wait_with_exit_5() {
    let temp = tempfile::tempdir().unwrap();
    let work = temp.path();
    seed(
        work,
        work,
        "unit-a",
        1,
        &[
            terminal(Outcome::Failed),
            NativeEvent::OwnershipUnverified {
                detail: "boundary pid alive".into(),
            },
        ],
    );
    let mut identity = identity();
    identity.workers = vec![native_worker("unit-a", 1)];
    let clock = FakeClock::new();
    let sleeper = FakeSleeper::new(clock.clone());
    let deadline = deadline_after(&clock, Duration::from_secs(3_600)).unwrap();

    let result = wait_for_workers(
        &identity,
        &deadline,
        &clock,
        &sleeper,
        &evidence(work),
        POLL_INTERVAL,
    );

    assert_eq!(result.outcome, TerminalOutcome::Unknown);
    assert_eq!(result.polls, 0);
    assert_eq!(sleeper.sleeps(), 0);
    assert_eq!(
        exit_code(&output::terminal_outcome(&result.outcome)),
        EXIT_UNKNOWN
    );
}

#[test]
fn native_bound_worker_round_trips_through_the_lease_json() {
    let fixture = fixture();
    let mut lease = acquire_owner(&fixture, Duration::from_secs(60));
    lease.identity.workers = vec![native_worker("unit-a", 1)];

    let json = serde_json::to_string(&lease).unwrap();
    assert!(!json.contains("lifecycle_identity"), "{json}");
    let parsed: WaitLease = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed, lease);
    assert_eq!(parsed.identity.workers[0].lifecycle_identity, None);

    let bare = r#"{"worker":{"kind":"codex","id":"unit-a"},"authority":null,"evidence":[]}"#;
    let worker: BoundWorker = serde_json::from_str(bare).unwrap();
    assert_eq!(worker.lifecycle_identity, None);
    assert_eq!(worker.native_invocation, None);
}

#[test]
fn bind_pins_the_invocation_and_rejects_another_worktree() {
    let temp = tempfile::tempdir().unwrap();
    let work = temp.path().join("work");
    std::fs::create_dir_all(&work).unwrap();
    seed(&work, temp.path(), "unit-a", 1, &[]);
    let scope = super::identity::IdentityScope {
        work_dir: work,
        canonical_repo: temp.path().to_owned(),
        canonical_worktree: temp.path().to_owned(),
        projects_root: temp.path().join("projects"),
        stage_id: STAGE.into(),
        loom_session_id: "loom-a".into(),
    };
    let spec: WorkerSpec = "codex:unit-a".parse().unwrap();
    let set = NativeSet::load(&scope, std::slice::from_ref(&spec)).unwrap();
    let bound = set.bind(&scope, &spec).unwrap().unwrap();
    assert_eq!(
        bound.native_invocation,
        native_worker("unit-a", 1).native_invocation
    );
    assert_eq!(bound.lifecycle_identity, None);
    assert_eq!(bound.evidence[0].identity, invocation_id(1));

    let other = super::identity::IdentityScope {
        canonical_worktree: temp.path().join("elsewhere"),
        work_dir: scope.work_dir.clone(),
        canonical_repo: scope.canonical_repo.clone(),
        projects_root: scope.projects_root.clone(),
        stage_id: STAGE.into(),
        loom_session_id: "loom-a".into(),
    };
    let error = set.bind(&other, &spec).unwrap_err().to_string();
    assert!(error.contains("another worktree"), "{error}");
}

fn thread_started() -> NativeEvent {
    NativeEvent::ThreadStarted {
        thread_id: "thread-1".into(),
    }
}

#[test]
fn progress_age_counts_only_provider_events() {
    let temp = tempfile::tempdir().unwrap();
    let work = temp.path();
    seed(
        work,
        work,
        "quiet",
        1,
        &[
            NativeEvent::BoundaryCreated,
            NativeEvent::Message {
                message_id: "msg-1".into(),
                state: MessageState::Queued,
                turn_id: None,
                detail: None,
            },
            NativeEvent::ServerRequestRefused {
                method: "item/tool".into(),
            },
        ],
    );
    seed(work, work, "started", 2, &[thread_started()]);
    let budget = Duration::from_secs(600);
    let now = chrono::Utc::now() + chrono::Duration::seconds(90);

    let quiet = active_progress(work, &native_worker("quiet", 1), now, budget).unwrap();
    assert_eq!(quiet.age_secs, None);
    let started = active_progress(work, &native_worker("started", 2), now, budget).unwrap();
    let age = started.age_secs.unwrap();
    assert!((89..=91).contains(&age), "{age}");
}

#[test]
fn progress_age_clamps_a_future_event_to_zero() {
    let temp = tempfile::tempdir().unwrap();
    let work = temp.path();
    seed(work, work, "unit-a", 1, &[NativeEvent::Progress]);
    let now = chrono::Utc::now() - chrono::Duration::seconds(3_600);

    let progress = active_progress(
        work,
        &native_worker("unit-a", 1),
        now,
        Duration::from_secs(600),
    );

    assert_eq!(progress.unwrap().age_secs, Some(0));
}

#[test]
fn only_an_active_worker_reports_progress() {
    let temp = tempfile::tempdir().unwrap();
    let work = temp.path();
    seed(
        work,
        work,
        "done",
        1,
        &[thread_started(), terminal(Outcome::Succeeded), released()],
    );
    seed(work, work, "stalled", 2, &[thread_started()]);
    let now = chrono::Utc::now() + chrono::Duration::seconds(3_600);
    let budget = Duration::from_secs(600);

    assert!(active_progress(work, &native_worker("done", 1), now, budget).is_none());
    assert!(active_progress(work, &native_worker("stalled", 2), now, budget).is_none());
    assert!(active_progress(work, &native_worker("missing", 3), now, budget).is_none());
}

#[test]
fn progress_line_ends_with_the_age_or_no_progress_yet() {
    assert_eq!(
        super::output::progress_line("codex:unit-a", Some(42)),
        "codex:unit-a active, progress 42s ago"
    );
    assert_eq!(
        super::output::progress_line("codex:unit-a", None),
        "codex:unit-a active, no progress yet"
    );
}

/// Claude workers and legacy Codex jobs get no line, and a wait with no native
/// worker never starts the throttle clock.
#[test]
fn progress_is_silent_without_native_workers() {
    let temp = tempfile::tempdir().unwrap();
    let evidence = super::evidence::LifecycleEvidence {
        work_dir: temp.path().to_path_buf(),
        stall_budget: Duration::from_secs(600),
    };
    let mut last_emit = None;
    let mut out: Vec<u8> = Vec::new();

    super::emit_active_native_progress(
        &evidence,
        &identity().workers,
        chrono::Utc::now(),
        &mut last_emit,
        &mut out,
    )
    .unwrap();

    assert!(out.is_empty());
    assert_eq!(last_emit, None);
}
