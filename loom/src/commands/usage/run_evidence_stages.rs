//! The stage-derived fields of a run's evidence: retries, fixes, the critical
//! path and the open review findings of the stages that ran in the window.

use std::collections::HashMap;
use std::path::Path;

use anyhow::{bail, Context, Result};

use super::comparison_schema::{FindingEvidence, FindingSeverity};
use super::time_range::TimeRange;
use crate::models::stage::{Stage, StageStatus};

pub(super) struct StageEvidence {
    pub(super) retry_count: u32,
    pub(super) fix_count: u32,
    pub(super) critical_path_ms: Option<u64>,
    pub(super) unresolved_findings: Vec<FindingEvidence>,
}

/// Evidence of the run whose stages started inside `range`. Stage records are
/// mutable, so the result is the state at the time of the call; the producer
/// never reconstructs a past run.
pub(super) fn stage_evidence(work_dir: &Path, range: &TimeRange) -> Result<StageEvidence> {
    let mut all = crate::verify::transitions::list_all_stages(work_dir)
        .context("reading the stage records")?;
    // Directory order is arbitrary; sort first so the findings list and the
    // stage a straddle abort names are the same on every call over one state.
    all.sort_by(|a, b| a.id.cmp(&b.id));
    let mut members = Vec::new();
    for stage in all {
        if is_member(&stage, range)? {
            members.push(stage);
        }
    }
    let mut unresolved_findings = Vec::new();
    for stage in &members {
        unresolved_findings.extend(open_findings(work_dir, stage)?);
    }
    Ok(StageEvidence {
        retry_count: saturating_sum(members.iter().map(|stage| stage.retry_count)),
        fix_count: saturating_sum(members.iter().map(|stage| stage.fix_attempts)),
        critical_path_ms: critical_path_ms(&members),
        unresolved_findings,
    })
}

/// Whether the stage belongs to the run. A stage that began before `--since`
/// and was still going at it, or that ended after `--until`, cannot be split
/// between runs, so it aborts the producer.
fn is_member(stage: &Stage, range: &TimeRange) -> Result<bool> {
    let Some(started) = stage.started_at else {
        return Ok(false);
    };
    if range.until.is_some_and(|until| started > until) {
        return Ok(false);
    }
    if started < range.since {
        if stage.completed_at.is_some_and(|done| done >= range.since) {
            bail!(
                "stage {} straddles --since: it started before the window and completed inside it",
                stage.id
            );
        }
        return Ok(false);
    }
    if let (Some(done), Some(until)) = (stage.completed_at, range.until) {
        if done > until {
            bail!(
                "stage {} straddles --until: it completed after the window",
                stage.id
            );
        }
    }
    Ok(true)
}

fn saturating_sum(values: impl Iterator<Item = u32>) -> u32 {
    values.fold(0u32, u32::saturating_add)
}

fn open_findings(work_dir: &Path, stage: &Stage) -> Result<Vec<FindingEvidence>> {
    let open = crate::verify::review::store::open_findings(work_dir, &stage.id)
        .with_context(|| format!("reading the review findings of stage {}", stage.id))?;
    Ok(open
        .into_iter()
        .map(|finding| FindingEvidence {
            // A carried id `<origin>/F-r-k` holds a `/`, which `valid_id` refuses.
            finding_id: format!("{}:{}", stage.id, finding.id).replace('/', ":"),
            severity: map_severity(&finding.finding.severity),
        })
        .collect())
}

fn map_severity(severity: &str) -> FindingSeverity {
    match severity {
        "critical" => FindingSeverity::Critical,
        "major" => FindingSeverity::High,
        "minor" => FindingSeverity::Medium,
        _ => FindingSeverity::Low,
    }
}

fn duration_ms(stage: &Stage) -> Option<u64> {
    if stage.status != StageStatus::Completed {
        return None;
    }
    let millis = (stage.completed_at? - stage.started_at?).num_milliseconds();
    u64::try_from(millis).ok()
}

/// The longest dependency chain of stage durations; waiting between stages is
/// not counted. `None` when any member is unfinished, lacks a timestamp, has a
/// negative duration, names a dependency outside the members, or the
/// dependencies form a cycle.
fn critical_path_ms(members: &[Stage]) -> Option<u64> {
    let index: HashMap<&str, &Stage> = members.iter().map(|s| (s.id.as_str(), s)).collect();
    let mut memo: HashMap<&str, u64> = HashMap::new();
    let mut longest = None;
    for stage in members {
        let path = path_to(stage, &index, &mut memo, &mut Vec::new())?;
        longest = Some(longest.map_or(path, |best: u64| best.max(path)));
    }
    longest
}

fn path_to<'a>(
    stage: &'a Stage,
    index: &HashMap<&'a str, &'a Stage>,
    memo: &mut HashMap<&'a str, u64>,
    visiting: &mut Vec<&'a str>,
) -> Option<u64> {
    if let Some(known) = memo.get(stage.id.as_str()) {
        return Some(*known);
    }
    if visiting.contains(&stage.id.as_str()) {
        return None;
    }
    visiting.push(stage.id.as_str());
    let mut deepest = 0u64;
    for dependency in &stage.dependencies {
        let dependency = index.get(dependency.as_str())?;
        deepest = deepest.max(path_to(dependency, index, memo, visiting)?);
    }
    visiting.pop();
    let total = duration_ms(stage)?.saturating_add(deepest);
    memo.insert(stage.id.as_str(), total);
    Some(total)
}
