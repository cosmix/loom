//! Attestation: the ledger ([`LEDGER_FILE`]) loom's `reference-transaction`
//! hook appends host-side ref moves to, and whether the hook can be trusted
//! to have written a line for every such move ([`attestation_mode`]).
//!
//! Ledger lines are `attest <from> <to> <ref>` and `abort <from> <to> <ref>`
//! with full object ids (the all-zero id for an absent value). Only the host
//! writes the file, but it is read as untrusted input: any other line is
//! skipped.

use anyhow::{ensure, Context, Result};
use nix::unistd::{access, AccessFlags};
use std::fs::{File, OpenOptions};
use std::io::{ErrorKind, Write};
use std::path::{Path, PathBuf};

use super::{HOOK_MARKER, LEDGER_FILE};
use crate::git::hooks::{configured_hooks_path, installable_hooks_dir};
use crate::git::runner::run_git_checked;

/// Whether moves made through git on the host are attested in the ledger.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AttestationMode {
    Active,
    /// The hook cannot attest; `reason` says why, for the operator.
    Off {
        reason: String,
    },
}

/// One live `attest` line of the ledger for one ref.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct LedgerStep {
    pub(super) from: String,
    pub(super) to: String,
}

/// `Active` when loom's hook is installed and executable where git runs hooks
/// from, and the hook finds `work_dir` as its state directory; otherwise `Off`
/// with the reason.
///
/// Git runs hooks from the common git directory, or from `core.hooksPath`. A
/// `core.hooksPath` counts only as loom's own install site
/// ([`installable_hooks_dir`]): inside this repository and untracked. Active
/// never trusts more than off does, since the guard only adds the hold on a move
/// without a ledger line, and a session's hook runs in a sandbox that denies the
/// ledger. A relative `core.hooksPath` resolves against the checkout git runs
/// in, so a target moved from a linked worktree, which lacks the hook, holds
/// until `loom target accept`.
pub fn attestation_mode(repo_root: &Path, work_dir: &Path) -> AttestationMode {
    match off_reason(repo_root, work_dir) {
        Some(reason) => AttestationMode::Off { reason },
        None => AttestationMode::Active,
    }
}

/// The hook git runs in `repo_root`, or why no hook there can attest.
fn attesting_hook(repo_root: &Path, common: &Path) -> Result<PathBuf, String> {
    let Some(configured) = configured_hooks_path(repo_root) else {
        let hook = common.join("hooks/reference-transaction");
        return if hook_has_marker(&hook) {
            Ok(hook)
        } else {
            Err("loom's reference-transaction hook is not installed".to_string())
        };
    };
    let directory = installable_hooks_dir(repo_root).map_err(|_| {
        format!(
            "core.hooksPath is set to {configured}, outside this repository, in a directory \
             git tracks, or through a symlink, so loom cannot rely on a hook there"
        )
    })?;
    let hook = directory.join("reference-transaction");
    if !hook_has_marker(&hook) {
        return Err(format!(
            "core.hooksPath is set to {configured}, and loom's reference-transaction hook is \
             not installed there"
        ));
    }
    Ok(hook)
}

fn hook_has_marker(hook: &Path) -> bool {
    std::fs::read_to_string(hook).is_ok_and(|script| script.contains(HOOK_MARKER))
}

fn off_reason(repo_root: &Path, work_dir: &Path) -> Option<String> {
    let Ok(common) = common_dir(repo_root) else {
        return Some("the git common directory could not be found".to_string());
    };
    let hook_path = match attesting_hook(repo_root, &common) {
        Ok(hook_path) => hook_path,
        Err(reason) => return Some(reason),
    };
    // git runs a hook only when `access(X_OK)` allows it.
    if access(&hook_path, AccessFlags::X_OK).is_err() {
        return Some(
            "loom's reference-transaction hook is not executable, so git does not run it"
                .to_string(),
        );
    }
    // The hook looks for `.loom/work` beside the common git directory.
    let hook_state = common.parent().map(|dir| dir.join(".loom/work"));
    let found = match (
        hook_state.map(|dir| dir.canonicalize()),
        work_dir.canonicalize(),
    ) {
        (Some(Ok(hook_state)), Ok(work_dir)) => hook_state == work_dir,
        _ => false,
    };
    (!found).then(|| "the hook cannot find this state directory".to_string())
}

