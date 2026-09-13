//! On-disk cache of fully evaluated acceptance-criterion passes.
//!
//! Cache entries are optional evidence. Any uncertainty while establishing an
//! input fingerprint or reading a record becomes a miss and a real execution.

use anyhow::{Context, Result};
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
use std::time::Duration;

use super::cache_contract::{AssertionVerdict, CachedCriterionPass, CriterionContract};
use super::cache_fingerprint::InputFingerprint;
use super::cache_ignore;
use super::result::CriterionResult;

const CACHE_SUBDIR: &str = "acceptance-cache";
const MAX_CACHE_RECORD_BYTES: u64 = 32 * 1024;

/// Whether the acceptance runner may consult and update the pass cache.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CachePolicy {
    #[default]
    Use,
    Bypass,
}

impl CachePolicy {
    /// Read the process-wide emergency bypass once at configuration time.
    pub fn from_env() -> Self {
        match std::env::var("LOOM_ACCEPTANCE_CACHE") {
            Ok(value) if value.trim() == "0" => CachePolicy::Bypass,
            _ => CachePolicy::Use,
        }
    }
}

const FORBIDDEN_LITERALS: [&str; 4] = ["~/", "mktemp", "http://", "https://"];

/// Reject known ambient, ignored, and external inputs before fingerprinting.
pub(super) fn is_cacheable(command: &str, acceptance_dir: &Path) -> bool {
    if FORBIDDEN_LITERALS
        .iter()
        .any(|pattern| command.contains(*pattern))
    {
        return false;
    }
    if command.contains('$') {
        return false;
    }
    !cache_ignore::references_ignored_path(command, acceptance_dir)
}

/// Filename digest binding the semantic contract to its input fingerprint.
pub(super) fn compute_cache_key(
    contract: &CriterionContract,
    fingerprint: &InputFingerprint,
) -> Option<String> {
    let contract_digest = contract.digest()?;
    let mut hasher = Sha256::new();
    hasher.update(b"acceptance-cache-key-v2\0");
    hasher.update(contract_digest.as_bytes());
    hasher.update([0]);
    hasher.update(fingerprint.digest.as_bytes());
    Some(hex::encode(hasher.finalize()))
}

/// Return only a structurally complete record that certifies both digests.
pub(super) fn lookup_pass(
    work_dir: &Path,
    contract: &CriterionContract,
    fingerprint: &InputFingerprint,
) -> Option<CachedCriterionPass> {
    let digest = compute_cache_key(contract, fingerprint)?;
    let path = cache_file_path(work_dir, &digest);
    let metadata = path.symlink_metadata().ok()?;
    if !metadata.is_file() || metadata.len() > MAX_CACHE_RECORD_BYTES {
        return None;
    }
    let content = crate::fs::locking::locked_read(&path).ok()?;
    if u64::try_from(content.len()).ok()? > MAX_CACHE_RECORD_BYTES {
        return None;
    }
    let record: CachedCriterionPass = serde_json::from_str(&content).ok()?;
    record.certifies(contract, fingerprint).then_some(record)
}

/// Persist a pass only when its complete verdict is certifiable.
pub(super) fn store_pass(
    work_dir: &Path,
    contract: &CriterionContract,
    fingerprint: &InputFingerprint,
    result: &CriterionResult,
    original_duration: Duration,
    verdict: AssertionVerdict,
) -> Result<bool> {
    let Some(record) =
        CachedCriterionPass::from_result(contract, fingerprint, result, original_duration, verdict)
    else {
        return Ok(false);
    };
    let digest = compute_cache_key(contract, fingerprint)
        .context("Failed to digest acceptance cache contract")?;
    ensure_cache_dir(work_dir)?;
    let path = cache_file_path(work_dir, &digest);
    let json = serde_json::to_string_pretty(&record)
        .context("Failed to serialize acceptance cache record")?;
    if u64::try_from(json.len()).unwrap_or(u64::MAX) > MAX_CACHE_RECORD_BYTES {
        return Ok(false);
    }
    crate::fs::locking::locked_write(&path, &json)?;
    Ok(true)
}

pub(super) fn cache_file_path(work_dir: &Path, digest: &str) -> PathBuf {
    work_dir.join(CACHE_SUBDIR).join(format!("{digest}.json"))
}

fn ensure_cache_dir(work_dir: &Path) -> Result<PathBuf> {
    let dir = work_dir.join(CACHE_SUBDIR);
    let mut builder = std::fs::DirBuilder::new();
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        builder.mode(0o700);
    }
    match builder.create(&dir) {
        Ok(()) => {}
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
        Err(error) => {
            return Err(error).with_context(|| format!("Failed to create {}", dir.display()))
        }
    }
    Ok(dir)
}
