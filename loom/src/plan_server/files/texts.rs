//! The text of every discovered document, and the per-turn pins diffs compare against.
//!
//! `current` follows the watcher (`apply` runs on every batch) and the server's own writes
//! (`refresh_path`). `begin_turn` and `end_turn` both re-read every document from disk at that
//! moment, so a write the watcher has not polled yet is seen by both: one made just before the
//! turn began is part of its pins, one made just before it ended is its change. Once a turn has
//! ended it keeps only the documents it changed, and the store keeps the newest `MAX_TURNS`
//! turns. Every path is repo-relative (`doc/...`); the summary store is never held.

use std::collections::{BTreeSet, HashMap, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError, RwLock};

use anyhow::Result;

use crate::planning::discover::{discover, read_doc, SUMMARIES};
use crate::planning::project::Project;
use crate::planning::sections::normalized_hash;
use crate::planning::watch::Batch;

/// The id of one chat turn (`<session id>/<turn id>`).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct TurnId(pub String);

/// A document's text and its `normalized_hash`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PinnedText {
    pub text: Arc<str>,
    pub hash: String,
}

impl PinnedText {
    fn of(text: String) -> PinnedText {
        let hash = normalized_hash(&text);
        PinnedText {
            text: Arc::from(text),
            hash,
        }
    }
}

/// One document a turn changed. `before_hash` is `None` for a file the turn created,
/// `after_hash` is `None` for one it deleted.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TurnChange {
    pub path: PathBuf,
    pub before_hash: Option<String>,
    pub after_hash: Option<String>,
}

type Texts = HashMap<PathBuf, PinnedText>;

/// How many turns the store remembers; beginning one more drops the oldest. A turn that has
/// ended holds only the texts of the documents it changed, but those still stay for the life of
/// the process, and a diff is opened for a recent turn. Revisit when a screen browses turns
/// older than this.
pub(crate) const MAX_TURNS: usize = 64;

/// One turn's texts. Until the turn ends `pins` holds every document; `finish` then cuts both
/// maps down to the documents the turn changed.
struct TurnTexts {
    pins: Texts,
    end: Texts,
    ended: bool,
}

impl TurnTexts {
    /// Record `end` (every document read when the turn ended) and return what changed, sorted by
    /// path. A turn ends once: a later call returns the changes recorded then.
    fn finish(&mut self, end: Texts) -> Vec<TurnChange> {
        if self.ended {
            return changes(&self.pins, &self.end);
        }
        let changed = changes(&self.pins, &end);
        self.pins.retain(|path, _| is_listed(&changed, path));
        self.end = end
            .into_iter()
            .filter(|(path, _)| is_listed(&changed, path))
            .collect();
        self.ended = true;
        changed
    }
}

/// The remembered turns and the order they began in.
#[derive(Default)]
struct Turns {
    by_id: HashMap<TurnId, TurnTexts>,
    order: VecDeque<TurnId>,
}

impl Turns {
    /// Remember `texts` for `turn`, replacing an earlier turn of that id, and drop the oldest
    /// turns past `MAX_TURNS`.
    fn begin(&mut self, turn: &TurnId, texts: TurnTexts) {
        if self.by_id.insert(turn.clone(), texts).is_some() {
            self.order.retain(|id| id != turn);
        }
        self.order.push_back(turn.clone());
        while self.order.len() > MAX_TURNS {
            if let Some(oldest) = self.order.pop_front() {
                self.by_id.remove(&oldest);
            }
        }
    }
}

/// The texts the server holds, stored in `ctx.extensions()`.
#[derive(Default)]
pub struct TextStore {
    /// The repository root `load` read; `None` for an empty store, which has nothing to re-read.
    root: Option<PathBuf>,
    current: RwLock<Texts>,
    turns: Mutex<Turns>,
}

impl TextStore {
    /// Read every document of `project.tree`. A document that cannot be read as bounded
    /// UTF-8 text (binary, over `MAX_DOC_BYTES`, a symlink that no longer resolves) has no text.
    pub fn load(root: &Path, project: &Project) -> Result<TextStore> {
        let paths = project.tree.docs.iter().map(|doc| doc.path.clone());
        Ok(TextStore {
            root: Some(root.to_path_buf()),
            current: RwLock::new(read_all(root, paths)),
            turns: Mutex::default(),
        })
    }

    /// Follow a watcher batch: move renamed texts, drop removed ones, re-read added and
    /// changed paths (a path that no longer reads loses its text).
    pub fn apply(&self, root: &Path, batch: &Batch) {
        let mut current = self.current.write().unwrap_or_else(PoisonError::into_inner);
        for (from, to) in &batch.renamed {
            match current.remove(from) {
                Some(text) if is_held(to) => current.insert(to.clone(), text),
                _ => None,
            };
        }
        for path in &batch.removed {
            current.remove(path);
        }
        for path in batch.added.iter().chain(&batch.changed) {
            reread(&mut current, root, path);
        }
    }

    /// Re-read one document into `current` (a path that no longer reads loses its text). The
    /// server calls it after its own write, so a turn that begins before the watcher polls
    /// still pins the saved text.
    pub fn refresh_path(&self, root: &Path, rel: &Path) {
        let mut current = self.current.write().unwrap_or_else(PoisonError::into_inner);
        reread(&mut current, root, rel);
    }

