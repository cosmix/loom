//! The one grant a stage's criteria get beside their tree: the plan's memory
//! journals, read-only, for a knowledge-distill stage. Its standard criterion
//! `loom memory pending --strict` reads `<state root>/memory/*.md`, which a
//! stage worktree reaches only through its `.loom/work` symlink to the main
//! checkout's state root, outside the tree. Everything else that command
//! reads sits in the worktree (its memory spool) or needs no read access: it
//! finds the state root by the link alone, `<worktree>/.loom/work` being a
//! directory (`fs::work_dir::WorkDir::resolve` falls back to it when
//! `config.toml` is not visible).
//!
//! The routes that run a stage's criteria on its own tree take the grant from
//! [`memory_journals_grant`]. The host child (`host_verify::child`) and the
//! landing gate after a resolved merge (`verify::completion::resolved`) pass
//! the daemon's state root, so there the grant never depends on the tree's
//! symlink, which the stage can repoint. `loom stage complete` and
//! `loom check` (`commands::stage::acceptance_runner`) pass the state root
//! the command loads the stage from, which inside a worktree is the one that
//! worktree's `.loom/work` link names. Base runs, contracts, dry runs and
//! provisioning get no grant.

use std::io::ErrorKind;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

use crate::fs::memory::memory_dir;
use crate::fs::work_dir::WorkDir;
use crate::models::stage::{Stage, StageType};

/// A state root's memory journals directory, readable and never writable by a
/// confined command.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MemoryJournals {
    /// The canonical directory.
    pub(super) dir: PathBuf,
    /// Where a worktree's state-root link reaches it: the main checkout's
    /// canonical root joined with its state root's spelling (`.loom/work`)
    /// and `memory`. It differs from `dir` when a component of that spelling
    /// is itself a symlink; bubblewrap then mounts the journals at both, so
    /// the link does not dangle inside the sandbox.
    pub(super) linked: PathBuf,
}

/// The grant for `stage`'s criteria given the state root `work_dir`: the
/// memory journals for a knowledge-distill stage, `None` for every other
/// stage and for a state root without a `memory` directory.
pub fn memory_journals_grant(stage: &Stage, work_dir: &Path) -> Result<Option<MemoryJournals>> {
    if stage.stage_type != StageType::KnowledgeDistill {
        return Ok(None);
    }
    let journals = memory_dir(work_dir);
    let dir = match journals.canonicalize() {
        Ok(dir) => dir,
        Err(error) if error.kind() == ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error).with_context(|| format!("resolve {}", journals.display())),
    };
    let main = WorkDir::new(work_dir)?
        .main_project_root()
        .context("the main checkout of the state root is unavailable")?;
    let main = main
        .canonicalize()
        .with_context(|| format!("resolve {}", main.display()))?;
    let linked = memory_dir(WorkDir::new(&main)?.root());
    Ok(Some(MemoryJournals { dir, linked }))
}

#[cfg(test)]
#[path = "memory_journals_tests.rs"]
mod tests;
