//! The content of a checkout as a pass-cache input: every tracked file and
//! every untracked file git does not ignore, as `(path, mode, blob id)`.
//! Commit identity, refs and staging are left out, so a check on a dirty
//! tree and a completion on the commit holding the same bytes share a key.

use sha1::Sha1;
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::fs::Metadata;
use std::io::ErrorKind;
use std::path::{Component, Path};

use super::{digest_regular_file, hash_field};
use crate::git::worktree::WorktreeGit;
use crate::verify::criteria::cache_executable::is_executable;

const MAX_GIT_CONTEXT_BYTES: usize = 8 * 1024 * 1024;

/// `--no-optional-locks` (as `GIT_OPTIONAL_LOCKS=0`): neither listing
/// rewrites the index it refreshes, so a fingerprint never takes
/// `index.lock` from the operator's own git.
const LS_FILES: [&str; 5] = ["--no-optional-locks", "ls-files", "-s", "-v", "-z"];
/// `core.fileMode=true` makes status list an exec-bit change even where the
/// repository ignores modes, so the bit is read from disk like any edit.
const STATUS: [&str; 8] = [
    "--no-optional-locks",
    "-c",
    "core.fileMode=true",
    "status",
    "--porcelain=v2",
    "--no-renames",
    "--untracked-files=all",
    "-z",
];

#[derive(Debug, Clone, Copy)]
enum ObjectFormat {
    Sha1,
    Sha256,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Entry {
    mode: &'static str,
    oid: String,
}

/// Hash the manifest of the checkout `git` runs in, whose root is
/// `repo_root`, reading changed files within `remaining` bytes. `None`, an
/// uncacheable run, when any part cannot be established.
///
/// Soundness: equal manifests mean byte-identical regular files at the same
/// paths with the same exec bits, and nothing else exists outside ignored
/// paths (symlinks and submodules refuse the run).
/// - A path git lists as changed, staged, deleted or untracked is read from
///   disk here: its entry is the blob id of the bytes on disk, absent when
///   the file is gone, whatever the index holds.
/// - A tracked path status does not list matches its index entry as git
///   judges it: git compares the stat data the index records and re-hashes
///   a file whose stat data changed or is racily clean. Three things make
///   git skip that comparison: assume-unchanged (`h`) and skip-worktree
///   (`S`) entries, which refuse the run, and fsmonitor, which every runner
///   call disables (`NO_HOOKS_ARGS`).
/// - The index is listed before and after status and must not change, so
///   status compared the disk against the very entries the manifest holds.
///
/// A blob id is a collision-resistant hash of exact bytes. Deliberate
/// limits, shared with git: a file rewritten with its size and modification
/// time restored is invisible to the stat comparison, and an unlisted
/// path's blob id names its content after git's clean filters and
/// end-of-line conversion. Revisit if a criterion can depend on bytes git
/// normalises, or on stat-preserving rewrites.
pub(super) fn hash_manifest(
    git: &WorktreeGit,
    repo_root: &Path,
    remaining: &mut u64,
    hasher: &mut Sha256,
) -> Option<()> {
    let manifest = capture(git, repo_root, remaining)?;
    hash_field(hasher, b"manifest");
    hash_field(hasher, &u64::try_from(manifest.len()).ok()?.to_le_bytes());
    for (path, entry) in &manifest {
        hash_field(hasher, path.as_bytes());
        hash_field(hasher, entry.mode.as_bytes());
        hash_field(hasher, entry.oid.as_bytes());
    }
    Some(())
}

fn capture(
    git: &WorktreeGit,
    repo_root: &Path,
    remaining: &mut u64,
) -> Option<BTreeMap<String, Entry>> {
    let format = object_format(git)?;
    let index = git_bytes(git, &LS_FILES)?;
    let status = git_bytes(git, &STATUS)?;
    if git_bytes(git, &LS_FILES)? != index {
        return None;
    }
    let mut manifest = index_entries(&index, format)?;
    for path in status_paths(&status)? {
        match disk_entry(repo_root, &path, format, remaining)? {
            Some(entry) => manifest.insert(path, entry),
            None => manifest.remove(&path),
        };
    }
    Some(manifest)
}

fn git_bytes(git: &WorktreeGit, args: &[&str]) -> Option<Vec<u8>> {
    let output = git.run(args).ok()?;
    if !output.status.success() || output.stdout.len() > MAX_GIT_CONTEXT_BYTES {
        return None;
    }
    Some(output.stdout)
}

fn object_format(git: &WorktreeGit) -> Option<ObjectFormat> {
    let output = git_bytes(git, &["rev-parse", "--show-object-format"])?;
    match output.trim_ascii() {
        b"sha1" => Some(ObjectFormat::Sha1),
        b"sha256" => Some(ObjectFormat::Sha256),
        _ => None,
    }
}

fn index_entries(raw: &[u8], format: ObjectFormat) -> Option<BTreeMap<String, Entry>> {
    let mut entries = BTreeMap::new();
    for record in raw.split(|byte| *byte == 0).filter(|item| !item.is_empty()) {
        let (path, entry) = index_entry(record, format)?;
        entries.insert(path, entry);
    }
    Some(entries)
}

/// One `ls-files -s -v` record, `H <mode> <oid> 0\t<path>`. Any other tag
/// (`h` assume-unchanged, `S` skip-worktree, `M` unmerged), a nonzero stage
/// or a mode other than a regular file's refuses the run.
fn index_entry(record: &[u8], format: ObjectFormat) -> Option<(String, Entry)> {
    let record = std::str::from_utf8(record).ok()?;
    let (fields, path) = record.split_once('\t')?;
    let fields: Vec<&str> = fields.split(' ').collect();
    let [tag, mode, oid, stage] = fields.as_slice() else {
        return None;
    };
    let valid = *tag == "H" && *stage == "0" && format.is_oid(oid);
    let mode = regular_mode(mode).filter(|_| valid && safe_relative_path(path))?;
    let entry = Entry {
        mode,
        oid: (*oid).to_string(),
    };
    Some((path.to_string(), entry))
}

fn regular_mode(mode: &str) -> Option<&'static str> {
    match mode {
        "100644" => Some("100644"),
        "100755" => Some("100755"),
        _ => None,
    }
}

