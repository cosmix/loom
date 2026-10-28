//! Layer 1 write lists of a planning session: `sandbox.filesystem.denyWrite` and `allowWrite`.
//!
//! Every entry is an existing path listed at build time without following a symlink: a symlink
//! is denied by its own path like any other entry, and a kept name of the wrong kind (a symlink
//! where a directory is expected) is denied too, so nothing is ever listed through a link. No
//! entry lies beneath another: bubblewrap refuses a deny mount inside a read-only bind
//! (`architecture/execution-containment.md`).

use std::ffi::OsString;
use std::os::fd::OwnedFd;
use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};

use super::{Build, Target};
use crate::fs::safe_fs::safe_open_dirfd;
use crate::fs::safe_read::{is_not_found, list_dir_no_follow, EntryKind};
use crate::planning::paths::{symlink_component, LockReason};

const DOC: &str = "doc";
const LOOM: &str = "loom";
const SUMMARIES: &str = "summaries";
const PLANS: &str = "plans";
const BRIEFS: &str = "briefs";
/// A pressure session's notes file at the project root.
pub(crate) const OMISSIONS: &str = "planning-omissions.md";

/// The project tree, listed through one no-follow descriptor of its root.
pub(super) struct Tree {
    root: PathBuf,
    fd: OwnedFd,
}

impl Tree {
    pub(super) fn open(root: &Path) -> Result<Tree> {
        let fd = safe_open_dirfd(root)
            .with_context(|| format!("cannot open the project root {}", root.display()))?;
        Ok(Tree {
            root: root.to_path_buf(),
            fd,
        })
    }

    /// The entries of the repo-relative directory `dir`; an absent directory has none.
    fn entries(&self, dir: &Path) -> Result<Vec<(OsString, EntryKind)>> {
        match list_dir_no_follow(&self.fd, dir) {
            Ok(entries) => Ok(entries),
            Err(error) if is_not_found(&error) => Ok(Vec::new()),
            Err(error) => Err(error.context(format!("cannot list {}", dir.display()))),
        }
    }

    /// Push `dir/<e>` onto `deny` for every entry of `dir` except the names in `keep` that have
    /// the kind beside them; return the kept names present.
    fn confine(
        &self,
        dir: &Path,
        keep: &[(&str, EntryKind)],
        deny: &mut Vec<PathBuf>,
    ) -> Result<Vec<String>> {
        let mut kept = Vec::new();
        for (name, kind) in self.entries(dir)? {
            let wanted = name.to_str().filter(|name| keep.contains(&(*name, kind)));
            match wanted {
                Some(name) => kept.push(name.to_owned()),
                None => deny.push(dir.join(name)),
            }
        }
        Ok(kept)
    }

    /// The entries of `dir` when it is a directory reached without a symlink, else none.
    pub(super) fn real_dir_entries(&self, dir: &Path) -> Result<Vec<(OsString, EntryKind)>> {
        if symlink_component(&self.root, dir)?.is_some() {
            return Ok(Vec::new());
        }
        let real = std::fs::symlink_metadata(self.root.join(dir)).is_ok_and(|meta| meta.is_dir());
        if real {
            self.entries(dir)
        } else {
            Ok(Vec::new())
        }
    }
}

/// `(denyWrite, allowWrite)` of the session, absolute and sorted.
pub(super) fn write_lists(build: &Build<'_>) -> Result<(Vec<String>, Vec<String>)> {
    let deny = match &build.target {
        Target::Authoring => authoring(build)?,
        Target::Summary => summary(&build.tree)?,
        Target::Pressure { plan, bundle } => pressure(&build.tree, plan, bundle)?,
    };
    let rt = build.inputs.runtime;
    let allow = match &build.target {
        Target::Pressure { .. } => vec![build.done.as_path()],
        _ => vec![
            rt.drafts.as_path(),
            rt.claims.as_path(),
            rt.summaries_lock.as_path(),
        ],
    };
    let root = build.inputs.project_root;
    let deny = collapse(deny)
        .iter()
        .map(|rel| text(&root.join(rel)))
        .collect::<Result<Vec<String>>>()?;
    Ok((sorted(deny), sorted(text_all(allow)?)))
}

