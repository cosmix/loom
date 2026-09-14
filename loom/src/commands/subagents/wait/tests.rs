use std::collections::{BTreeMap, VecDeque};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use anyhow::Result;
use tempfile::TempDir;

use super::engine::{EvidenceSource, Sleeper};
use super::identity::parse_worker_set;
use super::lease::{acquire, Acquired, BootClock, LeaseDir, OwnerProbe};
use super::model::{
    exit_code, BoundWorker, EventOutcome, LeaseOwner, WaitIdentity, WaitLease, WorkerKind,
    WorkerSpec, EXIT_BUSY, EXIT_STALLED, EXIT_TIMEOUT, EXIT_UNKNOWN, EXIT_WORKER_TERMINAL,
};
use crate::process::IdentityStatus;
use crate::subagent_lifecycle::{CodexExecution, WorkerIdentity, WorkerOutcome};

#[derive(Clone)]
pub(super) struct FakeClock(Arc<Mutex<ClockState>>);

struct ClockState {
    boot_id: String,
    monotonic_ns: u64,
    unix_secs: u64,
}

impl FakeClock {
    pub(super) fn new() -> Self {
        Self(Arc::new(Mutex::new(ClockState {
            boot_id: "boot-a".into(),
            monotonic_ns: 0,
            unix_secs: 1_000_000,
        })))
    }

    pub(super) fn advance(&self, duration: Duration) {
        let mut state = self.0.lock().unwrap();
        state.monotonic_ns += u64::try_from(duration.as_nanos()).unwrap();
        state.unix_secs += duration.as_secs();
    }

    pub(super) fn set_boot_id(&self, boot_id: &str) {
        self.0.lock().unwrap().boot_id = boot_id.into();
    }

    pub(super) fn set_monotonic_ns(&self, monotonic_ns: u64) {
        self.0.lock().unwrap().monotonic_ns = monotonic_ns;
    }

    pub(super) fn set_unix_secs(&self, unix_secs: u64) {
        self.0.lock().unwrap().unix_secs = unix_secs;
    }
}

impl BootClock for FakeClock {
    fn boot_id(&self) -> Result<String> {
        Ok(self.0.lock().unwrap().boot_id.clone())
    }

    fn monotonic_ns(&self) -> Result<u64> {
        Ok(self.0.lock().unwrap().monotonic_ns)
    }

    fn unix_secs(&self) -> u64 {
        self.0.lock().unwrap().unix_secs
    }
}

#[derive(Clone)]
pub(super) struct FakeProbe {
    current: Arc<Mutex<LeaseOwner>>,
    statuses: Arc<Mutex<Vec<(LeaseOwner, IdentityStatus)>>>,
}

impl FakeProbe {
    pub(super) fn new(owner: LeaseOwner) -> Self {
        Self {
            current: Arc::new(Mutex::new(owner)),
            statuses: Arc::new(Mutex::new(Vec::new())),
        }
    }

    pub(super) fn set_current(&self, owner: LeaseOwner) {
        *self.current.lock().unwrap() = owner;
    }

    pub(super) fn set_status(&self, owner: LeaseOwner, status: IdentityStatus) {
        self.statuses.lock().unwrap().push((owner, status));
    }
}

impl OwnerProbe for FakeProbe {
    fn current(&self) -> LeaseOwner {
        *self.current.lock().unwrap()
    }

    fn status(&self, owner: LeaseOwner) -> IdentityStatus {
        self.statuses
            .lock()
            .unwrap()
            .iter()
            .rev()
            .find(|(candidate, _)| *candidate == owner)
            .map_or(IdentityStatus::VerifiedAlive, |(_, status)| *status)
    }
}

pub(super) struct FakeSleeper {
    clock: FakeClock,
    sleeps: Mutex<u64>,
    next_boot_id: Mutex<Option<String>>,
}

impl FakeSleeper {
    pub(super) fn new(clock: FakeClock) -> Self {
        Self {
            clock,
            sleeps: Mutex::new(0),
            next_boot_id: Mutex::new(None),
        }
    }

    pub(super) fn sleeps(&self) -> u64 {
        *self.sleeps.lock().unwrap()
    }

    pub(super) fn change_boot_on_next_sleep(&self, boot_id: &str) {
        *self.next_boot_id.lock().unwrap() = Some(boot_id.into());
    }
}

impl Sleeper for FakeSleeper {
    fn sleep(&self, duration: Duration) {
        *self.sleeps.lock().unwrap() += 1;
        self.clock.advance(duration);
        if let Some(boot_id) = self.next_boot_id.lock().unwrap().take() {
            self.clock.set_boot_id(&boot_id);
        }
    }
}

pub(super) struct FakeEvidence {
    scripts: Mutex<BTreeMap<String, VecDeque<WorkerOutcome>>>,
}

impl FakeEvidence {
    pub(super) fn new(scripts: impl IntoIterator<Item = (String, Vec<WorkerOutcome>)>) -> Self {
        Self {
            scripts: Mutex::new(
                scripts
                    .into_iter()
                    .map(|(id, outcomes)| (id, outcomes.into()))
                    .collect(),
            ),
        }
    }
}

