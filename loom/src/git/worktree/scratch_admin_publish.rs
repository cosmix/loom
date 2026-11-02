//! Applying what a command changed in its private git directory to the real
//! administrative directory (see `scratch_admin`).

use anyhow::{bail, Context, Result};
use std::ffi::{OsStr, OsString};
use std::path::Path;

use super::files::{append_head_reflog, is_present, swap, Stamp};
use super::{
    is_shared_index, Copied, ScratchAdmin, COMMONDIR, CONFIG_WORKTREE, HEAD, INDEX, LOGS, REFS,
};

impl ScratchAdmin {
    /// Apply what the command changed to the real administrative directory.
    pub(in crate::git::worktree) fn publish(self) -> Result<()> {
        let created = self.created_names()?;
        let index_may_move = self.publish_shared_indexes(&created)?;
        for copy in &self.copied {
            let settled = is_shared_index(&copy.name) || (copy.name == INDEX && !index_may_move);
            if !settled {
                self.publish_copied(copy)?;
            }
        }
        for name in &self.linked {
            self.publish_linked(name)?;
        }
        self.publish_reflog();
        for name in created.iter().filter(|name| !is_shared_index(name)) {
            self.publish_new(name)?;
        }
        Ok(())
    }

    /// What git made in the private directory that the real one has no entry for.
    fn created_names(&self) -> Result<Vec<OsString>> {
        let listing = || format!("cannot list {}", self.dir.path().display());
        let mut created = Vec::new();
        for entry in std::fs::read_dir(self.dir.path()).with_context(listing)? {
            let name = entry.with_context(listing)?.file_name();
            let known =
                self.linked.contains(&name) || self.copied.iter().any(|copy| copy.name == name);
            if !known && !is_private(&name) {
                created.push(name);
            }
        }
        Ok(created)
    }

    /// Publish the new `sharedindex.<hash>` files, which the new `index` names, so
    /// they must arrive first. A name already there is the same content from a
    /// concurrent git. Returns whether the index may follow: a failure drops it
    /// with the refresh of a command that only refreshes, and fails any other, since
    /// an `index` naming a missing file breaks every git in the worktree.
    fn publish_shared_indexes(&self, created: &[OsString]) -> Result<bool> {
        for name in created.iter().filter(|name| is_shared_index(name)) {
            let private = self.dir.path().join(name);
            match swap(&self.admin, &self.admin_path, name, None, Some(&private)) {
                Ok(()) => {}
                Err(_) if is_present(&self.admin, name) => {}
                Err(error) if self.index_write_is_optional => {
                    tracing::debug!("dropped an index refresh: {error:#}");
                    return Ok(false);
                }
                Err(error) => return Err(error),
            }
        }
        Ok(true)
    }

    fn publish_copied(&self, copy: &Copied) -> Result<()> {
        let private = self.dir.path().join(&copy.name);
        let now = Stamp::of_private(&private)?;
        if now == Some(copy.private) {
            return Ok(());
        }
        let source = now.map(|_| private.as_path());
        let swapped = swap(
            &self.admin,
            &self.admin_path,
            &copy.name,
            Some(copy.real),
            source,
        );
        self.settle_swap(&copy.name, swapped)
    }

    /// A refresh that lost a race keeps the file a concurrent git just wrote;
    /// the next refresh repeats the work.
    fn settle_swap(&self, name: &OsStr, swapped: Result<()>) -> Result<()> {
        match swapped {
            Err(error) if self.index_write_is_optional && name == INDEX => {
                tracing::debug!("dropped an index refresh: {error:#}");
                Ok(())
            }
            other => other,
        }
    }

    fn publish_linked(&self, name: &OsStr) -> Result<()> {
        let private = self.dir.path().join(name);
        match std::fs::symlink_metadata(&private) {
            Ok(meta) if meta.file_type().is_symlink() => Ok(()),
            Ok(_) => self.publish_new(name),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                remove_dir(&self.admin_path.join(name))
            }
            Err(error) => {
                Err(error).with_context(|| format!("cannot inspect {}", private.display()))
            }
        }
    }

    fn publish_new(&self, name: &OsStr) -> Result<()> {
        let private = self.dir.path().join(name);
        let meta = std::fs::symlink_metadata(&private)
            .with_context(|| format!("cannot inspect {}", private.display()))?;
        if !meta.is_file() {
            bail!(
                "git created {} in its private directory; loom publishes only files",
                private.display()
            );
        }
        let swapped = swap(&self.admin, &self.admin_path, name, None, Some(&private));
        self.settle_swap(name, swapped)
    }

    /// Append the entries git wrote for `HEAD`. The ref already moved, so a
    /// failure (a link planted after the first look, a capsule that denies the
    /// directory) costs a reflog line, never the command.
    fn publish_reflog(&self) {
        let entries = match std::fs::read(self.dir.path().join(LOGS).join(HEAD)) {
            Ok(entries) if !entries.is_empty() => entries,
            _ => return,
        };
        if let Err(error) = append_head_reflog(&self.admin, &entries) {
            tracing::warn!(
                "dropped a HEAD reflog entry of {}: {error:#}",
                self.admin_path.display()
            );
        }
    }
}

/// Entries that belong to this process, never to the worktree: its own files,
/// its private `logs` and `refs`, and git's lock files for them.
fn is_private(name: &OsStr) -> bool {
    [COMMONDIR, CONFIG_WORKTREE, LOGS, REFS]
        .iter()
        .any(|own| name == *own)
        || Path::new(name).extension() == Some(OsStr::new("lock"))
}

/// Remove a directory whose private link git deleted.
fn remove_dir(path: &Path) -> Result<()> {
    let removed = match std::fs::symlink_metadata(path) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => Err(error),
        Ok(meta) if meta.is_dir() => std::fs::remove_dir_all(path),
        Ok(_) => std::fs::remove_file(path),
    };
    removed.with_context(|| format!("cannot remove {}", path.display()))
}
