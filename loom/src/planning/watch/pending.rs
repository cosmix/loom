//! Differences accumulated between deliveries of the watcher.

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

use super::Batch;

/// Differences accumulated since the last delivery, relative to the state at that delivery.
#[derive(Debug, Default)]
pub(super) struct Pending {
    added: BTreeSet<PathBuf>,
    changed: BTreeSet<PathBuf>,
    removed: BTreeSet<PathBuf>,
    /// New path -> original path.
    renamed: BTreeMap<PathBuf, PathBuf>,
}

impl Pending {
    pub(super) fn merge(&mut self, diff: Batch) {
        diff.removed.into_iter().for_each(|p| self.remove(p));
        diff.added.into_iter().for_each(|p| self.add(p));
        diff.changed.into_iter().for_each(|p| self.change(p));
        diff.renamed
            .into_iter()
            .for_each(|(from, to)| self.rename(from, to));
    }

    fn add(&mut self, path: PathBuf) {
        // Removed and back again within one window: the old content may differ.
        if self.removed.remove(&path) {
            self.changed.insert(path);
        } else {
            self.added.insert(path);
        }
    }

    fn change(&mut self, path: PathBuf) {
        if !self.added.contains(&path) {
            self.changed.insert(path);
        }
    }

    fn remove(&mut self, path: PathBuf) {
        self.changed.remove(&path);
        if self.added.remove(&path) {
            return;
        }
        let Some(origin) = self.renamed.remove(&path) else {
            self.removed.insert(path);
            return;
        };
        // The original path existed at the last delivery; it may have been re-created since.
        if self.added.remove(&origin) {
            self.changed.insert(origin);
        } else {
            self.removed.insert(origin);
        }
    }

    fn rename(&mut self, from: PathBuf, to: PathBuf) {
        let was_changed = self.changed.remove(&from);
        if self.added.remove(&from) {
            self.add(to);
            return;
        }
        let origin = self.renamed.remove(&from).unwrap_or(from);
        let refilled = self.removed.remove(&to);
        if origin != to {
            self.renamed.insert(to.clone(), origin);
        }
        if was_changed || refilled {
            self.changed.insert(to);
        }
    }

    pub(super) fn take(&mut self) -> Batch {
        let Pending {
            added,
            changed,
            removed,
            renamed,
        } = std::mem::take(self);
        let mut renamed: Vec<(PathBuf, PathBuf)> =
            renamed.into_iter().map(|(to, from)| (from, to)).collect();
        renamed.sort();
        Batch {
            added: added.into_iter().collect(),
            changed: changed.into_iter().collect(),
            removed: removed.into_iter().collect(),
            renamed,
            initial: false,
        }
    }
}
