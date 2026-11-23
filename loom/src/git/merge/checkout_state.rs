//! Classify the uncommitted state of the operator's checkout against a merge.
//!
//! Inputs are `git status --porcelain=v2 -z --untracked-files=all` run in the
//! checkout and `git diff --name-status -z --no-renames <old> <merged>`. A path
//! is "touched" when `old..merged` modifies, adds or deletes it. Everything
//! here is pure; what the disk holds is read through a [`CheckoutProbe`].

use std::collections::BTreeSet;

/// What to do with the checkout's uncommitted state before a fast-forward.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Classification {
    /// No tracked change and no untracked file is on a touched path; git
    /// carries the local changes through the fast-forward.
    NoOverlap,
    /// The only overlap is untracked files the merge adds with identical
    /// bytes; move them aside, then fast-forward.
    RemoveUntracked { paths: Vec<String> },
    /// Refuse: a tracked change on a touched path, an untracked file with
    /// different content on one, or an unmerged index entry
    /// (`merge --ff-only` refuses those). A blocked merge moves nothing.
    Blocked { paths: Vec<String> },
}

/// Parsed `status --porcelain=v2 -z` output.
#[derive(Debug, Default, PartialEq, Eq)]
struct StatusEntries {
    tracked: BTreeSet<String>,
    unmerged: Vec<String>,
    untracked: Vec<String>,
}

/// What the disk holds at a path.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiskEntry {
    Absent,
    RegularFile,
    Directory,
    /// A symlink or any other non-regular, non-directory entry.
    Other,
}

/// The checks `classify` cannot make from text: it reads the checkout.
pub trait CheckoutProbe {
    /// What the disk holds at `path` (a symlink is not followed).
    fn disk(&self, path: &str) -> DiskEntry;
    /// Whether the regular file at `path` has exactly the bytes the merge
    /// would write there.
    fn equals_blob(&self, path: &str) -> bool;
    /// Whether `path` exists in the tree the merge starts from.
    fn in_base_tree(&self, path: &str) -> bool;
}

/// Paths the merge touches, from `diff --name-status -z --no-renames`.
struct Touched {
    all: BTreeSet<String>,
    added: Vec<String>,
}

impl Touched {
    /// Whether `path` is touched or sits above or below a touched path.
    fn overlaps(&self, path: &str) -> bool {
        self.all.iter().any(|t| nested_or_equal(path, t))
    }
}

/// `a == b`, `a` below `b`, or `b` below `a`, in path components.
fn nested_or_equal(a: &str, b: &str) -> bool {
    let below = |inner: &str, outer: &str| {
        inner
            .strip_prefix(outer)
            .is_some_and(|rest| rest.starts_with('/'))
    };
    a == b || below(a, b) || below(b, a)
}

/// Classify the checkout.
///
/// A status path overlaps a touched path when they are equal or one is a
/// directory prefix of the other. Only an exact-path untracked overlap can be
/// byte-compared and moved aside; a prefix overlap blocks, and so does a
/// tracked change on a touched path. Ignored files do not appear in status,
/// so each path the merge adds that status does not list is looked up on
/// disk through `probe`.
pub fn classify(
    status_v2_z: &str,
    diff_name_status_z: &str,
    probe: &dyn CheckoutProbe,
) -> Classification {
    let status = parse_status(status_v2_z);
    if !status.unmerged.is_empty() {
        return Classification::Blocked {
            paths: status.unmerged,
        };
    }
    let touched = parse_touched(diff_name_status_z);

    let mut remove_untracked = BTreeSet::new();
    let mut differing = BTreeSet::new();
    classify_untracked(
        &status,
        &touched,
        probe,
        &mut remove_untracked,
        &mut differing,
    );
    classify_unlisted_additions(
        &status,
        &touched,
        probe,
        &mut remove_untracked,
        &mut differing,
    );
    if !differing.is_empty() {
        return Classification::Blocked {
            paths: differing.into_iter().collect(),
        };
    }

    let tracked: Vec<String> = status
        .tracked
        .into_iter()
        .filter(|p| touched.overlaps(p))
        .collect();
    overlap_action(tracked, remove_untracked.into_iter().collect())
}

/// The action once no untracked file blocks: fast-forward as is, move equal
/// untracked files aside first, or refuse on a tracked overlap.
fn overlap_action(tracked: Vec<String>, remove_untracked: Vec<String>) -> Classification {
    match (tracked.is_empty(), remove_untracked.is_empty()) {
        (true, true) => Classification::NoOverlap,
        (true, false) => Classification::RemoveUntracked {
            paths: remove_untracked,
        },
        (false, _) => Classification::Blocked { paths: tracked },
    }
}

