//! Contracts of stage native-ingress: the worker-control relay kind, `start`
//! admission (idempotence, conflict, per-stage cap, brief containment) and the
//! settlement of a supervisor that died without a terminal record.
//!
//! Layout every admission test uses: `<tmp>/repo` is the repository root,
//! `<tmp>/repo/.loom/work` the `work_dir`, and `<tmp>/repo/.worktrees/<stage>`
//! the stage worktree (`Stage::worktree` names it).

use std::fs;
use std::path::{Path, PathBuf};

use chrono::Utc;

use super::admission::{admit_start, AdmissionOutcome, StartRequest};
use super::ids::mint_invocation_id;
use super::index::unit_state;
use super::model::{
    BoundaryIdentity, NativeEvent, NativeManifest, Outcome, Ownership, NATIVE_DEADLINE_SECS,
};
use super::runner::settle_lost_supervisor;
use super::store::EventLog;
use super::store::{create_invocation, invocation_dir, read_events, stage_dir, write_boundary};
use crate::models::session::SessionType;
use crate::models::stage::{Implementer, Implementers, Stage};
use crate::relay::{verdict, MatrixVerdict, RequestKind};
use crate::utils::sha256_hex;

const STAGE: &str = "native-ingress-contract";
const OWNER: &str = "session-lead";
const MODEL: &str = "gpt-6.1-terra";
const EFFORT: &str = "xhigh";
const BRIEF: &[u8] = b"Implement the parser.\n";

struct Fixture {
    _tmp: tempfile::TempDir,
    work: PathBuf,
    worktree: PathBuf,
    stage: Stage,
}

fn fixture() -> Fixture {
    let tmp = tempfile::TempDir::new().expect("temp dir");
    let repo = tmp
        .path()
        .canonicalize()
        .expect("canonical temp dir")
        .join("repo");
    let work = repo.join(".loom").join("work");
    let worktree = repo.join(".worktrees").join(STAGE);
    fs::create_dir_all(&work).expect("work dir");
    fs::create_dir_all(&worktree).expect("worktree");
    let stage = Stage {
        id: STAGE.to_string(),
        worktree: Some(STAGE.to_string()),
        implementers: Implementers::new(vec![Implementer::Codex, Implementer::Claude]),
        ..Stage::default()
    };
    Fixture {
        _tmp: tmp,
        work,
        worktree,
        stage,
    }
}

fn request(unit: &str, brief_path: &Path, contents: &[u8], model: &str) -> StartRequest {
    StartRequest {
        unit_id: unit.to_string(),
        brief_path: brief_path.to_path_buf(),
        brief_sha256: sha256_hex(contents),
        model: model.to_string(),
        effort: EFFORT.to_string(),
    }
}

impl Fixture {
    /// Writes `BRIEF` at `<worktree>/brief.md` and returns a request naming it.
    fn start(&self, unit: &str, model: &str) -> StartRequest {
        let path = self.worktree.join("brief.md");
        fs::write(&path, BRIEF).expect("brief");
        request(unit, &path, BRIEF, model)
    }

    fn admit(&self, request: &StartRequest) -> Result<AdmissionOutcome, String> {
        admit_start(&self.work, &self.stage, OWNER, request)
    }

    /// Names of the invocation directories of the stage; none when the stage
    /// directory was never created.
    fn invocations(&self) -> Vec<String> {
        let Ok(entries) = fs::read_dir(stage_dir(&self.work, STAGE)) else {
            return Vec::new();
        };
        entries
            .map(|entry| {
                entry
                    .expect("dir entry")
                    .file_name()
                    .to_string_lossy()
                    .into_owned()
            })
            .collect()
    }
}

fn admitted(result: Result<AdmissionOutcome, String>, what: &str) -> String {
    match result {
        Ok(AdmissionOutcome::Admitted { invocation_id }) => invocation_id,
        Ok(AdmissionOutcome::AlreadyAdmitted { invocation_id }) => {
            panic!("{what}: AlreadyAdmitted {invocation_id}, expected Admitted")
        }
        Err(text) => panic!("{what}: refused: {text}"),
    }
}

