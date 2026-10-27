//! Every ended adjudication round of a stage, for its history: ruled by an
//! applied verdict, or closed without one when the stage escalated or was
//! reset. An open dispute, and one whose verdict is filed but not yet
//! applied, is the stage's current state, not its history.
//!
//! The status payload is rebuilt about once a second, so a dispute's end
//! markers are checked before any record is read, and a parsed round is kept
//! while the stamp of its end records is unchanged: an ended dispute's records
//! are not rewritten.

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{LazyLock, Mutex, PoisonError};
use std::time::SystemTime;

use chrono::{DateTime, Utc};
use serde_json::Value;

use crate::fs::work_dir::WorkDir;
use crate::models::dispute::CLOSED_MARKER;
use crate::models::stage::{IssueKind, Stage, StageIssue};

use super::disputes::{frontmatter, sorted_dispute_ids, text, timestamp};
use super::sanitize::valid_stage_id;

/// Most parsed rounds kept, across every stage the process has shown; the
/// cache is emptied when it fills. Revisit if a daemon serves more ended
/// disputes than this and the re-reads show in the frame time.
const MAX_CACHED_ROUNDS: usize = 1024;

/// One ended round, with the reason its dispute was filed with.
#[derive(Debug, Clone)]
pub(super) struct Round {
    pub(super) issue: StageIssue,
    /// The filing's reason line by line, as `Stage::review_reason` held it
    /// while the dispute was open; empty when the request names none.
    pub(super) reason_lines: Vec<String>,
}

/// When each of a dispute's end records was last written; `None` for one
/// that is absent.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct EndStamp {
    applied: Option<SystemTime>,
    closed: Option<SystemTime>,
    verdict: Option<SystemTime>,
}

impl EndStamp {
    fn of(dir: &Path) -> Self {
        Self {
            applied: modified(&dir.join("applied.marker")),
            closed: modified(&dir.join(CLOSED_MARKER)),
            verdict: modified(&dir.join("verdict.md")),
        }
    }
}

type Cache = HashMap<PathBuf, (EndStamp, Round)>;

static ROUNDS: LazyLock<Mutex<Cache>> = LazyLock::new(Mutex::default);

/// Every ended round of `stage`, oldest first.
pub(super) fn adjudication_rounds(stage: &Stage, work_dir: &WorkDir) -> Vec<Round> {
    if !valid_stage_id(&stage.id) {
        return Vec::new();
    }
    let stage_dir = work_dir.disputes_dir().join(&stage.id);
    sorted_dispute_ids(&stage_dir)
        .into_iter()
        .filter_map(|id| ended_round(&stage_dir.join(id.to_string()), id))
        .collect()
}

/// The dispute's round if it ended, parsed at most once per end stamp.
fn ended_round(dir: &Path, id: u32) -> Option<Round> {
    let stamp = EndStamp::of(dir);
    if stamp.applied.is_none() && stamp.closed.is_none() {
        return None;
    }
    // The lock is held only for the lookup and the insert, never across the
    // file reads.
    let hit = {
        let cache = ROUNDS.lock().unwrap_or_else(PoisonError::into_inner);
        cache
            .get(dir)
            .filter(|(cached, _)| *cached == stamp)
            .map(|(_, round)| round.clone())
    };
    if hit.is_some() {
        return hit;
    }
    let round = read_round(dir, id, &stamp);
    let mut cache = ROUNDS.lock().unwrap_or_else(PoisonError::into_inner);
    if cache.len() >= MAX_CACHED_ROUNDS {
        cache.clear();
    }
    cache.insert(dir.to_path_buf(), (stamp, round.clone()));
    Some(round)
}

/// The round of an ended dispute: its kind and reason, then the ruling and
/// its reasoning when an applied verdict carries one, each in full.
fn read_round(dir: &Path, id: u32, stamp: &EndStamp) -> Round {
    let request = frontmatter(&dir.join("request.md"));
    let verdict = match stamp.applied {
        Some(_) => frontmatter(&dir.join("verdict.md")),
        None => Value::Null,
    };
    let ruling = verdict.get("verdict");
    let (ended_at, ended_by) = ending(&verdict, stamp);
    let kind = text(request.get("kind")).unwrap_or_else(|| "dispute".to_owned());
    let reason = text(request.get("reason"));
    let reason_lines: Vec<String> = reason
        .iter()
        .flat_map(|reason| reason.lines().map(str::to_owned))
        .collect();
    let mut evidence = vec![format!("{kind} dispute {id}")];
    evidence.extend(reason_lines.iter().cloned());
    if let Some(reasoning) = text(ruling.and_then(|ruling| ruling.get("reasoning"))) {
        evidence.extend(reasoning.lines().map(str::to_owned));
    }
    let issue = StageIssue {
        kind: IssueKind::Adjudication,
        failure_type: None,
        detected_at: timestamp(request.get("created_at")),
        evidence,
        ended_at,
        ended_by,
        dispute_id: Some(id),
    };
    Round {
        issue,
        reason_lines,
    }
}

/// When and how the dispute ended. An applied verdict ends it at its own
/// `created_at`, or when `verdict.md` was written if that does not parse; a
/// closure ends it when the marker was written.
fn ending(verdict: &Value, stamp: &EndStamp) -> (DateTime<Utc>, String) {
    let decision = text(
        verdict
            .get("verdict")
            .and_then(|ruling| ruling.get("verdict")),
    );
    match (decision, stamp.applied, stamp.closed) {
        (Some(decision), Some(applied), _) => (
            timestamp(verdict.get("created_at"))
                .or_else(|| stamp.verdict.map(DateTime::from))
                .unwrap_or_else(|| applied.into()),
            format!("adjudicator verdict: {decision}"),
        ),
        (_, _, Some(closed)) if stamp.verdict.is_some() && stamp.applied.is_none() => (
            closed.into(),
            "closed before its verdict was applied".to_owned(),
        ),
        (_, _, Some(closed)) => (closed.into(), "closed without a verdict".to_owned()),
        // Applied with no readable ruling: the apply cap gave up on it.
        (_, applied, None) => (
            applied.map_or_else(Utc::now, DateTime::from),
            "ended without a readable verdict".to_owned(),
        ),
    }
}

/// When the record at `path` was last written; `None` when it is absent.
/// The link itself is stamped, never its target.
fn modified(path: &Path) -> Option<SystemTime> {
    fs::symlink_metadata(path).ok()?.modified().ok()
}

#[cfg(test)]
#[path = "rounds_tests.rs"]
mod tests;
