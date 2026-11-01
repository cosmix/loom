//! Index project prose (`doc/**/*.md` by default) into the same structural
//! catalog the curated knowledge tree feeds.
//!
//! Today, a file with no tree-sitter grammar produces only a bare file node
//! (`crate::context::extract::lexical`) and the source ranker drops file
//! nodes outright, so design docs, proposals and plans under `doc/` are
//! unreachable by retrieval no matter how relevant their content. This module
//! closes that gap the same way the knowledge tree already is reachable:
//! chunk prose files with the existing heading chunker
//! ([`crate::fs::knowledge::chunker::chunk_file`]) and fold the result into
//! [`crate::fs::knowledge::catalog::Catalog::chunks`].
//!
//! ## Why the `prose:` prefix
//!
//! Every chunk id this module produces is prefixed [`PROSE_ID_PREFIX`]. Two
//! independent reasons make that non-negotiable:
//!
//! - **Collision-proofing.** A curated chunk id is derived from a path
//!   relative to the *knowledge root* (`architecture.md#topic#0`); a prose
//!   chunk id is derived from a path relative to the *project root*
//!   (`doc/design.md#topic#0`). Two different roots can produce the same
//!   relative path for unrelated files, and a curated author cannot see, let
//!   alone avoid, what an arbitrary project doc happens to be named. The
//!   prefix makes the two id spaces disjoint by construction instead of by
//!   convention.
//! - **Channel-of-origin signal.** The Knowledge Brief a stage or prompt
//!   receives renders chunk ids as-is (see `pack.rs`'s item rendering). A
//!   reader who sees `prose:doc/design.md#...` knows immediately this is an
//!   indexed project doc, not curated, reviewed knowledge — the same
//!   distinction [`crate::context::schema::LifecycleState`] draws for staleness,
//!   drawn here for provenance instead.
//!
//! Catalog building and structural fingerprinting resolve prose roots through
//! [`sources_for_knowledge_root`]. Keep their configuration and exclusion
//! policy consistent.

use crate::context::config::RetrievalConfig;
use crate::context::schema::LifecycleState;
use crate::context::source_graph::MAX_EXTRACTED_FILE_BYTES;
use crate::fs::knowledge::chunker::{chunk_file, KnowledgeChunk};
use crate::fs::safe_fs::{open_safely, safe_open_dirfd};
use crate::fs::safe_read::read_bounded_at;
use crate::fs::work_dir::WorkDir;
use std::os::fd::{AsRawFd, OwnedFd};
use std::path::{Path, PathBuf};

#[path = "prose_walk.rs"]
mod walk;

#[cfg(test)]
#[path = "tests_prose_safety.rs"]
mod tests_safety;

/// Prefix on every prose chunk id, and on every prose entry in
/// `tree_revision`'s fingerprint list. See the module doc for why this must
/// exist and why it must be checked at both sites.
pub(crate) const PROSE_ID_PREFIX: &str = "prose:";

/// Where prose lives for one project, resolved once so `catalog::build` and
/// `fingerprint_tree` can never disagree about what is indexed.
///
/// Built only by [`sources_for_knowledge_root`] — there is no public
/// constructor, because every field here is meaningless unless it was
/// derived consistently from the same `knowledge_root`.
pub(crate) struct ProseSources {
    /// Project root the relative paths in [`ProseSources::files`] are
    /// resolved against.
    project_root: PathBuf,
    /// The knowledge root this instance was resolved for. Any directory
    /// equal to this is skipped entirely during the walk — see
    /// [`ProseSources::files`].
    knowledge_root: PathBuf,
    /// Configured prose roots relative to the anchored project directory.
    roots: Vec<PathBuf>,
    project_fd: OwnedFd,
}

