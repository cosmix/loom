//! `.loom/work/reviews/<stage>/`: the recorded review rounds, and the rulings
//! and carried findings the review gate reads (DESIGN D12).
//!
//! | Path | Written by |
//! | --- | --- |
//! | `round-<n>.json` | the review-harvest hook delegate, once per round; never replaced |
//! | `rulings.json` | the adjudication of a findings dispute |
//! | `carried.json` | the adjudication that deferred another stage's finding to this one |
//!
//! None is written by the stage under review: its sandbox only reads `reviews/`.

use anyhow::{bail, Context, Result};
use chrono::{DateTime, Utc};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashSet};
use std::io;
use std::path::{Path, PathBuf};

use super::report::Finding;
use crate::fs::locking::{atomic_write_locked, locked_dir_update};
use crate::fs::safe_read::{is_not_found, read_bounded};
use crate::verify::contracts::store::canonical_work_dir;

/// The `version` of every record in `reviews/<stage>/`.
pub const RECORD_VERSION: u32 = 1;
const REVIEWS_DIR: &str = "reviews";
pub(in crate::verify) const RULINGS_FILE: &str = "rulings.json";
pub(in crate::verify) const CARRIED_FILE: &str = "carried.json";
const MAX_RECORD_BYTES: usize = 4 * 1024 * 1024;

/// `round-<n>.json`: one harvested review.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReviewRound {
    pub version: u32,
    pub round: u32,
    pub agent_id: String,
    pub harvested_at: DateTime<Utc>,
    /// The `sha256:<hex>` change fingerprint of the worktree the review saw.
    pub fingerprint: String,
    /// Path to sha256 hex, or `deleted`, as the fingerprint hashed them.
    pub files: BTreeMap<String, String>,
    /// Why the reviewer's `loom-review` block could not be read; such a round
    /// records no findings.
    pub malformed: Option<String>,
    pub findings: Vec<RecordedFinding>,
    pub resolved: Vec<String>,
    pub unresolved: Vec<String>,
    pub suggestion_memory_ids: Vec<String>,
}

impl ReviewRound {
    pub fn is_well_formed(&self) -> bool {
        self.malformed.is_none()
    }
}

/// A finding as its round records it, with the id [`finding_id`] gives it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RecordedFinding {
    pub id: String,
    #[serde(flatten)]
    pub finding: Finding,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RulingKind {
    Uphold,
    Dismiss,
    Defer,
}

impl RulingKind {
    /// `dismiss` and `defer` close a finding in the disputing stage; `uphold`
    /// leaves it open.
    pub fn closes(self) -> bool {
        matches!(self, Self::Dismiss | Self::Defer)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Ruling {
    pub finding: String,
    pub ruling: RulingKind,
    pub target_stage: Option<String>,
    pub dispute: u32,
}

/// `rulings.json`; an absent file loads as no rulings.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Rulings {
    pub version: u32,
    pub rulings: Vec<Ruling>,
}

impl Default for Rulings {
    fn default() -> Self {
        Self {
            version: RECORD_VERSION,
            rulings: Vec::new(),
        }
    }
}

/// A finding another stage's dispute deferred to this stage; its id is
/// `<origin-stage>/F-<round>-<k>`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CarriedFinding {
    pub id: String,
    pub origin_stage: String,
    pub finding: Finding,
    pub dispute: u32,
}

/// `carried.json`; an absent file loads as nothing carried.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Carried {
    pub version: u32,
    pub carried: Vec<CarriedFinding>,
}

impl Default for Carried {
    fn default() -> Self {
        Self {
            version: RECORD_VERSION,
            carried: Vec::new(),
        }
    }
}

/// A finding that still blocks completion; `origin_stage` is set for a
/// carried one.
#[derive(Debug, Clone, PartialEq)]
pub struct OpenFinding {
    pub id: String,
    pub origin_stage: Option<String>,
    pub finding: Finding,
}

