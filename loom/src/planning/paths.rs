//! The shared write-path predicate and locked-document set.
//!
//! Plan 3's save route and plan 6's planning hook both ask this module whether a path under
//! `doc/` may be written, so the editor and the sessions agree on what a locked plan is.
//! The locked set is computed from the current tree on every [`LockedSet::scan`], and the
//! name rules lock a path whether or not it exists yet.
//!
//! Every name comparison is made on the lowercased path: on a case-insensitive file system
//! (macOS) `doc/plans/in_progress-plan-x.md` opens `IN_PROGRESS-PLAN-x.md`. Folding only
//! ever adds matches, so on a case-sensitive file system it locks a few more names that
//! differ from a locked one by case alone.

use std::collections::BTreeMap;
use std::fmt;
use std::io::ErrorKind;
use std::path::{Component, Path, PathBuf};

use anyhow::{bail, Context, Result};
use serde::ser::SerializeStruct;
use serde::{Serialize, Serializer};

use crate::fs::plan_lifecycle::{DONE_PREFIX, IN_PROGRESS_PREFIX, RETIRED_PREFIX};
use crate::plan::inputs::collect_inputs;
use crate::planning::discover::{discover, DocTree, Document, SUMMARIES};
use crate::planning::slug::{plan_state, PlanState};

#[cfg(test)]
mod tests;

const PLANS: &str = "doc/plans";
const ARCHIVE: &str = "doc/plans/archive";
const BRIEFS: &str = "doc/plans/briefs";
const KNOWLEDGE: &str = "doc/loom/knowledge";
/// Longest request path, in bytes, `check_relpath_syntax` accepts.
const MAX_RELPATH_BYTES: usize = 1024;

/// Why a path is in the locked set.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "reason", rename_all = "snake_case")]
pub enum LockReason {
    /// A plan file whose own name says it is running, done or retired
    /// (JSON `{"reason":"plan_state","state":"running"}`).
    #[serde(serialize_with = "serialize_plan_state")]
    PlanState(PlanState),
    /// A path under `doc/plans/archive/`.
    Archive,
    /// A run review, `doc/plans/REVIEW-*.md`.
    RunReview,
    /// A brief of a locked plan: under its bundle `doc/plans/briefs/<slug>/` or an input
    /// `collect_inputs` lists for it.
    BriefOfLockedPlan {
        /// The locked plan's slug.
        slug: String,
    },
}

/// An internally tagged enum cannot carry a bare string in a newtype variant, so the state
/// becomes a `state` field beside the tag.
fn serialize_plan_state<S: Serializer>(
    state: &PlanState,
    serializer: S,
) -> Result<S::Ok, S::Error> {
    let mut fields = serializer.serialize_struct("PlanState", 1)?;
    fields.serialize_field("state", state)?;
    fields.end()
}

/// Where a write lands, for the write table of section 12.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DocArea {
    /// The locked set: never writable.
    Locked(LockReason),
    /// `doc/loom/knowledge/`: only through `loom knowledge`.
    Knowledge,
    /// `doc/loom/summaries/`: only through `loom plan summary seal`.
    Summaries,
    /// Any other path under `doc/`.
    Doc,
    /// Anything outside `doc/`, or a path that is not plain and relative.
    OutsideDoc,
}

/// The locked documents of a tree, as one scan found them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LockedSet {
    /// Lowercased inputs of locked plans, other than the plans themselves.
    briefs: BTreeMap<PathBuf, LockReason>,
    /// Lowercased bundle directories of locked plans, with each plan's slug.
    bundles: Vec<(PathBuf, String)>,
    /// Existing locked files plus the archive directory, sorted.
    existing: Vec<PathBuf>,
}

impl LockedSet {
    /// Scan the tree under `root` (the repository root). An error, including one from
    /// `collect_inputs`, must make the caller deny: the set would be incomplete.
    pub fn scan(root: &Path) -> Result<LockedSet> {
        let tree = discover(root)?;
        let mut set = LockedSet {
            briefs: BTreeMap::new(),
            bundles: Vec::new(),
            existing: Vec::new(),
        };
        for (slug, plan) in locked_plan_files(&tree) {
            let bundle = fold(&Path::new(BRIEFS).join(&slug));
            if !set.bundles.iter().any(|(prefix, _)| *prefix == bundle) {
                set.bundles.push((bundle, slug.clone()));
            }
            let inputs = collect_inputs(root, &plan).with_context(|| {
                format!("cannot list the inputs of locked plan {}", plan.display())
            })?;
            for file in inputs.files.into_keys().filter(|file| *file != plan) {
                let reason = LockReason::BriefOfLockedPlan { slug: slug.clone() };
                set.briefs.entry(fold(&file)).or_insert(reason);
            }
        }
        set.existing = existing_locked(root, &tree.docs, &set)?;
        Ok(set)
    }

