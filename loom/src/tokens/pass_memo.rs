//! What a thread's tracker passes over one work root remember between passes,
//! so a pass that changes nothing reads and writes as little as it can: the
//! checkpoint as the last pass left it with the stamp and hash of its last
//! save, the hash, `observed_at` and stamp of the last `flow.json` written,
//! and the last `quota_left` with the stamps of the files it read.
//!
//! Nothing is reused unless the file it stands for still carries the stamp
//! the memo took, and a failed or over-cap write forgets what it would have
//! replaced, so the next pass reads from disk as a pass without a memo does.

use super::checkpoint::{Checkpoint, Saved, MAX_CHECKPOINT_BYTES};
use super::flow_build::fit_flow;
use super::model::{QuotaLeft, TokenFlow, FLOW_FILE};
use super::quota_points::{self, quota_inputs};
use super::tracker::{write_flow, CHECKPOINT_FILE};
use anyhow::{Context, Result};
use std::collections::hash_map::{DefaultHasher, RandomState};
use std::collections::HashSet;
use std::hash::{BuildHasher, Hasher};
use std::io::{self, Write};
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

/// An unchanged flow is still rewritten this often, so readers see a fresh
/// `observed_at`.
pub(super) const FLOW_HEARTBEAT_SECS: i64 = 60;
/// How long a computed `quota_left` is reused while no quota file changes.
pub(super) const QUOTA_LEFT_REFRESH_SECS: i64 = 30;

/// The keys of every hash a memo takes. The hashed JSON derives from transcripts, so
/// a fixed key would let a transcript be crafted to collide; these are random per
/// process and fixed for its life, which is as long as a memo lives.
static HASH_KEYS: OnceLock<RandomState> = OnceLock::new();

fn new_hasher() -> DefaultHasher {
    HASH_KEYS.get_or_init(RandomState::new).build_hasher()
}

/// A file's identity and version: a rename onto the path changes the inode,
/// a write the length or the mtime.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct FileStamp {
    inode: u64,
    len: u64,
    mtime_nanos: i128,
}

impl FileStamp {
    /// The stamp of `path` itself, without following a symlink; `None` when
    /// it cannot be read, as when the file is missing.
    pub(super) fn of(path: &Path) -> Option<Self> {
        let metadata = std::fs::symlink_metadata(path).ok()?;
        let mtime_nanos =
            i128::from(metadata.mtime()) * 1_000_000_000 + i128::from(metadata.mtime_nsec());
        Some(Self {
            inode: metadata.ino(),
            len: metadata.len(),
            mtime_nanos,
        })
    }
}

/// The memo of one work root, kept by the thread that runs its passes.
pub(super) struct PassMemo {
    work_root: PathBuf,
    /// The checkpoint the last pass ended with; set only while `saved`
    /// describes the file holding its JSON.
    checkpoint: Option<Checkpoint>,
    saved: Option<LastSave>,
    flow: Option<LastFlow>,
    quota: Option<LastQuota>,
}

/// `checkpoint.json` right after the memo's last save, and the hash of the
/// compact JSON saved.
#[derive(Debug, Clone, Copy)]
struct LastSave {
    stamp: FileStamp,
    hash: u64,
}

/// The last `flow.json` written: the hash of its JSON with `observed_at`
/// zeroed, the `observed_at` it carries, and its stamp right after the write.
struct LastFlow {
    hash: u64,
    observed_at: i64,
    stamp: FileStamp,
}

/// The last `quota_left`, the `now` it was computed at, and the stamps the
/// files it read had just before.
struct LastQuota {
    left: Vec<QuotaLeft>,
    computed_at: i64,
    stamps: [Option<FileStamp>; 4],
}

impl PassMemo {
    /// An empty memo for `work_root`, taken as given.
    pub(super) fn new(work_root: &Path) -> Self {
        Self {
            work_root: work_root.to_path_buf(),
            checkpoint: None,
            saved: None,
            flow: None,
            quota: None,
        }
    }

    /// Whether this memo belongs to `work_root`; another path, even one
    /// naming the same directory, gets a fresh memo.
    pub(super) fn is_for(&self, work_root: &Path) -> bool {
        self.work_root == work_root
    }

    /// The checkpoint a pass starts from: the memo's while `checkpoint.json`
    /// is as the memo's last save left it, else the file's, as
    /// [`Checkpoint::load_reporting`] reads it, with its warning. The memo
    /// holds no checkpoint again until [`Self::save_checkpoint`] returns one.
    pub(super) fn take_checkpoint(&mut self, tokens_dir: &Path) -> (Checkpoint, Option<String>) {
        let stamp = FileStamp::of(&tokens_dir.join(CHECKPOINT_FILE));
        match (self.checkpoint.take(), self.saved) {
            (Some(checkpoint), Some(saved)) if Some(saved.stamp) == stamp => (checkpoint, None),
            _ => {
                self.saved = None;
                Checkpoint::load_reporting(tokens_dir)
            }
        }
    }

