//! Settlement of supervisors that vanished, and the pass that finds them.
//!
//! `events.jsonl` has one writer at a time, so the daemon appends to it only
//! when the supervisor is proven gone: the process `supervisor.json` names is
//! dead (or the host rebooted), or the daemon itself created `supervisor.json`
//! as a tombstone (`pid` 0), which makes a late supervisor refuse at its first
//! act. A record without `supervisor.json` is left to its supervisor for
//! `CLAIM_GRACE` after admission; after that it is tombstoned and settled.

use std::collections::HashSet;
use std::os::fd::AsRawFd;
use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::{Context, Result};
use chrono::Utc;
use serde::Deserialize;

use crate::fs::safe_fs::{safe_create_new_in_workdir, safe_open_dirfd};
use crate::native_worker::boundary::Quiescence;
use crate::native_worker::index;
use crate::native_worker::model::{NativeEvent, NativeUnitState, Outcome, Ownership, Phase};
use crate::native_worker::store::{self, EventLog, StoreError};
use crate::native_worker::store_io::read_json;
use crate::native_worker::supervisor::{prove_and_reclaim, unprovable_spawn};
use crate::process::{verify_process_identity, IdentityStatus, ProcessIdentity};

#[path = "runner_repair.rs"]
mod repair;

/// How long an admitted record may stay without `supervisor.json`.
const CLAIM_GRACE: Duration = Duration::from_secs(30);
const CLAIM_FILE: &str = "supervisor.json";

/// `supervisor.json`: the process that owns the invocation; `pid` 0 is a
/// daemon tombstone.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct SupervisorClaim {
    pid: i32,
    start: String,
}

/// What one pass over the records may do.
pub(super) struct Pass<'a> {
    pub(super) work_dir: &'a Path,
    /// Invocations whose supervisor is a child this runner still holds.
    pub(super) held: &'a HashSet<String>,
    /// Invocations this process already spawned a supervisor for.
    pub(super) spawned: &'a HashSet<String>,
    /// Whether a young admitted record may be started.
    pub(super) may_spawn: bool,
}

/// An action the pass leaves to its caller, or reports.
pub(super) enum Step {
    Spawn {
        invocation_id: String,
        dir: PathBuf,
    },
    Settled(String),
    /// A bad condition first seen in this pass. The pass runs every tick, so a
    /// condition that persists is reported once, and again only after it cleared.
    Warn(Warning),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(super) enum WarnKind {
    Corrupt,
    CannotSettle,
    UnreadableClaim,
    UnverifiableIdentity,
}

/// A warning's identity: a stage's corrupt records have no invocation id.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub(super) struct WarnKey {
    pub(super) stage_id: String,
    pub(super) invocation_id: Option<String>,
    pub(super) kind: WarnKind,
}

pub(super) struct Warning {
    pub(super) key: WarnKey,
    pub(super) detail: String,
}

impl Warning {
    fn new(stage_id: &str, invocation_id: Option<&str>, kind: WarnKind, detail: String) -> Self {
        let key = WarnKey {
            stage_id: stage_id.to_string(),
            invocation_id: invocation_id.map(str::to_string),
            kind,
        };
        Self { key, detail }
    }

    pub(super) fn log(&self) {
        let what = match self.key.kind {
            WarnKind::Corrupt => "native worker records are corrupt; skipped",
            WarnKind::CannotSettle => "cannot settle native worker",
            WarnKind::UnreadableClaim => "cannot read supervisor.json",
            WarnKind::UnverifiableIdentity => {
                "native worker supervisor identity is unverifiable; treated as alive"
            }
        };
        let invocation_id = self.key.invocation_id.as_deref().unwrap_or("-");
        tracing::warn!(stage_id = %self.key.stage_id, %invocation_id, detail = %self.detail,
            "{what} (repeats suppressed until it clears)");
    }
}

/// Notes `decide` makes about one record, before the pass attributes them.
type Notes = Vec<(WarnKind, String)>;

enum Decision {
    Wait,
    Spawn,
    Settle,
}

