//! Git side of compare: ref resolution, changed documents, text at a commit.
//!
//! This is the review loop's git injection surface: a ref the reviewer types and document
//! paths both reach `git`. Refs are validated, then resolved through `--end-of-options` to a
//! 40-hex commit id; every other call takes only such an id, and paths follow `--` (or sit
//! inside a `<commit>:<path>` object name after the request-path check).

use std::collections::HashSet;
use std::fmt;
use std::path::Path;

use anyhow::{anyhow, bail, Context, Result};

use crate::git::runner::{run_git, run_git_bool, run_git_checked_bytes};
use crate::plan_server::files::doc::MAX_EDIT_BYTES;
use crate::planning::discover::{is_editor_temporary, SUMMARIES};
use crate::planning::paths::check_relpath_syntax;

/// The longest ref a reviewer may type, in bytes.
const MAX_REF_BYTES: usize = 256;

/// The `git cat-file` diagnostics (under `LC_ALL=C`) for an object that is not readable at a
/// commit: a path missing from its tree, or a submodule entry whose commit lives in another
/// repository.
const UNREADABLE_OBJECT_DIAGNOSTICS: [&str; 3] = [
    "does not exist in '",
    "exists on disk, but not in '",
    "could not get object info",
];

/// Why a typed ref did not resolve.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RefError {
    /// The text is not acceptable as a ref; git never ran.
    Invalid(String),
    /// Git does not know the ref as a commit.
    Unknown(String),
}

impl fmt::Display for RefError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            RefError::Invalid(reason) => write!(f, "invalid ref: {reason}"),
            RefError::Unknown(raw) => write!(f, "unknown ref: {raw}"),
        }
    }
}

impl std::error::Error for RefError {}

/// How a document differs between a commit and the working tree.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChangedDocChange {
    /// New since the commit (committed later, staged, or untracked).
    Added,
    /// Present in both with different content or type.
    Changed,
    /// Present at the commit, gone now.
    Removed,
}

/// A document under `doc/` that differs from a commit.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChangedDoc {
    /// Repository-relative path starting with `doc/`.
    pub path: String,
    /// The kind of difference.
    pub change: ChangedDocChange,
}

/// True when `text` is a full 40-hex lowercase commit id.
pub(crate) fn is_commit_id(text: &str) -> bool {
    text.len() == 40 && text.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
}

/// Refuse anything but a full commit id, so no user text reaches git as a ref.
fn require_commit(commit: &str) -> Result<()> {
    if !is_commit_id(commit) {
        bail!("a commit must be 40 lowercase hex digits");
    }
    Ok(())
}

fn check_ref_syntax(raw: &str) -> Result<(), RefError> {
    let reason = if raw.is_empty() {
        "empty"
    } else if raw.len() > MAX_REF_BYTES {
        "longer than 256 bytes"
    } else if raw.starts_with('-') {
        "starts with '-'"
    } else if raw.chars().any(|c| c.is_control() || c.is_whitespace()) {
        "holds a control character or whitespace"
    } else {
        return Ok(());
    };
    Err(RefError::Invalid(reason.to_owned()))
}

/// Resolve a typed ref to the 40-hex id of the commit it names.
pub fn resolve_ref(root: &Path, raw: &str) -> Result<String, RefError> {
    check_ref_syntax(raw)?;
    let spec = format!("{raw}^{{commit}}");
    let args = [
        "rev-parse",
        "--verify",
        "--quiet",
        "--end-of-options",
        spec.as_str(),
    ];
    let unknown = || RefError::Unknown(raw.to_owned());
    let output = run_git(&args, root).map_err(|_| unknown())?;
    if !output.status.success() {
        return Err(unknown());
    }
    let id = String::from_utf8_lossy(&output.stdout).trim().to_owned();
    if is_commit_id(&id) {
        Ok(id)
    } else {
        Err(unknown())
    }
}

/// Every document under `doc/` that discovery could list (see `discoverable`) and that differs
/// between `commit` and the working tree, untracked documents included, sorted by path. Renames
/// are a removal plus an addition.
pub fn changed_since(root: &Path, commit: &str) -> Result<Vec<ChangedDoc>> {
    require_commit(commit)?;
    let diff_args = [
        "diff",
        "--name-status",
        "-z",
        "--no-renames",
        commit,
        "--",
        "doc/",
    ];
    let mut docs = parse_name_status(&run_git_checked_bytes(&diff_args, root)?)?;
    let untracked_args = [
        "ls-files",
        "--others",
        "--exclude-standard",
        "-z",
        "--",
        "doc/",
    ];
    let untracked = run_git_checked_bytes(&untracked_args, root)?;
    docs.extend(
        untracked
            .split(|&byte| byte == 0)
            .filter_map(utf8_path)
            .map(|path| ChangedDoc {
                path,
                change: ChangedDocChange::Added,
            }),
    );
    docs.retain(|doc| discoverable(Path::new(&doc.path)));
    docs.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(docs)
}

