//! The source graph of a worktree as it stands on disk, built in memory.
//!
//! Verification judges the tree a stage leaves behind, which no published
//! layer describes: a base layer is a committed revision, and a stage sandbox
//! cannot write the shared cache or an overlay. [`build_for_worktree`]
//! therefore reads the published base layer nearest to `HEAD` among its
//! ancestors, re-extracts every file that differs from it, and resolves the
//! result. Nothing is written: no `ensure_snapshot`, `publish_base` or
//! `save_overlay`, and only git commands that never refresh the index.
//!
//! Without a usable base layer it extracts every source file under the
//! working directory instead and says so in [`WorktreeGraph::degraded`].

use anyhow::{Context, Result};
use std::collections::{BTreeMap, BTreeSet};
use std::io::ErrorKind;
use std::path::{Path, PathBuf};

use crate::context::extract::{self, extract_file};
use crate::context::graph_store::{FileEntry, GraphStore, ResolvedGraph};
use crate::context::refresh::BoxedExtractor;
use crate::context::resolve_graph;
use crate::context::source_graph::{body_hash, FileCoverage, MAX_EXTRACTED_FILE_BYTES};
use crate::context::store::CACHE_RELATIVE_DIR;
use crate::fs::safe_read::read_bounded;
use crate::fs::work_dir::WorkDir;
use crate::git::branch::{commits_ahead_of, is_ancestor_of};
use crate::git::runner::run_git_checked;
use crate::git::worktree::is_worktree_scaffold_path;

/// A worktree's resolved source graph and what it was built from.
#[derive(Debug, Clone, PartialEq)]
pub struct WorktreeGraph {
    /// The base layer with every changed file re-extracted, after
    /// [`resolve_graph`].
    pub graph: ResolvedGraph,
    /// One-line reason the graph was extracted from scratch rather than
    /// layered on a base; `None` when a base layer was used.
    pub degraded: Option<String>,
    /// Worktree-relative paths that differ from the base revision, untracked
    /// files included and worktree scaffold excluded.
    pub changed: Vec<PathBuf>,
}

/// Build the graph of the worktree containing `working_dir`, discovering
/// everything with read-only git.
///
/// The worktree is `--show-toplevel`; the project root, where the shared cache
/// lives, is the parent of the absolute `--git-common-dir`; the base revision
/// is the published base layer nearest to `HEAD` among its ancestors, or
/// `HEAD` itself when there is none (which then degrades).
pub fn build_for_worktree(working_dir: &Path) -> Result<WorktreeGraph> {
    let toplevel = run_git_checked(&["rev-parse", "--show-toplevel"], working_dir)?;
    let worktree = PathBuf::from(toplevel);
    let common_dir = PathBuf::from(run_git_checked(
        &["rev-parse", "--path-format=absolute", "--git-common-dir"],
        working_dir,
    )?);
    let project_root = common_dir
        .parent()
        .with_context(|| format!("git common dir has no parent: {}", common_dir.display()))?;
    let base_dir = read_only_store(project_root)?.base_dir();
    let base_revision = match nearest_ancestor_base(&base_dir, &worktree)? {
        Some(revision) => revision,
        None => run_git_checked(&["rev-parse", "HEAD"], &worktree)?,
    };
    let changed = changed_paths(&worktree, &base_revision)?;
    build_worktree_graph(
        project_root,
        &worktree,
        working_dir,
        &base_revision,
        &changed,
    )
}

/// Build the graph from the base layer published for `base_revision` with
/// every path in `changed` (worktree-relative) re-read from `worktree`.
///
/// When no base layer exists for `base_revision`, every tracked or untracked
/// source file under `working_dir` is extracted instead and
/// [`WorktreeGraph::degraded`] names the reason.
pub fn build_worktree_graph(
    project_root: &Path,
    worktree: &Path,
    working_dir: &Path,
    base_revision: &str,
    changed: &[PathBuf],
) -> Result<WorktreeGraph> {
    let extractors = extract::registry();
    let (mut graph, degraded) = match read_only_store(project_root)?.load_base(base_revision)? {
        Some(layer) => {
            let mut graph = ResolvedGraph {
                base_revision: layer.revision,
                overlaid: BTreeSet::new(),
                files: layer.files,
            };
            for path in changed {
                if let Some(key) = apply_file(&mut graph.files, worktree, path, &extractors) {
                    graph.overlaid.insert(key);
                }
            }
            (graph, None)
        }
        None => {
            let graph = from_scratch(worktree, working_dir, &extractors)?;
            let reason = format!(
                "no source-graph base layer is published for {base_revision}; extracted {} \
                 source files under {} instead",
                graph.files.len(),
                working_dir.display()
            );
            (graph, Some(reason))
        }
    };
    resolve_graph(&mut graph);
    Ok(WorktreeGraph {
        graph,
        degraded,
        changed: changed.to_vec(),
    })
}

/// A store over `project_root`'s shared cache. Constructing one touches
/// nothing on disk, and this module only ever calls its readers.
fn read_only_store(project_root: &Path) -> Result<GraphStore> {
    let work_dir = WorkDir::new(project_root)?;
    Ok(GraphStore::new(
        &project_root.join(CACHE_RELATIVE_DIR),
        work_dir.root(),
    ))
}

