//! Classifies the `.req` files in a scratch directory for the quota check.
//!
//! A control ticket is never swept (`commands/hook/relay/sweep.rs`), so one
//! whose Bash call lost its relay line stays on disk for good. Counting it
//! against the unconsumed-ticket limit would let every missed rerun shrink the
//! session's quota permanently. Nothing in the CLI deletes a ticket: age
//! cannot tell a stranded ticket from one whose call is still running, and a
//! stranded ticket keeps `loom request status` answering pending relay. A file
//! that does not read as a ticket is never relayed either; the quota refusal
//! names it so the lead can delete it.

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

/// At most this many ids are named in a refusal, per kind of file.
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
    /// Files, counted in `live`, that do not open as a plain file, exceed the
    /// ticket cap, do not decode as a ticket or have a non-UTF-8 name. No
    /// relay ever picks them up.
    pub undecodable: usize,
    /// Up to ten ids of the undecodable files, in directory order.
    pub undecodable_ids: Vec<String>,
}

/// How one `.req` file counts.
enum Class {
    /// A decodable control ticket older than [`STALE_AFTER`]; carries its id.
    StaleControl(String),
    /// A decodable ticket that still counts against the live limit.
    Live,
    /// Anything that does not read as a ticket; also counts as live, so a
    /// planted file never frees quota.
    Undecodable,
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
        match classify_one(scratch_dir, &name, now) {
            Class::StaleControl(id) => {
                counts.stale_control += 1;
                if counts.stale_ids.len() < MAX_NAMED_IDS {
                    counts.stale_ids.push(id);
                }
            }
            Class::Live => counts.live += 1,
            Class::Undecodable => {
                counts.live += 1;
                counts.undecodable += 1;
                if counts.undecodable_ids.len() < MAX_NAMED_IDS {
                    let name = name.to_string_lossy();
                    let id = name.strip_suffix(".req").unwrap_or(&name);
                    counts.undecodable_ids.push(id.to_string());
                }
            }
        }
    }
    Ok(counts)
}

/// Decode first, then apply the age rule. A name that is not UTF-8, a
/// symlink, a non-regular file, an oversize file or one that does not decode
/// is undecodable; a control ticket older than [`STALE_AFTER`] is stale;
/// every other ticket is live. Every file is opened without following a link
/// and read within the ticket cap.
fn classify_one(scratch_dir: &Path, name: &OsStr, now: SystemTime) -> Class {
    let Some(name) = name.to_str() else {
        return Class::Undecodable;
    };
    let Some((ticket, modified)) = read_ticket(scratch_dir, name) else {
        return Class::Undecodable;
    };
    let age = now.duration_since(modified).unwrap_or_default();
    if ticket.kind.is_control() && age > STALE_AFTER {
        Class::StaleControl(ticket.id)
    } else {
        Class::Live
    }
}

fn read_ticket(scratch_dir: &Path, name: &str) -> Option<(Ticket, SystemTime)> {
    let file = open_regular_no_follow(scratch_dir, name, libc::O_RDONLY)
        .ok()
        .flatten()?;
    let modified = file.metadata().ok()?.modified().ok()?;
    let bytes = read_open_bounded(file, Path::new(name), MAX_TICKET_BYTES).ok()?;
    Some((Ticket::decode(&bytes).ok()?, modified))
}

// APFS rejects file names that are not valid UTF-8 (EILSEQ), so the only
// fixture here cannot exist on macOS.
#[cfg(all(test, not(target_os = "macos")))]
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
        assert_eq!(counts.undecodable, 1);
    }
}
