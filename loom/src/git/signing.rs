//! Signing for the commits loom writes itself.
//!
//! When `commit.gpgsign` is true, every commit the daemon writes (a session's
//! staged index, a merge) is signed with `git commit-tree -S` under a 30 s
//! bound, because no terminal exists to prompt for a passphrase. The daemon
//! runs from a stripped environment, so the two variables a signer needs
//! (`GNUPGHOME` and `SSH_AUTH_SOCK`) are captured once at startup into a
//! [`SigningEnv`], removed from the process environment (so no stage process
//! inherits them), and set on each signing command alone.

use anyhow::{bail, Result};
use std::ffi::{OsStr, OsString};
use std::fmt;
use std::path::Path;
use std::process::Output;
use std::sync::OnceLock;
use std::time::Duration;

use crate::git::runner::{
    run_git_checked, run_git_in_env_within, run_git_with_env_within, GIT_READ_TIMEOUT,
};
use crate::process::ProcessTimeoutError;

/// The bound on every signing call: an agent that needs a passphrase no one
/// can type would otherwise hold the commit forever.
pub const SIGN_TIMEOUT: Duration = Duration::from_secs(30);

const GNUPGHOME: &str = "GNUPGHOME";
const SSH_AUTH_SOCK: &str = "SSH_AUTH_SOCK";

/// The variables a signer needs and no stage process may see.
pub const SIGNING_ENV_NAMES: [&str; 2] = [GNUPGHOME, SSH_AUTH_SOCK];

/// Most stderr lines and bytes a signing failure carries.
const TAIL_LINES: usize = 8;
const TAIL_BYTES: usize = 1000;

const PROBE_MESSAGE: &str = "loom signing probe";

/// The signer's environment, captured from the process that started loom.
#[derive(Debug, Clone, Default)]
pub struct SigningEnv {
    pub gnupghome: Option<OsString>,
    pub ssh_auth_sock: Option<OsString>,
}

impl SigningEnv {
    /// `GNUPGHOME` and `SSH_AUTH_SOCK` as this process sees them now.
    pub fn capture_from_process() -> Self {
        Self {
            gnupghome: std::env::var_os(GNUPGHOME),
            ssh_auth_sock: std::env::var_os(SSH_AUTH_SOCK),
        }
    }

    /// `GNUPGHOME` and `SSH_AUTH_SOCK`, each only when set.
    pub fn env_pairs(&self) -> Vec<(&'static str, &OsStr)> {
        [
            (GNUPGHOME, &self.gnupghome),
            (SSH_AUTH_SOCK, &self.ssh_auth_sock),
        ]
        .into_iter()
        .filter_map(|(name, value)| value.as_deref().map(|value| (name, value)))
        .collect()
    }
}

static INSTALLED: OnceLock<SigningEnv> = OnceLock::new();
static NOT_INSTALLED: SigningEnv = SigningEnv {
    gnupghome: None,
    ssh_auth_sock: None,
};

/// Keep `env` for every signing call of this process. A second call is
/// ignored: the first capture is the one taken before the variables left the
/// process environment.
pub fn install(env: SigningEnv) {
    let _already_installed = INSTALLED.set(env);
}

/// The installed signing environment, or an empty one when nothing was
/// installed.
pub fn installed() -> &'static SigningEnv {
    INSTALLED.get().unwrap_or(&NOT_INSTALLED)
}

/// Capture and install the signing environment, then remove its variables
/// from the process environment. Call it before any thread exists: changing
/// the environment races every thread that reads it.
pub fn take_from_process() {
    install(SigningEnv::capture_from_process());
    for name in SIGNING_ENV_NAMES {
        std::env::remove_var(name);
    }
}

/// The installed signing environment when [`take_from_process`] ran, else
/// what the process environment holds now.
pub fn current() -> SigningEnv {
    current_from(INSTALLED.get(), SigningEnv::capture_from_process)
}

fn current_from(
    installed: Option<&SigningEnv>,
    capture: impl FnOnce() -> SigningEnv,
) -> SigningEnv {
    installed.cloned().unwrap_or_else(capture)
}

/// A `git commit-tree` that wrote no commit. `signing` is true when the
/// signed call failed (non-zero exit, spawn failure, timeout) or the signing
/// setting could not be read: only the operator can fix those. Otherwise
/// `detail` is the runner's own failure text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommitTreeError {
    pub signing: bool,
    pub detail: String,
}

impl fmt::Display for CommitTreeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.signing {
            write!(f, "commit signing failed: {}", self.detail)
        } else {
            f.write_str(&self.detail)
        }
    }
}

impl std::error::Error for CommitTreeError {}

/// The environment a signing helper runs git under.
#[derive(Clone, Copy)]
enum GitEnv<'a> {
    /// The process environment plus the signing variables.
    Process(&'a SigningEnv),
    /// Exactly these variables (`env_clear()` first).
    Exact(&'a [(OsString, OsString)]),
}

impl GitEnv<'_> {
    fn run(self, repo: &Path, args: &[&str], timeout: Duration) -> Result<Output> {
        match self {
            Self::Process(env) => run_git_with_env_within(repo, args, &env.env_pairs(), timeout),
            Self::Exact(pairs) => run_git_in_env_within(repo, args, pairs, timeout),
        }
    }
}

/// `commit.gpgsign` of `repo` (`git config --type=bool`): unset is false, and
/// a value git cannot read as a boolean is an error, never a silent "off".
pub fn signing_enabled(repo: &Path) -> Result<bool> {
    read_gpgsign(repo, GitEnv::Process(&NOT_INSTALLED))
}

/// [`signing_enabled`] under exactly the variables `env`.
pub(crate) fn signing_enabled_in(repo: &Path, env: &[(OsString, OsString)]) -> Result<bool> {
    read_gpgsign(repo, GitEnv::Exact(env))
}

