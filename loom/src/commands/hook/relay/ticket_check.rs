//! The ticket files a session leaves in its scratch directory: where they
//! sit, when one counts, and its removal once relayed.
//!
//! A ticket counts only when it is exactly the file its relay line describes
//! — or, for a ticket no line describes, when it is a well-formed ticket file
//! of this session's own.

use std::fs::File;
use std::io::Read;
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};

use crate::fs::safe_read::open_regular_no_follow;
use crate::relay::{sha256_hex, RelayLine, Ticket, MAX_TICKET_BYTES};

/// `<id>.req`, the name the CLI writes a ticket under.
fn ticket_file_name(id: &str) -> String {
    format!("{id}.req")
}

/// The ticket file one relay line names.
pub(super) fn ticket_path(scratch_dir: &Path, line: &RelayLine) -> PathBuf {
    ticket_file(scratch_dir, &line.id)
}

/// `<scratch_dir>/<id>.req`, where the CLI leaves a ticket.
pub(super) fn ticket_file(scratch_dir: &Path, id: &str) -> PathBuf {
    scratch_dir.join(ticket_file_name(id))
}

/// Remove a relayed ticket. A failure is reported, not fatal: relaying the
/// same id again is already a no-op. A missing ticket counts as removed: two
/// hook invocations can race to relay the same id, and whichever loses the
/// race finds it already gone.
pub(super) fn consume(ticket: &Path) -> String {
    match std::fs::remove_file(ticket) {
        Ok(()) => String::new(),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => String::new(),
        Err(error) => format!(
            " (its ticket {} could not be removed: {error})",
            ticket.display()
        ),
    }
}

/// Open `<scratch_dir>/<id>.req` without following a link, then require a
/// single-link regular file owned by `uid`, exactly `line.bytes` long and
/// within the ticket cap, whose SHA-256 and `v`/`id`/`kind` match the line.
pub(super) fn read_verified(scratch_dir: &Path, line: &RelayLine, uid: u32) -> Result<Ticket> {
    let expected = u64::from(line.bytes);
    if expected > MAX_TICKET_BYTES as u64 {
        bail!("the line claims {expected} bytes, above the {MAX_TICKET_BYTES}-byte ticket cap");
    }
    let (file, len) = open_owned(scratch_dir, &line.id, uid)?;
    if len != expected {
        bail!("the ticket is {len} bytes, the line says {expected}");
    }
    let bytes = read_whole(file, expected)?;
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

/// Read `<scratch_dir>/<id>.req` for a ticket no line in this call describes:
/// the same file shape [`read_verified`] requires — a single-link regular
/// file owned by `uid`, within the ticket cap — decoding to a request that
/// names itself `id`.
///
/// Dropping the announced size and hash takes no guarantee away. Both are
/// written by the same session that wrote the ticket, so they never bound it
/// to anything that session could not produce on its own; the documented
/// manual recovery rebuilds a line from the very file it describes. What
/// authorises a relay is the session proof plus the kinds derived from the
/// Bash command's own text, and a swept ticket passes through both.
pub(super) fn read_unlisted(scratch_dir: &Path, id: &str, uid: u32) -> Result<Ticket> {
    let (file, len) = open_owned(scratch_dir, id, uid)?;
    if len > MAX_TICKET_BYTES as u64 {
        bail!("the ticket is {len} bytes, above the {MAX_TICKET_BYTES}-byte ticket cap");
    }
    let bytes = read_whole(file, len)?;
    let ticket = Ticket::decode(&bytes).context("the ticket is not a valid request")?;
    if ticket.id != id {
        bail!(
            "the ticket names {} {}, its file is named {id}.req",
            ticket.kind,
            ticket.id
        );
    }
    Ok(ticket)
}

/// The open ticket file and its length: a regular file with a single link,
/// reached without following a symlink at any component, owned by `uid`.
fn open_owned(scratch_dir: &Path, id: &str, uid: u32) -> Result<(File, u64)> {
    let file = open_regular_no_follow(scratch_dir, &ticket_file_name(id), libc::O_RDONLY)
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
    Ok((file, metadata.len()))
}

/// Read exactly `len` bytes, refusing a file that changed size meanwhile.
fn read_whole(file: File, len: u64) -> Result<Vec<u8>> {
    let mut bytes = Vec::new();
    file.take(len + 1)
        .read_to_end(&mut bytes)
        .context("the ticket could not be read")?;
    if bytes.len() as u64 != len {
        bail!("the ticket changed size while it was read");
    }
    Ok(bytes)
}

#[cfg(test)]
#[path = "tests_ticket.rs"]
mod tests;
