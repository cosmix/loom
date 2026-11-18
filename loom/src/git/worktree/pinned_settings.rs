//! The operator's identity and signing settings for a command run with a
//! private git directory.
//!
//! An `includeIf "gitdir:..."` matches against the git directory's path, and a
//! pinned command's path is private (`scratch_admin`). An operator whose
//! `user.email` or `commit.gpgsign` comes from such an include would otherwise
//! get unsigned commits under the wrong name, silently. So these keys are read
//! with `GIT_DIR` naming the real directory and passed on the command line.
//!
//! One `git config --list --show-scope` reads them. Only `system`, `global` and
//! `local` values count: `config.worktree`, which a session can write, arrives
//! labelled `worktree` and is read as data, nothing more.
//!
//! A real directory git cannot use (a session can break its own `HEAD`) gives no
//! error here: `git config` then runs as if outside a repository, every
//! `includeIf "gitdir:"` is false, and no `local` value is listed. That absence
//! comes from the same process and the same look at the directory as the
//! includes, so refusing on it leaves no moment for a writer to flip `HEAD` back.

use anyhow::{bail, Result};
use std::ffi::{OsStr, OsString};
use std::path::Path;
use std::process::Output;

use crate::git::runner::{run_git_in_env_within, run_git_with_env, subcommand, GIT_READ_TIMEOUT};

/// Subcommands that write a commit, read these settings to decide how to, or
/// record an identity in a reflog.
const USES_IDENTITY: [&str; 8] = [
    "commit-tree",
    "commit",
    "merge",
    "config",
    "update-ref",
    "stash",
    "tag",
    "var",
];

/// Lowercased as `git config --list` prints section and variable names.
const IDENTITY_KEYS: [&str; 16] = [
    "user.name",
    "user.email",
    "user.useconfigonly",
    "user.signingkey",
    "author.name",
    "author.email",
    "committer.name",
    "committer.email",
    "commit.gpgsign",
    "gpg.format",
    "gpg.program",
    "gpg.mintrustlevel",
    "gpg.openpgp.program",
    "gpg.x509.program",
    "gpg.ssh.program",
    "gpg.ssh.defaultkeycommand",
];

/// Scopes a session cannot write.
const OPERATOR_SCOPES: [&str; 3] = ["system", "global", "local"];

/// The directories the read runs with, and the variables it runs under.
pub(super) struct SettingsSource<'a> {
    /// `GIT_DIR`, `GIT_WORK_TREE` and `GIT_COMMON_DIR` naming the real directories.
    pub pin_env: &'a [(&'a str, &'a OsStr)],
    pub work_tree: &'a Path,
    /// The exact environment the command runs under; `None` is this process's.
    pub env: Option<&'a [(OsString, OsString)]>,
}

impl SettingsSource<'_> {
    fn read(&self, args: &[&str]) -> Result<Output> {
        let Some(env) = self.env else {
            return run_git_with_env(args, self.pin_env, self.work_tree);
        };
        let mut vars = env.to_vec();
        vars.extend(
            self.pin_env
                .iter()
                .map(|&(key, value)| (OsString::from(key), value.to_os_string())),
        );
        run_git_in_env_within(self.work_tree, args, &vars, GIT_READ_TIMEOUT)
    }
}

/// `-c key=value` arguments carrying the settings `args` needs, each key once
/// with its last value, in the order git itself applies the files. A command
/// that writes no commit reads nothing.
pub(super) fn identity_args(args: &[&str], source: &SettingsSource<'_>) -> Result<Vec<String>> {
    if !subcommand(args).is_some_and(|name| USES_IDENTITY.contains(&name)) {
        return Ok(Vec::new());
    }
    let output = source.read(&["config", "--list", "--includes", "--show-scope", "--null"])?;
    if !output.status.success() {
        bail!(
            "cannot resolve the operator's identity: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    let listing = String::from_utf8_lossy(&output.stdout);
    let (settings, saw_local) = collect_identity(&listing);
    if !saw_local {
        bail!(
            "cannot resolve the operator's identity: git could not use the git directory, so \
             its includes did not apply"
        );
    }
    Ok(settings
        .into_iter()
        .flat_map(|setting| ["-c".to_string(), setting])
        .collect())
}

/// The identity settings of a `--show-scope --null --list` listing
/// (`scope\0key\nvalue\0` records, `scope\0key\0` for a key with no value), a
/// later value replacing an earlier one, and whether any `local` value appeared.
fn collect_identity(listing: &str) -> (Vec<String>, bool) {
    let mut settings: Vec<String> = Vec::new();
    let mut saw_local = false;
    let mut fields = listing.split('\0');
    while let (Some(scope), Some(record)) = (fields.next(), fields.next()) {
        saw_local |= scope == "local";
        if !OPERATOR_SCOPES.contains(&scope) {
            continue;
        }
        let (key, value) = match record.split_once('\n') {
            Some((key, value)) => (key, Some(value)),
            None => (record, None),
        };
        if !IDENTITY_KEYS.contains(&key) {
            continue;
        }
        settings.retain(|known| known.split('=').next() != Some(key));
        settings.push(match value {
            Some(value) => format!("{key}={value}"),
            None => key.to_string(),
        });
    }
    (settings, saw_local)
}

#[cfg(test)]
#[path = "pinned_settings_tests.rs"]
mod tests;
