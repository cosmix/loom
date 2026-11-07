//! A static refusal returned to the live contract writer.
//!
//! A freeze job refused only by the static gate, or interrupted or stopped by
//! the daemon, is something the writer can fix or rerun itself: its waiting
//! freeze command reads the refusal. The record keeps how many such refusals
//! of the session were returned (`DaemonRefusal::returned`); the third is
//! recorded with `returned = 0`, which the tick files as a `freeze-refusal`
//! dispute at once, and a returned one is filed once the writer stays quiet.

use anyhow::{ensure, Context, Result};
use chrono::Utc;
use std::path::Path;

use super::{
    daemon_refusal, refusal_dir, write_refusal, DaemonRefusal, JobRefusalOrigin, RefusalSource,
};
use crate::fs::locking::locked_dir_update;

/// The static refusals one writer session gets back before the next is filed.
pub const STATIC_RETURN_LIMIT: u32 = 3;

/// What [`record_static_refusal`] recorded.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StaticRefusal {
    /// Returned to the writer: the `count`-th of its session, below the limit.
    Returned { count: u32 },
    /// Recorded for the tick to file as a dispute.
    Filed,
    /// A newer job of the session already recorded its refusal.
    Superseded,
}

/// Record the static refusal `problems` of the host job `origin` for
/// `session_id`, counting it against the session's [`STATIC_RETURN_LIMIT`].
/// Ordered as `record_job_refusal` orders job refusals: the same job again
/// answers the record's state without counting twice, and an older job than
/// the one on record is [`StaticRefusal::Superseded`]. A record of another
/// session does not count.
pub fn record_static_refusal(
    work_dir: &Path,
    stage_id: &str,
    session_id: &str,
    origin: JobRefusalOrigin,
    problems: &[String],
) -> Result<StaticRefusal> {
    let dir = refusal_dir(work_dir, stage_id)?;
    locked_dir_update(&dir, || {
        let existing = daemon_refusal(work_dir, stage_id)?
            .filter(|existing| existing.session_id == session_id);
        let mut prior = 0;
        if let Some(existing) = existing {
            let recorded = existing
                .job_origin
                .context("unversioned refusal has unknown job order")?;
            if recorded == origin {
                return Ok(state_of(existing.returned));
            }
            ensure!(
                recorded.submission_order != origin.submission_order,
                "distinct host jobs share a refusal order"
            );
            if recorded.submission_order > origin.submission_order {
                return Ok(StaticRefusal::Superseded);
            }
            prior = existing.returned;
        }
        let count = prior.saturating_add(1);
        let returned = if count < STATIC_RETURN_LIMIT {
            count
        } else {
            0
        };
        let record = DaemonRefusal {
            session_id: session_id.to_string(),
            problems: problems.to_vec(),
            at: Utc::now(),
            source: RefusalSource::Daemon,
            job_origin: Some(origin),
            declined: None,
            returned,
        };
        write_refusal(&dir, &record)?;
        Ok(state_of(returned))
    })
    .with_context(|| format!("failed to record the static refusal of stage '{stage_id}'"))
}

fn state_of(returned: u32) -> StaticRefusal {
    if returned > 0 {
        StaticRefusal::Returned { count: returned }
    } else {
        StaticRefusal::Filed
    }
}

#[cfg(test)]
#[path = "static_return_tests.rs"]
mod tests;
