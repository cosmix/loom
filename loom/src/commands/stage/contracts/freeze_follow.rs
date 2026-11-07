//! Following the daemon's application of a finished freeze job.
//!
//! A freeze job that passed or failed is decided only once the daemon applies
//! it (`host_verify::apply`, which writes the job's `applied` marker last), and
//! only the application tells the writer whether a refusal was returned to it
//! or filed for an adjudicator. The wait keeps the request state while the
//! application is pending, so a call that times out here is followed by a rerun
//! of the same command. The follow runs in the writer's sandbox: it reads the
//! job directory under the state root, as `loom check` does.

use std::io::Write;
use std::time::{Duration, Instant};

use anyhow::Result;

use crate::host_verify::apply::APPLIED_MARKER;
use crate::host_verify::wait::{self, WaitSite};
use crate::host_verify::{store, JobKind, JobRecord};

const POLL: Duration = Duration::from_secs(1);
/// The marker note of an accepted freeze.
const FROZE: &str = "froze the contracts";
/// The start of every refusal note; any other non-freeze note is an ignored run.
const REFUSED: &str = "refused the contract run";
/// The marker note of a static refusal handed back to the writer; the word
/// `returned` is followed by `(<n> of 3)`.
const RETURNED: &str = "refused the contract run: returned to the contract writer";

/// Wait up to `budget` for the daemon to apply the finished freeze job
/// `record`, print how it did and return the exit code: 0 for a freeze, 1 for
/// a refusal returned to the writer (who fixes and freezes again) and 1 for a
/// refusal filed or an application that ignored the run. Without an applied
/// marker at the end of the budget it prints the `pending` heading, keeps the
/// request state so the next call follows the same job, and returns 0.
pub(super) fn follow_application(
    site: &WaitSite<'_>,
    record: &JobRecord,
    budget: Duration,
    out: &mut dyn Write,
) -> Result<i32> {
    let end = Instant::now() + budget;
    loop {
        let marker =
            store::load_job_file(site.work_dir, site.stage_id, &record.id, APPLIED_MARKER)?;
        if let Some(marker) = marker {
            wait::release(site, record)?;
            let note = String::from_utf8_lossy(&marker);
            return report(&note, out);
        }
        if Instant::now() >= end {
            let heading = wait::heading(JobKind::Freeze, "pending", &record.head);
            writeln!(
                out,
                "{heading}: the run finished and the daemon has not applied it yet. Run the same \
                 command again to keep waiting."
            )?;
            out.flush()?;
            return Ok(0);
        }
        std::thread::sleep(POLL.min(end.saturating_duration_since(Instant::now())));
    }
}

/// The marker's note as one line, then the line that tells the writer what to
/// do. A returned refusal adds nothing: the failing output above it says what
/// to fix. A note that is no refusal at all came from an ignored run.
fn report(note: &str, out: &mut dyn Write) -> Result<i32> {
    let line = note.split_whitespace().collect::<Vec<_>>().join(" ");
    writeln!(out, "{line}")?;
    let (code, advice) = if line.starts_with(FROZE) {
        (0, Some("Stop now; loom ends this session."))
    } else if line.starts_with(RETURNED) {
        (1, None)
    } else if line.starts_with(REFUSED) {
        let filed = "This refusal is filed for an adjudicator; stop and let loom end this session.";
        (1, Some(filed))
    } else {
        let ignored = "The daemon ignored this run and filed nothing; if loom does not end this \
                       session, run the freeze again.";
        (1, Some(ignored))
    };
    if let Some(advice) = advice {
        writeln!(out, "{advice}")?;
    }
    out.flush()?;
    Ok(code)
}

#[cfg(test)]
#[path = "freeze_follow_tests.rs"]
mod tests;
