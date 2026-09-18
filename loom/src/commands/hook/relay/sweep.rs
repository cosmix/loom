//! Recovering tickets whose relay line never reached the hook.
//!
//! A ticket is relayed when the Bash call that wrote it printed the ticket's
//! line into output this hook can read. Output goes missing in ordinary ways:
//! a loop writing more tickets in one call than
//! [`crate::relay::MAX_LINES_PER_CALL`] lines are extracted from it, stdout
//! redirected or piped through `tail`, a command run from a script or in the
//! background. The ticket then stays in the scratch directory, counting
//! against the CLI's unconsumed-ticket quota until every further write from
//! that session fails.
//!
//! So a relay call relays its own lines and then sweeps the rest: every
//! `.req` file no line of this call named, whose kind the Bash command's own
//! text allows and which is not a control kind.

use std::ffi::OsStr;
use std::path::Path;
use std::time::SystemTime;

use super::{ticket_check, Request};
use crate::fs::inbox::validate_request_id;
use crate::relay::{RelayLine, RequestKind};

/// A `.req` file this call saw no line for: a ticket an earlier call left
/// behind. Its kind is unknown until the file decodes, so a refusal can only
/// name it by id.
pub(super) struct Candidate {
    id: String,
    modified: SystemTime,
}

impl Candidate {
    /// What a refusal calls it, beside the `<kind> <id>` labels lines carry.
    pub(super) fn label(&self) -> String {
        format!("ticket {}", &self.id[..8])
    }
}

/// Where the request a reply reports came from. This changes the wording
/// only: a recovered ticket is relayed on exactly the terms a lined one is.
pub(super) enum Source {
    /// This Bash call's own output carried the ticket's line.
    Line,
    /// No line named it; an earlier call left it in the scratch directory.
    Recovered,
}

impl Source {
    /// The parenthetical a reply puts after `received <label>`.
    pub(super) fn note(&self) -> &'static str {
        match self {
            Source::Line => "(the daemon applies it within ~5s)",
            Source::Recovered => {
                "(recovered: an earlier Bash call wrote it and its output never reached the \
                 relay hook; the daemon applies it within ~5s)"
            }
        }
    }
}

/// One recorded ticket's reply. `Full` also says the session inbox is at
/// capacity, so no further ticket can be written in this call either.
pub(super) enum Recorded {
    Reply(String),
    Full(String),
}

/// Every leftover ticket in `scratch_dir` this call could sweep, oldest
/// first, ties by id, so one call's replies come out in a stable order.
///
/// Empty when no allowed kind is sweepable, which keeps a Bash call that
/// could not relay a leftover anyway from reading the directory at all — and,
/// since the caller proves the session only when it has something to relay,
/// from provoking a refusal of its own.
pub(super) fn candidates(
    scratch_dir: &Path,
    ticketed: &[RelayLine],
    allowed: &[RequestKind],
) -> Vec<Candidate> {
    if !allowed.iter().copied().any(sweepable) {
        return Vec::new();
    }
    let Ok(names) = std::fs::read_dir(scratch_dir) else {
        return Vec::new();
    };
    let mut found: Vec<Candidate> = names
        .flatten()
        .filter_map(|name| {
            let id = request_id(&name.file_name())?;
            if ticketed.iter().any(|line| line.id == id) {
                return None;
            }
            // `DirEntry::metadata` does not follow a symlink, so a link left
            // under a ticket name keeps its own timestamp here and is refused
            // when the sweep opens it.
            let modified = name.metadata().ok()?.modified().ok()?;
            Some(Candidate { id, modified })
        })
        .collect();
    found.sort_by(|left, right| {
        left.modified
            .cmp(&right.modified)
            .then_with(|| left.id.cmp(&right.id))
    });
    found
}

/// Relay every candidate whose file reads as one of this session's tickets
/// and whose kind the Bash command allows.
///
/// A kind the command does not allow is passed over in silence: a line is
/// refused once, but a leftover ticket would repeat that refusal on every
/// later call. A file that does not read as a ticket is reported and left
/// where it is — only an operator can resolve it.
pub(super) fn recover(request: &Request<'_>, candidates: &[Candidate]) -> Vec<String> {
    let mut report = Vec::new();
    for candidate in candidates {
        let read =
            ticket_check::read_unlisted(&request.env.scratch_dir, &candidate.id, request.env.uid);
        let ticket = match read {
            Ok(ticket) => ticket,
            Err(error) => {
                report.push(format!(
                    "LOOM relay: refused {}: {error:#}",
                    candidate.label()
                ));
                continue;
            }
        };
        if !sweepable(ticket.kind) || !request.allowed.contains(&ticket.kind) {
            continue;
        }
        match request.record(ticket, Source::Recovered) {
            Recorded::Reply(reply) => report.push(reply),
            Recorded::Full(reply) => {
                report.push(reply);
                break;
            }
        }
    }
    report
}

/// Only `memory` and `telemetry` are ever swept. A control kind a teammate
/// wrote was refused when its own line appeared; relaying it from a later
/// call would record that teammate's block, handoff or verdict as the main
/// agent's own.
fn sweepable(kind: RequestKind) -> bool {
    !kind.is_control()
}

/// The request id a `.req` file name carries. A name of any other shape was
/// not written by a loom CLI, so it is never swept and never quoted back in a
/// reply.
fn request_id(name: &OsStr) -> Option<String> {
    let id = name.to_str()?.strip_suffix(".req")?;
    validate_request_id(id).ok()?;
    Some(id.to_string())
}

#[cfg(test)]
#[path = "tests_sweep.rs"]
mod tests;
