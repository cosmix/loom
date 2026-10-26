//! A polling watcher over the `doc/` tree.
//!
//! Every poll walks `doc/` (summary store included) with discovery's no-follow skip rules,
//! compares each file's size, mtime and inode with the previous walk, and hashes only the files
//! that differ. Differences accumulate until a walk finds nothing new; that walk delivers them
//! as one [`Batch`]. Summary writes the engine registered in [`OwnWrites`] are not reported.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use anyhow::{Context, Result};

use crate::context::source_graph::body_hash;
use crate::planning::discover::{read_doc_bytes, walk_doc, WalkEntry, WalkScope};
use crate::planning::sections::normalized_hash;

use pending::Pending;

/// The summary store; only paths under it are checked against [`OwnWrites`].
const SUMMARIES: &str = "doc/loom/summaries";
/// Prefix of the hash given to a file that could not be read.
const UNREADABLE: &str = "unreadable:";
/// Longest single sleep of the spawned loop, so a stop request is seen promptly.
const STOP_CHECK: Duration = Duration::from_millis(50);

/// One delivery of changes under `doc/`. Every list is sorted.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Batch {
    /// Files that did not exist at the previous delivery.
    pub added: Vec<PathBuf>,
    /// Files whose content hash changed (including the new path of a renamed file edited after
    /// the move).
    pub changed: Vec<PathBuf>,
    /// Files that no longer exist and were not renamed.
    pub removed: Vec<PathBuf>,
    /// `(from, to)` pairs: a file vanished and another appeared with the same hash.
    pub renamed: Vec<(PathBuf, PathBuf)>,
    /// True for the first walk's batch, which lists every file as added.
    pub initial: bool,
}

impl Batch {
    fn is_empty(&self) -> bool {
        self.added.is_empty()
            && self.changed.is_empty()
            && self.removed.is_empty()
            && self.renamed.is_empty()
    }
}

/// Hashes of summary files the engine wrote itself, shared between the engine and the watcher.
/// A registration is consumed by the first delivery that reports its path.
#[derive(Debug, Clone, Default)]
pub struct OwnWrites(Arc<Mutex<HashMap<PathBuf, String>>>);

impl OwnWrites {
    /// Record that the engine is about to write `rel` with `normalized_hash` `hash`.
    pub fn register(&self, rel: &Path, hash: &str) {
        self.map().insert(rel.to_path_buf(), hash.to_string());
    }

    /// Withdraw the registration of `rel` (the engine's write failed).
    pub fn unregister(&self, rel: &Path) {
        self.map().remove(rel);
    }

    fn take(&self, rel: &Path) -> Option<String> {
        self.map().remove(rel)
    }

    fn map(&self) -> MutexGuard<'_, HashMap<PathBuf, String>> {
        // The map holds plain values, so a panic elsewhere cannot leave it inconsistent.
        self.0.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

/// What the previous walk saw of one file.
#[derive(Debug, Clone, PartialEq, Eq)]
struct FileState {
    size: u64,
    mtime_ns: i128,
    inode: u64,
    hash: String,
}

impl FileState {
    fn same_triple(&self, entry: &WalkEntry) -> bool {
        (self.size, self.mtime_ns, self.inode) == (entry.size, entry.mtime_ns, entry.inode)
    }
}

/// Polls `doc/` and batches the differences between walks.
#[derive(Debug)]
pub struct Watcher {
    root: PathBuf,
    own: OwnWrites,
    /// `None` until the first successful walk.
    known: Option<HashMap<PathBuf, FileState>>,
    pending: Pending,
    last_error: Arc<Mutex<Option<String>>>,
}

impl Watcher {
    /// A watcher over `root/doc/` (`root` is the repository root) sharing `own` with the engine.
    pub fn new(root: PathBuf, own: OwnWrites) -> Self {
        Watcher {
            root,
            own,
            known: None,
            pending: Pending::default(),
            last_error: Arc::default(),
        }
    }

    /// One walk. The first successful walk returns `initial: true` with every file added; later
    /// walks return the accumulated batch on the first walk that finds nothing new, else `None`.
    /// The error slot holds the message of a failed walk and is cleared by a successful one.
    pub fn poll(&mut self) -> Result<Option<Batch>> {
        let result = self.walk();
        let message = result.as_ref().err().map(|e| format!("{e:#}"));
        *self
            .last_error
            .lock()
            .unwrap_or_else(PoisonError::into_inner) = message;
        result
    }

    /// The message of the last walk's failure, `None` after a successful walk.
    pub fn last_error(&self) -> Option<String> {
        self.last_error
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }

    /// The shared slot behind [`Watcher::last_error`], to read it after [`Watcher::spawn`].
    pub fn error_slot(&self) -> Arc<Mutex<Option<String>>> {
        Arc::clone(&self.last_error)
    }

    /// Run [`Watcher::poll`] every `interval` on a thread named `loom-plan-watch` until `stop` is
    /// set, handing every batch to `on_batch`. A failed walk is kept in the error slot and the
    /// loop continues. Fails only when the thread cannot be created.
    pub fn spawn(
        mut self,
        interval: Duration,
        stop: Arc<AtomicBool>,
        mut on_batch: impl FnMut(Batch) + Send + 'static,
    ) -> Result<JoinHandle<()>> {
        std::thread::Builder::new()
            .name("loom-plan-watch".into())
            .spawn(move || {
                while !stop.load(Ordering::Relaxed) {
                    if let Ok(Some(batch)) = self.poll() {
                        on_batch(batch);
                    }
                    sleep_unless_stopped(interval, &stop);
                }
            })
            .context("cannot start the doc/ watcher thread")
    }