/// The project root a `doc/loom/knowledge` path belongs to. Moved here from
/// `catalog.rs` (previously a private `fn project_root`) so the catalog
/// build and the tree fingerprint derive it identically, through this one
/// function, rather than keeping two copies of the same three-parent walk.
pub(crate) fn project_root_of(knowledge_root: &Path) -> Option<PathBuf> {
    let loom = knowledge_root.parent()?;
    let doc = loom.parent()?;
    let project = doc.parent()?;
    (knowledge_root.file_name()? == "knowledge"
        && loom.file_name()? == "loom"
        && doc.file_name()? == "doc")
        .then(|| project.to_path_buf())
}

/// Resolve the prose sources for one knowledge root.
///
/// Returns `None` when no project root can be derived from `knowledge_root`
/// (see [`project_root_of`]), or when every configured `prose_roots` entry
/// is absent or unsafe — including the documented opt-out `prose_roots = []`,
/// which performs no walk or fingerprinting.
pub(crate) fn sources_for_knowledge_root(knowledge_root: &Path) -> Option<ProseSources> {
    let project_root = project_root_of(knowledge_root)?;
    let config = RetrievalConfig::load(&config_root(&project_root));
    if config.prose_roots.is_empty() {
        return None;
    }
    let project_fd = match safe_open_dirfd(&project_root) {
        Ok(fd) => fd,
        Err(error) => {
            tracing::debug!(path = %project_root.display(), %error, "skipping unreadable prose project root");
            return None;
        }
    };
    // Check every root component from the project descriptor. A configured
    // symlink must not bypass the descendant traversal's no-follow policy.
    let roots: Vec<PathBuf> = config
        .prose_roots
        .iter()
        .map(|relative| {
            let path = Path::new(relative);
            if path == Path::new(".") { PathBuf::new() } else { path.to_path_buf() }
        })
        .filter(|relative| {
            if relative.as_os_str().is_empty() {
                return true;
            }
            let flags = libc::O_RDONLY | libc::O_DIRECTORY | libc::O_NONBLOCK;
            match open_safely(project_fd.as_raw_fd(), relative, flags, 0) {
                Ok(_) => true,
                Err(error) => {
                    tracing::debug!(path = %relative.display(), %error, "skipping unsafe prose root");
                    false
                }
            }
        })
        .collect();
    if roots.is_empty() {
        return None;
    }
    Some(ProseSources {
        project_root,
        knowledge_root: knowledge_root.to_path_buf(),
        roots,
        project_fd,
    })
}

/// The root whose `.loom/config.toml` governs prose indexing: the MAIN
/// project root when this checkout is a linked worktree (its `.loom/work` is
/// a symlink into the main repository), otherwise the checkout itself.
///
/// Without this, a stage running in `.worktrees/<id>/` would silently fall
/// back to DEFAULT prose roots while the host ran on the operator's — and
/// since both share one context store, the tree revision would flap between
/// them and every retrieval on either side would rebuild the catalog the
/// other just built.
///
/// Deliberately not `WorkDir::new(project_root)` directly, which searches
/// UPWARD from `project_root` and could bind a temp-dir fixture to an
/// unrelated enclosing project. Instead, only a state-dir path that IS a
/// symlink right at `project_root` (either spelling; nested checked first)
/// is handed to `WorkDir::new`, so the only upward search that can run
/// starts from the symlink's already-resolved target — the hop count for
/// either layout stays [`WorkDir::project_root`]'s call, never recomputed
/// here.
fn config_root(project_root: &Path) -> PathBuf {
    for candidate in [
        project_root.join(".loom").join("work"),
        project_root.join(".work"),
    ] {
        if candidate.is_symlink() {
            if let Some(root) = WorkDir::new(&candidate)
                .ok()
                .and_then(|wd| wd.project_root().map(Path::to_path_buf))
            {
                return root;
            }
        }
    }
    project_root.to_path_buf()
}

impl ProseSources {
    /// Candidate prose files, project-relative, sorted and deduplicated.
    /// The descriptor read is the authoritative size and type check.
    pub(crate) fn files(&self) -> Vec<PathBuf> {
        let mut collected = Vec::new();
        let knowledge_relative = self.knowledge_root.strip_prefix(&self.project_root).ok();
        for root in &self.roots {
            walk::collect_prose_files(&self.project_fd, root, knowledge_relative, &mut collected);
        }
        collected.sort();
        collected.dedup();
        collected
    }