    /// The current text of `rel`.
    pub fn current(&self, rel: &Path) -> Option<PinnedText> {
        let current = self.current.read().unwrap_or_else(PoisonError::into_inner);
        current.get(rel).cloned()
    }

    /// Re-read every document from disk (`current` takes what was read), pin the texts for
    /// `turn`, and replace any earlier pins and end texts of it.
    pub fn begin_turn(&self, turn: &TurnId) {
        let pins = self.refresh_all();
        let texts = TurnTexts {
            pins,
            end: Texts::new(),
            ended: false,
        };
        self.turns().begin(turn, texts);
    }

    /// Re-read every document now (those of `project.tree` and any discovery lists that the
    /// project has not seen yet), keep the texts of the documents that changed since
    /// `begin_turn` as the turn's pins and end texts, and return what changed, sorted by path.
    /// A turn that was never begun (or was dropped as too old) has no changes.
    pub fn end_turn(&self, turn: &TurnId, root: &Path, project: &Project) -> Vec<TurnChange> {
        match self.turns().by_id.get(turn) {
            None => return Vec::new(),
            Some(texts) if texts.ended => return changes(&texts.pins, &texts.end),
            Some(_) => {}
        }
        let mut paths = listed_paths(root);
        paths.extend(project.tree.docs.iter().map(|d| d.path.clone()));
        let end = read_all(root, paths);
        match self.turns().by_id.get_mut(turn) {
            Some(texts) => texts.finish(end),
            None => Vec::new(),
        }
    }

    /// Whether `turn` has ended (`false` for a turn that was never begun).
    pub fn turn_ended(&self, turn: &TurnId) -> bool {
        self.turns()
            .by_id
            .get(turn)
            .is_some_and(|texts| texts.ended)
    }

    /// The text `rel` had when `turn` began. Once the turn has ended only a document it
    /// changed has one.
    pub fn turn_pin(&self, turn: &TurnId, rel: &Path) -> Option<PinnedText> {
        self.turns().by_id.get(turn)?.pins.get(rel).cloned()
    }

    /// The text `rel` had when `turn` ended: `None` for a document the turn deleted or did
    /// not change, and for a turn that has not ended.
    pub fn turn_end(&self, turn: &TurnId, rel: &Path) -> Option<PinnedText> {
        self.turns().by_id.get(turn)?.end.get(rel).cloned()
    }

    /// Re-read the documents `current` holds and any that `doc/` lists now, and return the
    /// texts. Holds the write lock throughout, as `apply` does, so a `refresh_path` after the
    /// server's own write cannot be overwritten by a read that predates it.
    fn refresh_all(&self) -> Texts {
        let Some(root) = self.root.as_deref() else {
            return self
                .current
                .read()
                .unwrap_or_else(PoisonError::into_inner)
                .clone();
        };
        let mut paths = listed_paths(root);
        let mut current = self.current.write().unwrap_or_else(PoisonError::into_inner);
        paths.extend(current.keys().cloned());
        for rel in &paths {
            reread(&mut current, root, rel);
        }
        current.clone()
    }

    fn turns(&self) -> MutexGuard<'_, Turns> {
        // The turns hold plain values, so a panic elsewhere cannot leave them inconsistent.
        self.turns.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

/// Every document `doc/` lists now; empty when discovery fails.
fn listed_paths(root: &Path) -> BTreeSet<PathBuf> {
    match discover(root) {
        Ok(tree) => tree.docs.into_iter().map(|doc| doc.path).collect(),
        Err(error) => {
            tracing::warn!("rediscovering doc/ failed: {error:#}");
            BTreeSet::new()
        }
    }
}

/// Discovery never lists the summary store; the watcher's batches do.
fn is_held(rel: &Path) -> bool {
    rel.starts_with("doc") && !rel.starts_with(SUMMARIES)
}

/// Put the text `rel` has on disk now into `current`, or remove its entry when it has none to hold.
fn reread(current: &mut Texts, root: &Path, rel: &Path) {
    match is_held(rel).then(|| read_text(root, rel)).flatten() {
        Some(text) => current.insert(rel.to_path_buf(), text),
        None => current.remove(rel),
    };
}

fn read_text(root: &Path, rel: &Path) -> Option<PinnedText> {
    match read_doc(root, rel) {
        Ok(text) => Some(PinnedText::of(text)),
        Err(error) => {
            tracing::debug!("no text held for {}: {error:#}", rel.display());
            None
        }
    }
}

fn read_all(root: &Path, paths: impl IntoIterator<Item = PathBuf>) -> Texts {
    paths
        .into_iter()
        .filter(|path| is_held(path))
        .filter_map(|path| read_text(root, &path).map(|text| (path, text)))
        .collect()
}

fn changes(pins: &Texts, end: &Texts) -> Vec<TurnChange> {
    let paths: BTreeSet<&PathBuf> = pins.keys().chain(end.keys()).collect();
    paths
        .into_iter()
        .filter_map(|path| {
            let before_hash = pins.get(path).map(|t| t.hash.clone());
            let after_hash = end.get(path).map(|t| t.hash.clone());
            (before_hash != after_hash).then(|| TurnChange {
                path: path.clone(),
                before_hash,
                after_hash,
            })
        })
        .collect()
}

/// Whether `path` is one of `changed`, which is sorted by path.
fn is_listed(changed: &[TurnChange], path: &Path) -> bool {
    changed
        .binary_search_by(|change| change.path.as_path().cmp(path))
        .is_ok()
}
