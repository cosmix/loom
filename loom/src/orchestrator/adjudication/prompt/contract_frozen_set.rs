//! The frozen-set evidence of a contract dispute briefing: what an accept
//! would re-freeze besides the disputed contract's own file (see
//! `apply_contract::accept`), so the judge sees every file it vouches for.

use anyhow::{Context, Result};
use std::collections::BTreeSet;
use std::path::Path;

use super::KindPromptInput;
use crate::fs::safe_read::{is_not_found, read_bounded};
use crate::orchestrator::adjudication::prompt::diff::unified_diff;
use crate::plan::schema::ContractSpec;
use crate::relay::sha256_hex;
use crate::stage_service::contracts::locate;
use crate::verify::contracts::changes::harness_files;
use crate::verify::contracts::normalize;
use crate::verify::contracts::store::{self, FreezeRecord, FrozenFile};

pub(super) const FROZEN_SET_HEADING: &str = "## Other frozen files changed since the freeze";
/// Bounds on the frozen-set evidence: one file's diff or content, and the
/// whole section.
const MAX_FROZEN_SET_DIFF_BYTES: usize = 8 * 1024;
const MAX_FROZEN_SET_SECTION_BYTES: usize = 32 * 1024;

/// [`FROZEN_SET_HEADING`] and its body: every frozen file other than the
/// disputed contract's whose worktree bytes differ from the freeze (a bounded
/// diff of each, and a line naming each deleted one), then every harness match
/// the freeze does not hold yet. An accept re-freezes the changed and new
/// files and drops the deleted.
pub(super) fn section(input: &KindPromptInput<'_>, contract: Option<&ContractSpec>) -> String {
    let record = store::load_freeze(input.work_dir, &input.stage.id);
    let body = match (record, input.worktree) {
        (_, None) => "(stage worktree unavailable)\n\n".to_string(),
        (Ok(None), _) => "(the contracts are not frozen)\n\n".to_string(),
        (Err(error), _) => format!("(freeze record unreadable: {error:#})\n\n"),
        (Ok(Some(record)), Some(_)) => {
            let disputed = contract.map(|contract| normalize(&contract.file));
            changes(input, &record, disputed.as_deref())
        }
    };
    format!("{FROZEN_SET_HEADING}\n\n{body}")
}

/// The entries of [`section`], within [`MAX_FROZEN_SET_SECTION_BYTES`].
fn changes(input: &KindPromptInput<'_>, record: &FreezeRecord, disputed: Option<&str>) -> String {
    let mut out = String::new();
    let entries =
        changed_entries(input, record, disputed).chain(new_entries(input, record, disputed));
    for entry in entries {
        if out.len() + entry.len() > MAX_FROZEN_SET_SECTION_BYTES {
            out.push_str("(more changed frozen files omitted; compare them in the worktree)\n\n");
            break;
        }
        out.push_str(&entry);
    }
    if out.is_empty() {
        out.push_str("(none)\n\n");
    }
    out
}

/// One entry per recorded file other than `disputed` that the worktree changed
/// or deleted since the freeze.
fn changed_entries<'a>(
    input: &'a KindPromptInput<'a>,
    record: &'a FreezeRecord,
    disputed: Option<&'a str>,
) -> impl Iterator<Item = String> + 'a {
    record
        .files
        .iter()
        .filter(move |file| Some(file.path.as_str()) != disputed)
        .filter_map(move |file| drift(input, file))
}

/// The entry for `file` if the worktree no longer holds the frozen bytes.
fn drift(input: &KindPromptInput<'_>, file: &FrozenFile) -> Option<String> {
    let path = &file.path;
    let current = read_bounded(
        &input.site.path,
        Path::new(path),
        store::MAX_FROZEN_FILE_BYTES,
    );
    let entry = match current {
        Ok(bytes) if sha256_hex(&bytes) == file.sha256 => return None,
        Ok(bytes) => changed_entry(input, path, &bytes),
        Err(error) if is_not_found(&error) => {
            format!("- `{path}` is deleted from the worktree\n\n")
        }
        Err(error) => format!("- `{path}` is unreadable in the worktree: {error:#}\n\n"),
    };
    Some(entry)
}

