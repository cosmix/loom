//! A ticket counts only when it is exactly the file its relay line describes,
//! sitting in this session's own scratch directory.

use std::io::Read;
use std::os::unix::fs::MetadataExt;
use std::path::Path;

use anyhow::{bail, Context, Result};

use crate::fs::safe_read::open_regular_no_follow;
use crate::relay::{sha256_hex, RelayLine, Ticket, MAX_TICKET_BYTES};

/// `<id>.req`, the name the CLI writes a ticket under.
pub(super) fn ticket_file_name(id: &str) -> String {
    format!("{id}.req")
}

/// Open `<scratch_dir>/<id>.req` without following a link, then require a
/// single-link regular file owned by `uid`, exactly `line.bytes` long and
/// within the ticket cap, whose SHA-256 and `v`/`id`/`kind` match the line.
pub(super) fn read_verified(scratch_dir: &Path, line: &RelayLine, uid: u32) -> Result<Ticket> {
    let expected = u64::from(line.bytes);
    if expected > MAX_TICKET_BYTES as u64 {
        bail!("the line claims {expected} bytes, above the {MAX_TICKET_BYTES}-byte ticket cap");
    }
    let file = open_regular_no_follow(scratch_dir, &ticket_file_name(&line.id), libc::O_RDONLY)
        .context("the ticket is not a plain single-link file")?
        .context("the ticket disappeared before it could be read")?;
    let metadata = file
        .metadata()
        .context("the ticket could not be inspected")?;
    if metadata.uid() != uid {
        bail!(
            "the ticket is owned by uid {}, expected {uid}",
            metadata.uid()
        );
    }
    if metadata.len() != expected {
        bail!(
            "the ticket is {} bytes, the line says {expected}",
            metadata.len()
        );
    }
    let mut bytes = Vec::new();
    file.take(expected + 1)
        .read_to_end(&mut bytes)
        .context("the ticket could not be read")?;
    if bytes.len() as u64 != expected {
        bail!("the ticket changed size while it was read");
    }
    if sha256_hex(&bytes) != line.sha256 {
        bail!("the ticket's SHA-256 does not match the line");
    }
    let ticket = Ticket::decode(&bytes).context("the ticket is not a valid request")?;
    if ticket.id != line.id || ticket.kind != line.kind {
        bail!(
            "the ticket names {} {}, the line names {} {}",
            ticket.kind,
            ticket.id,
            line.kind,
            line.id
        );
    }
    Ok(ticket)
}

#[cfg(test)]
#[path = "tests_ticket.rs"]
mod tests;
