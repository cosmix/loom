//! The tree digest a session and the daemon both compute, so a host job runs
//! on exactly the tree the session saw. It hashes `git status` entries and the
//! current content of each changed path; a committed clean tree hashes the
//! empty input.

use std::fs::File;
use std::io::Read;
use std::os::unix::ffi::OsStrExt;
use std::path::Path;
use std::process::Command;

use anyhow::{bail, ensure, Context, Result};
use sha2::{Digest, Sha256};

use crate::handoff::completion::content::completion_scaffold;

/// Largest file the digest will read; a larger one is an error, not a skip.
const MAX_FILE_BYTES: u64 = 64 * 1024 * 1024;

/// Digest of the whole checkout.
pub fn tree_digest(checkout: &Path) -> Result<String> {
    digest(checkout, None)
}

/// Digest scoped to the knowledge directory of the repository at `repo_root`.
pub fn knowledge_tree_digest(repo_root: &Path) -> Result<String> {
    digest(
        repo_root,
        Some(crate::git::target_guard::knowledge_prefix()),
    )
}

fn digest(checkout: &Path, scope: Option<&str>) -> Result<String> {
    let status = porcelain(checkout, scope)?;
    let mut hasher = Sha256::new();
    for record in entries(&status)? {
        if record.untracked && completion_scaffold(&record.path, checkout) {
            continue;
        }
        hasher.update(record.raw);
        hasher.update([0]);
        hash_content(&mut hasher, &checkout.join(&record.path))?;
    }
    Ok(hex::encode(hasher.finalize()))
}

/// `git` in `checkout`, read the same way in a session and in the daemon: no
/// optional locks, no user or system config, no inherited repository
/// variables.
fn git_command(checkout: &Path) -> Command {
    let mut command = Command::new("git");
    command
        .arg("--no-optional-locks")
        .current_dir(checkout)
        .env_remove("GIT_DIR")
        .env_remove("GIT_INDEX_FILE")
        .env_remove("GIT_WORK_TREE")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_NOSYSTEM", "1");
    command
}

fn run(mut command: Command, checkout: &Path) -> Result<Vec<u8>> {
    let output = command
        .output()
        .with_context(|| format!("failed to run git in {}", checkout.display()))?;
    ensure!(
        output.status.success(),
        "git failed in {}: {}",
        checkout.display(),
        String::from_utf8_lossy(&output.stderr).trim()
    );
    Ok(output.stdout)
}

/// The commit `checkout`'s HEAD names.
pub fn head(checkout: &Path) -> Result<String> {
    let mut command = git_command(checkout);
    command.args(["rev-parse", "--verify", "HEAD"]);
    let out = run(command, checkout)?;
    Ok(String::from_utf8_lossy(&out).trim().to_string())
}

fn porcelain(checkout: &Path, scope: Option<&str>) -> Result<Vec<u8>> {
    let mut command = git_command(checkout);
    command.args([
        "status",
        "--porcelain=v2",
        "-z",
        "--untracked-files=all",
        "--no-renames",
    ]);
    if let Some(prefix) = scope {
        command.args(["--", prefix]);
    }
    run(command, checkout)
}

struct Entry<'a> {
    raw: &'a [u8],
    path: String,
    untracked: bool,
}

/// Split the `-z` output into entries. Ordinary (`1`) and unmerged (`u`)
/// entries carry the path after a fixed number of space-separated fields;
/// `--no-renames` leaves no `2` entry.
fn entries(status: &[u8]) -> Result<Vec<Entry<'_>>> {
    let mut parsed = Vec::new();
    for raw in status
        .split(|byte| *byte == 0)
        .filter(|raw| !raw.is_empty())
    {
        let (fields, untracked) = match raw[0] {
            b'1' => (9, false),
            b'u' => (11, false),
            b'?' => (2, true),
            other => bail!("unexpected git status entry type '{}'", other as char),
        };
        let path = raw
            .splitn(fields, |byte| *byte == b' ')
            .nth(fields - 1)
            .context("malformed git status entry")?;
        let path = std::str::from_utf8(path).context("git status path is not UTF-8")?;
        parsed.push(Entry {
            raw,
            path: path.to_string(),
            untracked,
        });
    }
    Ok(parsed)
}

/// One type byte and the content framed by an 8-byte big-endian length:
/// `F` a regular file, `L` a symlink's target, `-` a path that is absent or
/// of another type.
fn hash_content(hasher: &mut Sha256, path: &Path) -> Result<()> {
    let metadata = match std::fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            absent(hasher);
            return Ok(());
        }
        Err(error) => return Err(error).with_context(|| format!("cannot stat {}", path.display())),
    };
    let kind = metadata.file_type();
    if kind.is_symlink() {
        let target = std::fs::read_link(path)
            .with_context(|| format!("cannot read link {}", path.display()))?;
        let bytes = target.as_os_str().as_bytes();
        hasher.update(b"L");
        hasher.update((bytes.len() as u64).to_be_bytes());
        hasher.update(bytes);
    } else if kind.is_file() {
        ensure!(
            metadata.len() <= MAX_FILE_BYTES,
            "{} is {} bytes, over the {MAX_FILE_BYTES}-byte limit for a tree digest",
            path.display(),
            metadata.len()
        );
        hash_file(hasher, path, metadata.len())?;
    } else {
        absent(hasher);
    }
    Ok(())
}

fn absent(hasher: &mut Sha256) {
    hasher.update(b"-");
    hasher.update([0u8; 8]);
}

/// Hash `length` bytes of a regular file. The file is read through a bounded
/// reader, so one that grows after the stat cannot exceed the limit.
fn hash_file(hasher: &mut Sha256, path: &Path, length: u64) -> Result<()> {
    let mut bytes = Vec::with_capacity(length as usize);
    File::open(path)
        .with_context(|| format!("cannot open {}", path.display()))?
        .take(MAX_FILE_BYTES + 1)
        .read_to_end(&mut bytes)
        .with_context(|| format!("cannot read {}", path.display()))?;
    ensure!(
        bytes.len() as u64 <= MAX_FILE_BYTES,
        "{} grew past the {MAX_FILE_BYTES}-byte limit for a tree digest",
        path.display()
    );
    hasher.update(b"F");
    hasher.update((bytes.len() as u64).to_be_bytes());
    hasher.update(&bytes);
    Ok(())
}