/// Appends what a supervisor that died without settling would have: a
/// `Terminal { Unknown }` when none exists, then the ownership verdict. A
/// no-op when ownership was already released or marked unverified. Call only
/// once the supervisor is proven gone.
pub fn settle_lost_supervisor(dir: &Path) -> Result<()> {
    let events = store::read_events(dir).context("events.jsonl")?;
    let ownership_settled = events.iter().any(|record| {
        matches!(
            record.event,
            NativeEvent::OwnershipReleased { .. } | NativeEvent::OwnershipUnverified { .. }
        )
    });
    if ownership_settled {
        return Ok(());
    }
    let mut log = EventLog::open(dir).context("events.jsonl")?;
    let has_terminal = events
        .iter()
        .any(|record| matches!(record.event, NativeEvent::Terminal { .. }));
    let boundary = store::read_boundary(dir).context("boundary.json")?;
    // A boundary process may have run without an identity on record: nothing
    // can prove it gone.
    let unrecorded = boundary.is_none() && store::spawn_begun(dir);
    if !has_terminal {
        log.append(lost_terminal(unrecorded))
            .context("terminal event")?;
    }
    let verdict = match (unprovable_spawn(dir, boundary.as_ref()), boundary) {
        (Some(detail), _) => Quiescence::Unverified(detail),
        // A proven release whose private root could not be removed appends
        // nothing: the record stays unsettled and the next pass retries.
        (None, Some(identity)) => prove_and_reclaim(&identity).map_err(anyhow::Error::msg)?,
        // No spawn marker: no provider process was ever started.
        (None, None) => Quiescence::Released,
    };
    let event = match verdict {
        Quiescence::Released => NativeEvent::OwnershipReleased {
            by: "daemon".to_string(),
        },
        Quiescence::Unverified(detail) => NativeEvent::OwnershipUnverified { detail },
    };
    log.append(event).context("ownership event")?;
    Ok(())
}

fn lost_terminal(unrecorded: bool) -> NativeEvent {
    let (outcome, reason) = if unrecorded {
        #[cfg(target_os = "macos")]
        let reason = "supervisor exited after starting the macOS boundary and before recording it";
        #[cfg(not(target_os = "macos"))]
        let reason = "supervisor exited after starting bubblewrap and before recording it";
        (Outcome::Failed, reason)
    } else {
        (
            Outcome::Unknown,
            "supervisor exited without a terminal record",
        )
    };
    NativeEvent::Terminal {
        outcome,
        reason: reason.to_string(),
    }
}

/// Daemon start: settles every supervisor an earlier daemon left that is
/// gone. Starts nothing. Returns the invocation ids it settled.
pub fn recover_after_restart(work_dir: &Path) -> Vec<String> {
    let none = HashSet::new();
    let pass = Pass {
        work_dir,
        held: &none,
        spawned: &none,
        may_spawn: false,
    };
    run_pass(&pass, &mut HashSet::new(), &mut HashSet::new())
        .into_iter()
        .filter_map(|step| match step {
            Step::Settled(invocation_id) => Some(invocation_id),
            Step::Warn(warning) => {
                warning.log();
                None
            }
            Step::Spawn { .. } => None,
        })
        .collect()
}

/// Looks at every record not held and not yet settled; settles the lost
/// ones itself and returns the starts for the caller. `warned` holds the
/// warnings of the previous pass; only the ones absent from it are returned.
pub(super) fn run_pass(
    pass: &Pass<'_>,
    settled: &mut HashSet<String>,
    warned: &mut HashSet<WarnKey>,
) -> Vec<Step> {
    let mut steps = Vec::new();
    let mut found = Vec::new();
    for stage_id in index::stages(pass.work_dir) {
        pass_stage(pass, &stage_id, settled, &mut steps, &mut found);
    }
    let previous = std::mem::take(warned);
    for warning in found {
        if warned.insert(warning.key.clone()) && !previous.contains(&warning.key) {
            steps.push(Step::Warn(warning));
        }
    }
    steps
}

/// The stage's units. Healthy stages pay nothing extra: torn journal tails are
/// repaired, and the units reloaded, only once a record failed to load. Held
/// invocations are never repaired.
fn load_units(
    work_dir: &Path,
    stage_id: &str,
    held: &HashSet<String>,
    found: &mut Vec<Warning>,
) -> Vec<Result<NativeUnitState, StoreError>> {
    let units = index::stage_units(work_dir, stage_id);
    if units.iter().any(Result::is_err)
        && repair::repair_lost_tails(work_dir, stage_id, held, found)
    {
        return index::stage_units(work_dir, stage_id);
    }
    units
}

