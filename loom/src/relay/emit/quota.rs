//! Guards the CLI's per-session ticket quota
//! ([`crate::relay::MAX_UNCONSUMED_TICKETS`]).

use crate::relay::MAX_UNCONSUMED_TICKETS;
use anyhow::{bail, Context, Result};
use std::fs;
use std::path::Path;

/// Refuse when `scratch_dir` already holds `MAX_UNCONSUMED_TICKETS` or more
/// `.req` files the relay hook has not yet picked up.
pub(super) fn require_headroom(scratch_dir: &Path) -> Result<()> {
    let count = count_unconsumed(scratch_dir)?;
    if count >= MAX_UNCONSUMED_TICKETS {
        bail!(
            "this session already has {count} unconsumed relay tickets (limit \
             {MAX_UNCONSUMED_TICKETS}); run `loom memory note --help` in the foreground, with its \
             output unfiltered, to relay the backlog — a ticket written under a redirect, a \
             line-dropping pipe, a script file or a background call stays queued until then"
        );
    }
    Ok(())
}

fn count_unconsumed(scratch_dir: &Path) -> Result<usize> {
    let entries = fs::read_dir(scratch_dir)
        .with_context(|| format!("failed to read scratch directory {}", scratch_dir.display()))?;
    let mut count = 0usize;
    for entry in entries {
        let entry = entry
            .with_context(|| format!("failed to read an entry in {}", scratch_dir.display()))?;
        if entry.path().extension().and_then(|ext| ext.to_str()) == Some("req") {
            count += 1;
        }
    }
    Ok(count)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn allows_up_to_the_limit_minus_one() {
        let dir = TempDir::new().unwrap();
        for n in 0..MAX_UNCONSUMED_TICKETS - 1 {
            fs::write(dir.path().join(format!("{n}.req")), b"").unwrap();
        }
        require_headroom(dir.path()).unwrap();
    }

    #[test]
    fn refuses_at_the_limit() {
        let dir = TempDir::new().unwrap();
        for n in 0..MAX_UNCONSUMED_TICKETS {
            fs::write(dir.path().join(format!("{n}.req")), b"").unwrap();
        }
        let err = require_headroom(dir.path()).unwrap_err().to_string();
        assert!(err.contains("loom memory note --help"));
        assert!(!err.contains("wait for the relay hook"));
    }

    #[test]
    fn ignores_non_req_files() {
        let dir = TempDir::new().unwrap();
        fs::write(dir.path().join("not-a-ticket.tmp"), b"").unwrap();
        require_headroom(dir.path()).unwrap();
    }
}
