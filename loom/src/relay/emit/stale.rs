//! Classifies the `.req` files in a scratch directory for the quota check.
//!
//! A control ticket is never swept (`commands/hook/relay/sweep.rs`), so one
//! whose Bash call lost its relay line stays on disk for good. Counting it
//! against the unconsumed-ticket limit would let every missed rerun shrink the
//! session's quota permanently. Nothing here deletes a ticket: age cannot tell
//! a stranded ticket from one whose call is still running, and a stranded
//! ticket keeps `loom request status` answering pending relay.

use crate::fs::safe_read::{open_regular_no_follow, read_open_bounded};
use crate::relay::{Ticket, MAX_TICKET_BYTES};
use anyhow::{Context, Result};
use std::ffi::OsStr;
use std::fs;
use std::path::Path;
use std::time::{Duration, SystemTime};

/// A control ticket older than this stops counting against the 32-ticket
/// limit. The floor only decides which tickets stop counting, so a wrong guess
/// costs quota, never a request. Revisit if tickets gain a writer identity.
pub(super) const STALE_AFTER: Duration = Duration::from_secs(60);

/// At most this many stale control ids are named in the scratch-limit refusal.
const MAX_NAMED_IDS: usize = 10;

/// How the `.req` files in one scratch directory count.
#[derive(Debug, Default, PartialEq, Eq)]
pub(super) struct TicketCounts {
    /// Tickets that count against `MAX_UNCONSUMED_TICKETS`: every swept kind,
    /// every fresh control ticket, and every file that cannot be classified.
    pub live: usize,
    /// Control tickets older than [`STALE_AFTER`].
    pub stale_control: usize,
    /// Up to ten ids of the stale control tickets, in directory order.
    pub stale_ids: Vec<String>,
}

/// Count the `.req` files in `scratch_dir` as of `now`.
pub(super) fn classify(scratch_dir: &Path, now: SystemTime) -> Result<TicketCounts> {
    let entries = fs::read_dir(scratch_dir)
        .with_context(|| format!("failed to read scratch directory {}", scratch_dir.display()))?;
    let mut counts = TicketCounts::default();
    for entry in entries {
        let entry = entry
            .with_context(|| format!("failed to read an entry in {}", scratch_dir.display()))?;
        let name = entry.file_name();
        if Path::new(&name).extension() != Some(OsStr::new("req")) {
            continue;
        }
        // A name that is not UTF-8 cannot be opened or decoded here, so it
        // counts as live like any other file that cannot be classified.
        match name
            .to_str()
            .and_then(|name| stale_control_id(scratch_dir, name, now))
        {
            Some(id) => {
                counts.stale_control += 1;
                if counts.stale_ids.len() < MAX_NAMED_IDS {
                    counts.stale_ids.push(id);
                }
            }
            None => counts.live += 1,
        }
    }
    Ok(counts)
}

/// The ticket id when `name` is a decodable control ticket older than
/// [`STALE_AFTER`]. `None` for anything else, including a symlink, a
/// non-regular file, an oversize file or one that does not decode: those
/// count as live, so a planted file never frees quota.
fn stale_control_id(scratch_dir: &Path, name: &str, now: SystemTime) -> Option<String> {
    let file = open_regular_no_follow(scratch_dir, name, libc::O_RDONLY)
        .ok()
        .flatten()?;
    let modified = file.metadata().ok()?.modified().ok()?;
    let age = now.duration_since(modified).unwrap_or_default();
    if age <= STALE_AFTER {
        return None;
    }
    let bytes = read_open_bounded(file, Path::new(name), MAX_TICKET_BYTES).ok()?;
    let ticket = Ticket::decode(&bytes).ok()?;
    ticket.kind.is_control().then_some(ticket.id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::ffi::OsStrExt;

    #[test]
    fn a_req_file_with_a_non_utf8_name_counts_as_live() {
        let dir = tempfile::tempdir().unwrap();
        let name = OsStr::from_bytes(b"ticket-\xff.req");
        fs::write(dir.path().join(name), b"not a ticket").unwrap();

        let counts = classify(dir.path(), SystemTime::now() + STALE_AFTER * 2).unwrap();

        assert_eq!(counts.live, 1);
        assert_eq!(counts.stale_control, 0);
    }
}
