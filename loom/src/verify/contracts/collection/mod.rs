//! Test collection for contracts: the scaffold a contract writer may add so
//! a contract file is compiled into a test target, the strict check of that
//! scaffold, and the chain check proving the frozen file is the one compiled
//! under the contract's test name (DESIGN D8; plan contract-phase-autonomy,
//! Decisions 1-7).
//!
//! [`rust`] models Rust over a [`FileSource`]. This module reads one stage
//! for it: the base through read-only git (`git cat-file blob`, never
//! `git show`, which lists a directory and may apply textconv) and the
//! worktree through bounded reads that follow no symlink. It is the one base
//! reader callers outside `crate::verify` use, since `changes::git` is
//! private to `crate::verify`.

mod harness;
pub mod rust;

use anyhow::{bail, Context, Result};
use std::cell::RefCell;
use std::collections::{BTreeSet, HashMap};
use std::path::{Path, PathBuf};

use super::changes::git;
use super::store::{validate_relative, MAX_FROZEN_FILE_BYTES};
use super::{normalize, resolve_adapter};
use crate::fs::safe_read::read_to_string_bounded;
use crate::git::worktree::WorktreeGit;
use crate::plan::schema::ContractSpec;
use rust::join;

pub use harness::harness_source_matches;

/// A path relative to the source root, '/'-separated, to the file's content;
/// None when the file is absent or unreadable.
pub type FileSource<'a> = &'a dyn Fn(&str) -> Option<String>;

/// A module the scaffold declares.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScaffoldModule {
    pub name: String,
    /// The contract file's own module: the signal suggests #[cfg(test)] on
    /// its declaration. The diff accepts #[cfg(test)] above any derived one.
    pub test_only: bool,
}

/// One edit a contract writer may make so a contract file is collected.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ScaffoldEdit {
    /// Add one out-of-line `mod <module.name>;` item to the existing file `path`.
    Declare {
        path: String,
        module: ScaffoldModule,
    },
    /// Create exactly one of `paths` (`<dir>/mod.rs` first, then `<dir>.rs`
    /// unless that path is itself an auto target root), holding only the
    /// declarations of `modules`.
    CreateRoot {
        paths: Vec<String>,
        modules: Vec<ScaffoldModule>,
    },
}

/// What the freeze needs, computed once and shared by the writer's
/// pre-check and the daemon's re-check.
#[derive(Debug)]
pub(crate) struct ScaffoldCheck {
    /// Scaffold paths relative to the stage working_dir.
    pub paths: BTreeSet<String>,
    pub problems: Vec<String>,
}

/// The stage's merged scaffold derived over the stage base, paths relative
/// to `working_dir`; an edit wholly outside it is dropped. Empty when the
/// stage has no Rust contract.
pub(crate) fn stage_scaffold(
    repo: &WorktreeGit,
    worktree_root: &Path,
    working_dir: &Path,
    base: &str,
    contracts: &[ContractSpec],
) -> Result<Vec<ScaffoldEdit>, String> {
    StageTree::open(repo, worktree_root, working_dir, base, contracts)
        .map_err(|error| format!("{error:#}"))?
        .scaffold()
}

/// The freeze's scaffold check: the stage scaffold (a derivation error is
/// one problem), the strict diff of the worktree against the base, and,
/// when that diff is clean, the chain check of every Rust contract and the
/// run directory of every one that sets `cwd` (Decision 6), each as
/// `contract '<id>': <problem>`.
pub(crate) fn freeze_scaffold(
    repo: &WorktreeGit,
    worktree_root: &Path,
    working_dir: &Path,
    base: &str,
    contracts: &[ContractSpec],
) -> Result<ScaffoldCheck> {
    let stage = StageTree::open(repo, worktree_root, working_dir, base, contracts)?;
    let mut check = ScaffoldCheck {
        paths: BTreeSet::new(),
        problems: Vec::new(),
    };
    if stage.contracts.is_empty() {
        return Ok(check);
    }
    let edits = match stage.scaffold() {
        Ok(edits) => edits,
        Err(problem) => {
            check.problems.push(problem);
            return Ok(check);
        }
    };
    check.paths = edits.iter().flat_map(edit_paths).cloned().collect();
    // The edits are relative to the working directory; read them there.
    let base_source = |path: &str| stage.base_file(&join(&stage.prefix, path));
    let current_source = |path: &str| stage.current_file(&join(&stage.prefix, path));
    check.problems = rust::scaffold_problems(&edits, &base_source, &current_source);
    if check.problems.is_empty() {
        check.problems = stage.chain_problems();
    }
    Ok(check)
}

/// Every path an edit may touch.
fn edit_paths(edit: &ScaffoldEdit) -> &[String] {
    match edit {
        ScaffoldEdit::Declare { path, .. } => std::slice::from_ref(path),
        ScaffoldEdit::CreateRoot { paths, .. } => paths,
    }
}

/// One stage's Rust contracts, with cached readers of its base and its
/// worktree; paths relative to the worktree root.
struct StageTree<'a> {
    repo: &'a WorktreeGit,
    /// The worktree root, canonical.
    root: PathBuf,
    base: &'a str,
    /// The working directory beneath the root, "" at the root.
    prefix: String,
    /// Each Rust contract with its file relative to the root.
    contracts: Vec<(&'a ContractSpec, String)>,
    base_files: RefCell<HashMap<String, Option<String>>>,
    current_files: RefCell<HashMap<String, Option<String>>>,
}