    /// Why `rel` (repo-relative, `doc/...`, syntax-checked) is locked: an input of a locked
    /// plan first, then the name rules and the bundle prefixes.
    pub fn lock_reason(&self, rel: &Path) -> Option<LockReason> {
        let folded = fold(rel);
        if let Some(reason) = self.briefs.get(&folded) {
            return Some(reason.clone());
        }
        name_reason(&folded).or_else(|| {
            self.bundles
                .iter()
                .find(|(prefix, _)| folded.starts_with(prefix))
                .map(|(_, slug)| LockReason::BriefOfLockedPlan { slug: slug.clone() })
        })
    }

    /// The existing locked files, repo-relative and sorted: plans, run reviews, briefs, and
    /// `doc/plans/archive` itself in place of the files under it, so no entry lies beneath
    /// another (the spawn-time deny lists need that).
    pub fn locked_paths(&self) -> Vec<PathBuf> {
        self.existing.clone()
    }
}

/// Every plan file, winner or stray, whose own name puts it past `Written`, with its slug.
fn locked_plan_files(tree: &DocTree) -> Vec<(String, PathBuf)> {
    let mut files = Vec::new();
    for entry in tree.plans.values() {
        for path in std::iter::once(&entry.path).chain(&entry.strays) {
            if plan_state(path).is_some_and(|state| state > PlanState::Written) {
                files.push((entry.slug.clone(), path.clone()));
            }
        }
    }
    files
}

/// The discovered documents the set locks, outside the archive, plus the archive directory
/// when it is a real directory.
fn existing_locked(root: &Path, docs: &[Document], set: &LockedSet) -> Result<Vec<PathBuf>> {
    let mut existing: Vec<PathBuf> = docs
        .iter()
        .map(|doc| &doc.path)
        .filter(|path| !path.starts_with(ARCHIVE) && set.lock_reason(path).is_some())
        .cloned()
        .collect();
    if is_real_dir(root, Path::new(ARCHIVE))? {
        existing.push(PathBuf::from(ARCHIVE));
    }
    existing.sort();
    Ok(existing)
}

fn is_real_dir(root: &Path, rel: &Path) -> Result<bool> {
    if symlink_component(root, rel)?.is_some() {
        return Ok(false);
    }
    match std::fs::symlink_metadata(root.join(rel)) {
        Ok(meta) => Ok(meta.is_dir()),
        Err(error) if error.kind() == ErrorKind::NotFound => Ok(false),
        Err(error) => Err(error).with_context(|| format!("cannot stat {}", rel.display())),
    }
}

fn fold(path: &Path) -> PathBuf {
    PathBuf::from(path.to_string_lossy().to_lowercase())
}

/// The name rules on a lowercased path: a locked plan spelling, then the archive, then a
/// run review directly in `doc/plans/`.
fn name_reason(folded: &Path) -> Option<LockReason> {
    let state = case_canonical_plan(folded).and_then(|plan| plan_state(&plan));
    if let Some(state) = state.filter(|state| *state > PlanState::Written) {
        return Some(LockReason::PlanState(state));
    }
    if folded.starts_with(ARCHIVE) {
        return Some(LockReason::Archive);
    }
    let name = folded.file_name()?.to_str()?;
    let review = folded.parent() == Some(Path::new(PLANS))
        && name.starts_with("review-")
        && name.ends_with(".md");
    review.then_some(LockReason::RunReview)
}

/// Restore the upper-case lifecycle and `PLAN-` tokens of a lowercased plan file name, so
/// `plan_state` judges it as the file system would resolve it.
fn case_canonical_plan(folded: &Path) -> Option<PathBuf> {
    let name = folded.file_name()?.to_str()?;
    let (prefix, rest) = [IN_PROGRESS_PREFIX, DONE_PREFIX, RETIRED_PREFIX]
        .iter()
        .find_map(|prefix| {
            let rest = name.strip_prefix(&prefix.to_lowercase())?;
            Some((*prefix, rest))
        })
        .unwrap_or(("", name));
    let slug_and_ext = rest.strip_prefix("plan-")?;
    Some(folded.with_file_name(format!("{prefix}PLAN-{slug_and_ext}")))
}