fn refused(result: Result<AdmissionOutcome, String>, what: &str) -> String {
    match result {
        Ok(AdmissionOutcome::Admitted { invocation_id })
        | Ok(AdmissionOutcome::AlreadyAdmitted { invocation_id }) => {
            panic!("{what}: accepted as {invocation_id}, expected a refusal")
        }
        Err(text) => text,
    }
}

#[test]
fn subagent_cannot_issue_worker_control() {
    let kind = RequestKind::WorkerControl;
    assert!(kind.is_control(), "worker control must be a control kind");
    assert!(RequestKind::all().contains(&kind), "all() must list it");
    assert_eq!(kind.to_string(), "subagents:control");
    assert_eq!(verdict(SessionType::Stage, kind), MatrixVerdict::Apply);
    for session in [
        SessionType::Knowledge,
        SessionType::Merge,
        SessionType::BaseConflict,
        SessionType::Adjudication,
        SessionType::Contract,
    ] {
        assert_eq!(
            verdict(session, kind),
            MatrixVerdict::Refuse,
            "a {session:?} session must not issue worker control"
        );
    }
}

#[test]
fn repeated_start_returns_same_invocation() {
    let fx = fixture();
    let start = fx.start("unit-a", MODEL);
    let first = admitted(fx.admit(&start), "first start");
    match fx.admit(&start) {
        Ok(AdmissionOutcome::AlreadyAdmitted { invocation_id }) => {
            assert_eq!(invocation_id, first, "the retry named another invocation")
        }
        Ok(AdmissionOutcome::Admitted { invocation_id }) => {
            panic!("the retry minted a second invocation {invocation_id}")
        }
        Err(text) => panic!("the retry was refused: {text}"),
    }
    assert_eq!(fx.invocations(), vec![first]);
}

#[test]
fn changed_request_for_running_unit_is_refused() {
    let fx = fixture();
    let first = admitted(fx.admit(&fx.start("unit-a", MODEL)), "first start");
    let changed = fx.start("unit-a", "gpt-6.2-sol");
    let text = refused(fx.admit(&changed), "a changed request");
    assert!(
        text.contains(&first),
        "the refusal must name the running invocation {first}: {text}"
    );
    assert_eq!(fx.invocations(), vec![first]);
}

fn settle_released(work: &Path, invocation_id: &str) {
    let dir = invocation_dir(work, STAGE, invocation_id);
    let mut log = EventLog::open(&dir).expect("event log");
    let terminal = NativeEvent::Terminal {
        outcome: Outcome::Succeeded,
        reason: "turn completed".to_string(),
    };
    log.append(terminal).expect("terminal event");
    let released = NativeEvent::OwnershipReleased {
        by: "supervisor".to_string(),
    };
    log.append(released).expect("release event");
}

#[test]
fn seventh_worker_is_refused() {
    let fx = fixture();
    let ids: Vec<String> = (1..=6)
        .map(|n| {
            let unit = format!("unit-{n}");
            admitted(fx.admit(&fx.start(&unit, MODEL)), &unit)
        })
        .collect();
    let seventh = fx.start("unit-7", MODEL);
    refused(fx.admit(&seventh), "a seventh active worker");
    assert_eq!(fx.invocations().len(), 6, "the refused start left a record");
    settle_released(&fx.work, &ids[0]);
    admitted(fx.admit(&seventh), "the seventh after one finished");
    assert_eq!(fx.invocations().len(), 7);
}

#[test]
fn brief_outside_worktree_is_refused() {
    let fx = fixture();
    let outside = tempfile::TempDir::new().expect("outside dir");
    let path = outside
        .path()
        .canonicalize()
        .expect("canonical")
        .join("brief.md");
    fs::write(&path, BRIEF).expect("outside brief");
    let start = request("unit-a", &path, BRIEF, MODEL);
    refused(fx.admit(&start), "a brief outside the worktree");
    assert!(fx.invocations().is_empty(), "a refused start left a record");
}

#[test]
fn brief_symlink_is_refused() {
    let fx = fixture();
    let outside = tempfile::TempDir::new().expect("outside dir");
    let target = outside
        .path()
        .canonicalize()
        .expect("canonical")
        .join("host.md");
    fs::write(&target, BRIEF).expect("host file");
    let link = fx.worktree.join("brief.md");
    std::os::unix::fs::symlink(&target, &link).expect("symlink");
    let start = request("unit-a", &link, BRIEF, MODEL);
    refused(fx.admit(&start), "a symlinked brief");
    assert!(fx.invocations().is_empty(), "a refused start left a record");
}

