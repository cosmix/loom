//! Guards the CLI's per-session ticket quota
//! ([`crate::relay::MAX_UNCONSUMED_TICKETS`] and
//! [`crate::relay::MAX_SCRATCH_TICKETS`]).

use super::stale::{self, TicketCounts};
use crate::relay::{MAX_SCRATCH_TICKETS, MAX_UNCONSUMED_TICKETS};
use anyhow::{bail, Result};
use std::path::Path;
use std::time::SystemTime;

/// Refuse a new ticket when `scratch_dir` holds too many.
///
/// Two limits apply. `MAX_UNCONSUMED_TICKETS` counts the live tickets: swept
/// kinds, fresh control tickets and anything unclassifiable. Control tickets
/// older than a minute are stranded (no later call relays them), so they do
/// not count there. `MAX_SCRATCH_TICKETS` counts every ticket and keeps the
/// directory bounded. No ticket is deleted by either check.
pub(super) fn require_headroom(scratch_dir: &Path, now: SystemTime) -> Result<()> {
    let TicketCounts {
        live,
        stale_control,
        stale_ids,
    } = stale::classify(scratch_dir, now)?;
    if live >= MAX_UNCONSUMED_TICKETS {
        bail!(
            "this session already has {live} unconsumed relay tickets (limit \
             {MAX_UNCONSUMED_TICKETS}). Memory tickets: run `loom memory note --help` alone, in \
             the foreground, with its output unfiltered, to relay them. Check tickets: rerun \
             `loom check` the same way."
        );
    }
    let total = live + stale_control;
    if total >= MAX_SCRATCH_TICKETS {
        bail!(
            "the scratch directory holds {total} relay tickets (limit {MAX_SCRATCH_TICKETS}), \
             {stale_control} of them unrelayed control tickets older than {} s: {}. For each, run \
             `loom request status <id>`; when it reports pending relay and you no longer need \
             that request, delete `$LOOM_SCRATCH_DIR/<id>.req`, otherwise rerun its command \
             alone in the foreground.",
            stale::STALE_AFTER.as_secs(),
            stale_ids.join(", ")
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    fn require(dir: &TempDir) -> Result<()> {
        require_headroom(dir.path(), SystemTime::now())
    }

    #[test]
    fn allows_up_to_the_limit_minus_one() {
        let dir = TempDir::new().unwrap();
        for n in 0..MAX_UNCONSUMED_TICKETS - 1 {
            fs::write(dir.path().join(format!("{n}.req")), b"").unwrap();
        }
        require(&dir).unwrap();
    }

    #[test]
    fn refuses_at_the_limit() {
        let dir = TempDir::new().unwrap();
        for n in 0..MAX_UNCONSUMED_TICKETS {
            fs::write(dir.path().join(format!("{n}.req")), b"").unwrap();
        }
        let err = require(&dir).unwrap_err().to_string();
        assert!(err.contains("loom memory note --help"));
        assert!(err.contains("rerun `loom check`"));
        assert!(!err.contains("wait for the relay hook"));
    }

    #[test]
    fn ignores_non_req_files() {
        let dir = TempDir::new().unwrap();
        fs::write(dir.path().join("not-a-ticket.tmp"), b"").unwrap();
        require(&dir).unwrap();
    }
}
