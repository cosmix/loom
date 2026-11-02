//! Git run in a stage worktree by a process outside the stage's sandbox.
//!
//! A linked worktree's `.git` is a file naming its git directory, and the
//! agent working in the worktree can rewrite it to name a directory of its
//! own. That directory's configuration could define `filter.<name>.clean`,
//! which `git diff` and `git status` run when they re-hash a file whose stat
//! data changed, or any other command git takes from configuration. Git that
//! the daemon starts with default discovery would run it outside the sandbox.
//!
//! A pinned [`WorktreeGit`] never reads that file. It sets `GIT_DIR` and
//! `GIT_WORK_TREE` (what `--git-dir` and `--work-tree` set) to the worktree's
//! administrative directory in the main repository and to the worktree, and
//! `GIT_COMMON_DIR` to the main repository's common directory. Both directories
//! come from the main repository's records. Repository configuration comes
//! from the main repository's `config` alone, beside the user's global and
//! system files, so a filter defined there (git-lfs) keeps working.
//!
//! `GIT_DIR` names a private directory built for each command, not the
//! administrative directory itself: a session can write that one, and git
//! follows a link planted at `logs/HEAD`, `index` or `COMMIT_EDITMSG`, and reads
//! `config.worktree` when the repository enables `extensions.worktreeConfig`.
//! See `scratch_admin`. The operator's identity is passed explicitly, since an
//! `includeIf "gitdir:"` does not match the private path (`pinned_settings`).

use anyhow::{ensure, Context, Result};
use std::ffi::{OsStr, OsString};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::Output;
use std::time::Duration;

use crate::fs::work_dir::WorkDir;
use crate::git::runner::{
    refuse_planted_links, refuse_ref_path_links, run_git, run_git_in_env_within,
    run_git_stream_with_env, run_git_with_env, subcommand, GIT_READ_TIMEOUT, REPO_REDIRECT_ENV,
};
use crate::git::worktree::config_worktree::worktree_config_enabled;
use crate::git::worktree::scratch_admin::{NoPrivateDirectory, ScratchAdmin};

#[path = "pinned_settings.rs"]
mod settings;

#[path = "pinned_registry.rs"]
mod registry;

use registry::registered_admin_dir;
pub(super) use registry::{common_dir, names_work_tree, registered_checkout};
use settings::{identity_args, SettingsSource};

/// Names in the common directory git's ref store writes whatever `GIT_DIR`
/// names. Capsules deny `logs` whole and `packed-refs` until a host rewrite drops
/// that single-file deny; `refs` stays writable to a session. So this look stops
/// what a session planted. A writer that flips a link between this look and
/// git's open is not stopped.
const COMMON_WRITTEN_THROUGH: [&str; 4] = ["packed-refs", "logs", "refs", "refs/heads"];

/// Subcommands that write the index only to refresh its cached data.
const REFRESHES_INDEX_ONLY: [&str; 3] = ["status", "diff", "write-tree"];

/// Keeps a command in a stage worktree from creating the branch's reflog, which
/// a session's next `git reset` could not append to (`session_denies_ref_store`).
/// A log that exists still gets its entry, `logs/HEAD` included.
const NO_NEW_BRANCH_REFLOG: [&str; 2] = ["-c", "core.logAllRefUpdates=false"];

/// How loom runs git in one worktree.
#[derive(Debug, Clone)]
pub struct WorktreeGit {
    work_tree: PathBuf,
    pin: Option<Pin>,
}

/// The directories a pinned worktree's git runs with.
#[derive(Debug, Clone)]
struct Pin {
    repo_root: PathBuf,
    git_dir: PathBuf,
    common_dir: PathBuf,
}

impl WorktreeGit {
    /// [`NO_NEW_BRANCH_REFLOG`] for a session, as the variable `git -c` itself sets
    /// for its children. The documented `GIT_CONFIG_KEY_0` is not used: tools that
    /// scrub names holding `KEY` from their children's environment (the codex lane
    /// can) would leave `GIT_CONFIG_COUNT` behind, and git then refuses every
    /// command with `missing config key` (measured).
    pub(crate) const SESSION_GIT_PARAMETERS: (&'static str, &'static str) =
        ("GIT_CONFIG_PARAMETERS", "'core.logallrefupdates=false'");

    /// Pin the checkout reached through Loom's shared state, including stages.
    pub(crate) fn pinned_checkout(worktree: &Path) -> Result<Self> {
        let root = WorkDir::new(worktree)?
            .main_project_root()
            .context("cannot resolve main project root")?;
        if root.canonicalize()? == worktree.canonicalize()? {
            Self::pinned_main(&root)
        } else {
            Self::pinned(&root, worktree)
        }
    }

    /// Git in `worktree` as the worktree's own `.git` directs it. Only for a
    /// directory that is no stage worktree, or for a process that already
    /// runs with the privileges of whoever controls the worktree: a stage
    /// agent's own CLI inside its sandbox.
    pub fn discovered(worktree: &Path) -> Self {
        Self {
            work_tree: worktree.to_path_buf(),
            pin: None,
        }
    }

