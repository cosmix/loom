//! Session delivery deduplication and compaction reset.
//!
//! Hook recipients are session-scoped; a spawned session also retains credit
//! for its spawn brief across compaction.

use super::{delivery_dir, validate_recipient_id, DeliveryRecord};
use crate::fs::locking::locked_dir_update;
use anyhow::{Context, Result};
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;
use std::fs;
use std::io::ErrorKind;
use std::path::Path;

/// Filesystem-safe prompt identity. Blank session ids share the nosession suffix;
/// other session ids are hashed before use as path components.
pub fn hook_recipient_id(scope: &str, session_id: Option<&str>) -> String {
    let suffix = match session_id.map(str::trim).filter(|id| !id.is_empty()) {
        Some(id) => session8(id),
        None => "nosession".to_string(),
    };
    format!("prompt-{scope}-{suffix}")
}

/// Filesystem-safe identity for one issued worker brief: the record's file name and
/// its recipient id.
///
/// All three inputs are hashed: the nonce is public (the brief's marker line
/// carries it), and neither a hook session nor a stage-controlled value may
/// become a path component. The `worker-` namespace also keeps these keys
/// disjoint from prompt-hook recipients.
pub fn worker_recipient_id(stage: &str, parent_session: &str, nonce: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(b"worker-brief\0");
    for value in [stage, parent_session, nonce] {
        hasher.update(value.len().to_le_bytes());
        hasher.update(value.as_bytes());
    }
    let result = hasher.finalize();
    format!("worker-{}", hex::encode(&result[..16]))
}

/// First 8 bytes of `sha256(value)`, hex-encoded: 16 characters, all drawn
/// from `[0-9a-f]`. Same two-step shape as
/// [`crate::context::retrieve::context_epoch`]'s own digest.
fn session8(value: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(value.as_bytes());
    let result = hasher.finalize();
    hex::encode(&result[..8])
}

/// Delivered pairs in this epoch for the hook recipient and, when applicable,
/// the spawn recipient identified by the caller.
pub fn delivered_to_session(
    records: &[DeliveryRecord],
    epoch: &str,
    recipient_id: &str,
    spawn_recipient: Option<&str>,
) -> BTreeSet<(String, String)> {
    records
        .iter()
        .filter(|record| {
            record.context_epoch == epoch
                && (record.recipient_id == recipient_id
                    || spawn_recipient == Some(record.recipient_id.as_str()))
        })
        .flat_map(|record| &record.delivered)
        .map(|node| (node.node_id.clone(), node.content_hash.clone()))
        .collect()
}

/// Reset one recipient after compaction without touching other records in the
/// shared delivery directory. Missing directories and records are already reset.
pub fn discard_session_delivery(
    work_dir: &Path,
    plan: &str,
    stage: &str,
    recipient_id: &str,
) -> Result<()> {
    validate_recipient_id(recipient_id)?;
    let dir = delivery_dir(work_dir, plan, stage);
    if !dir.exists() {
        return Ok(());
    }
    let path = dir.join(format!("{recipient_id}.json"));
    locked_dir_update(&dir, || match fs::remove_file(&path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error).with_context(|| format!("Failed to remove {}", path.display())),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::context::delivery::DeliveredNode;

    fn record(recipient: &str, epoch: &str, id: &str) -> DeliveryRecord {
        DeliveryRecord {
            recipient_id: recipient.into(),
            launch_id: String::new(),
            context_epoch: epoch.into(),
            delivered: vec![DeliveredNode {
                node_id: id.into(),
                content_hash: "hash".into(),
            }],
            written_at: chrono::Utc::now(),
        }
    }

    #[test]
    fn delivery_union_respects_recipient_and_epoch() {
        let records = [
            record("own", "a", "own"),
            record("spawn", "a", "spawn"),
            record("other", "a", "other"),
            record("own", "b", "old"),
        ];
        let ids = |spawn| {
            delivered_to_session(&records, "a", "own", spawn)
                .into_iter()
                .map(|(id, _)| id)
                .collect::<BTreeSet<_>>()
        };
        assert_eq!(ids(None), BTreeSet::from(["own".to_string()]));
        assert_eq!(ids(Some("own")), ids(None));
        assert_eq!(
            ids(Some("spawn")),
            BTreeSet::from(["own".to_string(), "spawn".to_string()])
        );
        assert_eq!(
            delivered_to_session(&records, "b", "own", Some("spawn"))
                .into_iter()
                .map(|(id, _)| id)
                .collect::<Vec<_>>(),
            vec!["old"]
        );
    }
}