/// The id of the `k`-th finding (from 1) of review round `round`.
pub fn finding_id(round: u32, k: usize) -> String {
    format!("F-{round}-{k}")
}

/// The number the next harvested round takes: one past the highest recorded.
pub fn next_round(work_dir: &Path, stage_id: &str) -> Result<u32> {
    let root = canonical_work_dir(work_dir, stage_id)?;
    let last = round_numbers(&root, stage_id)?.last().copied().unwrap_or(0);
    last.checked_add(1)
        .context("review round numbers are exhausted")
}

/// Record `round` as `round-<n>.json`. Refuses a number already recorded: a
/// harvested review is never replaced.
pub fn write_round(work_dir: &Path, stage_id: &str, round: &ReviewRound) -> Result<()> {
    let root = canonical_work_dir(work_dir, stage_id)?;
    validate_round(round)?;
    let dir = stage_dir(&root, stage_id);
    let json = serde_json::to_string_pretty(round)?;
    locked_dir_update(&dir, || {
        let path = dir.join(round_file(round.round));
        if path.symlink_metadata().is_ok() {
            bail!(
                "review round {} of stage '{stage_id}' is already recorded",
                round.round
            );
        }
        atomic_write_locked(&path, &json)
    })
}

/// Every recorded round of the stage, sorted by round number.
pub fn load_rounds(work_dir: &Path, stage_id: &str) -> Result<Vec<ReviewRound>> {
    let root = canonical_work_dir(work_dir, stage_id)?;
    round_numbers(&root, stage_id)?
        .into_iter()
        .map(|number| load_round(&root, stage_id, number))
        .collect()
}

pub fn load_rulings(work_dir: &Path, stage_id: &str) -> Result<Rulings> {
    let rulings: Rulings = load_optional(work_dir, stage_id, RULINGS_FILE)?.unwrap_or_default();
    check_version(
        rulings.version,
        &format!("{RULINGS_FILE} of stage '{stage_id}'"),
    )?;
    Ok(rulings)
}

pub fn load_carried(work_dir: &Path, stage_id: &str) -> Result<Carried> {
    let carried: Carried = load_optional(work_dir, stage_id, CARRIED_FILE)?.unwrap_or_default();
    check_version(
        carried.version,
        &format!("{CARRIED_FILE} of stage '{stage_id}'"),
    )?;
    Ok(carried)
}

/// The stage's findings, own and carried, that are still open.
pub fn open_findings(work_dir: &Path, stage_id: &str) -> Result<Vec<OpenFinding>> {
    let rounds = load_rounds(work_dir, stage_id)?;
    let rulings = load_rulings(work_dir, stage_id)?;
    let carried = load_carried(work_dir, stage_id)?;
    Ok(open_among(&rounds, &rulings, &carried))
}

/// Every finding of every well-formed round and every carried finding, minus
/// those a LATER well-formed round lists as `resolved` and those ruled
/// `dismiss` or `defer`. Every round of this stage is later than a carried
/// finding, which another stage's review raised.
pub fn open_among(
    rounds: &[ReviewRound],
    rulings: &Rulings,
    carried: &Carried,
) -> Vec<OpenFinding> {
    let well_formed: Vec<&ReviewRound> = rounds.iter().filter(|r| r.is_well_formed()).collect();
    let resolved_after = |round: u32, id: &str| {
        well_formed
            .iter()
            .any(|later| later.round > round && later.resolved.iter().any(|done| done == id))
    };
    let ruled_closed: HashSet<&str> = rulings
        .rulings
        .iter()
        .filter(|ruling| ruling.ruling.closes())
        .map(|ruling| ruling.finding.as_str())
        .collect();
    let mut open = Vec::new();
    for round in &well_formed {
        for recorded in &round.findings {
            if !resolved_after(round.round, &recorded.id) {
                open.push(OpenFinding {
                    id: recorded.id.clone(),
                    origin_stage: None,
                    finding: recorded.finding.clone(),
                });
            }
        }
    }
    for item in &carried.carried {
        if !resolved_after(0, &item.id) {
            open.push(OpenFinding {
                id: item.id.clone(),
                origin_stage: Some(item.origin_stage.clone()),
                finding: item.finding.clone(),
            });
        }
    }
    open.retain(|finding| !ruled_closed.contains(finding.id.as_str()));
    open
}

