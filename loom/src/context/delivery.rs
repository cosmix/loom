//! Records of what retrieval actually handed to a recipient.
//!
//! A delivery record answers one question — "has this recipient already been
//! given these exact bytes?" — so a second retrieval in the same session can
//! skip what the first already quoted instead of repeating it.
//!
//! The record is an *optimisation*, never state the run depends on. Nothing here
//! may fail a spawn or a hook: a missing directory reads as "nothing delivered",
//! and an unreadable or malformed file is skipped rather than propagated.
//!
//! Suppression is scoped to a [`context_epoch`]: once a derived layer is
//! rebuilt, the same id may describe different bytes, so every record from an
//! older epoch is ignored and delivery re-opens.

use crate::context::graph_store::GraphStore;
use crate::context::retrieve::context_epoch;
use crate::context::schema::ContextPack;
use crate::fs::safe_fs::{
    flock_exclusive, open_safely, safe_create_dir_all_in_workdir, safe_open_dirfd,
};
use crate::fs::safe_read::{list_dir_no_follow, read_bounded_at, EntryKind, OverLimit};
use crate::fs::safe_replace::publish_in_dir;
use crate::models::stage::Stage;
use anyhow::{bail, Context, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use std::fs;
use std::os::fd::AsRawFd;
use std::path::{Path, PathBuf};
use uuid::Uuid;

/// Session identity, delivery deduplication, and compaction reset.
mod session;
pub use session::{
    delivered_to_session, discard_session_delivery, hook_recipient_id, worker_recipient_id,
};

/// Delivery records live beside the stage overlay, in this subdirectory.
const DELIVERY_RELATIVE_DIR: &str = "session-retrieval";
const WORKER_DELIVERY_RELATIVE_DIR: &str = "worker-brief";
/// Accumulated records exceeding this limit are left unchanged; later
/// deliveries may be repeated rather than losing the previous union.
pub(crate) const DELIVERY_RECORD_MAX_BYTES: usize = 4 * 1024 * 1024;

/// Plan component used for a stage that names no plan of its own.
const DEFAULT_PLAN_KEY: &str = "default";

/// The plan component of a stage's context overlay path.
///
/// Every writer and reader of a delivery record must derive this the same way:
/// the path is the join key, so two derivations file records where nothing looks
/// for them — a spawn record under `default/` that the prompt hook then misses
/// under the configured plan's directory, silently re-delivering everything the
/// spawn brief already carried.
pub fn plan_key(stage: &Stage) -> &str {
    plan_key_from(stage.plan_id.as_deref())
}

/// [`plan_key`] for a caller that holds the plan id but not the [`Stage`].
///
/// The two MUST agree: this is the same derivation, exposed for the paths that
/// reach a plan id another way. A blank id is treated as absent, because an
/// empty path component would file the record in the stage directory's parent.
pub fn plan_key_from(plan_id: Option<&str>) -> &str {
    plan_id
        .map(str::trim)
        .filter(|id| !id.is_empty())
        .unwrap_or(DEFAULT_PLAN_KEY)
}

/// One unit handed to one recipient.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeliveredNode {
    /// Identity of the delivered unit, as carried on the pack item.
    pub node_id: String,
    /// The unit's content hash at delivery time, so a changed body re-delivers.
    pub content_hash: String,
}

/// What was delivered to whom, when, and against which derived generation.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DeliveryRecord {
    /// Who received it: a session id, or a hook invocation's recipient key.
    pub recipient_id: String,
    /// Unique per delivery event; two deliveries never share one. On a record
    /// that [`record_delivery`] has folded into, it names the newest event.
    pub launch_id: String,
    /// [`context_epoch`] of the pack that produced this delivery.
    pub context_epoch: String,
    /// Selected units — in pack order as built, as a sorted de-duplicated set
    /// once [`record_delivery`] has folded a later delivery into it.
    pub delivered: Vec<DeliveredNode>,
    /// When the record was built.
    pub written_at: DateTime<Utc>,
}

impl DeliveryRecord {
    /// Build from a pack, stamping a fresh `launch_id`.
    pub fn from_pack(recipient_id: impl Into<String>, pack: &ContextPack) -> Self {
        DeliveryRecord {
            recipient_id: recipient_id.into(),
            launch_id: Uuid::new_v4().to_string(),
            context_epoch: context_epoch(pack),
            delivered: pack
                .items
                .iter()
                .map(|item| DeliveredNode {
                    node_id: item.id.as_str().to_string(),
                    content_hash: item.content_hash.clone(),
                })
                .collect(),
            written_at: Utc::now(),
        }
    }
}

/// The stage overlay directory, resolved through [`GraphStore`] so a delivery
/// record always lands beside the overlay it describes rather than at a
/// hand-joined path that could drift from it.
///
/// [`GraphStore::overlay_dir`] reads only the store's work root, so the cache
/// root handed to the constructor never reaches the returned path; resolving the
/// real one would make this fallible for no gain. The store stays local to this
/// helper so no caller can reach its base-layer paths through it.
fn stage_overlay_dir(work_dir: &Path, plan: &str, stage: &str) -> PathBuf {
    GraphStore::new(work_dir, work_dir).overlay_dir(plan, stage)
}