/// `gpg.format` of `repo` under exactly the variables `env`; `None` when unset.
pub(crate) fn signing_format_in(
    repo: &Path,
    env: &[(OsString, OsString)],
) -> Result<Option<String>> {
    read_config(repo, GitEnv::Exact(env), &["config", "gpg.format"])
}

fn read_gpgsign(repo: &Path, git: GitEnv<'_>) -> Result<bool> {
    let value = read_config(repo, git, &["config", "--type=bool", "commit.gpgsign"])?;
    Ok(value.as_deref() == Some("true"))
}

/// A `git config` read: exit 0 is the value, exit 1 is unset, anything else
/// is an error carrying git's stderr.
fn read_config(repo: &Path, git: GitEnv<'_>, args: &[&str]) -> Result<Option<String>> {
    let output = git.run(repo, args, GIT_READ_TIMEOUT)?;
    match output.status.code() {
        Some(0) => Ok(Some(
            String::from_utf8_lossy(&output.stdout).trim().to_string(),
        )),
        Some(1) => Ok(None),
        _ => bail!(
            "git {} failed: {}",
            args.join(" "),
            String::from_utf8_lossy(&output.stderr).trim()
        ),
    }
}

/// Write a commit of `tree` with `parents` and `message`, signed when
/// `commit.gpgsign` is true, with the installed signing environment.
pub fn commit_tree(
    repo: &Path,
    tree: &str,
    parents: &[&str],
    message: &str,
) -> Result<String, CommitTreeError> {
    let sign = signing_enabled(repo).map_err(|error| CommitTreeError {
        signing: true,
        detail: format!("cannot read commit.gpgsign: {error:#}"),
    })?;
    commit_tree_with(repo, tree, parents, message, sign.then(installed))
}

/// `commit-tree <tree> -p <parent>... [-S] -m <message>`: signed with
/// `signer`'s variables when it is set, through the plain runner otherwise.
fn commit_tree_with(
    repo: &Path,
    tree: &str,
    parents: &[&str],
    message: &str,
    signer: Option<&SigningEnv>,
) -> Result<String, CommitTreeError> {
    let mut args = vec!["commit-tree", tree];
    for &parent in parents {
        args.extend(["-p", parent]);
    }
    let Some(env) = signer else {
        args.extend(["-m", message]);
        return run_git_checked(&args, repo).map_err(|error| CommitTreeError {
            signing: false,
            detail: format!("{error:#}"),
        });
    };
    args.extend(["-S", "-m", message]);
    run_signed(repo, &args, GitEnv::Process(env))
}

/// Run a signed `commit-tree` under [`SIGN_TIMEOUT`]; every failure is a
/// signing failure carrying the tail of git's stderr.
fn run_signed(repo: &Path, args: &[&str], git: GitEnv<'_>) -> Result<String, CommitTreeError> {
    let failure = |detail: String| CommitTreeError {
        signing: true,
        detail,
    };
    let output = git
        .run(repo, args, SIGN_TIMEOUT)
        .map_err(|error| failure(run_failure(&error)))?;
    if !output.status.success() {
        let tail = stderr_tail(&output.stderr);
        return Err(failure(if tail.is_empty() {
            format!(
                "git commit-tree -S failed ({}) and printed nothing",
                output.status
            )
        } else {
            tail
        }));
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

/// The detail of a signing call that did not complete.
fn run_failure(error: &anyhow::Error) -> String {
    if error.downcast_ref::<ProcessTimeoutError>().is_some() {
        format!(
            "the signer did not finish within {} s; the signing agent probably needs a cached \
             passphrase, and no terminal can prompt for it",
            SIGN_TIMEOUT.as_secs()
        )
    } else {
        format!("{error:#}")
    }
}

/// The last [`TAIL_LINES`] lines of `stderr`, at most [`TAIL_BYTES`] bytes
/// (cut at the front, on a character boundary).
fn stderr_tail(stderr: &[u8]) -> String {
    let text = String::from_utf8_lossy(stderr);
    let lines: Vec<&str> = text.trim().lines().collect();
    let tail = lines[lines.len().saturating_sub(TAIL_LINES)..].join("\n");
    let mut start = tail.len().saturating_sub(TAIL_BYTES);
    while !tail.is_char_boundary(start) {
        start += 1;
    }
    tail[start..].to_string()
}

/// [`probe_in`] over the process environment plus `env`'s variables; only
/// the tests call it.
#[cfg(test)]
pub(crate) fn probe(repo: &Path, env: &SigningEnv) -> Result<()> {
    probe_with(repo, GitEnv::Process(env))
}

/// Sign an empty-tree commit under exactly the variables `env`, so a setup
/// that cannot sign unattended is found before a stage needs it.
pub(crate) fn probe_in(repo: &Path, env: &[(OsString, OsString)]) -> Result<()> {
    probe_with(repo, GitEnv::Exact(env))
}

fn probe_with(repo: &Path, git: GitEnv<'_>) -> Result<()> {
    let empty_tree = run_git_checked(&["hash-object", "-w", "-t", "tree", "/dev/null"], repo)?;
    let args = [
        "commit-tree",
        empty_tree.as_str(),
        "-S",
        "-m",
        PROBE_MESSAGE,
    ];
    if let Err(error) = run_signed(repo, &args, git) {
        bail!(
            "commit.gpgsign is true but a test signature failed: {}. Loom signs every stage and \
             merge commit as the daemon, without a terminal. Cache the passphrase in gpg-agent \
             or use an SSH key held by ssh-agent, check it with git commit-tree -S, then run \
             loom run again.",
            error.detail
        );
    }
    Ok(())
}

#[cfg(test)]
pub(crate) mod tests;
