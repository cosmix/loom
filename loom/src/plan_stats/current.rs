//! The current copy: `<work dir>/plan-stats.json`, what the status feed reads.
//!
//! The copy carries the run's plan id and is only trusted while that id is the initialized
//! run's and every persisted stage is terminal. A new initialization, a reset stage or a
//! planted file therefore never keeps the overview on screen. `loom init` and `loom clean
//! --state` remove the file; the archive under the loom home is not touched.

use std::fs::File;
use std::io;
use std::os::unix::io::OwnedFd;
use std::path::Path;

use anyhow::{Context, Result};

use super::identity;
use super::record::{PlanStatsRecord, MAX_RECORD_BYTES};
use super::sanitize::{has_known_names, sanitize};
use crate::fs::cache_io;
use crate::fs::safe_read::read_bounded_at;
use crate::models::stage::StageStatus;
use crate::verify::transitions::list_all_stages;

/// File name of the current copy inside the work directory.
pub const CURRENT_FILE: &str = "plan-stats.json";

/// Install `record` as the current copy, replacing an earlier one atomically.
pub fn write_current(work_dir: &Path, record: &PlanStatsRecord) -> Result<()> {
    let bytes = record.to_json()?;
    let text = String::from_utf8(bytes).context("the plan stats record is not UTF-8")?;
    cache_io::atomic_write(&work_dir.join(CURRENT_FILE), &text)
        .with_context(|| format!("cannot write {CURRENT_FILE}"))
}

/// Delete the current copy; an absent one is fine.
pub fn remove_current(work_dir: &Path) -> Result<()> {
    match std::fs::remove_file(work_dir.join(CURRENT_FILE)) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error).with_context(|| format!("cannot remove {CURRENT_FILE}")),
    }
}

/// The current copy, neutralized for display, when it is the initialized run's and the plan
/// is finished (every stage completed or skipped). `None` for anything else: absent,
/// unreadable, over the bound, wrong schema, another run's, a stage type, status or lane the
/// build never writes, or a plan with a stage in play.
pub fn read_current(work_dir: &Path) -> Option<PlanStatsRecord> {
    let mut record = read_file(work_dir)?;
    let run = identity::read(work_dir).ok().flatten()?;
    if record.plan_id != run.plan_id() || !has_known_names(&record) || !plan_is_finished(work_dir) {
        return None;
    }
    sanitize(&mut record);
    Some(record)
}

/// The directory is opened through any link (a worktree's `.loom/work` is one); the file
/// is opened no-follow and bounded.
fn read_file(work_dir: &Path) -> Option<PlanStatsRecord> {
    let dir: OwnedFd = File::open(work_dir).ok()?.into();
    let bytes = read_bounded_at(&dir, Path::new(CURRENT_FILE), MAX_RECORD_BYTES).ok()?;
    PlanStatsRecord::from_json(&bytes).ok()
}

fn plan_is_finished(work_dir: &Path) -> bool {
    list_all_stages(work_dir).is_ok_and(|stages| {
        !stages.is_empty()
            && stages
                .iter()
                .all(|stage| matches!(stage.status, StageStatus::Completed | StageStatus::Skipped))
    })
}