    fn walk(&mut self) -> Result<Option<Batch>> {
        let (_, entries) = walk_doc(&self.root, WalkScope::IncludeSummaries)?;
        let Some(known) = self.known.as_ref() else {
            let next = scan(&self.root, entries, &HashMap::new());
            let mut added: Vec<PathBuf> = next.keys().cloned().collect();
            added.sort();
            self.known = Some(next);
            let batch = Batch {
                added,
                initial: true,
                ..Batch::default()
            };
            return Ok(Some(self.drop_own_writes(batch)));
        };
        let next = scan(&self.root, entries, known);
        let diff = diff(known, &next);
        self.known = Some(next);
        if !diff.is_empty() {
            self.pending.merge(diff);
            return Ok(None);
        }
        let pending = self.pending.take();
        let batch = self.drop_own_writes(pending);
        Ok((!batch.is_empty()).then_some(batch))
    }

    /// Drop added or changed summary paths whose current hash the engine registered. Every
    /// registration of a reported summary path is consumed, matching or stale.
    fn drop_own_writes(&self, mut batch: Batch) -> Batch {
        let known = self.known.as_ref();
        let is_own = |rel: &PathBuf| {
            if !rel.starts_with(SUMMARIES) {
                return false;
            }
            let Some(registered) = self.own.take(rel) else {
                return false;
            };
            known
                .and_then(|k| k.get(rel))
                .is_some_and(|state| state.hash == registered)
        };
        batch.added.retain(|rel| !is_own(rel));
        batch.changed.retain(|rel| !is_own(rel));
        batch
    }
}

fn sleep_unless_stopped(interval: Duration, stop: &AtomicBool) {
    let deadline = Instant::now() + interval;
    while !stop.load(Ordering::Relaxed) {
        let left = deadline.saturating_duration_since(Instant::now());
        if left.is_zero() {
            return;
        }
        std::thread::sleep(left.min(STOP_CHECK));
    }
}

/// The state of every walked file, reusing the previous hash when the triple is unchanged.
fn scan(
    root: &Path,
    entries: Vec<WalkEntry>,
    known: &HashMap<PathBuf, FileState>,
) -> HashMap<PathBuf, FileState> {
    entries
        .into_iter()
        .map(|entry| {
            let hash = match known.get(&entry.rel) {
                Some(state) if state.same_triple(&entry) => state.hash.clone(),
                _ => hash_entry(root, &entry),
            };
            let WalkEntry {
                rel,
                size,
                mtime_ns,
                inode,
                ..
            } = entry;
            (
                rel,
                FileState {
                    size,
                    mtime_ns,
                    inode,
                    hash,
                },
            )
        })
        .collect()
}

/// The content hash of a walked file, or the sentinel `unreadable:<size>:<mtime_ns>` when it
/// cannot be read (over the document size cap, a swapped symlink, a permission error).
fn hash_entry(root: &Path, entry: &WalkEntry) -> String {
    let Ok(bytes) = read_doc_bytes(root, &entry.rel) else {
        return format!("{UNREADABLE}{}:{}", entry.size, entry.mtime_ns);
    };
    let markdown = entry.rel.extension().is_some_and(|ext| ext == "md");
    match std::str::from_utf8(&bytes) {
        Ok(text) if markdown => normalized_hash(text),
        _ => body_hash(&bytes),
    }
}

/// Added, changed and removed files between two walks, with same-hash remove/add pairs moved
/// to `renamed`. A changed triple with an equal hash is a touch and reports nothing; an
/// unreadable file is reported whenever its triple changes.
fn diff(old: &HashMap<PathBuf, FileState>, new: &HashMap<PathBuf, FileState>) -> Batch {
    let mut out = Batch::default();
    for (rel, state) in new {
        let differs = |prev: &FileState| {
            prev != state && (prev.hash != state.hash || state.hash.starts_with(UNREADABLE))
        };
        match old.get(rel) {
            None => out.added.push(rel.clone()),
            Some(prev) if differs(prev) => out.changed.push(rel.clone()),
            Some(_) => {}
        }
    }
    out.removed = old
        .keys()
        .filter(|rel| !new.contains_key(*rel))
        .cloned()
        .collect();
    out.added.sort();
    out.changed.sort();
    out.removed.sort();
    pair_renames(&mut out, old, new);
    out
}

/// Pair every removed path with the first added path (by sorted path) of the same hash.
/// Unreadable files never pair: their sentinel says nothing about content.
fn pair_renames(
    out: &mut Batch,
    old: &HashMap<PathBuf, FileState>,
    new: &HashMap<PathBuf, FileState>,
) {
    let readable = |state: &FileState| !state.hash.starts_with(UNREADABLE);
    let mut by_hash: HashMap<&str, Vec<&PathBuf>> = HashMap::new();
    for rel in out.added.iter().rev() {
        if let Some(state) = new.get(rel).filter(|s| readable(s)) {
            by_hash.entry(state.hash.as_str()).or_default().push(rel);
        }
    }
    let mut renamed = Vec::new();
    for from in &out.removed {
        let candidates = old
            .get(from)
            .and_then(|state| by_hash.get_mut(state.hash.as_str()));
        if let Some(to) = candidates.and_then(Vec::pop) {
            renamed.push((from.clone(), to.clone()));
        }
    }
    out.added
        .retain(|rel| !renamed.iter().any(|(_, to)| to == rel));
    out.removed
        .retain(|rel| !renamed.iter().any(|(from, _)| from == rel));
    out.renamed = renamed;
}

mod pending;

#[cfg(test)]
mod tests;
