//! Finishing a plan: archive its record once, then expose the current copy.

use std::path::Path;

use anyhow::{Context, Result};

use super::build::build;
use super::current::write_current;
use super::identity;
use super::publish::{archive_path, archived, loom_home, publish, Published};

/// Archive the finished plan of `work_dir` and install the current copy; called after
/// the scheduler completed every stage.
///
/// The archive is written first and wins: when the run already has one (a retry after an
/// interruption between the two writes, or a concurrent publisher) it is used verbatim,
/// never rebuilt, so the current copy always equals the archive. Calling it again is
/// harmless and answers [`Published::AlreadyPublished`].
pub fn finish_plan(work_dir: &Path) -> Result<Published> {
    let run = identity::ensure(work_dir)?;
    let plan_id = run.plan_id();
    let home = loom_home()?;
    let (record, published) = match archived(&home, &plan_id)? {
        Some(record) => {
            let path = archive_path(&home, &plan_id);
            (record, Published::AlreadyPublished(path))
        }
        None => {
            let built = build(work_dir)?;
            let published = publish(&built, &home)?;
            match published {
                Published::Written(_) => (built, published),
                Published::AlreadyPublished(_) => {
                    let winner = archived(&home, &plan_id)?
                        .context("the archive vanished after another publisher wrote it")?;
                    (winner, published)
                }
            }
        }
    };
    write_current(work_dir, &record)?;
    Ok(published)
}