/// Classify a write to `rel` (repo-relative). Pure: the locked set first, then the knowledge
/// base and the summary store, then the rest of `doc/`. A path with a `..` component, a
/// root, or nothing below `doc` is `OutsideDoc`.
pub fn classify_write(locked: &LockedSet, rel: &Path) -> DocArea {
    let plain = rel
        .components()
        .all(|part| matches!(part, Component::Normal(_)));
    let mut parts = rel.components();
    let under_doc =
        parts.next() == Some(Component::Normal("doc".as_ref())) && parts.next().is_some();
    if !plain || !under_doc {
        return DocArea::OutsideDoc;
    }
    if let Some(reason) = locked.lock_reason(rel) {
        return DocArea::Locked(reason);
    }
    let folded = fold(rel);
    if folded.starts_with(KNOWLEDGE) {
        DocArea::Knowledge
    } else if folded.starts_with(SUMMARIES) {
        DocArea::Summaries
    } else {
        DocArea::Doc
    }
}

/// Why a request path failed section 12's "Paths in requests" syntax checks.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PathSyntaxError {
    /// The path is empty.
    Empty,
    /// The path starts with `/`.
    Absolute,
    /// The path holds a backslash.
    Backslash,
    /// The path holds a control character (C0, DEL or C1, NUL included).
    ControlChar(char),
    /// The path is longer than 1024 bytes.
    TooLong(usize),
    /// The path has an empty component (`a//b`, a trailing `/`).
    EmptyComponent,
    /// The path has a `.` or `..` component.
    DotComponent,
}

impl fmt::Display for PathSyntaxError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Empty => write!(f, "the path is empty"),
            Self::Absolute => write!(f, "the path is absolute"),
            Self::Backslash => write!(f, "the path holds a backslash"),
            Self::ControlChar(c) => {
                write!(
                    f,
                    "the path holds control character U+{:04X}",
                    u32::from(*c)
                )
            }
            Self::TooLong(len) => write!(f, "the path is {len} bytes, over {MAX_RELPATH_BYTES}"),
            Self::EmptyComponent => write!(f, "the path has an empty component"),
            Self::DotComponent => write!(f, "the path has a `.` or `..` component"),
        }
    }
}

impl std::error::Error for PathSyntaxError {}

fn is_control(c: char) -> bool {
    matches!(u32::from(c), 0..=0x1f | 0x7f | 0x80..=0x9f)
}

/// Check a request path relative to `doc/` and return it repo-relative (`doc/<raw>`).
pub fn check_relpath_syntax(raw: &str) -> Result<PathBuf, PathSyntaxError> {
    if raw.is_empty() {
        return Err(PathSyntaxError::Empty);
    }
    if raw.len() > MAX_RELPATH_BYTES {
        return Err(PathSyntaxError::TooLong(raw.len()));
    }
    if raw.starts_with('/') {
        return Err(PathSyntaxError::Absolute);
    }
    if raw.contains('\\') {
        return Err(PathSyntaxError::Backslash);
    }
    if let Some(c) = raw.chars().find(|c| is_control(*c)) {
        return Err(PathSyntaxError::ControlChar(c));
    }
    for part in raw.split('/') {
        match part {
            "" => return Err(PathSyntaxError::EmptyComponent),
            "." | ".." => return Err(PathSyntaxError::DotComponent),
            _ => {}
        }
    }
    Ok(Path::new("doc").join(raw))
}

/// The first prefix of `rel` below `root` that is a symlink (lstat, no follow), repo-relative.
/// `None` when there is none, or when a component is missing or not a directory, since
/// nothing past it exists to be a link. A `..`, `.` or root component is an error.
pub fn symlink_component(root: &Path, rel: &Path) -> Result<Option<PathBuf>> {
    let mut prefix = PathBuf::new();
    for part in rel.components() {
        let Component::Normal(part) = part else {
            bail!("not a plain relative path: {}", rel.display());
        };
        prefix.push(part);
        let meta = match std::fs::symlink_metadata(root.join(&prefix)) {
            Ok(meta) => meta,
            Err(error) if error.kind() == ErrorKind::NotFound => return Ok(None),
            Err(error) => {
                return Err(error).with_context(|| format!("cannot stat {}", prefix.display()))
            }
        };
        if meta.file_type().is_symlink() {
            return Ok(Some(prefix));
        }
        if !meta.is_dir() {
            return Ok(None);
        }
    }
    Ok(None)
}