/// Paths of `status --porcelain=v2 --no-renames -z`: ordinary changes
/// (`1`, eight fields before the path) and untracked files (`?`). Unmerged
/// entries and anything unexpected refuse the run.
fn status_paths(raw: &[u8]) -> Option<Vec<String>> {
    let mut paths = Vec::new();
    for token in raw.split(|byte| *byte == 0).filter(|item| !item.is_empty()) {
        let fields = match token.first().copied() {
            Some(b'1') => 8,
            Some(b'?') => 1,
            _ => return None,
        };
        let path = std::str::from_utf8(skip_fields(token, fields)?).ok()?;
        if !safe_relative_path(path) {
            return None;
        }
        paths.push(path.to_string());
    }
    Some(paths)
}

fn skip_fields(mut value: &[u8], count: usize) -> Option<&[u8]> {
    for _ in 0..count {
        let index = value.iter().position(|byte| *byte == b' ')?;
        value = &value[index + 1..];
    }
    Some(value)
}

fn safe_relative_path(path: &str) -> bool {
    let path = Path::new(path);
    !path.as_os_str().is_empty()
        && path
            .components()
            .all(|component| matches!(component, Component::Normal(_)))
}

/// What `relative` holds on disk: `Some(None)` when it is gone, the entry of
/// a regular file, and `None` (refuse) for a symlink, directory or anything
/// unreadable, as the cache never hashes what a link names.
fn disk_entry(
    repo_root: &Path,
    relative: &str,
    format: ObjectFormat,
    remaining: &mut u64,
) -> Option<Option<Entry>> {
    let path = repo_root.join(relative);
    let metadata = match path.symlink_metadata() {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == ErrorKind::NotFound => return Some(None),
        Err(_) => return None,
    };
    if !metadata.file_type().is_file() {
        return None;
    }
    let mode = if is_executable(&metadata) {
        "100755"
    } else {
        "100644"
    };
    let oid = format.blob_oid(&path, &metadata, remaining)?;
    Some(Some(Entry { mode, oid }))
}

impl ObjectFormat {
    fn is_oid(self, value: &str) -> bool {
        let len = match self {
            ObjectFormat::Sha1 => 40,
            ObjectFormat::Sha256 => 64,
        };
        value.len() == len
            && value
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    }

    /// The id git gives a blob of the file's bytes, read within `remaining`.
    fn blob_oid(self, path: &Path, metadata: &Metadata, remaining: &mut u64) -> Option<String> {
        match self {
            ObjectFormat::Sha1 => blob_oid_with(Sha1::new(), path, metadata, remaining),
            ObjectFormat::Sha256 => blob_oid_with(Sha256::new(), path, metadata, remaining),
        }
    }
}

/// `digest_regular_file` refuses a file whose length moved from the
/// `metadata` length the header names.
fn blob_oid_with<D: Digest>(
    mut hasher: D,
    path: &Path,
    metadata: &Metadata,
    remaining: &mut u64,
) -> Option<String> {
    hasher.update(format!("blob {}\0", metadata.len()).as_bytes());
    let digest = digest_regular_file(path, metadata, remaining, hasher)?;
    Some(hex::encode(digest))
}

#[cfg(test)]
#[path = "cache_manifest_tests.rs"]
mod tests;
