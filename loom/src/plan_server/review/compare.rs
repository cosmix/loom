//! Git side of compare: ref resolution, changed documents, text at a commit.
//!
//! This is the review loop's git injection surface: a ref the reviewer types and document
//! paths both reach `git`. Refs are validated, then resolved through `--end-of-options` to a
//! 40-hex commit id; every other call takes only such an id, and paths follow `--` (or sit
//! inside a `<commit>:<path>` object name after the request-path check).

use std::fmt;
use std::path::Path;

use anyhow::{anyhow, bail, Context, Result};

use crate::git::runner::{run_git, run_git_bool, run_git_records};
use crate::planning::discover::SUMMARIES;
use crate::planning::paths::check_relpath_syntax;

/// The longest ref a reviewer may type, in bytes.
const MAX_REF_BYTES: usize = 256;

/// The `git cat-file` diagnostics (under `LC_ALL=C`) for a path missing from a commit's tree.
const MISSING_PATH_DIAGNOSTICS: [&str; 2] = ["does not exist in '", "exists on disk, but not in '"];

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

/// Every document under `doc/` (summaries excepted) that differs between `commit` and the
/// working tree, untracked documents included, sorted by path. Renames are a removal plus an
/// addition.
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
    let mut docs = parse_name_status(&run_git_records(&diff_args, root)?)?;
    let untracked_args = [
        "ls-files",
        "--others",
        "--exclude-standard",
        "-z",
        "--",
        "doc/",
    ];
    let untracked = run_git_records(&untracked_args, root)?;
    docs.extend(
        untracked
            .split('\0')
            .filter(|path| !path.is_empty())
            .map(|path| ChangedDoc {
                path: path.to_owned(),
                change: ChangedDocChange::Added,
            }),
    );
    docs.retain(|doc| !Path::new(&doc.path).starts_with(SUMMARIES));
    docs.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(docs)
}

/// Parse `git diff --name-status -z --no-renames`: NUL-separated status and path pairs.
fn parse_name_status(records: &str) -> Result<Vec<ChangedDoc>> {
    let mut fields = records.split('\0');
    let mut docs = Vec::new();
    while let Some(status) = fields.next().filter(|status| !status.is_empty()) {
        let path = fields
            .next()
            .filter(|path| !path.is_empty())
            .with_context(|| format!("git diff status {status:?} has no path"))?;
        let change = match status {
            "A" => ChangedDocChange::Added,
            "M" | "T" => ChangedDocChange::Changed,
            "D" => ChangedDocChange::Removed,
            other => bail!("git diff reported unexpected status {other:?} for {path}"),
        };
        docs.push(ChangedDoc {
            path: path.to_owned(),
            change,
        });
    }
    Ok(docs)
}

/// The text of `path` (relative to `doc/`) at `commit`, decoded lossily; `None` when the path
/// does not exist in that commit's tree.
pub fn text_at(root: &Path, commit: &str, path: &str) -> Result<Option<String>> {
    require_commit(commit)?;
    let rel = check_relpath_syntax(path).map_err(|e| anyhow!("invalid document path: {e}"))?;
    let rel = rel.to_str().context("document path is not UTF-8")?;
    let object = format!("{commit}:{rel}");
    let output = run_git(&["cat-file", "blob", &object], root)?;
    if output.status.success() {
        return Ok(Some(String::from_utf8_lossy(&output.stdout).into_owned()));
    }
    let stderr = String::from_utf8_lossy(&output.stderr);
    if MISSING_PATH_DIAGNOSTICS
        .iter()
        .any(|diagnostic| stderr.contains(diagnostic))
    {
        return Ok(None);
    }
    bail!("git cat-file blob {object} failed: {}", stderr.trim())
}

/// True when `commit` is a 40-hex id naming a commit present in the repository.
pub fn commit_exists(root: &Path, commit: &str) -> bool {
    is_commit_id(commit) && run_git_bool(&["cat-file", "-e", &format!("{commit}^{{commit}}")], root)
}