impl<'a> StageTree<'a> {
    fn open(
        repo: &'a WorktreeGit,
        worktree_root: &Path,
        working_dir: &Path,
        base: &'a str,
        contracts: &'a [ContractSpec],
    ) -> Result<Self> {
        let (root, prefix) = working_prefix(worktree_root, working_dir)?;
        let mut rust_contracts = Vec::new();
        for contract in contracts {
            let is_rust = resolve_adapter(contract, working_dir)
                .is_some_and(|adapter| adapter.language() == "rust");
            if !is_rust {
                continue;
            }
            let file = normalize(&contract.file);
            validate_relative(&file).with_context(|| format!("contract '{}' file", contract.id))?;
            rust_contracts.push((contract, join(&prefix, &file)));
        }
        if !rust_contracts.is_empty() {
            check_base(repo, base)?;
        }
        Ok(Self {
            repo,
            root,
            base,
            prefix,
            contracts: rust_contracts,
            base_files: RefCell::default(),
            current_files: RefCell::default(),
        })
    }

    /// `path` at the stage base.
    fn base_file(&self, path: &str) -> Option<String> {
        cached(&self.base_files, path, || {
            read_base(self.repo, self.base, path)
        })
    }

    /// `path` in the worktree: absent when missing, a symlink at any
    /// component, not a regular file, over the frozen-file limit, or not
    /// UTF-8. Each path is read once, so one check sees one version of it.
    fn current_file(&self, path: &str) -> Option<String> {
        cached(&self.current_files, path, || {
            read_to_string_bounded(&self.root, Path::new(path), MAX_FROZEN_FILE_BYTES).ok()
        })
    }

    /// The merged scaffold, paths relative to the working directory.
    fn scaffold(&self) -> Result<Vec<ScaffoldEdit>, String> {
        let pairs: Vec<(&str, &str)> = self
            .contracts
            .iter()
            .map(|(contract, file)| (file.as_str(), contract.test.as_str()))
            .collect();
        let base = |path: &str| self.base_file(path);
        let edits = rust::derive_scaffold(&base, &pairs)?;
        Ok(edits
            .into_iter()
            .filter_map(|edit| within(&self.prefix, edit))
            .collect())
    }

    /// The chain check over the worktree, and the run directory of every
    /// contract that sets `cwd` (worktree-relative, as `run_dir` reads it).
    fn chain_problems(&self) -> Vec<String> {
        let current = |path: &str| self.current_file(path);
        let mut problems = Vec::new();
        for (contract, file) in &self.contracts {
            let mut found = rust::chain_problems(&current, file, &contract.test);
            if let Some(cwd) = &contract.cwd {
                found.extend(rust::run_dir_problems(&current, file, &normalize(cwd)));
            }
            problems.extend(
                found
                    .into_iter()
                    .map(|problem| format!("contract '{}': {problem}", contract.id)),
            );
        }
        problems
    }
}

/// The canonical worktree root and the working directory's path beneath it.
fn working_prefix(worktree_root: &Path, working_dir: &Path) -> Result<(PathBuf, String)> {
    let root = worktree_root
        .canonicalize()
        .with_context(|| format!("cannot resolve the worktree {}", worktree_root.display()))?;
    let dir = working_dir.canonicalize().with_context(|| {
        format!(
            "cannot resolve the working directory {}",
            working_dir.display()
        )
    })?;
    let inside = dir.strip_prefix(&root).with_context(|| {
        format!(
            "working directory {} is outside the worktree {}",
            dir.display(),
            root.display()
        )
    })?;
    let prefix = normalize(&inside.to_string_lossy());
    Ok((root, prefix))
}

/// Refuse a base git would not resolve to a commit, so a failed read below
/// can only mean an absent file.
fn check_base(repo: &WorktreeGit, base: &str) -> Result<()> {
    if base.is_empty() || base.starts_with('-') {
        bail!("stage base '{base}' is not a revision");
    }
    let commit = format!("{base}^{{commit}}");
    git(repo, &["rev-parse", "--verify", "--quiet", &commit])
        .with_context(|| format!("stage base '{base}' is not a commit"))?;
    Ok(())
}

/// The blob at `path` in `base`: `None` when git has none there (absent, or
/// a directory), or it is over the frozen-file limit or not UTF-8.
fn read_base(repo: &WorktreeGit, base: &str, path: &str) -> Option<String> {
    let blob = git(repo, &["cat-file", "blob", &format!("{base}:{path}")]).ok()?;
    if blob.len() > MAX_FROZEN_FILE_BYTES {
        return None;
    }
    String::from_utf8(blob).ok()
}

fn cached(
    cache: &RefCell<HashMap<String, Option<String>>>,
    path: &str,
    read: impl FnOnce() -> Option<String>,
) -> Option<String> {
    if let Some(known) = cache.borrow().get(path) {
        return known.clone();
    }
    let value = read();
    cache.borrow_mut().insert(path.to_string(), value.clone());
    value
}

/// `edit` with its paths relative to the working directory `prefix`,
/// dropping each path outside it; `None` when none is left.
fn within(prefix: &str, edit: ScaffoldEdit) -> Option<ScaffoldEdit> {
    let beneath = |path: &str| -> Option<String> {
        if prefix.is_empty() {
            return Some(path.to_string());
        }
        path.strip_prefix(prefix)?
            .strip_prefix('/')
            .map(str::to_string)
    };
    match edit {
        ScaffoldEdit::Declare { path, module } => Some(ScaffoldEdit::Declare {
            path: beneath(&path)?,
            module,
        }),
        ScaffoldEdit::CreateRoot { paths, modules } => {
            let paths: Vec<String> = paths.iter().filter_map(|path| beneath(path)).collect();
            (!paths.is_empty()).then_some(ScaffoldEdit::CreateRoot { paths, modules })
        }
    }
}

#[cfg(test)]
#[path = "tests.rs"]
mod tests;