fn validate_round(round: &ReviewRound) -> Result<()> {
    check_version(round.version, &format!("review round {}", round.round))?;
    if round.round == 0 {
        bail!("review rounds are numbered from 1");
    }
    if !round.is_well_formed() && !round.findings.is_empty() {
        bail!("malformed review round {} records findings", round.round);
    }
    for (index, recorded) in round.findings.iter().enumerate() {
        let expected = finding_id(round.round, index + 1);
        if recorded.id != expected {
            bail!(
                "finding {} of review round {} must have id '{expected}', not '{}'",
                index + 1,
                round.round,
                recorded.id
            );
        }
    }
    Ok(())
}

fn load_round(root: &Path, stage_id: &str, number: u32) -> Result<ReviewRound> {
    let relative = Path::new(REVIEWS_DIR)
        .join(stage_id)
        .join(round_file(number));
    let bytes = read_bounded(root, &relative, MAX_RECORD_BYTES)?;
    let round: ReviewRound = serde_json::from_slice(&bytes)
        .with_context(|| format!("invalid review round {number} of stage '{stage_id}'"))?;
    if round.round != number {
        bail!(
            "{} of stage '{stage_id}' records round {}",
            round_file(number),
            round.round
        );
    }
    validate_round(&round).with_context(|| format!("review round of stage '{stage_id}'"))?;
    Ok(round)
}

/// Sorted numbers of the stage's `round-<n>.json` files.
fn round_numbers(root: &Path, stage_id: &str) -> Result<Vec<u32>> {
    let dir = stage_dir(root, stage_id);
    let entries = match std::fs::read_dir(&dir) {
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
        listed => listed.with_context(|| format!("failed to list {}", dir.display()))?,
    };
    let mut numbers = Vec::new();
    for entry in entries {
        let entry = entry.with_context(|| format!("failed to list {}", dir.display()))?;
        numbers.extend(entry.file_name().to_str().and_then(parse_round_file));
    }
    numbers.sort_unstable();
    Ok(numbers)
}

/// The round number of a canonical `round-<n>.json` name. `round-01.json` or
/// `round-+1.json` would alias round 1, so neither counts.
fn parse_round_file(name: &str) -> Option<u32> {
    let digits = name.strip_prefix("round-")?.strip_suffix(".json")?;
    if digits.starts_with('0') || !digits.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    digits.parse().ok()
}

fn round_file(number: u32) -> String {
    format!("round-{number}.json")
}

pub(in crate::verify) fn load_optional<T: DeserializeOwned>(
    work_dir: &Path,
    stage_id: &str,
    file: &str,
) -> Result<Option<T>> {
    let root = canonical_work_dir(work_dir, stage_id)?;
    let relative = Path::new(REVIEWS_DIR).join(stage_id).join(file);
    let bytes = match read_bounded(&root, &relative, MAX_RECORD_BYTES) {
        Ok(bytes) => bytes,
        Err(error) if is_not_found(&error) => return Ok(None),
        Err(error) => return Err(error),
    };
    serde_json::from_slice(&bytes)
        .map(Some)
        .with_context(|| format!("invalid {file} for stage '{stage_id}'"))
}

pub(in crate::verify) fn check_version(version: u32, record: &str) -> Result<()> {
    if version != RECORD_VERSION {
        bail!("{record} has unsupported version {version}");
    }
    Ok(())
}

pub(in crate::verify) fn stage_dir(work_dir: &Path, stage_id: &str) -> PathBuf {
    work_dir.join(REVIEWS_DIR).join(stage_id)
}
