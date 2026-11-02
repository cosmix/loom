//! Symbolic links a session can plant where git would write through them. Each
//! function here is a look ahead of git's own open, so it stops a link that is
//! already there and not a writer that flips one at the right moment.

use anyhow::{bail, Result};
use std::path::Path;
use std::process::Command;

use super::subcommand_index;

/// Names in a git directory that git opens by path and writes through when
/// they are symbolic links (reflogs, the index and `packed-refs` by locking
/// beside the link's target, the message files by `fopen`).
const WRITTEN_THROUGH_LINKS: [&str; 16] = [
    "index",
    "HEAD",
    "ORIG_HEAD",
    "MERGE_HEAD",
    "MERGE_MODE",
    "MERGE_MSG",
    "MERGE_RR",
    "AUTO_MERGE",
    "SQUASH_MSG",
    "COMMIT_EDITMSG",
    "FETCH_HEAD",
    "packed-refs",
    "shallow",
    "gc.pid",
    "logs",
    "refs",
];

/// Refuse a symbolic link at any of `names` in `git_dir`.
pub(crate) fn refuse_planted_links(git_dir: &Path, names: &[&str]) -> Result<()> {
    for name in names {
        let path = git_dir.join(name);
        if std::fs::symlink_metadata(&path).is_ok_and(|meta| meta.is_symlink()) {
            bail!(
                "loom will not run git: {} is a symbolic link, and git would write through it. \
                 Git never creates one there; remove it",
                path.display()
            );
        }
    }
    Ok(())
}

/// Refuse a symbolic link on the way to a ref a `git update-ref` or `git branch`
/// in `args` writes or deletes under `common`. Git follows a directory link
/// under `refs/`, and `refs/` stays writable to a session because git locks a
/// branch even to keep it where it is. This is a look ahead of git's own open: a
/// writer flipping the link between the two is not stopped, and what it could
/// gain is an object id written to, or a file removed at, `<target>/<ref name>`.
pub(crate) fn refuse_ref_path_links(common: &Path, args: &[&str]) -> Result<()> {
    let position = subcommand_index(args);
    let named = args.iter().skip(position + 1).copied();
    let references: Vec<String> = match args.get(position).copied() {
        Some("update-ref") => named
            .filter(|arg| arg.starts_with("refs/"))
            .map(str::to_string)
            .collect(),
        Some("branch") => named
            .filter(|arg| !arg.starts_with('-'))
            .map(|name| format!("refs/heads/{name}"))
            .collect(),
        _ => return Ok(()),
    };
    for reference in references {
        let mut path = common.to_path_buf();
        for component in reference.split('/') {
            path.push(component);
            if std::fs::symlink_metadata(&path).is_ok_and(|meta| meta.is_symlink()) {
                bail!(
                    "loom will not run git: {} is a symbolic link on the way to {reference}, and \
                     git would write through it; remove it",
                    path.display()
                );
            }
        }
    }
    Ok(())
}

/// [`refuse_planted_links`] for a command git discovers from a checkout, whose
/// `.git` directory a stage session can write. A look ahead of git's own open
/// is a race against a writer that flips the link, so this stops a link that is
/// already there and nothing more: a command that must not lose that race runs
/// through `WorktreeGit::pinned_main`, whose git directory is private.
pub(super) fn refuse_links_in_discovered_git_dir(
    command: &Command,
    exec_args: &[&str],
) -> Result<()> {
    let pinned = command
        .get_envs()
        .any(|(name, value)| name == "GIT_DIR" && value.is_some());
    let Some(cwd) = command.get_current_dir().filter(|_| !pinned) else {
        return Ok(());
    };
    let git_dir = cwd.join(".git");
    if std::fs::symlink_metadata(&git_dir).is_ok_and(|meta| meta.is_dir()) {
        refuse_planted_links(&git_dir, &WRITTEN_THROUGH_LINKS)?;
        refuse_ref_path_links(&git_dir, exec_args)?;
    }
    Ok(())
}
