//! The directory a contract runs from, and the command that runs it there.
//!
//! A runner reads its configuration, or finds its project, in its cwd, so a
//! contract whose file sits in a sub-workspace with its own runner config
//! (`apps/web/bunfig.toml`) must run from that sub-workspace. The run
//! directory is, in order of precedence:
//!
//! 1. the contract's `cwd`, relative to the worktree root: the plan's word
//!    always decides;
//! 2. the one the freeze recorded for the contract's file
//!    (`store::recorded_cwd`), so the completion check runs a contract
//!    exactly where the freeze ran it, whatever runner config appears since;
//! 3. the nearest ancestor of the contract file, from its parent up to the
//!    worktree root, holding one of the adapter's `run_root_markers`;
//! 4. the stage's working directory.
//!
//! The freeze CLI, the freeze handler, the completion check, the dispute
//! re-freeze, the contract signal and the adjudication prompt all resolve it
//! with [`resolve_run`], so they agree on where and how a contract runs.

use anyhow::{bail, Context, Result};
use std::path::{Path, PathBuf};

use super::{normalize, store};
use crate::plan::schema::ContractSpec;
use crate::testrun::TestRunnerAdapter;

/// Where and how one contract runs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContractRun {
    /// The run directory, canonical and inside the worktree.
    pub dir: PathBuf,
    /// The run directory relative to the worktree root; `.` for the root.
    pub cwd: String,
    /// The shell command that runs the contract from `dir`: the adapter's
    /// single-test command, or the contract's `test` itself when no adapter
    /// applies, in which case only its exit code can be judged.
    pub command: String,
}

/// Resolve where `contract` runs and the command that runs it there.
/// `working_dir` is the stage's working directory, which the contract file is
/// relative to; `recorded` is the run directory the freeze recorded for the
/// contract's current file (`store::recorded_cwd`), if any, which the
/// contract's own `cwd` beats. The contract file must lie beneath the run
/// directory, and the run directory must exist inside `worktree_root`.
pub fn resolve_run(
    contract: &ContractSpec,
    adapter: Option<&dyn TestRunnerAdapter>,
    worktree_root: &Path,
    working_dir: &Path,
    recorded: Option<&str>,
) -> Result<ContractRun> {
    let id = &contract.id;
    let root = canonical(worktree_root)?;
    let working = canonical(working_dir)?;
    if !working.starts_with(&root) {
        bail!("the stage's working directory is outside its worktree");
    }
    let file_path = normalize(&contract.file);
    store::validate_relative(&file_path).with_context(|| format!("contract '{id}' file"))?;
    let file = working.join(file_path);
    let run_dir = match contract.cwd.as_deref().or(recorded) {
        Some(cwd) => root.join(plain_cwd(cwd).with_context(|| format!("contract '{id}' cwd"))?),
        None => adapter
            .and_then(|adapter| marker_dir(&file, &root, adapter.run_root_markers()))
            .unwrap_or(working),
    };
    let relative_file = beneath(&file, &run_dir)
        .with_context(|| format!("contract '{id}': its file is not beneath its run directory"))?;
    let dir = inside(&run_dir, &root).with_context(|| format!("contract '{id}'"))?;
    let command = match adapter {
        Some(adapter) => adapter.single_test_command(&relative_file, &contract.test, &dir),
        None => contract.test.clone(),
    };
    Ok(ContractRun {
        dir,
        // The run directory lies in the worktree, so it is beneath the root
        // unless it is the root itself.
        cwd: beneath(&run_dir, &root).unwrap_or_else(|_| ".".to_string()),
        command,
    })
}

/// `run_dir` canonicalized, refused unless it exists inside `root`.
fn inside(run_dir: &Path, root: &Path) -> Result<PathBuf> {
    let dir = run_dir
        .canonicalize()
        .with_context(|| format!("run directory {} does not exist", run_dir.display()))?;
    if !dir.starts_with(root) {
        bail!(
            "run directory {} is outside the worktree",
            run_dir.display()
        );
    }
    Ok(dir)
}

/// `path` relative to `dir`, `/`-joined; refused unless `path` lies strictly
/// beneath `dir`.
fn beneath(path: &Path, dir: &Path) -> Result<String> {
    match path.strip_prefix(dir) {
        Ok(relative) if !relative.as_os_str().is_empty() => {
            Ok(normalize(&relative.to_string_lossy()))
        }
        _ => bail!("{} is not beneath {}", path.display(), dir.display()),
    }
}

fn canonical(path: &Path) -> Result<PathBuf> {
    path.canonicalize()
        .with_context(|| format!("failed to resolve {}", path.display()))
}

/// `cwd` with `.` components dropped, empty for the worktree root; refused
/// unless it is a plain relative path.
fn plain_cwd(cwd: &str) -> Result<String> {
    let plain = normalize(cwd);
    if !plain.is_empty() {
        store::validate_relative(&plain)?;
    }
    Ok(plain)
}

/// The nearest ancestor of `file`, from its parent up to `root` inclusive,
/// holding one of `markers`. `file` need not exist yet.
fn marker_dir(file: &Path, root: &Path, markers: &[&str]) -> Option<PathBuf> {
    file.ancestors()
        .skip(1)
        .take_while(|dir| dir.starts_with(root))
        .find(|dir| has_marker(dir, markers))
        .map(Path::to_path_buf)
}

/// Whether `dir` holds a regular file named by one of `markers`; a `*.ext`
/// marker matches any file with that extension. A symlink never counts, so a
/// link planted in the worktree cannot pull a contract's run directory.
fn has_marker(dir: &Path, markers: &[&str]) -> bool {
    markers
        .iter()
        .any(|marker| match marker.strip_prefix("*.") {
            Some(extension) => {
                std::fs::read_dir(dir)
                    .into_iter()
                    .flatten()
                    .flatten()
                    .any(|entry| {
                        let path = entry.path();
                        path.extension().is_some_and(|ext| ext == extension) && is_regular(&path)
                    })
            }
            None => is_regular(&dir.join(marker)),
        })
}

/// Whether `path` is a regular file, without following a symlink.
fn is_regular(path: &Path) -> bool {
    std::fs::symlink_metadata(path).is_ok_and(|meta| meta.file_type().is_file())
}

#[cfg(test)]
#[path = "run_dir_tests.rs"]
mod tests;
