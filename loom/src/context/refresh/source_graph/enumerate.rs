use anyhow::Result;
use std::collections::BTreeMap;
use std::path::Path;

use super::{excluded, SourceGraphScope};
use crate::context::census::{attributes, attributes_at_head};
use crate::git::worktree::WorktreeGit;

/// Git modes of the entries that are never source files: a symlink, whose
/// working-tree path resolves to whatever it points at (possibly outside the
/// repository), and a gitlink (submodule), which is a directory.
const SYMLINK_MODE: &str = "120000";
const GITLINK_MODE: &str = "160000";

pub(crate) struct Enumeration {
    /// Path → blob object id, from `git ls-tree -r -z HEAD` (Base) or `git ls-files -s -z` (Overlay).
    /// Symlinks, gitlinks and paths git marks `linguist-generated` or `linguist-vendored` are left out.
    pub known: BTreeMap<String, String>,
    /// Existing untracked files from `git ls-files --others --exclude-standard -z` (Overlay only).
    pub untracked: Vec<String>,
    /// Known paths absent from the working tree (Overlay only): what the overlay tombstones
    /// because a tracked file was deleted or moved.
    pub deleted: Vec<String>,
}

pub(crate) fn enumerate(project_root: &Path, scope: &SourceGraphScope) -> Result<Enumeration> {
    let git = WorktreeGit::pinned_checkout(project_root)?;
    match scope {
        SourceGraphScope::Base { .. } => {
            let output = git.run_checked(&["ls-tree", "-r", "-z", "HEAD"])?;
            // A base layer is built from `known` alone, so a committed symlink
            // or gitlink simply never enters it; there is nothing to tombstone.
            let mut known = parse_known(&output, TreeRow::Committed);
            let candidates = known.keys().map(String::as_str).collect::<Vec<_>>();
            // The base is keyed by HEAD and shared across checkouts, so it
            // must not depend on one checkout's uncommitted `.gitattributes`.
            let attributes = attributes_at_head(project_root, &candidates)?;
            known.retain(|path, _| !attributes.is_vendored_or_generated(path));
            Ok(Enumeration {
                known,
                untracked: Vec::new(),
                deleted: Vec::new(),
            })
        }
        SourceGraphScope::Overlay { .. } => {
            let output = git.run_checked(&["ls-files", "-s", "-z"])?;
            let mut known = parse_known(&output, TreeRow::Index);
            let output = git.run_checked(&["ls-files", "--others", "--exclude-standard", "-z"])?;
            let mut untracked = nul_paths(&output)
                .filter(|path| {
                    !excluded(path)
                        && std::fs::symlink_metadata(project_root.join(path))
                            .is_ok_and(|metadata| metadata.file_type().is_file())
                })
                .map(str::to_string)
                .collect::<Vec<_>>();
            untracked.sort();
            filter_generated(project_root, &mut known, &mut untracked)?;
            let deleted = known
                .keys()
                .filter(|path| !project_root.join(path).exists())
                .cloned()
                .collect::<Vec<_>>();
            Ok(Enumeration {
                known,
                untracked,
                deleted,
            })
        }
    }
}

/// Drop attribute-filtered (generated or vendored) paths from `known` and
/// `untracked`. Run before `deleted` is derived: an attribute-filtered path
/// is not source, so it must never count as a deleted one. A failed lookup
/// propagates; falling back to "no filtering" would let a transient failure
/// write a layer that differs from the next run's under the same revision key.
fn filter_generated(
    project_root: &Path,
    known: &mut BTreeMap<String, String>,
    untracked: &mut Vec<String>,
) -> Result<()> {
    let candidates = known
        .keys()
        .chain(untracked.iter())
        .map(String::as_str)
        .collect::<Vec<_>>();
    let attributes = attributes(project_root, &candidates)?;
    known.retain(|path, _| !attributes.is_vendored_or_generated(path));
    untracked.retain(|path| !attributes.is_vendored_or_generated(path));
    Ok(())
}

#[derive(Clone, Copy)]
enum TreeRow {
    Committed,
    Index,
}

/// Split `ls-tree` / `ls-files -s` records into source files (path → blob oid);
/// a symlink or gitlink is never read, so it is left out.
fn parse_known(output: &str, row: TreeRow) -> BTreeMap<String, String> {
    nul_paths(output)
        .filter_map(|record| parse_record(record, row))
        .filter(|(mode, _, path)| !excluded(path) && !matches!(*mode, SYMLINK_MODE | GITLINK_MODE))
        .map(|(_, oid, path)| (path.to_string(), oid.to_string()))
        .collect()
}

/// `(mode, oid, path)` from one `<mode> [<kind>] <oid>[ <stage>]\t<path>` record.
fn parse_record(record: &str, row: TreeRow) -> Option<(&str, &str, &str)> {
    let (metadata, path) = record.split_once('\t')?;
    let mut fields = metadata.split_whitespace();
    let mode = fields.next()?;
    if matches!(row, TreeRow::Committed) {
        let _kind = fields.next()?;
    }
    let oid = fields.next()?;
    Some((mode, oid, path))
}

fn nul_paths(output: &str) -> impl Iterator<Item = &str> {
    output.split('\0').filter(|value| !value.is_empty())
}
