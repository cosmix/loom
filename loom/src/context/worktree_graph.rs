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

use crate::context::extract::{self, extractor_for, Lookup};
use crate::context::graph_store::{FileEntry, GraphLayer, GraphStore, ResolvedGraph};
use crate::context::refresh::snapshot::parser_version_matches;
use crate::context::refresh::BoxedExtractor;
use crate::context::source_graph::read::{read_source as read_snapshot, SourceRead};
use crate::context::source_graph::FileCoverage;
use crate::context::store::CACHE_RELATIVE_DIR;
use crate::context::view::{build_cold, relink, ResolvedView, ViewIdentity};
use crate::fs::work_dir::WorkDir;
use crate::git::worktree::{is_worktree_scaffold_path, WorktreeGit};
use crate::verify::review::fingerprint::checkout_git;

/// Overlay generation of the in-memory view a worktree graph is built as, so
/// it can never be mistaken for a persisted base view.
const WORKTREE_VIEW_GENERATION: &str = "worktree";

/// A worktree's resolved source graph and what it was built from.
#[derive(Debug, Clone, PartialEq)]
pub struct WorktreeGraph {
    /// The base layer with every changed file re-extracted and every
    /// cross-file edge resolved.
    pub graph: ResolvedGraph,
    /// One-line reason the graph was extracted from scratch rather than
    /// layered on a base; `None` when a base layer was used.
    pub degraded: Option<String>,
    /// Worktree-relative paths that differ from the base revision, untracked
    /// files included and worktree scaffold excluded.
    pub changed: Vec<PathBuf>,
}

/// Build the graph of the worktree containing `working_dir`, with read-only
/// git through `checkout_git`: as the worktree's own `.git` directs it, or,
/// for a stage worktree, pinned to the git directory the project registered
/// for it. Verification runs outside the stage's sandbox, where a `.git` file
/// the stage repointed would otherwise choose the toplevel, the base revision
/// and the changed set, and run the configuration of the directory it names.
///
/// The worktree is the checkout's root; the project root, where the shared
/// cache lives, is the parent of the absolute `--git-common-dir`; the base
/// revision is the published base layer nearest to `HEAD` among its
/// ancestors, or `HEAD` itself when there is none (which then degrades).
pub fn build_for_worktree(working_dir: &Path) -> Result<WorktreeGraph> {
    let git = checkout_git(working_dir)?;
    let common_dir =
        git.run_checked(&["rev-parse", "--path-format=absolute", "--git-common-dir"])?;
    let common_dir = PathBuf::from(common_dir.trim());
    let project_root = common_dir
        .parent()
        .with_context(|| format!("git common dir has no parent: {}", common_dir.display()))?;
    let base_dir = read_only_store(project_root)?.base_dir();
    let base_revision = match nearest_ancestor_base(&base_dir, &git)? {
        Some(revision) => revision,
        None => git.run_checked(&["rev-parse", "HEAD"])?.trim().to_string(),
    };
    let changed = changed_paths(&git, &base_revision)?;
    build_worktree_graph(project_root, &git, working_dir, &base_revision, &changed)
}

/// Build the graph from the base layer published for `base_revision` with
/// every path in `changed` (worktree-relative) re-read from `git`'s worktree.
///
/// When no base layer exists for `base_revision`, every tracked or untracked
/// source file under `working_dir` is extracted instead and
/// [`WorktreeGraph::degraded`] names the reason.
pub fn build_worktree_graph(
    project_root: &Path,
    git: &WorktreeGit,
    working_dir: &Path,
    base_revision: &str,
    changed: &[PathBuf],
) -> Result<WorktreeGraph> {
    let extractors = extract::registry();
    let worktree = git.work_tree();
    let store = read_only_store(project_root)?;
    // A base written under another schema counts as missing.
    let base = store
        .load_base(base_revision)?
        .filter(GraphLayer::has_current_schema);
    let (graph, degraded, seed) = match base {
        Some(layer) => {
            let seed = base_view(&store, base_revision, &layer, &extractors);
            let graph = layered_graph(layer, worktree, changed, &extractors);
            (graph, None, seed)
        }
        None => {
            let graph = from_scratch(git, working_dir, &extractors)?;
            let reason = format!(
                "no source-graph base layer is published for {base_revision}; extracted {} \
                 source files under {} instead",
                graph.files.len(),
                working_dir.display()
            );
            (graph, Some(reason), None)
        }
    };
    // Built in memory only: this graph is never persisted.
    let identity = ViewIdentity::current(base_revision, WORKTREE_VIEW_GENERATION);
    let view = match seed {
        Some(previous) => relink(&previous, graph, identity),
        None => build_cold(graph, identity),
    };
    Ok(WorktreeGraph {
        graph: view.graph,
        degraded,
        changed: changed.to_vec(),
    })
}

/// `layer` with every path in `changed` re-extracted from `worktree`, each
/// path that was re-extracted recorded as overlaid.
fn layered_graph(
    layer: GraphLayer,
    worktree: &Path,
    changed: &[PathBuf],
    extractors: &[BoxedExtractor],
) -> ResolvedGraph {
    let mut graph = ResolvedGraph {
        base_revision: layer.revision,
        overlaid: BTreeSet::new(),
        files: layer.files,
    };
    // An entry stamped by an extractor other than the current one is stale
    // even when its file did not change: re-extract it.
    let mut to_extract: BTreeSet<PathBuf> = changed.iter().cloned().collect();
    to_extract.extend(stale_paths(&graph.files, extractors).map(PathBuf::from));
    for path in &to_extract {
        if let Some(key) = apply_file(&mut graph.files, worktree, path, extractors) {
            graph.overlaid.insert(key);
        }
    }
    graph
}