/// Every top-level entry but `doc`, then the existing locked set collapsed to directories.
fn authoring(build: &Build<'_>) -> Result<Vec<PathBuf>> {
    let mut deny = Vec::new();
    build
        .tree
        .confine(Path::new(""), &[(DOC, EntryKind::Directory)], &mut deny)?;
    deny.extend(build.locked.locked_paths());
    deny.extend(locked_bundles(build)?.into_iter().map(|(dir, _)| dir));
    Ok(deny)
}

/// Every entry but the path down to `doc/loom/summaries`.
fn summary(tree: &Tree) -> Result<Vec<PathBuf>> {
    let mut deny = Vec::new();
    let dir = EntryKind::Directory;
    if !tree
        .confine(Path::new(""), &[(DOC, dir)], &mut deny)?
        .is_empty()
        && !tree
            .confine(Path::new(DOC), &[(LOOM, dir)], &mut deny)?
            .is_empty()
    {
        let loom = Path::new(DOC).join(LOOM);
        tree.confine(&loom, &[(SUMMARIES, dir)], &mut deny)?;
    }
    Ok(deny)
}

/// Every entry but `planning-omissions.md` and the path down to the plan and its bundle.
fn pressure(tree: &Tree, plan: &Path, bundle: &Path) -> Result<Vec<PathBuf>> {
    let (dir, file) = (EntryKind::Directory, EntryKind::File);
    let plan_name = utf8_name(plan)?;
    let slug = utf8_name(bundle)?;
    let plans = Path::new(DOC).join(PLANS);
    let mut deny = Vec::new();
    let top = tree.confine(Path::new(""), &[(DOC, dir), (OMISSIONS, file)], &mut deny)?;
    if !top.iter().any(|name| name == DOC)
        || tree
            .confine(Path::new(DOC), &[(PLANS, dir)], &mut deny)?
            .is_empty()
    {
        return Ok(deny);
    }
    let kept = tree.confine(&plans, &[(plan_name, file), (BRIEFS, dir)], &mut deny)?;
    if kept.iter().any(|name| name == BRIEFS) {
        tree.confine(&plans.join(BRIEFS), &[(slug, dir)], &mut deny)?;
    }
    Ok(deny)
}

/// The existing real bundle directories of locked plans, `doc/plans/briefs/<dir>`, each with
/// the locked plan's slug. A bundle directory is locked when the locked set locks the
/// directory path itself, which it does by the bundle prefix of a locked plan.
pub(super) fn locked_bundles(build: &Build<'_>) -> Result<Vec<(PathBuf, String)>> {
    let briefs = Path::new(DOC).join(PLANS).join(BRIEFS);
    let mut bundles = Vec::new();
    for (name, kind) in build.tree.real_dir_entries(&briefs)? {
        if kind != EntryKind::Directory {
            continue;
        }
        let rel = briefs.join(name);
        if let Some(LockReason::BriefOfLockedPlan { slug }) = build.locked.lock_reason(&rel) {
            bundles.push((rel, slug));
        }
    }
    Ok(bundles)
}

/// Sort `paths` and drop every path that has a kept path as an ancestor (or equals one).
fn collapse(mut paths: Vec<PathBuf>) -> Vec<PathBuf> {
    paths.sort();
    let mut kept: Vec<PathBuf> = Vec::new();
    for path in paths {
        if !kept.iter().any(|outer| path.starts_with(outer)) {
            kept.push(path);
        }
    }
    kept
}

fn utf8_name(path: &Path) -> Result<&str> {
    match path.file_name().and_then(|name| name.to_str()) {
        Some(name) => Ok(name),
        None => bail!("{} has no UTF-8 file name", path.display()),
    }
}

/// The settings text of `path`; a path JSON cannot carry exactly is refused, never mangled.
pub(super) fn text(path: &Path) -> Result<String> {
    match path.to_str() {
        Some(text) => Ok(text.to_owned()),
        None => bail!(
            "the planning sandbox cannot name {} in its settings: the path is not valid UTF-8; \
             rename it to a UTF-8 name",
            path.display()
        ),
    }
}

fn text_all<'p>(paths: impl IntoIterator<Item = &'p Path>) -> Result<Vec<String>> {
    paths.into_iter().map(text).collect()
}

fn sorted(mut list: Vec<String>) -> Vec<String> {
    list.sort();
    list
}
