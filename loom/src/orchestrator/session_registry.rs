//! One honest answer to "is an agent already running for this stage?".
//!
//! Every other consumer of session state — `loom attach`, orphan recovery,
//! `loom status` — discovers sessions by reading `.loom/work/sessions/*.md`, so the
//! session record is their discovery source. Current spawn paths write a
//! Spawning record before linking the stage and launching the agent, but the
//! post-launch PID/status update can fail, and older state may lack a record.
//!
//! The agent's OS-level evidence outlives the daemon, in two places the spawn
//! path writes: `.loom/work/pids/<tracking_key>-<session_id>.pid`
//! from the wrapper script (`read_pid_entry`), and, on the tmux lane, a
//! socket named `loom-<session_id>` (`socket_path_for`).
//!
//! This module reads that evidence back. [`live_sessions_for_stage`] answers
//! the question from records, [`orphan_evidence`] finds the agents the records
//! missed, and [`adopt_orphan`] rebuilds the missing record so every other
//! call site can see the agent again.

use anyhow::{Context, Result};
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use crate::models::session::{Session, SessionBackendKind, SessionStatus, SessionType};
use crate::models::stage::StageStatus;
use crate::verify::transitions::parse_stage_from_markdown;

use super::terminal::backend::SessionBackend;
use super::terminal::native::{pid_only_is_alive, session_process_status, SessionProcessStatus};

#[path = "session_registry/in_progress.rs"]
mod in_progress;
pub use in_progress::in_progress_sessions_for_stage;

/// Every session kind a spawned agent can carry. Each one derives a different
/// tracking key from the same stage id ([`Session::derive_tracking_key`]), so
/// a scan keyed on the stage alone has to try every one. `Contract` is here
/// because it is the stage's agent while it runs.
const SESSION_KINDS: [SessionType; 5] = [
    SessionType::Stage,
    SessionType::Contract,
    SessionType::Merge,
    SessionType::Knowledge,
    SessionType::BaseConflict,
];

/// How a scan answers "is this session's process alive?": through the
/// configured terminal backend, or through PID identity alone when no backend
/// can be built.
enum LivenessProbe {
    Backend(SessionBackend),
    PidOnly,
    #[cfg(test)]
    Error,
}

impl LivenessProbe {
    /// The configured backend, degrading to PID-only rather than failing.
    ///
    /// `SessionBackend::from_config` builds the native lane EAGERLY when the
    /// config says `native` (`backend.rs:99-102`), and that runs terminal
    /// detection, which bails on a headless host, in CI, or in a container.
    /// Propagating that would break `loom stage reset` and stop the daemon
    /// spawning ANY stage there, since both ask this module first. Degrading
    /// is what `backend.rs:307-311` already does for a native session whose
    /// lane will not build: "only the window-existence layer is unavailable,
    /// and the PID layers are the authoritative ones anyway".
    fn resolve(work_dir: &Path) -> Self {
        match SessionBackend::from_config(work_dir.to_path_buf()) {
            Ok(backend) => Self::Backend(backend),
            Err(error) => {
                // Once per process, not once per session and not once per
                // tick: on a terminal-less host this resolves on every scan,
                // and the condition never changes while the daemon runs.
                static DEGRADED: std::sync::Once = std::sync::Once::new();
                DEGRADED.call_once(|| {
                    tracing::debug!(
                        %error,
                        "No terminal backend available; answering session liveness from PID identity alone"
                    );
                });
                Self::PidOnly
            }
        }
    }

    fn is_alive(&self, work_dir: &Path, session: &Session) -> Result<bool> {
        let alive = match self {
            Self::Backend(backend) => backend.is_session_alive(session),
            Self::PidOnly => Ok(pid_only_is_alive(work_dir, session)),
            #[cfg(test)]
            Self::Error => anyhow::bail!("injected liveness failure"),
        }?;
        if !alive && session_process_status(work_dir, session) == SessionProcessStatus::Missing {
            anyhow::bail!("session '{}' has no PID identity evidence", session.id);
        }
        Ok(alive)
    }
}

/// Every session RECORD assigned to `stage_id` whose process is alive.
///
/// This is the question the executor must ask before spawning and recovery
/// must ask before rewriting a stage: not "does a session file exist" but "is
/// something actually running". An unreadable session file or failed liveness
/// probe makes ownership uncertain and returns an error. An unbuildable
/// terminal backend uses the PID-only probe instead.
pub fn live_sessions_for_stage(work_dir: &Path, stage_id: &str) -> Result<Vec<Session>> {
    let probe = LivenessProbe::resolve(work_dir);
    live_sessions_with_probe(&probe, work_dir, stage_id)
}

/// [`live_sessions_for_stage`] narrowed to one session kind. Adoption of a
/// stage's worker must use this with the stage's worker type: an
/// adjudication session carries the stage's own `stage_id` and would
/// otherwise be adopted as if it were the agent.
pub fn live_sessions_for_stage_of_type(
    work_dir: &Path,
    stage_id: &str,
    session_type: SessionType,
) -> Result<Vec<Session>> {
    Ok(live_sessions_for_stage(work_dir, stage_id)?
        .into_iter()
        .filter(|s| s.session_type == session_type)
        .collect())
}