fn pass_stage(
    pass: &Pass<'_>,
    stage_id: &str,
    settled: &mut HashSet<String>,
    steps: &mut Vec<Step>,
    found: &mut Vec<Warning>,
) {
    let mut corrupt = 0usize;
    for unit in load_units(pass.work_dir, stage_id, pass.held, found) {
        let Ok(state) = unit else {
            corrupt += 1;
            continue;
        };
        let invocation_id = state.manifest.invocation_id.clone();
        if settled.contains(&invocation_id) || pass.held.contains(&invocation_id) {
            continue;
        }
        if state.phase == Phase::Terminal && state.ownership != Ownership::Held {
            settled.insert(invocation_id);
            continue;
        }
        let dir = store::invocation_dir(pass.work_dir, stage_id, &invocation_id);
        let mut notes = Notes::new();
        match decide(pass, &dir, &state, &mut notes) {
            Decision::Wait => {}
            Decision::Spawn => steps.push(Step::Spawn {
                invocation_id: invocation_id.clone(),
                dir,
            }),
            Decision::Settle => match settle_lost_supervisor(&dir) {
                Ok(()) => steps.push(Step::Settled(invocation_id.clone())),
                Err(error) => notes.push((WarnKind::CannotSettle, format!("{error:#}"))),
            },
        }
        found.extend(
            notes
                .into_iter()
                .map(|(kind, detail)| Warning::new(stage_id, Some(&invocation_id), kind, detail)),
        );
    }
    if corrupt > 0 {
        let detail = format!("{corrupt} corrupt records");
        found.push(Warning::new(stage_id, None, WarnKind::Corrupt, detail));
    }
}

fn decide(pass: &Pass<'_>, dir: &Path, state: &NativeUnitState, notes: &mut Notes) -> Decision {
    let claim = match read_claim(dir) {
        Ok(claim) => claim,
        Err(error) => {
            notes.push((WarnKind::UnreadableClaim, error.to_string()));
            return Decision::Wait;
        }
    };
    match claim {
        Some(claim) if claim.pid <= 0 => Decision::Settle,
        Some(claim) if supervisor_gone(dir, &claim, notes) => Decision::Settle,
        Some(_) => Decision::Wait,
        None => {
            let invocation_id = &state.manifest.invocation_id;
            let age = Utc::now().signed_duration_since(state.manifest.created_at);
            // A creation time in the future (clock step) counts as young.
            let young = age.to_std().ok().is_none_or(|age| age < CLAIM_GRACE);
            if young && !pass.spawned.contains(invocation_id) {
                if pass.may_spawn && state.phase == Phase::Admitted {
                    return Decision::Spawn;
                }
                return Decision::Wait;
            }
            if tombstone(dir) {
                Decision::Settle
            } else {
                Decision::Wait
            }
        }
    }
}

/// Whether the process `claim` names no longer runs. An identity that cannot
/// be verified counts as alive.
fn supervisor_gone(dir: &Path, claim: &SupervisorClaim, notes: &mut Notes) -> bool {
    if let Ok(Some(identity)) = store::read_boundary(dir) {
        let rebooted =
            crate::process::boot_id::os_boot_id().is_ok_and(|boot_id| boot_id != identity.boot_id);
        if rebooted {
            return true;
        }
    }
    let identity = ProcessIdentity {
        pid: claim.pid as u32,
        start_time: claim.start.parse().ok(),
    };
    match verify_process_identity(identity) {
        IdentityStatus::Dead => true,
        IdentityStatus::VerifiedAlive => false,
        IdentityStatus::Unverifiable => {
            notes.push((WarnKind::UnverifiableIdentity, format!("pid {}", claim.pid)));
            false
        }
    }
}

fn read_claim(dir: &Path) -> Result<Option<SupervisorClaim>, StoreError> {
    read_json(dir, CLAIM_FILE)
}

/// Creates `supervisor.json` naming pid 0, so no supervisor can take the
/// invocation afterwards. False when the file exists or cannot be created.
pub(super) fn tombstone(dir: &Path) -> bool {
    let created = safe_open_dirfd(dir).and_then(|fd| {
        safe_create_new_in_workdir(
            fd.as_raw_fd(),
            Path::new(CLAIM_FILE),
            br#"{"pid":0,"start":""}"#,
        )
    });
    if let Err(error) = &created {
        tracing::debug!(dir = %dir.display(), error = %format!("{error:#}"), "supervisor.json tombstone not created");
    }
    created.is_ok()
}

/// Records of non-terminal invocations a live-or-unknown supervisor claimed.
pub(super) fn claimed_running(work_dir: &Path) -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    for stage_id in index::stages(work_dir) {
        for state in index::stage_units(work_dir, &stage_id)
            .into_iter()
            .flatten()
        {
            if state.phase == Phase::Terminal {
                continue;
            }
            let dir = store::invocation_dir(work_dir, &stage_id, &state.manifest.invocation_id);
            if matches!(read_claim(&dir), Ok(Some(claim)) if claim.pid > 0) {
                dirs.push(dir);
            }
        }
    }
    dirs
}

#[cfg(test)]
#[path = "runner_recover_tests.rs"]
mod tests;