/// The published base revision nearest to `HEAD` among its ancestors.
fn nearest_ancestor_base(base_dir: &Path, worktree: &Path) -> Result<Option<String>> {
    let mut ancestors = Vec::new();
    for revision in published_revisions(base_dir)? {
        // A revision git cannot resolve here is unusable, not an error: a
        // base can outlive the commit it describes.
        if matches!(is_ancestor_of(&revision, "HEAD", worktree), Ok(true)) {
            let distance = commits_ahead_of("HEAD", &revision, worktree)?;
            ancestors.push((distance, revision));
        }
    }
    Ok(ancestors.into_iter().min().map(|(_, revision)| revision))
}

/// Revisions that have a base layer file in `base_dir`. Only object-id stems
/// are kept, so nothing else found on disk reaches a git argument.
fn published_revisions(base_dir: &Path) -> Result<Vec<String>> {
    let entries = match std::fs::read_dir(base_dir) {
        Ok(entries) => entries,
        Err(error) if error.kind() == ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => {
            return Err(error)
                .with_context(|| format!("Failed to list base layers: {}", base_dir.display()));
        }
    };
    Ok(entries
        .flatten()
        .filter_map(|entry| {
            let name = entry.file_name().into_string().ok()?;
            let stem = name.strip_suffix(".json")?;
            is_object_id(stem).then(|| stem.to_string())
        })
        .collect())
}

fn is_object_id(value: &str) -> bool {
    matches!(value.len(), 40 | 64) && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

/// Worktree-relative paths that differ from `base_revision`, minus the
/// worktree scaffold: committed, staged and unstaged changes, then untracked
/// files. Plumbing `diff-index` never refreshes the index, so it writes
/// nothing; the cost is that a stat-dirty but unchanged file is listed too.
/// Without rename detection a move lists both its old and its new path.
fn changed_paths(worktree: &Path, base_revision: &str) -> Result<Vec<PathBuf>> {
    let diff = run_git_checked(
        &["diff-index", "--name-only", "-z", base_revision, "--"],
        worktree,
    )?;
    let untracked = run_git_checked(
        &["ls-files", "-z", "--others", "--exclude-standard"],
        worktree,
    )?;
    let paths: BTreeSet<&str> = nul_separated(&diff)
        .chain(nul_separated(&untracked))
        .filter(|path| !is_worktree_scaffold_path(path))
        .collect();
    Ok(paths.into_iter().map(PathBuf::from).collect())
}

/// Every tracked or untracked file under `working_dir` an extractor claims,
/// extracted from `worktree`.
fn from_scratch(
    worktree: &Path,
    working_dir: &Path,
    extractors: &[BoxedExtractor],
) -> Result<ResolvedGraph> {
    let listing = run_git_checked(
        &[
            "ls-files",
            "-z",
            "--full-name",
            "--cached",
            "--others",
            "--exclude-standard",
        ],
        working_dir,
    )?;
    let mut graph = ResolvedGraph::default();
    for path in nul_separated(&listing)
        .filter(|path| !is_worktree_scaffold_path(path))
        .map(Path::new)
        .filter(|path| extractors.iter().any(|extractor| extractor.supports(path)))
    {
        apply_file(&mut graph.files, worktree, path, extractors);
    }
    Ok(graph)
}

/// Replace `path`'s entry with a fresh extraction of the worktree copy, or
/// drop it when the worktree holds no regular file there. Returns the key
/// written, `None` when the entry was dropped.
fn apply_file(
    files: &mut BTreeMap<String, FileEntry>,
    worktree: &Path,
    path: &Path,
    extractors: &[BoxedExtractor],
) -> Option<String> {
    let key = path.to_string_lossy().into_owned();
    let entry = match read_source(worktree, path) {
        Ok(None) => {
            files.remove(&key);
            return None;
        }
        Ok(Some(bytes)) => extracted_entry(extractors, path, &bytes),
        Err(error) => unreadable_entry(&error),
    };
    files.insert(key.clone(), entry);
    Some(key)
}

/// The bytes of the regular file at `path` beneath `worktree`, `None` when
/// the path is absent or not a regular file (deleted, a symlink, a
/// submodule), which the refresh path never treats as source either. The
/// read refuses a symlink at any component and anything over the extraction
/// cap.
fn read_source(worktree: &Path, path: &Path) -> Result<Option<Vec<u8>>> {
    match std::fs::symlink_metadata(worktree.join(path)) {
        Ok(metadata) if metadata.is_file() => {
            read_bounded(worktree, path, MAX_EXTRACTED_FILE_BYTES).map(Some)
        }
        Ok(_) => Ok(None),
        Err(error) if error.kind() == ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error).with_context(|| format!("Failed to inspect {}", path.display())),
    }
}

fn extracted_entry(extractors: &[BoxedExtractor], path: &Path, bytes: &[u8]) -> FileEntry {
    let extraction = extract_file(extractors, path, bytes);
    FileEntry {
        content_hash: body_hash(bytes),
        nodes: extraction.nodes,
        edges: extraction.edges,
        coverage: extraction.coverage,
    }
}

/// A file that could not be read keeps an entry naming why, the shape the
/// refresh path gives it, so it is reported as degraded rather than omitted.
fn unreadable_entry(error: &anyhow::Error) -> FileEntry {
    FileEntry {
        coverage: FileCoverage::LexicalOnly {
            detail: format!("unreadable: {error:#}"),
        },
        ..FileEntry::default()
    }
}

fn nul_separated(output: &str) -> impl Iterator<Item = &str> {
    output.split('\0').filter(|value| !value.is_empty())
}

#[cfg(test)]
#[path = "worktree_graph_tests.rs"]
mod tests;