/// One entry per harness match the freeze does not hold, which an accept
/// freezes as new; a failed listing becomes a one-line note.
fn new_entries<'a>(
    input: &'a KindPromptInput<'a>,
    record: &FreezeRecord,
    disputed: Option<&str>,
) -> impl Iterator<Item = String> + 'a {
    let (paths, note) = match new_harness_paths(input, record, disputed) {
        Ok(paths) => (paths, None),
        Err(error) => (
            Vec::new(),
            Some(format!("(new harness files not listed: {error:#})\n\n")),
        ),
    };
    note.into_iter()
        .chain(paths.into_iter().map(move |path| new_entry(input, &path)))
}

/// The harness matches of the stage's worktree that neither the record nor
/// the disputed contract names, listed the way `accept` lists them.
fn new_harness_paths(
    input: &KindPromptInput<'_>,
    record: &FreezeRecord,
    disputed: Option<&str>,
) -> Result<Vec<String>> {
    if input.stage.harness.is_empty() {
        return Ok(Vec::new());
    }
    let work_dir = input
        .work_dir
        .canonicalize()
        .with_context(|| format!("failed to resolve {}", input.work_dir.display()))?;
    let site = locate(&work_dir, input.stage)?;
    let listed = harness_files(&site.git, &site.working_dir, &input.stage.harness)?;
    let recorded: BTreeSet<&str> = record.files.iter().map(|f| f.path.as_str()).collect();
    Ok(listed
        .into_iter()
        .filter(|path| !recorded.contains(path.as_str()) && Some(path.as_str()) != disputed)
        .collect())
}

/// The diff of the frozen copy of `path` against its `current` bytes, cut to
/// [`MAX_FROZEN_SET_DIFF_BYTES`].
fn changed_entry(input: &KindPromptInput<'_>, path: &str, current: &[u8]) -> String {
    let frozen = frozen_copy(input, path).ok();
    let before = frozen
        .as_deref()
        .and_then(|bytes| std::str::from_utf8(bytes).ok());
    let (Some(before), Ok(after)) = (before, std::str::from_utf8(current)) else {
        return format!("- `{path}` changed (no text diff: a side is unreadable or not UTF-8)\n\n");
    };
    let mut diff = unified_diff(before, after, path);
    cut(&mut diff, "diff");
    format!("- `{path}` changed:\n\n```diff\n{diff}```\n\n")
}

/// `path`'s content as an accept would freeze it, cut to
/// [`MAX_FROZEN_SET_DIFF_BYTES`].
fn new_entry(input: &KindPromptInput<'_>, path: &str) -> String {
    let read = read_bounded(
        &input.site.path,
        Path::new(path),
        store::MAX_FROZEN_FILE_BYTES,
    );
    match read.map(String::from_utf8) {
        Ok(Ok(mut text)) => {
            cut(&mut text, "content");
            if !text.ends_with('\n') {
                text.push('\n');
            }
            format!("- `{path}` is new and will be frozen:\n\n```text\n{text}```\n\n")
        }
        Ok(Err(_)) => {
            format!("- `{path}` is new and will be frozen (content not shown: not UTF-8)\n\n")
        }
        Err(error) => format!("- `{path}` is new but unreadable in the worktree: {error:#}\n\n"),
    }
}

/// Cut `text` to [`MAX_FROZEN_SET_DIFF_BYTES`] at a char boundary, ending it
/// with a note naming `what` when anything went.
fn cut(text: &mut String, what: &str) {
    if text.len() <= MAX_FROZEN_SET_DIFF_BYTES {
        return;
    }
    let mut end = MAX_FROZEN_SET_DIFF_BYTES;
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    text.truncate(end);
    text.push_str(&format!("\n... [{what} truncated] ...\n"));
}

/// The frozen copy of `path`, read without following a symlink.
fn frozen_copy(input: &KindPromptInput<'_>, path: &str) -> Result<Vec<u8>> {
    let root = input.work_dir.canonicalize()?;
    let copy = store::frozen_file_path(&root, &input.stage.id, path);
    let relative = copy.strip_prefix(&root)?;
    read_bounded(&root, relative, store::MAX_FROZEN_FILE_BYTES)
}

#[cfg(test)]
#[path = "contract_frozen_set_tests.rs"]
mod tests;