/// `.loom/work/context/<plan>/<stage>/session-retrieval/`
pub fn delivery_dir(work_dir: &Path, plan: &str, stage: &str) -> PathBuf {
    stage_overlay_dir(work_dir, plan, stage).join(DELIVERY_RELATIVE_DIR)
}

/// Worker-brief delivery records, filed at issue under their nonce-derived
/// key and kept apart from the prompt and spawn records.
pub(crate) fn worker_delivery_dir(work_dir: &Path, plan: &str, stage: &str) -> PathBuf {
    delivery_dir(work_dir, plan, stage).join(WORKER_DELIVERY_RELATIVE_DIR)
}

/// Reject a `recipient_id` that cannot safely become a file name.
///
/// The id reaches the filesystem as `<recipient_id>.json`, so anything that
/// could escape the delivery directory — a separator, a parent-directory hop —
/// or that is not a plain portable file-name character is refused outright
/// rather than sanitized: a silently rewritten id would file the delivery under
/// a name no reader ever looks up, which reads as "nothing delivered" forever.
fn validate_recipient_id(recipient_id: &str) -> Result<()> {
    if recipient_id.is_empty() {
        bail!("Invalid recipient id: an empty id cannot name a delivery record file");
    }
    if recipient_id.contains('/') || recipient_id.contains('\\') {
        bail!("Invalid recipient id '{recipient_id}': a path separator escapes the directory");
    }
    if recipient_id.contains("..") {
        bail!("Invalid recipient id '{recipient_id}': '..' escapes the delivery directory");
    }
    if let Some(bad) = recipient_id
        .chars()
        .find(|c| !c.is_ascii_alphanumeric() && !matches!(c, '.' | '_' | '-'))
    {
        bail!("Invalid recipient id '{recipient_id}': character '{bad}' is outside [A-Za-z0-9._-]");
    }
    Ok(())
}

/// Persist `record` to `<delivery_dir>/<recipient_id>.json`, crash-atomically
/// and under the per-stage directory lock.
///
/// A recipient id is deliberately stable — the prompt hook uses `prompt-<stage>`
/// for every question a stage asks — so repeated deliveries converge on one
/// file. Converging on one file means this MUST accumulate: a replacing write
/// would erase the previous delivery's set, and the next question would re-quote
/// verbatim what the one before it was already given, which is precisely what
/// the epoch suppression exists to prevent.
///
/// So `record` is folded into what is already on disk when both describe the
/// same [`context_epoch`], and replaces it outright when they do not — a rebuilt
/// derived layer legitimately re-opens delivery. The read and the write share
/// one critical section; reading outside the lock would be a lost update.
pub fn record_delivery(
    work_dir: &Path,
    plan: &str,
    stage: &str,
    record: &DeliveryRecord,
) -> Result<()> {
    validate_recipient_id(&record.recipient_id)?;

    fs::create_dir_all(work_dir)?;
    let root = fs::canonicalize(work_dir)?;
    let root_fd = safe_open_dirfd(&root)?;
    let dir = delivery_dir(work_dir, plan, stage);
    let relative = dir.strip_prefix(work_dir)?;
    safe_create_dir_all_in_workdir(root_fd.as_raw_fd(), relative, 0o777)?;
    let dir_fd = open_safely(
        root_fd.as_raw_fd(),
        relative,
        libc::O_RDONLY | libc::O_DIRECTORY | libc::O_NONBLOCK,
        0,
    )?;
    flock_exclusive(&dir_fd)?;
    let name = format!("{}.json", record.recipient_id);
    let merged = merge_with_recorded(&dir_fd, Path::new(&name), record)?;
    let json = serde_json::to_string_pretty(&merged)
        .context("Failed to serialize a context delivery record")?;
    if json.len() > DELIVERY_RECORD_MAX_BYTES {
        bail!("context delivery record exceeds {DELIVERY_RECORD_MAX_BYTES} bytes");
    }
    publish_in_dir(&dir_fd, name.as_ref(), json.as_bytes(), 0o666, false)
}