    /// Saves `checkpoint` unless its compact JSON is the JSON of the memo's
    /// last save, which `checkpoint.json` still held when the pass began, and
    /// keeps it for the next pass. Over [`MAX_CHECKPOINT_BYTES`], first
    /// empties the message tables of every agent not in `advanced` (their
    /// files did not change this pass) and tries again. The warning when it
    /// is still over: nothing is written, and the next pass loads the
    /// checkpoint before this one from disk. A save that fails forgets the
    /// checkpoint too.
    pub(super) fn save_checkpoint(
        &mut self,
        mut checkpoint: Checkpoint,
        tokens_dir: &Path,
        advanced: &HashSet<(&str, &str)>,
    ) -> Result<Option<String>> {
        let hash = json_hash(&checkpoint)?;
        if self.saved.is_some_and(|saved| saved.hash == hash) {
            self.checkpoint = Some(checkpoint);
            return Ok(None);
        }
        self.saved = None;
        let (saved, hash) = save_shedding(&mut checkpoint, tokens_dir, advanced, hash)?;
        if let Saved::OverCap(bytes) = saved {
            return Ok(Some(format!(
                "checkpoint is {bytes} bytes, over {MAX_CHECKPOINT_BYTES} even without idle \
                 message tables; {CHECKPOINT_FILE} not written"
            )));
        }
        // `atomic_write` renames a new file into place: stamp it after the write.
        if let Some(stamp) = FileStamp::of(&tokens_dir.join(CHECKPOINT_FILE)) {
            self.saved = Some(LastSave { stamp, hash });
            self.checkpoint = Some(checkpoint);
        }
        Ok(None)
    }

    /// Writes `flow` as [`write_flow`] does, unless `flow.json` already holds
    /// it: its JSON is unchanged but for `observed_at`, the last write was
    /// less than [`FLOW_HEARTBEAT_SECS`] before `now`, and the file is as that
    /// write left it. `false` when the flow does not fit and nothing was
    /// written. A write that fails forgets the last one.
    pub(super) fn publish_flow(
        &mut self,
        tokens_dir: &Path,
        flow: &mut TokenFlow,
        now: i64,
    ) -> Result<bool> {
        let hash = flow_hash(flow)?;
        let path = tokens_dir.join(FLOW_FILE);
        if let (Some(hash), Some(last)) = (hash, &self.flow) {
            let fresh = (0..FLOW_HEARTBEAT_SECS).contains(&(now - last.observed_at));
            if last.hash == hash && fresh && FileStamp::of(&path) == Some(last.stamp) {
                return Ok(true);
            }
        }
        self.flow = None;
        let written = write_flow(tokens_dir, flow)?;
        if let (true, Some(hash), Some(stamp)) = (written, hash, FileStamp::of(&path)) {
            self.flow = Some(LastFlow {
                hash,
                observed_at: flow.observed_at,
                stamp,
            });
        }
        Ok(written)
    }

    /// [`quota_points::quota_left`] at `now`, reused from an earlier pass
    /// while it is less than [`QUOTA_LEFT_REFRESH_SECS`] old and no file it
    /// read changed since.
    pub(super) fn quota_left(&mut self, work_root: &Path, now: i64) -> Vec<QuotaLeft> {
        // Stamped before the read: a file that changes during it reads as
        // changed on the next pass.
        let stamps = quota_inputs(work_root).map(|path| FileStamp::of(&path));
        if let Some(last) = &self.quota {
            let fresh = (0..QUOTA_LEFT_REFRESH_SECS).contains(&(now - last.computed_at));
            if fresh && last.stamps == stamps {
                return last.left.clone();
            }
        }
        let left = quota_points::quota_left(work_root, now);
        self.quota = Some(LastQuota {
            left: left.clone(),
            computed_at: now,
            stamps,
        });
        left
    }
}

/// Saves `checkpoint`, whose JSON hashes to `hash`; over the cap, sheds the
/// tables of agents not in `advanced` and saves again. What the last save did
/// and the hash of the checkpoint it was given.
fn save_shedding(
    checkpoint: &mut Checkpoint,
    tokens_dir: &Path,
    advanced: &HashSet<(&str, &str)>,
    hash: u64,
) -> Result<(Saved, u64)> {
    let saved = checkpoint.save(tokens_dir)?;
    if saved == Saved::Written {
        return Ok((saved, hash));
    }
    checkpoint.clear_tables_except(advanced);
    let saved = checkpoint.save(tokens_dir)?;
    let hash = match saved {
        Saved::Written => json_hash(checkpoint)?,
        Saved::OverCap(_) => hash,
    };
    Ok((saved, hash))
}

/// The hash of the JSON [`fit_flow`] makes of `flow` with `observed_at`
/// zeroed; `None` when it does not fit. Sheds `flow`'s idle series when
/// `fit_flow` does, as the write that follows would.
fn flow_hash(flow: &mut TokenFlow) -> Result<Option<u64>> {
    let observed_at = std::mem::replace(&mut flow.observed_at, 0);
    let json = fit_flow(flow);
    flow.observed_at = observed_at;
    Ok(json?.map(|json| {
        let mut hasher = new_hasher();
        hasher.write(json.as_bytes());
        hasher.finish()
    }))
}

/// The hash of `checkpoint`'s compact JSON, streamed into the hasher rather
/// than built as a string. The keys are those of [`HASH_KEYS`].
fn json_hash(checkpoint: &Checkpoint) -> Result<u64> {
    let mut writer = HashWriter(new_hasher());
    serde_json::to_writer(&mut writer, checkpoint).context("hashing the token checkpoint")?;
    Ok(writer.0.finish())
}

struct HashWriter(DefaultHasher);

impl Write for HashWriter {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.0.write(buf);
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

#[cfg(test)]
#[path = "pass_memo_tests.rs"]
mod tests;