/// True when discovery could list `rel` (repository-relative): no component is a name the walk
/// skips and the path is outside the summary store. Section 12 makes discovery the allowlist
/// for request paths; reads at a commit apply its rules to the path itself, because a document
/// removed since the commit is no longer in the current tree.
pub(crate) fn discoverable(rel: &Path) -> bool {
    !rel.starts_with(SUMMARIES)
        && rel.components().all(|component| {
            component
                .as_os_str()
                .to_str()
                .is_some_and(|name| !is_editor_temporary(name))
        })
}

/// A non-empty git path record as text; `None` for an empty record or a name that is not UTF-8,
/// which discovery skips too.
fn utf8_path(record: &[u8]) -> Option<String> {
    std::str::from_utf8(record)
        .ok()
        .filter(|path| !path.is_empty())
        .map(str::to_owned)
}

/// Parse `git diff --name-status -z --no-renames`: NUL-separated status and path pairs. A path
/// git lists twice (an unmerged path can appear as `U` and again as `M`) keeps its first entry.
pub(super) fn parse_name_status(records: &[u8]) -> Result<Vec<ChangedDoc>> {
    let mut fields = records.split(|&byte| byte == 0);
    let mut docs = Vec::new();
    let mut seen = HashSet::new();
    while let Some(status) = fields.next().filter(|status| !status.is_empty()) {
        let status = String::from_utf8_lossy(status);
        let path = fields
            .next()
            .filter(|path| !path.is_empty())
            .with_context(|| format!("git diff status {status:?} has no path"))?;
        let change = match status.as_ref() {
            "A" => ChangedDocChange::Added,
            // `U` is an unmerged path during a merge conflict: its text differs from the commit.
            "M" | "T" | "U" => ChangedDocChange::Changed,
            "D" => ChangedDocChange::Removed,
            other => bail!(
                "git diff reported unexpected status {other:?} for {}",
                String::from_utf8_lossy(path)
            ),
        };
        if let Some(path) = utf8_path(path).filter(|path| seen.insert(path.clone())) {
            docs.push(ChangedDoc { path, change });
        }
    }
    Ok(docs)
}

/// `text_at` refused a blob over file mode's 1 MiB limit without reading it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BlobTooLarge;

impl fmt::Display for BlobTooLarge {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "the committed document is over {MAX_EDIT_BYTES} bytes")
    }
}

impl std::error::Error for BlobTooLarge {}

/// The text of `path` (relative to `doc/`) at `commit`, decoded lossily; `None` when the path
/// does not exist in that commit's tree or names something other than a file (a directory or a
/// submodule entry). A blob over 1 MiB is a `BlobTooLarge` error, checked by size before the blob
/// is read, so a large blob is never buffered.
pub fn text_at(root: &Path, commit: &str, path: &str) -> Result<Option<String>> {
    require_commit(commit)?;
    let rel = check_relpath_syntax(path).map_err(|e| anyhow!("invalid document path: {e}"))?;
    let rel = rel.to_str().context("document path is not UTF-8")?;
    // The commit is a full id, so the size and the content come from one immutable object.
    let object = format!("{commit}:{rel}");
    let Some(size) = blob_size(root, &object)? else {
        return Ok(None);
    };
    if size > MAX_EDIT_BYTES {
        return Err(BlobTooLarge.into());
    }
    let bytes = run_git_checked_bytes(&["cat-file", "blob", &object], root)?;
    Ok(Some(String::from_utf8_lossy(&bytes).into_owned()))
}

/// The size in bytes of `object` when it is a blob; `None` when it is no blob at the commit: the
/// path is missing, names a directory, or names a submodule entry.
fn blob_size(root: &Path, object: &str) -> Result<Option<usize>> {
    if cat_file(root, "-t", object)?.as_deref() != Some("blob") {
        return Ok(None);
    }
    let Some(size) = cat_file(root, "-s", object)? else {
        return Ok(None);
    };
    size.parse()
        .map(Some)
        .with_context(|| format!("git cat-file -s {object} printed no size"))
}

/// The trimmed output of `git cat-file <flag> <object>`; `None` when the object is not readable
/// at its commit (see `UNREADABLE_OBJECT_DIAGNOSTICS`).
fn cat_file(root: &Path, flag: &str, object: &str) -> Result<Option<String>> {
    let output = run_git(&["cat-file", flag, object], root)?;
    if output.status.success() {
        return Ok(Some(
            String::from_utf8_lossy(&output.stdout).trim().to_owned(),
        ));
    }
    let stderr = String::from_utf8_lossy(&output.stderr);
    if UNREADABLE_OBJECT_DIAGNOSTICS
        .iter()
        .any(|diagnostic| stderr.contains(diagnostic))
    {
        return Ok(None);
    }
    bail!("git cat-file {flag} {object} failed: {}", stderr.trim())
}

/// True when `commit` is a 40-hex id naming a commit present in the repository.
pub fn commit_exists(root: &Path, commit: &str) -> bool {
    is_commit_id(commit) && run_git_bool(&["cat-file", "-e", &format!("{commit}^{{commit}}")], root)
}