/// `record` folded into the record already at `path`, or `record` unchanged when
/// there is nothing to fold — no file, an unreadable or malformed one, or one
/// written under a different epoch. An oversized record is preserved and the
/// update is rejected because its existing union cannot be safely loaded.
///
/// The caller holds the delivery directory lock; the bounded reader takes no
/// lock of its own, avoiding a recursive directory lock.
fn merge_with_recorded(
    dir: &std::os::fd::OwnedFd,
    path: &Path,
    record: &DeliveryRecord,
) -> Result<DeliveryRecord> {
    let raw = match read_bounded_at(dir, path, DELIVERY_RECORD_MAX_BYTES) {
        Ok(raw) => String::from_utf8(raw).ok(),
        Err(error) if error.downcast_ref::<OverLimit>().is_some() => return Err(error),
        Err(_) => None,
    };
    let existing = raw
        .and_then(|raw| serde_json::from_str::<DeliveryRecord>(&raw).ok())
        .filter(|existing| existing.context_epoch == record.context_epoch);
    let Some(existing) = existing else {
        return Ok(record.clone());
    };

    // Sorted and de-duplicated so the folded set is the same however the
    // deliveries interleaved. Nothing reads this list in order: every consumer
    // turns it straight into a set.
    let union: BTreeSet<(String, String)> = existing
        .delivered
        .iter()
        .chain(record.delivered.iter())
        .map(|node| (node.node_id.clone(), node.content_hash.clone()))
        .collect();

    // The newest delivery names the record: `launch_id` and `written_at`
    // describe the event that last touched it, and its set is everything the
    // recipient holds for this epoch.
    Ok(DeliveryRecord {
        delivered: union
            .into_iter()
            .map(|(node_id, content_hash)| DeliveredNode {
                node_id,
                content_hash,
            })
            .collect(),
        ..record.clone()
    })
}

/// Every record written for this stage, ordered by recipient then launch.
///
/// Unreadable or malformed files are skipped, never fatal — a delivery record is
/// an optimisation, not state the run depends on. A directory that was never
/// written reads as no deliveries at all.
///
/// Reading takes no lock: [`record_delivery`] replaces a file by `rename`, so a
/// concurrent reader observes either the whole previous file or the whole new
/// one, and anything else is skipped by the malformed-file path.
pub fn load_deliveries(work_dir: &Path, plan: &str, stage: &str) -> Result<Vec<DeliveryRecord>> {
    let dir = delivery_dir(work_dir, plan, stage);
    let worker_dir = worker_delivery_dir(work_dir, plan, stage);
    let mut records = Vec::new();
    let Ok(root) = fs::canonicalize(work_dir) else {
        return Ok(records);
    };
    let Ok(root_fd) = safe_open_dirfd(&root) else {
        return Ok(records);
    };
    for dir in [&dir, &worker_dir] {
        if let Ok(relative) = dir.strip_prefix(work_dir) {
            load_delivery_dir(&root_fd, relative, &mut records);
        }
    }

    // `read_dir` order is unspecified; sort so two reads of one directory agree.
    records.sort_by(|left, right| {
        left.recipient_id
            .cmp(&right.recipient_id)
            .then_with(|| left.launch_id.cmp(&right.launch_id))
    });
    Ok(records)
}

fn load_delivery_dir(
    root_fd: &std::os::fd::OwnedFd,
    dir: &Path,
    records: &mut Vec<DeliveryRecord>,
) {
    let Ok(entries) = list_dir_no_follow(root_fd, dir) else {
        return;
    };
    for (name, kind) in entries {
        let path = dir.join(name);
        if kind != EntryKind::File {
            continue;
        }
        if path.extension().and_then(|ext| ext.to_str()) != Some("json") {
            continue;
        }
        let Ok(bytes) = read_bounded_at(root_fd, &path, DELIVERY_RECORD_MAX_BYTES) else {
            tracing::debug!("skipping unreadable delivery record: {}", path.display());
            continue;
        };
        match serde_json::from_slice::<DeliveryRecord>(&bytes) {
            Ok(record) => records.push(record),
            Err(error) => {
                tracing::debug!(
                    "skipping malformed delivery record {}: {error}",
                    path.display()
                );
            }
        }
    }
}

/// `(node_id, content_hash)` pairs already delivered under `epoch`.
///
/// Records from any other epoch are ignored: once a derived layer is rebuilt,
/// re-delivering the same id is correct, because its bytes may have changed.
pub fn delivered_in_epoch(records: &[DeliveryRecord], epoch: &str) -> BTreeSet<(String, String)> {
    records
        .iter()
        .filter(|record| record.context_epoch == epoch)
        .flat_map(|record| record.delivered.iter())
        .map(|node| (node.node_id.clone(), node.content_hash.clone()))
        .collect()
}

/// Chunk ids already delivered to the stages `dependencies` names.
///
/// These are the units a dependency stage was actually given, so they are the
/// best available answer to "what did the work I build on read?". Unreadable or
/// absent records contribute nothing — a missing dependency record means no
/// boost, never an error.
pub fn dependency_chunk_ids(work_dir: &Path, plan: &str, dependencies: &[String]) -> Vec<String> {
    let mut ids: BTreeSet<String> = BTreeSet::new();
    for dependency in dependencies {
        let Ok(records) = load_deliveries(work_dir, plan, dependency) else {
            continue;
        };
        for record in &records {
            ids.extend(record.delivered.iter().map(|node| node.node_id.clone()));
        }
    }
    ids.into_iter().collect()
}