/// The persisted view of `layer`'s base, which a relink starts from. `None`
/// when there is no such view or an entry needs re-extracting: relink copies
/// an unchanged file's previous entry, which would keep the stale one.
fn base_view(
    store: &GraphStore,
    base_revision: &str,
    layer: &GraphLayer,
    extractors: &[BoxedExtractor],
) -> Option<ResolvedView> {
    if stale_paths(&layer.files, extractors).next().is_some() {
        return None;
    }
    store.load_view(&ViewIdentity::current(base_revision, ""), None)
}

/// Paths whose entry was stamped by an extractor other than the current one.
fn stale_paths<'a>(
    files: &'a BTreeMap<String, FileEntry>,
    extractors: &'a [BoxedExtractor],
) -> impl Iterator<Item = &'a String> + 'a {
    files
        .iter()
        .filter(|(path, entry)| !parser_version_matches(entry, extractors, Path::new(path)))
        .map(|(path, _)| path)
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
fn nearest_ancestor_base(base_dir: &Path, git: &WorktreeGit) -> Result<Option<String>> {
    let mut ancestors = Vec::new();
    for revision in published_revisions(base_dir)? {
        if let Some(distance) = distance_to_head(git, &revision)? {
            ancestors.push((distance, revision));
        }
    }
    Ok(ancestors.into_iter().min().map(|(_, revision)| revision))
}

/// How many commits `HEAD` has beyond `revision`; `None` when `revision` is
/// no ancestor of `HEAD` or git cannot resolve it, which makes it unusable
/// rather than an error: a base can outlive the commit it describes.
fn distance_to_head(git: &WorktreeGit, revision: &str) -> Result<Option<usize>> {
    let ancestry = git.run(&["merge-base", "--is-ancestor", revision, "HEAD"]);
    if !ancestry.is_ok_and(|output| output.status.success()) {
        return Ok(None);
    }
    let range = format!("{revision}..HEAD");
    let count = git.run_checked(&["rev-list", "--count", &range])?;
    let count = count
        .trim()
        .parse()
        .with_context(|| format!("git rev-list --count {range} printed {count:?}"))?;
    Ok(Some(count))
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
fn changed_paths(git: &WorktreeGit, base_revision: &str) -> Result<Vec<PathBuf>> {
    let diff = git.run_checked(&["diff-index", "--name-only", "-z", base_revision, "--"])?;
    let untracked = git.run_checked(&["ls-files", "-z", "--others", "--exclude-standard"])?;
    let paths: BTreeSet<&str> = nul_separated(&diff)
        .chain(nul_separated(&untracked))
        .filter(|path| !is_worktree_scaffold_path(path))
        .collect();
    Ok(paths.into_iter().map(PathBuf::from).collect())
}

/// Every tracked or untracked file under `working_dir` an extractor claims,
/// extracted from `git`'s worktree.
fn from_scratch(
    git: &WorktreeGit,
    working_dir: &Path,
    extractors: &[BoxedExtractor],
) -> Result<ResolvedGraph> {
    let mut args = vec![
        "ls-files",
        "-z",
        "--full-name",
        "--cached",
        "--others",
        "--exclude-standard",
    ];
    // Git runs at the worktree root, so the directory is a pathspec.
    let scope = scope_pathspec(git, working_dir)?;
    if let Some(scope) = &scope {
        args.extend(["--", scope.as_str()]);
    }
    let listing = git.run_checked(&args)?;
    let worktree = git.work_tree();
    let mut graph = ResolvedGraph::default();
    for path in nul_separated(&listing)
        .filter(|path| !is_worktree_scaffold_path(path))
        .map(Path::new)
        .filter(|path| matches!(extractor_for(extractors, path), Lookup::Extractor(_)))
    {
        apply_file(&mut graph.files, worktree, path, extractors);
    }
    Ok(graph)
}

/// `working_dir` as a literal pathspec relative to `git`'s worktree root;
/// `None` for the root itself.
fn scope_pathspec(git: &WorktreeGit, working_dir: &Path) -> Result<Option<String>> {
    let directory = working_dir
        .canonicalize()
        .with_context(|| format!("Failed to resolve {}", working_dir.display()))?;
    let relative = directory.strip_prefix(git.work_tree()).with_context(|| {
        format!(
            "{} is outside the worktree {}",
            directory.display(),
            git.work_tree().display()
        )
    })?;
    if relative.as_os_str().is_empty() {
        return Ok(None);
    }
    let relative = relative
        .to_str()
        .with_context(|| format!("{} is not valid UTF-8", relative.display()))?;
    Ok(Some(format!(":(literal){relative}")))
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
        Ok(Some(source)) => source.entry(extractors, path),
        Err(error) => unreadable_entry(&error),
    };
    files.insert(key.clone(), entry);
    Some(key)
}

/// The bytes of the regular file at `path` beneath `worktree`, `None` when
/// the path is absent or not a regular file (deleted, a symlink, a
/// submodule), which the refresh path never treats as source either. The
/// read refuses symlinks and retains only capped source bytes while hashing
/// the complete file, exactly as the refresh path does.
fn read_source(worktree: &Path, path: &Path) -> Result<Option<SourceRead>> {
    match std::fs::symlink_metadata(worktree.join(path)) {
        Ok(metadata) if metadata.is_file() => read_snapshot(worktree, path).map(Some),
        Ok(_) => Ok(None),
        Err(error) if error.kind() == ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error).with_context(|| format!("Failed to inspect {}", path.display())),
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