impl EvidenceSource for FakeEvidence {
    fn outcome(&self, worker: &BoundWorker) -> WorkerOutcome {
        let mut scripts = self.scripts.lock().unwrap();
        let script = scripts.get_mut(&worker.worker.id).unwrap();
        if script.len() > 1 {
            script.pop_front().unwrap()
        } else {
            script.front().unwrap().clone()
        }
    }
}

pub(super) struct Fixture {
    pub(super) _temp: TempDir,
    pub(super) dir: LeaseDir,
    pub(super) identity: WaitIdentity,
    pub(super) clock: FakeClock,
    pub(super) probe: FakeProbe,
}

pub(super) fn fixture() -> Fixture {
    let temp = tempfile::Builder::new()
        .prefix("loom-wait-")
        .tempdir_in(std::env::temp_dir())
        .unwrap();
    let identity = identity();
    let dir = LeaseDir::open(temp.path(), &identity).unwrap();
    Fixture {
        _temp: temp,
        dir,
        identity,
        clock: FakeClock::new(),
        probe: FakeProbe::new(owner(100, Some(10))),
    }
}

pub(super) fn identity() -> WaitIdentity {
    WaitIdentity {
        canonical_repo: "/repo".into(),
        canonical_worktree: "/repo/worktree".into(),
        stage_id: "stage-a".into(),
        loom_session_id: "loom-a".into(),
        parent_session_id: "parent-a".into(),
        workers: vec![claude_worker("claude-a"), codex_worker("codex-a")],
    }
}

fn claude_worker(id: &str) -> BoundWorker {
    BoundWorker {
        worker: WorkerSpec {
            kind: WorkerKind::Claude,
            id: id.into(),
        },
        lifecycle_identity: WorkerIdentity::ClaudeSubagent {
            stage_id: "stage-a".into(),
            loom_session_id: "loom-a".into(),
            parent_session_id: "parent-a".into(),
            agent_id: id.into(),
            agent_type: "general-purpose".into(),
            transcript_path: "/tmp/transcript.jsonl".into(),
        },
        authority: None,
        evidence: Vec::new(),
    }
}

fn codex_worker(id: &str) -> BoundWorker {
    BoundWorker {
        worker: WorkerSpec {
            kind: WorkerKind::Codex,
            id: id.into(),
        },
        lifecycle_identity: WorkerIdentity::Codex {
            stage_id: "stage-a".into(),
            loom_session_id: "loom-a".into(),
            parent_session_id: "parent-a".into(),
            forwarder_agent_id: "forwarder-a".into(),
            unit_id: id.into(),
            invocation_id: "invocation-a".into(),
            workspace_root: "/repo/worktree".into(),
            execution: CodexExecution::Direct {
                thread_id: "thread-a".into(),
                tool_use_id: "tool-a".into(),
            },
        },
        authority: None,
        evidence: Vec::new(),
    }
}

pub(super) fn owner(pid: u32, start_time: Option<u64>) -> LeaseOwner {
    LeaseOwner { pid, start_time }
}

pub(super) fn acquire_owner(fixture: &Fixture, timeout: Duration) -> WaitLease {
    match acquire(
        &fixture.dir,
        &fixture.identity,
        timeout,
        "revision-a",
        &fixture.clock,
        &fixture.probe,
    )
    .unwrap()
    {
        Acquired::Owner(lease) => lease,
        acquired => panic!("expected owner, got {acquired:?}"),
    }
}

#[test]
fn parse_worker_set_rejects_invalid_sets() {
    let cases = [
        Vec::<String>::new(),
        vec!["claude:x".into(), "claude:x".into()],
        vec!["claude:x".into(), "codex:x".into()],
        vec!["claude:../x".into()],
    ];

    let errors: Vec<_> = cases
        .iter()
        .map(|case| parse_worker_set(case).unwrap_err().to_string())
        .collect();
    assert_eq!(
        errors,
        [
            "watch now requires --worker claude:<agent-id> or --worker codex:<unit-id>",
            "duplicate worker selector",
            "conflicting worker id aliases",
            "worker id is empty or unsafe",
        ]
    );
}

#[test]
fn exit_code_maps_every_outcome_exactly() {
    let outcomes = [
        EventOutcome::Waiting,
        EventOutcome::Succeeded,
        EventOutcome::TimedOut,
        EventOutcome::Failed,
        EventOutcome::Cancelled,
        EventOutcome::AlreadyWaiting,
        EventOutcome::Busy,
        EventOutcome::Stalled,
        EventOutcome::Unknown,
        EventOutcome::Interrupted,
    ];

    assert_eq!(
        outcomes.map(|outcome| exit_code(&outcome)),
        [0, 0, 2, 3, 3, 4, 4, EXIT_STALLED, 5, 5]
    );
}

/// Exit 6 is the wait's own code for a hung worker and must not collide with
/// the deadline (2), terminal (3), ownership (4) or unknown (5) codes.
#[test]
fn stalled_exit_code_is_distinct() {
    assert_eq!(EXIT_STALLED, 6);
    assert!(![
        0,
        EXIT_TIMEOUT,
        EXIT_WORKER_TERMINAL,
        EXIT_BUSY,
        EXIT_UNKNOWN
    ]
    .contains(&EXIT_STALLED));
}
