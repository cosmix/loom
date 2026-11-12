//! Host-owned checkout identity captured before any stage can access the tree.

use anyhow::{bail, ensure, Context, Result};
use serde::{Deserialize, Serialize};
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::os::unix::fs::{MetadataExt, OpenOptionsExt};
use std::path::{Path, PathBuf};

use crate::fs::safe_read::read_to_string_bounded;
use crate::fs::work_dir::WorkDir;
use crate::fs::work_state::holds_orchestration_state;

const FILE: &str = "main-checkout-identity.json";
const LIMIT: usize = 16384;

pub(crate) fn main_identity_exists(state: &Path) -> bool {
    match fs::symlink_metadata(state.join(FILE)) {
        Ok(_) => true,
        Err(error) => error.kind() != std::io::ErrorKind::NotFound,
    }
}

#[derive(Serialize, Deserialize)]
struct Directory {
    path: PathBuf,
    device: u64,
    inode: u64,
}

impl Directory {
    fn capture(path: PathBuf) -> Result<Self> {
        let path = path.canonicalize()?;
        let meta = fs::metadata(&path)?;
        ensure!(meta.is_dir(), "{} is not a directory", path.display());
        Ok(Self {
            path,
            device: meta.dev(),
            inode: meta.ino(),
        })
    }

    fn verify(&self) -> Result<PathBuf> {
        let now = Self::capture(self.path.clone())?;
        ensure!(
            now.path == self.path && now.device == self.device && now.inode == self.inode,
            "checkout identity directory changed: {}",
            self.path.display()
        );
        Ok(now.path)
    }
}

#[derive(Serialize, Deserialize)]
struct Identity {
    checkout: Directory,
    git_dir: Directory,
    common_dir: Directory,
}

fn discover(root: &Path) -> Result<(PathBuf, PathBuf)> {
    let dot_git = root.join(".git");
    let meta = fs::symlink_metadata(&dot_git)
        .with_context(|| format!("cannot inspect {}", dot_git.display()))?;
    if meta.file_type().is_dir() {
        let dir = dot_git.canonicalize()?;
        Ok((dir.clone(), dir))
    } else {
        ensure!(
            meta.file_type().is_file(),
            "{} is not a git directory or file",
            dot_git.display()
        );
        super::pinned::registered_checkout(root)
    }
}

/// Initialize once, while the operator checkout is trusted and before any agent starts.
pub(crate) fn capture_main_identity(root: &Path, state: &Path) -> Result<()> {
    let checkout = Directory::capture(root.to_path_buf())?;
    let (git, common) = discover(&checkout.path)?;
    let identity = Identity {
        checkout,
        git_dir: Directory::capture(git)?,
        common_dir: Directory::capture(common)?,
    };
    let bytes = serde_json::to_vec(&identity)?;
    let path = state.join(FILE);
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(&path)
        .with_context(|| format!("cannot create checkout identity {}", path.display()))?;
    file.write_all(&bytes)?;
    file.sync_all()?;
    Ok(())
}

fn load(root: &Path, state: &Path) -> Result<(PathBuf, PathBuf)> {
    let raw = read_to_string_bounded(state, Path::new(FILE), LIMIT).with_context(|| {
        format!("checkout identity missing or unreadable in {}. Stop all sessions, verify the checkout and archive the old Loom state before reinitializing; do not infer identity from an active checkout", state.display())
    })?;
    let identity: Identity = serde_json::from_str(&raw).context("invalid checkout identity")?;
    ensure!(
        identity.checkout.verify()? == root.canonicalize()?,
        "checkout identity belongs to another checkout"
    );
    Ok((identity.git_dir.verify()?, identity.common_dir.verify()?))
}

/// Require an existing anchor before startup, including on legacy runs.
pub(crate) fn require_main_identity(root: &Path) -> Result<()> {
    let state = WorkDir::new(root)?;
    require_main_identity_at(root, state.root())
}

pub(crate) fn require_main_identity_at(root: &Path, state: &Path) -> Result<()> {
    load(root, state).map(|_| ())
}

/// The state directory that exists for `root`, in either layout, `None` when
/// there is none. Existence alone does not mean `loom init` ran
/// ([`initialized_state_of`]).
pub(super) fn state_dir_of(root: &Path) -> Result<Option<PathBuf>> {
    let state = WorkDir::new(root)?;
    if state.root().exists() {
        Ok(Some(state.root().to_path_buf()))
    } else if root.join(".work").exists() {
        Ok(Some(root.join(".work")))
    } else {
        Ok(None)
    }
}

/// The state directory, in either layout, that `loom init` or an orchestration
/// made: one holding a checkout identity, a plan config, or a stage or session
/// file. A state directory holding only what read-only commands and hooks
/// create (`context/`, `memory/`, `telemetry/`) marks nothing, so `loom map`
/// and `loom knowledge` work in a repository that was never initialized.
fn initialized_state_of(root: &Path) -> Result<Option<PathBuf>> {
    let nested = WorkDir::new(root)?.root().to_path_buf();
    Ok([nested, root.join(".work")]
        .into_iter()
        .find(|state| main_identity_exists(state) || holds_orchestration_state(state)))
}

/// Uninitialized standalone repositories retain their discovery-based API.
/// Every path that starts a session requires the identity on its own
/// (`require_main_identity`), whatever this resolves.
pub(super) fn identity_for(root: &Path) -> Result<(PathBuf, PathBuf)> {
    if let Some(state) = initialized_state_of(root)? {
        load(root, &state)
    } else if root.join(".git").exists() {
        discover(root)
    } else {
        bail!("no repository at {}", root.display())
    }
}

#[cfg(test)]
#[path = "main_identity_tests.rs"]
mod tests;
