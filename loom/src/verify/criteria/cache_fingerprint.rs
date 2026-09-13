//! Reusable-input fingerprints for acceptance cache entries.

use sha2::{Digest, Sha256};
use std::fs::{File, Metadata};
use std::io::Read;
use std::path::{Component, Path, PathBuf};
use std::process::Command;

use super::cache_executable;
use super::confine::PreparedCommand;
use crate::git::runner::{run_git, run_git_checked};
use crate::models::stage::CommandConfinement;

const HASH_BUDGET_BYTES: u64 = 64 * 1024 * 1024;
const MAX_GIT_CONTEXT_BYTES: usize = 8 * 1024 * 1024;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct InputFingerprint {
    pub(super) digest: String,
    pub(super) tree_head: String,
}

/// Stable inputs taken from the exact prepared command that will be spawned.
#[derive(Debug, Clone)]
pub(super) struct ExecutionIdentity {
    executable_paths: Vec<PathBuf>,
    environment_digest: String,
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
        Some(Self {
            executable_paths: cache_executable::resolve(prepared)?,
            environment_digest: hash_environment(command)?,
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
    let repo_root = resolve_repo_root(acceptance_dir)?;
    let acceptance_dir = acceptance_dir.canonicalize().ok()?;
    if !acceptance_dir.starts_with(&repo_root) {
        return None;
    }
    let head = run_git_checked(&["rev-parse", "HEAD"], &repo_root).ok()?;
    let status = git_bytes(
        &["status", "--porcelain=v2", "--untracked-files=all", "-z"],
        &repo_root,
    )?;
    let tracked = git_bytes(&["ls-files", "-s", "-z"], &repo_root)?;
    if !tracked_entries_are_regular(&tracked) {
        return None;
    }

    let mut hasher = Sha256::new();
    hash_field(&mut hasher, b"acceptance-cache-input-v2");
    hash_field(&mut hasher, acceptance_dir.as_os_str().as_encoded_bytes());
    hash_field(&mut hasher, repo_root.as_os_str().as_encoded_bytes());
    hash_field(&mut hasher, head.as_bytes());
    hash_field(&mut hasher, &status);
    hash_field(&mut hasher, identity.environment_digest.as_bytes());
    let mut remaining = hash_budget;
    for executable in &identity.executable_paths {
        hash_field(&mut hasher, executable.as_os_str().as_encoded_bytes());
        hash_executable(executable, &mut remaining, &mut hasher)?;
    }
    for relative in status_paths(&status)? {
        hash_repository_path(&repo_root, &relative, &mut remaining, &mut hasher)?;
    }
    Some(InputFingerprint {
        digest: hex::encode(hasher.finalize()),
        tree_head: head,
    })
}

fn git_bytes(args: &[&str], repo_root: &Path) -> Option<Vec<u8>> {
    let output = run_git(args, repo_root).ok()?;
    if !output.status.success() || output.stdout.len() > MAX_GIT_CONTEXT_BYTES {
        return None;
    }
    Some(output.stdout)
}

fn resolve_repo_root(acceptance_dir: &Path) -> Option<PathBuf> {
    let root = run_git_checked(&["rev-parse", "--show-toplevel"], acceptance_dir).ok()?;
    PathBuf::from(root).canonicalize().ok()
}

fn tracked_entries_are_regular(raw: &[u8]) -> bool {
    raw.split(|byte| *byte == 0)
        .filter(|entry| !entry.is_empty())
        .all(|entry| entry.starts_with(b"100644 ") || entry.starts_with(b"100755 "))
}

fn status_paths(raw: &[u8]) -> Option<Vec<PathBuf>> {
    let mut tokens = raw.split(|byte| *byte == 0).filter(|item| !item.is_empty());
    let mut paths = Vec::new();
    while let Some(token) = tokens.next() {
        let (fields, rename) = match token.first().copied() {
            Some(b'1') => (8, false),
            Some(b'2') => (9, true),
            Some(b'?') => (1, false),
            _ => return None,
        };
        let path = skip_fields(token, fields)?;
        let path = PathBuf::from(std::str::from_utf8(path).ok()?);
        if !safe_relative_path(&path) {
            return None;
        }
        paths.push(path);
        if rename {
            tokens.next()?;
        }
    }
    Some(paths)
}

fn skip_fields(mut value: &[u8], count: usize) -> Option<&[u8]> {
    for _ in 0..count {
        let index = value.iter().position(|byte| *byte == b' ')?;
        value = &value[index + 1..];
    }
    Some(value)
}

fn safe_relative_path(path: &Path) -> bool {
    !path.as_os_str().is_empty()
        && path
            .components()
            .all(|component| matches!(component, Component::Normal(_) | Component::CurDir))
}

fn hash_repository_path(
    repo_root: &Path,
    relative: &Path,
    remaining: &mut u64,
    hasher: &mut Sha256,
) -> Option<()> {
    let path = repo_root.join(relative);
    let metadata = path.symlink_metadata().ok()?;
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return None;
    }
    hash_field(hasher, relative.as_os_str().as_encoded_bytes());
    hash_regular_file(&path, &metadata, remaining, hasher)
}

fn hash_executable(invoked_path: &Path, remaining: &mut u64, hasher: &mut Sha256) -> Option<()> {
    let canonical = invoked_path.canonicalize().ok()?;
    let metadata = canonical.metadata().ok()?;
    if !metadata.is_file() || !cache_executable::is_executable(&metadata) {
        return None;
    }
    hash_field(hasher, canonical.as_os_str().as_encoded_bytes());
    hash_regular_file(&canonical, &metadata, remaining, hasher)
}

fn hash_regular_file(
    path: &Path,
    before: &Metadata,
    remaining: &mut u64,
    hasher: &mut Sha256,
) -> Option<()> {
    if before.len() > *remaining {
        return None;
    }
    *remaining -= before.len();
    let mut file = File::open(path).ok()?;
    let mut file_hasher = Sha256::new();
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
    if bytes_read != before.len() || !same_file_snapshot(before, &after) {
        return None;
    }
    hash_field(hasher, &file_hasher.finalize());
    Some(())
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

fn hash_field(hasher: &mut Sha256, value: &[u8]) {
    hasher.update(u64::try_from(value.len()).unwrap_or(u64::MAX).to_le_bytes());
    hasher.update(value);
}