/// Check one assigned session through the validated inventory. A missing
/// record or confirmed dead process is false; malformed inventory, a
/// mismatched owner, or a failed probe leaves ownership unknown.
pub fn assigned_session_is_live(work_dir: &Path, stage_id: &str, session_id: &str) -> Result<bool> {
    let sessions = in_progress::session_inventory(work_dir)?.strict_records()?;
    let Some(session) = sessions
        .into_iter()
        .find(|session| session.id == session_id)
    else {
        return Ok(false);
    };
    anyhow::ensure!(
        session.stage_id.as_deref() == Some(stage_id),
        "assigned session '{}' belongs to {:?}, not '{}'",
        session_id,
        session.stage_id,
        stage_id
    );
    LivenessProbe::resolve(work_dir)
        .is_alive(work_dir, &session)
        .with_context(|| format!("probing assigned session '{session_id}'"))
}

/// [`live_sessions_for_stage`] against an already-resolved probe, so a scan
/// over many stages pays terminal detection at most once.
fn live_sessions_with_probe(
    probe: &LivenessProbe,
    work_dir: &Path,
    stage_id: &str,
) -> Result<Vec<Session>> {
    let sessions = in_progress::session_inventory(work_dir)?.strict_records()?;
    let mut live = Vec::new();
    for session in sessions {
        if session.stage_id.as_deref() != Some(stage_id) {
            continue;
        }
        if !matches!(
            session.status,
            SessionStatus::Running | SessionStatus::Spawning
        ) {
            continue;
        }
        if probe
            .is_alive(work_dir, &session)
            .with_context(|| format!("probing session '{}'", session.id))?
        {
            live.push(session);
        }
    }
    Ok(live)
}

/// A live agent for which no session record exists.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OrphanEvidence {
    pub session_id: String,
    pub stage_id: String,
    pub tracking_key: String,
    pub session_type: SessionType,
    pub pid: u32,
    pub backend: SessionBackendKind,
}

/// The pid file this evidence was recovered from. Its stem is the key every
/// liveness probe looks up (`NativeBackend::window_title_and_pid_key`).
fn pid_file_path(work_dir: &Path, evidence: &OrphanEvidence) -> PathBuf {
    work_dir.join("pids").join(format!(
        "{}-{}.pid",
        evidence.tracking_key, evidence.session_id
    ))
}

/// Live agents with no session record, for every stage currently claiming
/// `Executing`.
///
/// Driven from the STAGE side rather than the pid-file side on purpose: a pid
/// filename is `<tracking_key>-<session_id>`, and neither half is delimited
/// unambiguously, so parsing a stage id back out of one is guesswork. Deriving
/// the expected prefix from a known stage id inverts that into an exact match.
///
/// Never returns `Err`: this feeds a best-effort recovery pass that must keep
/// the daemon running whatever it finds. An unreadable stage or pid file is
/// logged and contributes no evidence, which is the safe direction — nothing
/// gets adopted. A missing terminal is not a failure at all here; liveness
/// degrades to PID identity (see `LivenessProbe::resolve`), because a
/// container is exactly where an unadoptable orphan hurts most.
pub fn orphan_evidence(work_dir: &Path) -> Vec<OrphanEvidence> {
    let pid_files = list_pid_files(work_dir);
    if pid_files.is_empty() {
        return Vec::new();
    }

    let probe = LivenessProbe::resolve(work_dir);

    let mut evidence = Vec::new();
    // One session id can only be adopted once, even if two stages' tracking
    // keys both claim it (possible for pathological stage ids such as `a` and
    // `a-b`). First stage in sorted order wins, deterministically.
    let mut claimed: HashSet<String> = HashSet::new();

    for stage_id in executing_stage_ids(work_dir) {
        // A stage with a live RECORD is either healthy or the duplicate-agent
        // case; neither is an orphan, and neither is this pass's to resolve.
        let live = match live_sessions_with_probe(&probe, work_dir, &stage_id) {
            Ok(live) => live,
            Err(error) => {
                tracing::warn!(stage_id = %stage_id, %error, "Cannot establish orphan ownership");
                continue;
            }
        };
        if !live.is_empty() {
            continue;
        }

        let mut candidates = stage_candidates(work_dir, &stage_id, &pid_files, &claimed);
        // Newest pid file last, so `pop` takes the most recent attempt.
        candidates.sort_by_key(|(_, mtime)| *mtime);
        let Some((chosen, _)) = candidates.pop() else {
            continue;
        };
        for (loser, _) in &candidates {
            tracing::warn!(
                stage_id = %stage_id,
                session_id = %loser.session_id,
                "Multiple unrecorded live agents for one stage; adopting the newest and leaving this one untouched"
            );
        }
        claimed.insert(chosen.session_id.clone());
        evidence.push(chosen);
    }

    evidence
}

