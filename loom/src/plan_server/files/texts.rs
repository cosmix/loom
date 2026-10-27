//! The text of every discovered document, and the per-turn pins diffs compare against.
//!
//! `current` follows the watcher (`apply` runs on every batch). A turn pins the current texts
//! at `begin_turn`; `end_turn` re-reads every document from disk at that moment, so a write
//! made just before the turn ended is seen even when the watcher has not polled yet. Pins and
//! end texts live for the process. Every path is repo-relative (`doc/...`); the summary store
//! is never held.

use std::collections::{BTreeSet, HashMap};
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

#[derive(Default)]
struct TurnTexts {
    pins: Texts,
    end: Texts,
}

/// The texts the server holds, stored in `ctx.extensions()`.
#[derive(Default)]
pub struct TextStore {
    current: RwLock<Texts>,
    turns: Mutex<HashMap<TurnId, TurnTexts>>,
}

impl TextStore {
    /// Read every document of `project.tree`. A document that cannot be read as bounded
    /// UTF-8 text (binary, over `MAX_DOC_BYTES`, a symlink that no longer resolves) has no text.
    pub fn load(root: &Path, project: &Project) -> Result<TextStore> {
        let paths = project.tree.docs.iter().map(|doc| doc.path.clone());
        Ok(TextStore {
            current: RwLock::new(read_all(root, paths)),
            turns: Mutex::new(HashMap::new()),
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
            match read_text(root, path) {
                Some(text) if is_held(path) => current.insert(path.clone(), text),
                _ => current.remove(path),
            };
        }
    }

    /// The current text of `rel`.
    pub fn current(&self, rel: &Path) -> Option<PinnedText> {
        let current = self.current.read().unwrap_or_else(PoisonError::into_inner);
        current.get(rel).cloned()
    }

    /// Pin the current texts for `turn`, replacing any earlier pins and end texts of it.
    pub fn begin_turn(&self, turn: &TurnId) {
        let pins = self
            .current
            .read()
            .unwrap_or_else(PoisonError::into_inner)
            .clone();
        let texts = TurnTexts {
            pins,
            end: Texts::new(),
        };
        self.turns().insert(turn.clone(), texts);
    }

    /// Re-read every document now (those of `project.tree` and any discovery lists that the
    /// project has not seen yet), store them as the turn's end texts and return what changed
    /// since `begin_turn`, sorted by path. A turn that was never begun has no changes.
    pub fn end_turn(&self, turn: &TurnId, root: &Path, project: &Project) -> Vec<TurnChange> {
        if !self.turns().contains_key(turn) {
            return Vec::new();
        }
        let mut paths: BTreeSet<PathBuf> =
            project.tree.docs.iter().map(|d| d.path.clone()).collect();
        match discover(root) {
            Ok(tree) => paths.extend(tree.docs.into_iter().map(|d| d.path)),
            Err(error) => tracing::warn!("end of turn: rediscovering doc/ failed: {error:#}"),
        }
        let mut end = read_all(root, paths);
        let mut turns = self.turns();
        let Some(texts) = turns.get_mut(turn) else {
            return Vec::new();
        };
        // Turns live for the whole process: an unchanged document shares its pin's text
        // instead of keeping a second copy.
        for (rel, text) in end.iter_mut() {
            if let Some(pin) = texts.pins.get(rel).filter(|pin| pin.hash == text.hash) {
                *text = pin.clone();
            }
        }
        texts.end = end;
        changes(&texts.pins, &texts.end)
    }

    /// The text `rel` had when `turn` began.
    pub fn turn_pin(&self, turn: &TurnId, rel: &Path) -> Option<PinnedText> {
        self.turns().get(turn)?.pins.get(rel).cloned()
    }

    /// The text `rel` had when `turn` ended.
    pub fn turn_end(&self, turn: &TurnId, rel: &Path) -> Option<PinnedText> {
        self.turns().get(turn)?.end.get(rel).cloned()
    }

    fn turns(&self) -> MutexGuard<'_, HashMap<TurnId, TurnTexts>> {
        // The map holds plain values, so a panic elsewhere cannot leave it inconsistent.
        self.turns.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

/// Discovery never lists the summary store; the watcher's batches do.
fn is_held(rel: &Path) -> bool {
    rel.starts_with("doc") && !rel.starts_with(SUMMARIES)
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