/// Inode of the test process's own pid namespace (`/proc/self/ns/pid`), so
/// `verify_quiescent` finds live members in it.
#[cfg(target_os = "linux")]
pub(super) fn own_pid_namespace() -> u64 {
    let link = fs::read_link("/proc/self/ns/pid").expect("pid namespace link");
    let link = link.to_string_lossy();
    link.strip_prefix("pid:[")
        .and_then(|rest| rest.strip_suffix(']'))
        .and_then(|inode| inode.parse().ok())
        .expect("pid namespace inode")
}

/// macOS has no pid namespaces (nor `/proc`): `verify_quiescent` leaves every
/// boundary unverified there, whatever namespace a record names.
#[cfg(not(target_os = "linux"))]
pub(super) fn own_pid_namespace() -> u64 {
    0
}

/// A pid above any kernel's `pid_max` (at most 2^22), so no process has it.
const GONE_PID: i32 = i32::MAX - 7;

/// An invocation whose supervisor started a turn and then vanished: events
/// Admitted, BoundaryCreated, TurnStarted; boundary.json naming this
/// process's namespace; supervisor.json naming a pid that does not exist.
fn running_invocation_with_dead_supervisor(fx: &Fixture) -> PathBuf {
    let brief_path = fx.worktree.join("brief.md");
    let manifest = NativeManifest {
        v: 1,
        stage_id: STAGE.to_string(),
        unit_id: "unit-a".to_string(),
        invocation_id: mint_invocation_id().expect("invocation id"),
        owner_session_id: OWNER.to_string(),
        workspace_root: fx.worktree.clone(),
        brief_path,
        brief_sha256: sha256_hex(BRIEF),
        model: MODEL.to_string(),
        effort: EFFORT.to_string(),
        deadline_secs: NATIVE_DEADLINE_SECS,
        created_at: Utc::now(),
    };
    let dir = create_invocation(&fx.work, &manifest, BRIEF).expect("create invocation");
    let mut log = EventLog::open(&dir).expect("event log");
    log.append(NativeEvent::BoundaryCreated)
        .expect("boundary event");
    let turn_id = "turn-1".to_string();
    log.append(NativeEvent::TurnStarted { turn_id })
        .expect("turn event");
    let identity = BoundaryIdentity {
        supervisor_pid: GONE_PID,
        supervisor_start: "0".to_string(),
        boundary_pid: GONE_PID,
        pid_namespace: own_pid_namespace(),
        boot_id: crate::process::boot_id::os_boot_id().expect("boot id"),
        mac: None,
    };
    write_boundary(&dir, &identity).expect("boundary.json");
    let supervisor = format!("{{\"pid\": {GONE_PID}, \"start\": \"0\"}}");
    fs::write(dir.join("supervisor.json"), supervisor).expect("supervisor.json");
    dir
}

#[test]
fn supervisor_death_without_terminal_is_settled_unknown() {
    let fx = fixture();
    let dir = running_invocation_with_dead_supervisor(&fx);
    settle_lost_supervisor(&dir).expect("settle_lost_supervisor");
    let events: Vec<NativeEvent> = read_events(&dir)
        .expect("events")
        .into_iter()
        .map(|record| record.event)
        .collect();
    let expected_terminal = NativeEvent::Terminal {
        outcome: Outcome::Unknown,
        reason: "supervisor exited without a terminal record".to_string(),
    };
    assert!(events.contains(&expected_terminal), "events: {events:?}");
    assert!(
        matches!(events.last(), Some(NativeEvent::OwnershipUnverified { .. })),
        "a live namespace member must leave ownership unverified: {events:?}"
    );
    assert!(
        !events
            .iter()
            .any(|event| matches!(event, NativeEvent::OwnershipReleased { .. })),
        "ownership was released without proof: {events:?}"
    );
    let state = unit_state(&fx.work, STAGE, "unit-a")
        .expect("unit state")
        .expect("the unit has run");
    assert_eq!(state.outcome, Some(Outcome::Unknown));
    assert_eq!(state.ownership, Ownership::Unverified);
}