/// Every unrecorded-but-alive agent whose pid file belongs to `stage_id`,
/// paired with that pid file's mtime (its best available spawn time).
///
/// Per-pid-file evaluation lives in `orphan_candidates.rs` (split out to
/// keep this file at its 400-line ceiling).
fn stage_candidates(
    work_dir: &Path,
    stage_id: &str,
    pid_files: &[(String, SystemTime)],
    claimed: &HashSet<String>,
) -> Vec<(OrphanEvidence, SystemTime)> {
    let mut candidates = Vec::new();
    for kind in SESSION_KINDS {
        for (stem, mtime) in pid_files {
            if let Some(candidate) =
                orphan_candidates::orphan_candidate(work_dir, stage_id, kind, stem, *mtime, claimed)
            {
                candidates.push(candidate);
            }
        }
    }
    candidates
}

/// `(pid-file stem, mtime)` for every `.loom/work/pids/*.pid`. Read once per scan:
/// the stage loop below consults the whole list for each stage.
fn list_pid_files(work_dir: &Path) -> Vec<(String, SystemTime)> {
    let Ok(entries) = std::fs::read_dir(work_dir.join("pids")) else {
        return Vec::new();
    };

    entries
        .flatten()
        .filter_map(|entry| {
            let path = entry.path();
            if path.extension().is_none_or(|ext| ext != "pid") {
                return None;
            }
            let stem = path.file_stem()?.to_str()?.to_string();
            // An unreadable mtime sorts oldest rather than dropping the
            // candidate: losing the tie-break beats losing the agent.
            let mtime = entry
                .metadata()
                .and_then(|meta| meta.modified())
                .unwrap_or(SystemTime::UNIX_EPOCH);
            Some((stem, mtime))
        })
        .collect()
}

/// Ids of every stage file claiming `Executing`, sorted so a scan is
/// deterministic. An unreadable or unparseable stage file is skipped, not
/// fatal — the same tolerance the recovery pass applies to its own index.
fn executing_stage_ids(work_dir: &Path) -> Vec<String> {
    let Ok(entries) = std::fs::read_dir(work_dir.join("stages")) else {
        return Vec::new();
    };

    let mut ids: Vec<String> = entries
        .flatten()
        .filter_map(|entry| {
            let path = entry.path();
            if path.extension().is_none_or(|ext| ext != "md") {
                return None;
            }
            let content = crate::fs::locking::locked_read(&path).ok()?;
            let stage = parse_stage_from_markdown(&content).ok()?;
            (stage.status == StageStatus::Executing).then_some(stage.id)
        })
        .collect();
    ids.sort();
    ids
}

/// Rebuild and persist a session record from evidence. Returns the
/// reconstructed session.
///
/// Does NOT touch the stage file — linking `stage.session` needs the stage
/// lock and the caller's own re-validation, so it stays the caller's job.
///
/// The record produced here must satisfy every filter in
/// `viewer::load_live_tmux_session` (backend, `Running`, PID-verified
/// liveness, and a non-empty `tracking_key` so `window_title_and_pid_key`
/// resolves) or the adopted agent is still unattachable and the adoption bought
/// nothing.
pub fn adopt_orphan(work_dir: &Path, evidence: &OrphanEvidence) -> Result<Session> {
    let mut session = Session::new();
    session.id = evidence.session_id.clone();

    // The kind must be set BEFORE `assign_to_stage`, which derives the
    // tracking key from `(stage_id, session_type)`; assigning first would
    // stamp a `Stage`-shaped key onto a merge or knowledge session.
    session.session_type = evidence.session_type;
    session.assign_to_stage(evidence.stage_id.clone());
    // Then take the evidence's key verbatim: it is the string the pid file on
    // disk is actually named after, which is what every liveness lookup uses.
    session.tracking_key = evidence.tracking_key.clone();

    session.pid = Some(evidence.pid);
    session.backend = evidence.backend;
    session.status = SessionStatus::Running;

    // The wrapper writes the pid file at spawn, so its mtime is real evidence
    // of when this agent started — unlike `Session::new`'s "now", which would
    // report a two-hour-old session as seconds old in `loom status`.
    let pid_file = pid_file_path(work_dir, evidence);
    if let Ok(spawned_at) = std::fs::metadata(&pid_file).and_then(|meta| meta.modified()) {
        session.created_at = spawned_at.into();
    }

    // `worktree_path`, `context_tokens`, `last_active` and the `merge_*`
    // branches keep `Session::new`'s defaults: none of them leave OS-level
    // evidence, so the daemon's death took them for good. Nothing depends on
    // them being right — the parked heuristic
    // (`monitor::parked::stage_looks_finished`) declines on a missing worktree
    // path rather than guessing, and hang detection reads the agent's own
    // `.loom/work/heartbeat/<stage-id>.json`, not `last_active`.

    crate::fs::session_files::save_session(&session, work_dir)?;
    Ok(session)
}

#[path = "orphan_candidates.rs"]
mod orphan_candidates;

#[cfg(test)]
#[path = "tests_session_registry.rs"]
mod tests;