    /// Acquire discovered files through the same project descriptor. The
    /// callback consumes one bounded file at a time.
    pub(crate) fn for_each_file(&self, mut consume: impl FnMut(PathBuf, Vec<u8>)) {
        for relative in self.files() {
            match read_bounded_at(&self.project_fd, &relative, MAX_EXTRACTED_FILE_BYTES) {
                Ok(bytes) => consume(relative, bytes),
                Err(error) => {
                    tracing::debug!(path = %relative.display(), %error, "skipping unreadable prose file");
                }
            }
        }
    }

    /// Chunks for each readable file in [`ProseSources::files`], ids already
    /// prefixed with [`PROSE_ID_PREFIX`] and lifecycle derived from its path
    /// plus an explicit frontmatter `state:` override.
    ///
    /// The rest of the id shape is identical to a curated chunk's
    /// (`prose:doc/design.md#appendix-a#0`) so nothing downstream has to
    /// special-case it: `--require-id` accepts it as-is, and
    /// `split_once('#')` on it still yields the same `(file#anchor,
    /// occurrence)` split a curated id yields.
    ///
    /// `chunk.file` ends up PROJECT-relative (`doc/design.md`) here, where a
    /// curated chunk's `file` is knowledge-root-relative (`architecture.md`).
    /// That asymmetry is intentional — it is what makes `pack.rs`'s item-path
    /// rendering show the right path for each origin — but it is a real trap
    /// for a later reader who assumes every `KnowledgeChunk::file` resolves
    /// against the same root.
    pub(crate) fn chunks(&self) -> Vec<KnowledgeChunk> {
        let mut chunks = Vec::new();
        self.for_each_file(|relative, bytes| {
            let explicit = crate::fs::knowledge::frontmatter::file_state(&bytes);
            let Some(lifecycle) = prose_lifecycle(&relative, explicit) else {
                return;
            };
            let file_chunks = match chunk_file(&relative, &bytes) {
                Ok(file_chunks) => file_chunks,
                Err(error) => {
                    tracing::debug!(path = %relative.display(), %error, "skipping prose file that failed to chunk");
                    return;
                }
            };
            for mut chunk in file_chunks {
                chunk.id = format!("{PROSE_ID_PREFIX}{}", chunk.id);
                chunk.state = lifecycle;
                chunks.push(chunk);
            }
        });
        chunks
    }
}

/// Derive prose authority from a PROJECT-relative path. `None` means the file
/// is excluded from both chunking and fingerprinting.
pub(crate) fn prose_lifecycle(
    relative: &Path,
    explicit: Option<LifecycleState>,
) -> Option<LifecycleState> {
    let components: Vec<String> = relative
        .components()
        .map(|component| {
            component
                .as_os_str()
                .to_string_lossy()
                .trim()
                .to_lowercase()
        })
        .collect();
    let file_name = components.last().map(String::as_str).unwrap_or_default();
    let under_plans = components.iter().any(|component| component == "plans");

    if components.iter().any(|component| component == "archive") {
        return None;
    }
    if under_plans && file_name.starts_with("done-") {
        return None;
    }
    if let Some(explicit) = explicit {
        return Some(explicit);
    }
    if under_plans && file_name.starts_with("review-") {
        return Some(LifecycleState::Historical);
    }
    if under_plans && (file_name.starts_with("plan-") || file_name.starts_with("in_progress-plan-"))
    {
        return Some(LifecycleState::Draft);
    }
    if under_plans && components.iter().any(|component| component == "briefs") {
        return Some(LifecycleState::Draft);
    }
    if file_name.starts_with("report-") || file_name.starts_with("proposal-") {
        return Some(LifecycleState::Historical);
    }
    Some(LifecycleState::Active)
}
