//! Document discovery under `doc/`.
//!
//! The walk lists directories from a dirfd without following symlinks, because sessions can
//! write `doc/`. A symlink is kept only when it resolves to a regular file inside the
//! canonical `doc/` and outside the summary store.

use std::collections::BTreeMap;
use std::fs::Metadata;
use std::os::unix::fs::MetadataExt;
use std::path::{Component, Path, PathBuf};

use anyhow::{bail, Context, Result};

use crate::fs::safe_fs::safe_open_dirfd;
use crate::fs::safe_read::{list_dir_no_follow, read_bounded, EntryKind};
use crate::planning::kinds::{classify, doc_id, DocId, DocKind};
use crate::planning::slug::{resolve_plans, PlanEntry};

/// Largest document `read_doc` returns.
pub const MAX_DOC_BYTES: usize = 8 * 1024 * 1024;
/// Deepest directory level below `doc/` the walk descends into.
const MAX_DEPTH: usize = 16;
/// Most directory entries the walk lists before it gives up.
const MAX_ENTRIES: usize = 20_000;
const SUMMARIES: &str = "doc/loom/summaries";

/// One file under `doc/`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Document {
    /// Repo-relative path, starting `doc/`.
    pub path: PathBuf,
    /// The role inferred from the path.
    pub kind: DocKind,
    /// Stable identity of the document.
    pub id: DocId,
    /// Size in bytes (the target's, for a symlink).
    pub size: u64,
    /// True when the path is a symlink to a file inside `doc/`.
    pub via_symlink: bool,
}

/// Whether `doc/` could be walked.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DocDirState {
    /// `doc/` is a real directory.
    Present,
    /// `doc/` does not exist (or is not a directory).
    Missing,
    /// `doc/` is a symlink and is treated as missing.
    Symlinked,
}

/// Every document under `doc/` and the plans among them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DocTree {
    /// All documents, sorted by path.
    pub docs: Vec<Document>,
    /// Plans by slug, the most advanced copy winning.
    pub plans: BTreeMap<String, PlanEntry>,
    /// State of the `doc/` directory itself.
    pub doc_dir: DocDirState,
}

impl DocTree {
    /// The document at repo-relative `rel`.
    pub fn get(&self, rel: &Path) -> Option<&Document> {
        let index = self
            .docs
            .binary_search_by(|d| d.path.as_path().cmp(rel))
            .ok()?;
        self.docs.get(index)
    }

    /// The document with the given id string (e.g. `plan/foo`).
    pub fn by_id(&self, id: &str) -> Option<&Document> {
        self.docs.iter().find(|d| d.id.0 == id)
    }

    /// The plan with the given slug.
    pub fn plan(&self, slug: &str) -> Option<&PlanEntry> {
        self.plans.get(slug)
    }
}

/// Which skip rules the walk applies to the summary store.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum WalkScope {
    /// Skip `doc/loom/summaries/` (discovery).
    Documents,
    /// Walk it too (the watcher); every other skip rule is unchanged.
    IncludeSummaries,
}

/// One file the walk found, with the metadata the watcher compares.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct WalkEntry {
    pub rel: PathBuf,
    pub size: u64,
    pub mtime_ns: i128,
    pub inode: u64,
    pub via_symlink: bool,
}

fn is_editor_temporary(name: &str) -> bool {
    name.starts_with('.') || name.ends_with('~') || name.ends_with(".swp")
}

fn doc_dir_state(root: &Path) -> DocDirState {
    match std::fs::symlink_metadata(root.join("doc")) {
        Ok(meta) if meta.file_type().is_symlink() => DocDirState::Symlinked,
        Ok(meta) if meta.is_dir() => DocDirState::Present,
        _ => DocDirState::Missing,
    }
}

/// The metadata of the canonical target of `path` when it is a regular file inside
/// `canon_doc` and outside the summary store; the target's path with it.
fn allowed_link_target(canon_doc: &Path, path: &Path) -> Option<(PathBuf, Metadata)> {
    let target = std::fs::canonicalize(path).ok()?;
    let meta = std::fs::symlink_metadata(&target).ok()?;
    let inside = target.starts_with(canon_doc)
        && !target.starts_with(canon_doc.join("loom").join("summaries"));
    (meta.is_file() && inside).then_some((target, meta))
}

fn entry_of(rel: PathBuf, meta: &Metadata, via_symlink: bool) -> WalkEntry {
    let mtime_ns = i128::from(meta.mtime()) * 1_000_000_000 + i128::from(meta.mtime_nsec());
    WalkEntry {
        rel,
        size: meta.len(),
        mtime_ns,
        inode: meta.ino(),
        via_symlink,
    }
}