fn common_dir(repo_root: &Path) -> Result<PathBuf> {
    let args = ["rev-parse", "--path-format=absolute", "--git-common-dir"];
    Ok(PathBuf::from(run_git_checked(&args, repo_root)?))
}

fn open_ledger(work_dir: &Path) -> Result<File> {
    let path = work_dir.join(LEDGER_FILE);
    OpenOptions::new()
        .append(true)
        .create(true)
        .open(&path)
        .with_context(|| format!("Failed to open {}", path.display()))
}

/// Create the ledger, empty, unless it exists.
pub(super) fn create_ledger(work_dir: &Path) -> Result<()> {
    open_ledger(work_dir).map(drop)
}

/// Append `attest <from> <to> <reference>` to the ledger in one write.
pub fn append_attestation(work_dir: &Path, reference: &str, from: &str, to: &str) -> Result<()> {
    ensure!(
        is_object_id(from) && is_object_id(to),
        "an attestation needs two full object ids, got {from:?} and {to:?}"
    );
    ensure!(
        !reference.is_empty() && !reference.contains(char::is_whitespace),
        "an attestation needs a reference without whitespace, got {reference:?}"
    );
    let line = format!("attest {from} {to} {reference}\n");
    open_ledger(work_dir)?
        .write_all(line.as_bytes())
        .context("Failed to append to the attestation ledger")
}

/// 40 or 64 lowercase hex characters.
fn is_object_id(id: &str) -> bool {
    matches!(id.len(), 40 | 64) && id.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
}

fn is_zero(id: &str) -> bool {
    id.bytes().all(|b| b == b'0')
}

/// A well-formed ledger line.
struct Line<'a> {
    abort: bool,
    from: &'a str,
    to: &'a str,
    reference: &'a str,
}

impl Line<'_> {
    fn same_move(&self, other: &Line) -> bool {
        (self.from, self.to, self.reference) == (other.from, other.to, other.reference)
    }
}

fn parse_line(line: &str) -> Option<Line<'_>> {
    let mut fields = line.split(' ');
    let abort = match fields.next()? {
        "attest" => false,
        "abort" => true,
        _ => return None,
    };
    let (from, to, reference) = (fields.next()?, fields.next()?, fields.next()?);
    let well_formed =
        fields.next().is_none() && is_object_id(from) && is_object_id(to) && !reference.is_empty();
    well_formed.then_some(Line {
        abort,
        from,
        to,
        reference,
    })
}

/// The live `attest` steps for `reference`, in ledger order. An `abort` line
/// cancels the most recent earlier live `attest` line with the same three
/// fields. Steps from or to the all-zero id and steps from a tip to itself
/// (host `git gc` and `pack-refs` write both) drop out. Only complete
/// (newline-terminated) lines count; a missing ledger has no steps.
pub(super) fn ledger_steps(work_dir: &Path, reference: &str) -> Result<Vec<LedgerStep>> {
    let path = work_dir.join(LEDGER_FILE);
    let bytes = match std::fs::read(&path) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => {
            return Err(error).with_context(|| format!("Failed to read {}", path.display()))
        }
    };
    let text = String::from_utf8_lossy(&bytes);
    let mut live: Vec<Option<Line>> = Vec::new();
    let lines = text
        .split_inclusive('\n')
        .filter_map(|l| l.strip_suffix('\n'));
    for line in lines.filter_map(parse_line) {
        if !line.abort {
            live.push(Some(line));
            continue;
        }
        let cancelled = live
            .iter_mut()
            .rev()
            .find(|slot| slot.as_ref().is_some_and(|attest| attest.same_move(&line)));
        if let Some(slot) = cancelled {
            *slot = None;
        }
    }
    Ok(live
        .into_iter()
        .flatten()
        .filter(|l| l.reference == reference && l.from != l.to)
        .filter(|l| !is_zero(l.from) && !is_zero(l.to))
        .map(|l| LedgerStep {
            from: l.from.to_string(),
            to: l.to.to_string(),
        })
        .collect())
}
