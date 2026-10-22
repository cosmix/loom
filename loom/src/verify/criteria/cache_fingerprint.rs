//! Reusable-input fingerprints for acceptance cache entries.

use sha2::digest::Output;
use sha2::{Digest, Sha256};
use std::fs::{File, Metadata};
use std::io::Read;
use std::os::unix::fs::OpenOptionsExt;
use std::path::{Path, PathBuf};
use std::process::Command;

use super::cache_executable;
use super::cache_git_state::hash_git_state;
use super::confine::{GitDirs, PreparedCommand};
use crate::models::stage::CommandConfinement;
use crate::verify::review::fingerprint::checkout_git;

#[path = "cache_manifest.rs"]
mod manifest;

const HASH_BUDGET_BYTES: u64 = 64 * 1024 * 1024;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct InputFingerprint {
    pub(super) digest: String,
}

/// Stable inputs taken from the exact prepared command that will be spawned.
#[derive(Debug, Clone)]
pub(super) struct ExecutionIdentity {
    executable_paths: Vec<PathBuf>,
    environment_digest: String,
    containment_policy: String,
    /// The Git directories the command can read; their state is an input.
    git: Option<GitDirs>,
}

impl ExecutionIdentity {
    pub(super) fn from_prepared(
        prepared: &PreparedCommand,
        confinement: CommandConfinement,
    ) -> Option<Self> {
        if confinement == CommandConfinement::Inherit {
            return None;
        }
        let command = prepared.command();
        let mut executable_paths = cache_executable::resolve(prepared)?;
        executable_paths.push(prepared.containment_executable()?);
        Some(Self {
            executable_paths,
            environment_digest: hash_environment(command)?,
            containment_policy: prepared.containment_policy_revision(),
            git: prepared.git_dirs().cloned(),
        })
    }
}

pub(super) fn capture(
    acceptance_dir: &Path,
    identity: &ExecutionIdentity,
) -> Option<InputFingerprint> {
    capture_with_budget(acceptance_dir, identity, HASH_BUDGET_BYTES)
}

pub(super) fn capture_with_budget(
    acceptance_dir: &Path,
    identity: &ExecutionIdentity,
    hash_budget: u64,
) -> Option<InputFingerprint> {
    let acceptance_dir = acceptance_dir.canonicalize().ok()?;
    // The host child fingerprints outside every sandbox, and a criterion can
    // rewrite the worktree's `.git` file: `checkout_git` pins git.
    let git = checkout_git(&acceptance_dir).ok()?;
    let repo_root = git.work_tree().to_path_buf();
    if !acceptance_dir.starts_with(&repo_root) {
        return None;
    }

    let mut hasher = Sha256::new();
    hash_field(&mut hasher, b"acceptance-cache-input-v5");
    hash_field(&mut hasher, acceptance_dir.as_os_str().as_encoded_bytes());
    hash_field(&mut hasher, repo_root.as_os_str().as_encoded_bytes());
    hash_field(&mut hasher, identity.environment_digest.as_bytes());
    hash_field(&mut hasher, identity.containment_policy.as_bytes());
    let mut remaining = hash_budget;
    for executable in &identity.executable_paths {
        hash_field(&mut hasher, executable.as_os_str().as_encoded_bytes());
        hash_executable(executable, &mut remaining, &mut hasher)?;
    }
    manifest::hash_manifest(&git, &repo_root, &mut remaining, &mut hasher)?;
    hash_git_state(identity.git.as_ref(), &mut hasher)?;
    Some(InputFingerprint {
        digest: hex::encode(hasher.finalize()),
    })
}

fn hash_executable(invoked_path: &Path, remaining: &mut u64, hasher: &mut Sha256) -> Option<()> {
    let canonical = invoked_path.canonicalize().ok()?;
    let metadata = canonical.metadata().ok()?;
    if !metadata.is_file() || !cache_executable::is_executable(&metadata) {
        return None;
    }
    hash_field(hasher, canonical.as_os_str().as_encoded_bytes());
    let digest = digest_regular_file(&canonical, &metadata, remaining, Sha256::new())?;
    hash_field(hasher, &digest);
    Some(())
}

/// Feed the regular file at `path`, whose `before` metadata the caller read,
/// to `file_hasher` within `remaining` bytes, refusing it when it changed
/// while being read. `O_NONBLOCK` keeps a FIFO swapped in since that check
/// from blocking the open.
fn digest_regular_file<D: Digest>(
    path: &Path,
    before: &Metadata,
    remaining: &mut u64,
    mut file_hasher: D,
) -> Option<Output<D>> {
    if before.len() > *remaining {
        return None;
    }
    *remaining -= before.len();
    let mut file = File::options()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
        .open(path)
        .ok()?;
    let mut bytes_read = 0_u64;
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let count = file.read(&mut buffer).ok()?;
        if count == 0 {
            break;
        }
        bytes_read = bytes_read.checked_add(u64::try_from(count).ok()?)?;
        file_hasher.update(&buffer[..count]);
    }
    let after = file.metadata().ok()?;
    if bytes_read != before.len() || !after.is_file() || !same_file_snapshot(before, &after) {
        return None;
    }
    Some(file_hasher.finalize())
}

fn same_file_snapshot(before: &Metadata, after: &Metadata) -> bool {
    if before.len() != after.len() {
        return false;
    }
    matches!(
        (before.modified().ok(), after.modified().ok()),
        (Some(before), Some(after)) if before == after
    )
}

fn hash_environment(command: &Command) -> Option<String> {
    let mut entries = command
        .get_envs()
        .map(|(key, value)| {
            Some((
                key.as_encoded_bytes().to_vec(),
                value?.as_encoded_bytes().to_vec(),
            ))
        })
        .collect::<Option<Vec<_>>>()?;
    entries.sort();
    let mut hasher = Sha256::new();
    for (key, value) in entries {
        hash_field(&mut hasher, &key);
        hash_field(&mut hasher, &value);
    }
    Some(hex::encode(hasher.finalize()))
}

pub(super) fn hash_field(hasher: &mut Sha256, value: &[u8]) {
    hasher.update(u64::try_from(value.len()).unwrap_or(u64::MAX).to_le_bytes());
    hasher.update(value);
}

#[cfg(test)]
#[path = "cache_fingerprint_tests.rs"]
mod tests;