/// The entry of a regular file or an allowed symlink; `None` when it vanished or the link is
/// not allowed.
fn file_entry(root: &Path, canon_doc: &Path, rel: PathBuf, kind: EntryKind) -> Option<WalkEntry> {
    if kind == EntryKind::Symlink {
        let (_, meta) = allowed_link_target(canon_doc, &root.join(&rel))?;
        return Some(entry_of(rel, &meta, true));
    }
    let meta = std::fs::symlink_metadata(root.join(&rel)).ok()?;
    Some(entry_of(rel, &meta, false))
}

/// Walk `doc/` without following symlinks. A missing or symlinked `doc/` yields no entries.
pub(crate) fn walk_doc(root: &Path, scope: WalkScope) -> Result<(DocDirState, Vec<WalkEntry>)> {
    let state = doc_dir_state(root);
    if state != DocDirState::Present {
        return Ok((state, Vec::new()));
    }
    let canon_doc = std::fs::canonicalize(root.join("doc")).context("cannot resolve doc/")?;
    let fd = safe_open_dirfd(root)?;
    let mut found = Vec::new();
    let mut listed = 0usize;
    let mut pending = vec![(PathBuf::from("doc"), 0usize)];
    while let Some((dir, depth)) = pending.pop() {
        if depth > MAX_DEPTH {
            bail!(
                "doc/ nests deeper than {MAX_DEPTH} directory levels at {}",
                dir.display()
            );
        }
        let entries = list_dir_no_follow(&fd, &dir)?;
        listed += entries.len();
        if listed > MAX_ENTRIES {
            bail!("doc/ holds more than {MAX_ENTRIES} entries");
        }
        for (name, kind) in entries {
            let Some(name) = name.to_str().filter(|n| !is_editor_temporary(n)) else {
                continue;
            };
            let rel = dir.join(name);
            match kind {
                EntryKind::Directory => {
                    if scope == WalkScope::Documents && rel == Path::new(SUMMARIES) {
                        continue;
                    }
                    pending.push((rel, depth + 1));
                }
                EntryKind::File | EntryKind::Symlink => {
                    found.extend(file_entry(root, &canon_doc, rel, kind));
                }
                EntryKind::Special => {}
            }
        }
    }
    found.sort_by(|a, b| a.rel.cmp(&b.rel));
    Ok((state, found))
}

/// Walk `doc/` and classify every document.
pub fn discover(root: &Path) -> Result<DocTree> {
    let (doc_dir, entries) = walk_doc(root, WalkScope::Documents)?;
    let docs: Vec<Document> = entries
        .into_iter()
        .map(|e| {
            let kind = classify(&e.rel);
            let id = doc_id(&e.rel, &kind);
            Document {
                path: e.rel,
                kind,
                id,
                size: e.size,
                via_symlink: e.via_symlink,
            }
        })
        .collect();
    let plan_paths = docs
        .iter()
        .filter(|d| matches!(d.kind, DocKind::Plan { .. }))
        .map(|d| d.path.clone());
    let plans = resolve_plans(plan_paths);
    Ok(DocTree {
        docs,
        plans,
        doc_dir,
    })
}

/// Read a document as UTF-8; see `read_doc_bytes`. Invalid UTF-8 is an error naming the path.
pub fn read_doc(root: &Path, rel: &Path) -> Result<String> {
    String::from_utf8(read_doc_bytes(root, rel)?)
        .with_context(|| format!("{} is not valid UTF-8", rel.display()))
}

/// Read a document's bytes, no-follow and bounded by [`MAX_DOC_BYTES`]. An allowed symlink
/// is re-resolved and its target opened no-follow from the canonical root, so a swap
/// between the check and the open fails the open.
pub(crate) fn read_doc_bytes(root: &Path, rel: &Path) -> Result<Vec<u8>> {
    if !rel.starts_with("doc") || rel.components().any(|c| !matches!(c, Component::Normal(_))) {
        bail!("{} is not a path under doc/", rel.display());
    }
    let meta = std::fs::symlink_metadata(root.join(rel))
        .with_context(|| format!("cannot stat {}", rel.display()))?;
    if !meta.file_type().is_symlink() {
        return read_bounded(root, rel, MAX_DOC_BYTES);
    }
    if doc_dir_state(root) != DocDirState::Present {
        bail!("doc/ is not a real directory");
    }
    let canon_doc = std::fs::canonicalize(root.join("doc"))?;
    let Some((target, _)) = allowed_link_target(&canon_doc, &root.join(rel)) else {
        bail!(
            "{} is a symlink that does not resolve to a file inside doc/",
            rel.display()
        );
    };
    let canon_root = std::fs::canonicalize(root)?;
    let within = target
        .strip_prefix(&canon_root)
        .context("symlink target is outside the root")?;
    read_bounded(&canon_root, within, MAX_DOC_BYTES)
}