    /// Pin the trusted operator checkout. A linked checkout's pointer must
    /// name an administrative directory registered for that same checkout.
    /// The caller must establish that the operator checkout itself is trusted.
    pub fn pinned_main(repo_root: &Path) -> Result<Self> {
        let work_tree = repo_root
            .canonicalize()
            .with_context(|| format!("cannot resolve checkout {}", repo_root.display()))?;
        let (git_dir, common_dir) = super::main_identity::identity_for(&work_tree)?;
        Ok(Self {
            work_tree: work_tree.clone(),
            pin: Some(Pin {
                repo_root: work_tree,
                common_dir,
                git_dir,
            }),
        })
    }

    /// Git in `worktree` pinned to its administrative directory in the
    /// repository at `repo_root` (see the module docs). Fails, running
    /// nothing in the worktree, unless exactly one entry of
    /// `<common dir>/worktrees/` is a directory (not a symlink) whose
    /// `gitdir` file names `<worktree>/.git` and whose `commondir` file
    /// resolves to that common directory.
    pub fn pinned(repo_root: &Path, worktree: &Path) -> Result<Self> {
        let work_tree = worktree
            .canonicalize()
            .with_context(|| format!("cannot resolve the worktree {}", worktree.display()))?;
        let common_dir = common_dir(repo_root)?;
        let git_dir = registered_admin_dir(&common_dir, &work_tree)?;
        Ok(Self {
            work_tree,
            pin: Some(Pin {
                repo_root: repo_root.to_path_buf(),
                git_dir,
                common_dir,
            }),
        })
    }

    /// [`Self::pinned`] in the repository that holds the loom state
    /// directory `work_dir`.
    pub fn pinned_in_project_of(work_dir: &Path, worktree: &Path) -> Result<Self> {
        let workspace = WorkDir::new(work_dir)?;
        let repo_root = workspace
            .repo_root()
            .context("cannot resolve the repository root of the state directory")?;
        Self::pinned(repo_root, worktree)
    }

    /// The worktree git runs in.
    pub fn work_tree(&self) -> &Path {
        &self.work_tree
    }

    /// Common directory of a pinned identity, if this git is pinned.
    pub(crate) fn common_dir_path(&self) -> Option<&Path> {
        self.pin.as_ref().map(|pin| pin.common_dir.as_path())
    }

    pub(crate) fn git_dir_path(&self) -> Option<&Path> {
        self.pin.as_ref().map(|pin| pin.git_dir.as_path())
    }

    /// Whether the pinned operator checkout has a separate worktree git directory.
    pub(crate) fn is_linked_checkout(&self) -> bool {
        self.pin
            .as_ref()
            .is_some_and(|pin| pin.git_dir != pin.common_dir)
    }

    /// `git <args>` in the worktree, through [`run_git`]. A pinned run cannot
    /// read a `config.worktree` a session rewrites (see `Self::with_pin`).
    pub fn run(&self, args: &[&str]) -> Result<Output> {
        let Some(pin) = &self.pin else {
            return run_git(args, &self.work_tree);
        };
        self.with_pin(pin, args, None, |args, pin_env| {
            run_git_with_env(args, pin_env, &self.work_tree)
        })
    }

    /// [`Self::run`] that must succeed: git's stdout as UTF-8 text, untrimmed
    /// (NUL-separated records stay intact). Fails naming the command, the
    /// worktree and git's stderr when git exits non-zero, and when its output
    /// is not UTF-8, which for a listing means a path that cannot be matched.
    pub fn run_checked(&self, args: &[&str]) -> Result<String> {
        let output = self.run(args)?;
        ensure!(
            output.status.success(),
            "git {} failed in {}: {}",
            args.join(" "),
            self.work_tree.display(),
            String::from_utf8_lossy(&output.stderr).trim()
        );
        String::from_utf8(output.stdout).context("git records contain a non-UTF-8 path")
    }

    /// `git <args>`, a command that only reads, within the runner's read
    /// deadline, from this process's environment without `REPO_REDIRECT_ENV`
    /// and with `env` added: git finds the repository from the worktree, or
    /// the pin names it, as [`Self::run`] does.
    pub fn read_isolated(&self, args: &[&str], env: &[(&str, &OsStr)]) -> Result<Output> {
        let mut vars: Vec<(OsString, OsString)> = std::env::vars_os()
            .filter(|(key, _)| !REPO_REDIRECT_ENV.iter().any(|name| key == name))
            .collect();
        vars.extend(owned_vars(env));
        let Some(pin) = &self.pin else {
            return run_git_in_env_within(&self.work_tree, args, &vars, GIT_READ_TIMEOUT);
        };
        let source = vars.clone();
        self.with_pin(pin, args, Some(&source), |args, pin_env| {
            vars.extend(owned_vars(pin_env));
            run_git_in_env_within(&self.work_tree, args, &vars, GIT_READ_TIMEOUT)
        })
    }