/// Untracked files on a touched path: removable when equal, blocking when
/// they differ or only share a directory prefix with it.
fn classify_untracked(
    status: &StatusEntries,
    touched: &Touched,
    probe: &dyn CheckoutProbe,
    equal: &mut BTreeSet<String>,
    differing: &mut BTreeSet<String>,
) {
    for path in &status.untracked {
        if touched.all.contains(path) {
            compare_bytes(probe, path, equal, differing);
        } else if touched.overlaps(path) {
            differing.insert(path.clone());
        }
    }
}

fn compare_bytes(
    probe: &dyn CheckoutProbe,
    path: &str,
    equal: &mut BTreeSet<String>,
    differing: &mut BTreeSet<String>,
) {
    if probe.equals_blob(path) {
        equal.insert(path.to_string());
    } else {
        differing.insert(path.to_string());
    }
}

/// Files the merge adds that status does not list may still exist on disk
/// as ignored files; git would overwrite them silently.
fn classify_unlisted_additions(
    status: &StatusEntries,
    touched: &Touched,
    probe: &dyn CheckoutProbe,
    equal: &mut BTreeSet<String>,
    differing: &mut BTreeSet<String>,
) {
    let listed = |path: &String| status.tracked.contains(path) || status.untracked.contains(path);
    for path in touched.added.iter().filter(|p| !listed(p)) {
        match probe.disk(path) {
            DiskEntry::RegularFile => compare_bytes(probe, path, equal, differing),
            DiskEntry::Directory | DiskEntry::Other => {
                differing.insert(path.clone());
            }
            DiskEntry::Absent => differing.extend(blocking_parent(path, probe)),
        }
    }
}

/// The first parent directory of `path` that the disk holds as a non-directory
/// although the merge's starting tree does not have it as a file.
fn blocking_parent(path: &str, probe: &dyn CheckoutProbe) -> Option<String> {
    for (index, _) in path.match_indices('/') {
        let parent = &path[..index];
        match probe.disk(parent) {
            DiskEntry::Absent => return None,
            DiskEntry::Directory => {}
            _ if probe.in_base_tree(parent) => {}
            _ => return Some(parent.to_string()),
        }
    }
    None
}

/// Every path `status --porcelain=v2 -z` names: tracked changes (a rename's
/// original path too), unmerged entries and untracked files.
pub(super) fn status_paths(status_v2_z: &str) -> Vec<String> {
    let status = parse_status(status_v2_z);
    status
        .tracked
        .into_iter()
        .chain(status.unmerged)
        .chain(status.untracked)
        .collect()
}

/// Paths from `status --porcelain=v2 -z`. Fixed field counts with `splitn`
/// keep paths containing spaces intact; a rename (`2`) carries its original
/// path in the next NUL field, and both count as tracked changes.
fn parse_status(status: &str) -> StatusEntries {
    let mut entries = StatusEntries::default();
    let mut fields = status.split('\0');
    while let Some(entry) = fields.next() {
        match entry.chars().next() {
            Some('1') => push_path(&mut entries.tracked, entry, 9),
            Some('2') => {
                push_path(&mut entries.tracked, entry, 10);
                if let Some(orig) = fields.next().filter(|o| !o.is_empty()) {
                    entries.tracked.insert(orig.to_string());
                }
            }
            Some('u') => {
                if let Some(path) = entry.splitn(11, ' ').nth(10) {
                    entries.unmerged.push(path.to_string());
                }
            }
            Some('?') => {
                if let Some(path) = entry.strip_prefix("? ") {
                    entries.untracked.push(path.to_string());
                }
            }
            _ => {}
        }
    }
    entries
}

fn push_path(into: &mut BTreeSet<String>, entry: &str, field_count: usize) {
    if let Some(path) = entry.splitn(field_count, ' ').nth(field_count - 1) {
        into.insert(path.to_string());
    }
}

/// Paths from `diff --name-status -z --no-renames`: `<status>\0<path>\0` pairs.
fn parse_touched(diff: &str) -> Touched {
    let mut fields = diff.split('\0').filter(|f| !f.is_empty());
    let mut touched = Touched {
        all: BTreeSet::new(),
        added: Vec::new(),
    };
    while let (Some(status), Some(path)) = (fields.next(), fields.next()) {
        if status == "A" {
            touched.added.push(path.to_string());
        }
        touched.all.insert(path.to_string());
    }
    touched
}

#[cfg(test)]
mod tests;
