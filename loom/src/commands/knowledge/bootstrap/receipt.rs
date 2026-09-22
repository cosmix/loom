//! Bootstrap receipt: the cluster digests of the last completed run.
//!
//! `--refresh` reads this to decide which [`super::clusters::Cluster`]s a new
//! run must actually explore. The receipt is committed to git and can be
//! hand-edited by a teammate, so it is treated as untrusted input: unknown
//! fields, a bad version, or a symlinked/oversized file are all errors, never
//! a panic.

use std::collections::{BTreeMap, BTreeSet};
use std::io::ErrorKind;
use std::path::Path;

use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};

use super::clusters::Cluster;

/// Receipt file name, inside the knowledge root.
pub(super) const RECEIPT_FILENAME: &str = ".bootstrap-receipt.json";
/// Bumped whenever the receipt's shape changes incompatibly; [`Receipt::load`]
/// refuses any other version.
pub(super) const RECEIPT_VERSION: u32 = 1;

/// Cap on the receipt file's size; well past any real receipt, small enough
/// that a planted huge file is rejected rather than read.
pub(super) const MAX_RECEIPT_BYTES: usize = 1 << 20;

/// The record of a completed bootstrap run.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Receipt {
    pub version: u32,
    /// `ResolvedGraph::base_revision` at run time.
    pub source_revision: String,
    pub model: String,
    pub effort: String,
    /// RFC 3339, UTC.
    pub completed_at: String,
    pub clusters: Vec<ReceiptCluster>,
}

/// One cluster's recorded state as of the run that produced [`Receipt`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ReceiptCluster {
    pub id: String,
    pub digest: String,
    pub files: usize,
}

/// A cluster's status relative to the last completed run.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum ClusterStatus {
    /// No receipt entry for this cluster id.
    New,
    /// A receipt entry exists but its digest differs.
    Changed,
    /// A receipt entry exists with the same digest.
    Unchanged,
}

/// What a new run would do with each current cluster, and which receipt
/// entries no longer correspond to any current cluster.
#[derive(Debug, PartialEq, Eq)]
pub(super) struct RefreshPlan {
    /// One per current cluster, in the order `clusters` was given.
    pub statuses: Vec<(String, ClusterStatus)>,
    /// Receipt cluster ids absent from the current partition, sorted.
    pub removed: Vec<String>,
}

impl Receipt {
    /// Load the receipt beneath `knowledge_root`, or `Ok(None)` when it does
    /// not exist. Any other failure to stat, read, parse, or validate it is
    /// an error naming the path.
    pub(super) fn load(knowledge_root: &Path) -> Result<Option<Receipt>> {
        let path = knowledge_root.join(RECEIPT_FILENAME);
        match std::fs::symlink_metadata(&path) {
            Ok(_) => {}
            Err(error) if error.kind() == ErrorKind::NotFound => return Ok(None),
            Err(error) => {
                return Err(error).with_context(|| {
                    format!("Failed to stat bootstrap receipt: {}", path.display())
                });
            }
        }

        // No-follow, size-bounded read: a symlinked or oversized receipt is
        // an error here, never a read outside `knowledge_root`.
        let content = crate::fs::safe_read::read_to_string_bounded(
            knowledge_root,
            Path::new(RECEIPT_FILENAME),
            MAX_RECEIPT_BYTES,
        )
        .with_context(|| format!("Failed to read bootstrap receipt: {}", path.display()))?;

        let receipt: Receipt = serde_json::from_str(&content)
            .with_context(|| format!("Failed to parse bootstrap receipt: {}", path.display()))?;
        if receipt.version != RECEIPT_VERSION {
            bail!(
                "Bootstrap receipt {} has version {} but this build expects {}",
                path.display(),
                receipt.version,
                RECEIPT_VERSION
            );
        }
        Ok(Some(receipt))
    }

    /// Write this receipt beneath `knowledge_root` as pretty JSON with a
    /// trailing newline, under a directory lock.
    pub(super) fn save(&self, knowledge_root: &Path) -> Result<()> {
        std::fs::create_dir_all(knowledge_root).with_context(|| {
            format!(
                "Failed to create knowledge directory: {}",
                knowledge_root.display()
            )
        })?;
        let path = knowledge_root.join(RECEIPT_FILENAME);
        let mut content =
            serde_json::to_string_pretty(self).context("Failed to serialize bootstrap receipt")?;
        content.push('\n');
        crate::fs::locking::locked_write(&path, &content)
            .with_context(|| format!("Failed to write bootstrap receipt: {}", path.display()))
    }

    /// Build the receipt for a run that just explored `clusters`.
    pub(super) fn from_clusters(
        clusters: &[Cluster],
        source_revision: &str,
        model: &str,
        effort: &str,
    ) -> Receipt {
        Receipt {
            version: RECEIPT_VERSION,
            source_revision: source_revision.to_string(),
            model: model.to_string(),
            effort: effort.to_string(),
            completed_at: chrono::Utc::now().to_rfc3339(),
            clusters: clusters
                .iter()
                .map(|cluster| ReceiptCluster {
                    id: cluster.id.clone(),
                    digest: cluster.digest.clone(),
                    files: cluster.files.len(),
                })
                .collect(),
        }
    }
}

/// Classify every current cluster against the last completed run. With no
/// receipt, every cluster is [`ClusterStatus::New`] and nothing is removed.
pub(super) fn refresh_plan(clusters: &[Cluster], receipt: Option<&Receipt>) -> RefreshPlan {
    let previous: BTreeMap<&str, &str> = receipt
        .map(|receipt| {
            receipt
                .clusters
                .iter()
                .map(|cluster| (cluster.id.as_str(), cluster.digest.as_str()))
                .collect()
        })
        .unwrap_or_default();

    let mut seen = BTreeSet::new();
    let statuses = clusters
        .iter()
        .map(|cluster| {
            seen.insert(cluster.id.as_str());
            let status = match previous.get(cluster.id.as_str()) {
                None => ClusterStatus::New,
                Some(&digest) if digest == cluster.digest => ClusterStatus::Unchanged,
                Some(_) => ClusterStatus::Changed,
            };
            (cluster.id.clone(), status)
        })
        .collect();

    let removed = previous
        .keys()
        .filter(|id| !seen.contains(*id))
        .map(|id| id.to_string())
        .collect();

    RefreshPlan { statuses, removed }
}
