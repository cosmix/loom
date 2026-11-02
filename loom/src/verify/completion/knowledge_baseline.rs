//! Protected starting commits for checkout-rooted knowledge attempts.
use std::path::{Path, PathBuf};

use crate::fs::{safe_fs, safe_read};
use crate::verify::review::fingerprint::checkout_git;
use anyhow::{ensure, Context, Result};

fn path(session: &str) -> Result<PathBuf> {
    crate::validation::validate_id(session)?;
    Ok(Path::new("knowledge-baselines").join(session))
}

pub(crate) fn read(work_dir: &Path, session: &str) -> Result<String> {
    let relative = path(session)?;
    let hash = safe_read::read_to_string_bounded(work_dir, &relative, 128).context(
        "knowledge starting commit unavailable; an operator must identify the trusted pre-session commit and run loom knowledge recover-baseline <stage> <commit>",
    )?;
    let hash = hash.trim();
    validate_hash(hash)?;
    Ok(hash.to_owned())
}

pub(crate) fn capture(
    work_dir: &Path,
    repo: &Path,
    session: &str,
    previous: Option<&str>,
) -> Result<()> {
    let hash = match previous {
        Some(id) => read(work_dir, id)
            .context("previous knowledge session lacks a trusted starting commit")?,
        None => checkout_git(repo)?
            .run_checked(&["rev-parse", "HEAD"])?
            .trim()
            .to_owned(),
    };
    write(work_dir, session, &hash)
}

fn validate_hash(hash: &str) -> Result<()> {
    ensure!(
        (hash.len() == 40 || hash.len() == 64) && hash.bytes().all(|b| b.is_ascii_hexdigit()),
        "invalid knowledge starting commit"
    );
    Ok(())
}

fn write(work_dir: &Path, session: &str, hash: &str) -> Result<()> {
    use std::os::fd::AsRawFd;
    validate_hash(hash)?;
    safe_fs::safe_create_dir_all(work_dir, Path::new("knowledge-baselines"), 0o700)?;
    let relative = path(session)?;
    safe_fs::safe_create_new(work_dir, &relative, hash.as_bytes())
        .context("persisting knowledge starting commit")?;
    let dir = safe_fs::safe_open_dirfd(work_dir)?;
    let file = safe_fs::open_safely(dir.as_raw_fd(), &relative, libc::O_RDONLY, 0)?;
    std::fs::File::from(file).sync_all()?;
    let baseline_dir = safe_fs::open_safely(
        dir.as_raw_fd(),
        Path::new("knowledge-baselines"),
        libc::O_RDONLY | libc::O_DIRECTORY,
        0,
    )?;
    if let Err(error) = std::fs::File::from(baseline_dir).sync_all() {
        tracing::warn!(%error, "could not sync knowledge baseline directory");
    }
    Ok(())
}

pub(crate) fn recover(work_dir: &Path, repo: &Path, session: &str, hash: &str) -> Result<()> {
    validate_hash(hash)?;
    let git = checkout_git(repo)?;
    let resolved = git.run_checked(&["rev-parse", "--verify", &format!("{hash}^{{commit}}")])?;
    ensure!(
        resolved.trim().eq_ignore_ascii_case(hash),
        "baseline must be a full commit ID"
    );
    git.run_checked(&["merge-base", "--is-ancestor", hash, "HEAD"])
        .context("baseline is not an ancestor of HEAD")?;
    super::knowledge::check_against(repo, hash)?;
    write(work_dir, session, hash)
}