    /// Run under exactly the supplied environment, with repository selectors
    /// replaced by the pinned identity and the caller's deadline preserved.
    pub(crate) fn run_in_env_within(
        &self,
        args: &[&str],
        env: &[(OsString, OsString)],
        timeout: Duration,
    ) -> Result<Output> {
        let mut vars: Vec<_> = env
            .iter()
            .filter(|(key, _)| !REPO_REDIRECT_ENV.iter().any(|name| key == name))
            .cloned()
            .collect();
        let Some(pin) = &self.pin else {
            return run_git_in_env_within(&self.work_tree, args, &vars, timeout);
        };
        let source = vars.clone();
        self.with_pin(pin, args, Some(&source), |args, pin_env| {
            vars.extend(owned_vars(pin_env));
            run_git_in_env_within(&self.work_tree, args, &vars, timeout)
        })
    }

    /// Stream checked output without retaining the complete blob in memory.
    pub(crate) fn run_stream<T: Send + 'static>(
        &self,
        args: &[&str],
        consume: impl FnOnce(Box<dyn Read + Send>) -> Result<T> + Send + 'static,
    ) -> Result<T> {
        let Some(pin) = &self.pin else {
            return run_git_stream_with_env(args, &self.work_tree, &[], consume);
        };
        self.with_pin(pin, args, None, |args, pin_env| {
            run_git_stream_with_env(args, &self.work_tree, pin_env, consume)
        })
    }

    /// Run `command` on `args` with the variables that pin git to `pin`. They
    /// name a private directory built for this command, so no check precedes a
    /// reopen of a path a session can write, and what git changed there is
    /// published after it exits, on failure too (a conflicted merge has state
    /// to keep). `env` is the exact environment `command` uses, if not this
    /// process's.
    fn with_pin<T>(
        &self,
        pin: &Pin,
        args: &[&str],
        env: Option<&[(OsString, OsString)]>,
        command: impl FnOnce(&[&str], &[(&str, &OsStr)]) -> Result<T>,
    ) -> Result<T> {
        self.verify_pin(pin)?;
        refuse_planted_links(&pin.common_dir, &COMMON_WRITTEN_THROUGH)?;
        refuse_ref_path_links(&pin.common_dir, args)?;
        let worktree_config = worktree_config_enabled(&pin.repo_root, &pin.common_dir)?;
        let identity = identity_args(
            args,
            &SettingsSource {
                pin_env: &self.pin_env(pin, &pin.git_dir),
                work_tree: &self.work_tree,
                env,
            },
        )?;
        let mut argv: Vec<&str> = Vec::new();
        if self.work_tree != pin.repo_root {
            argv.extend_from_slice(&NO_NEW_BRANCH_REFLOG);
        }
        argv.extend(identity.iter().map(String::as_str));
        argv.extend_from_slice(args);
        let refreshes = subcommand(args).is_some_and(|name| REFRESHES_INDEX_ONLY.contains(&name));
        let prepared = ScratchAdmin::prepare(
            &pin.repo_root,
            &pin.git_dir,
            &pin.common_dir,
            worktree_config,
        );
        let scratch = match prepared {
            // A confined process follows no link past what its own sandbox allows,
            // so the real directory costs nothing. With the extension on it would
            // read `config.worktree`, which has always refused here.
            Err(error) if !worktree_config && error.is::<NoPrivateDirectory>() => {
                return command(&argv, &self.pin_env(pin, &pin.git_dir));
            }
            prepared => prepared?.with_optional_index_write(refreshes),
        };
        let outcome = command(&argv, &self.pin_env(pin, scratch.path()));
        let published = scratch.publish();
        let value = outcome?;
        published?;
        Ok(value)
    }

    fn verify_pin(&self, pin: &Pin) -> Result<()> {
        let root = pin.repo_root.canonicalize()?;
        let (main_git, common) = super::main_identity::identity_for(&root)?;
        ensure!(common == pin.common_dir, "pinned common directory changed");
        if self.work_tree == root {
            ensure!(
                main_git == pin.git_dir,
                "pinned checkout git directory changed"
            );
        }
        Ok(())
    }

    /// The variables that pin git to `git_dir`, the worktree and `pin`'s
    /// common directory.
    fn pin_env<'a>(&'a self, pin: &'a Pin, git_dir: &'a Path) -> [(&'static str, &'a OsStr); 3] {
        [
            ("GIT_DIR", git_dir.as_os_str()),
            ("GIT_WORK_TREE", self.work_tree.as_os_str()),
            ("GIT_COMMON_DIR", pin.common_dir.as_os_str()),
        ]
    }
}

fn owned_vars(env: &[(&str, &OsStr)]) -> Vec<(OsString, OsString)> {
    env.iter()
        .map(|&(key, value)| (OsString::from(key), value.to_os_string()))
        .collect()
}

#[cfg(test)]
#[path = "pinned_tests.rs"]
mod tests;
