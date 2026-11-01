//! Git hook installation for loom
//!
//! Installs the pre-commit hook that prevents accidental commits of .loom/work/
//! and .worktrees/, and the reference-transaction hook that attests host moves
//! of a guarded target ([`install_reference_transaction_hook`]).

use anyhow::{bail, Context, Result};
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;

mod reference_transaction;

pub use reference_transaction::{
    install_reference_transaction_hook, is_reference_transaction_hook_installed, HookInstall,
};

/// Marker for loom's pre-commit hook section (for idempotent installation)
const LOOM_HOOK_START_MARKER: &str = "# LOOM_PRE_COMMIT_HOOK_START";
const LOOM_HOOK_END_MARKER: &str = "# LOOM_PRE_COMMIT_HOOK_END";
const FOREIGN_HOOK_NAME: &str = "pre-commit.loom-original";

/// The pre-commit hook script content (embedded from loom-hooks/git-pre-commit-hook.sh)
const PRE_COMMIT_HOOK_CONTENT: &str = include_str!("../../../loom-hooks/git-pre-commit-hook.sh");

/// Install the pre-commit hook to the repository's .git/hooks directory
///
/// This function:
/// 1. Refuses if `repo_root` has no `.git` directory - there is nothing to
///    install into
/// 2. Creates the hooks directory if it doesn't exist
/// 3. Saves an executable foreign hook byte-for-byte as
///    `pre-commit.loom-original`, then installs a Bash hook that runs Loom
///    first and execs the original with its arguments and exit status
/// 4. Refuses ambiguous ownership or a hook without an executable shebang
/// 5. Repairs the executable mode of a current Loom hook
///
/// # Arguments
/// * `repo_root` - Path to the repository root
///
/// # Returns
/// * `Ok(true)` - Hook was installed or updated
/// * `Ok(false)` - Hook was already up to date
/// * `Err` - Installation failed
pub fn install_pre_commit_hook(repo_root: &Path) -> Result<bool> {
    if !repo_root.join(".git").is_dir() {
        bail!(
            "{} is not a git repository (no .git directory); run `loom init` to set one up",
            repo_root.display()
        );
    }

    let git_hooks_dir = repo_root.join(".git/hooks");
    fs::create_dir_all(&git_hooks_dir).with_context(|| {
        format!(
            "Failed to create hooks directory: {}",
            git_hooks_dir.display()
        )
    })?;
    let hook_path = git_hooks_dir.join("pre-commit");
    let loom_section = extract_loom_section(PRE_COMMIT_HOOK_CONTENT);
    let foreign_path = git_hooks_dir.join(FOREIGN_HOOK_NAME);
    let plain = wrapper_content(&loom_section, false);
    let chained = wrapper_content(&loom_section, true);
    match fs::read(&hook_path) {
        Ok(bytes) => install_over_existing(
            &hook_path,
            &foreign_path,
            &bytes,
            &plain,
            &chained,
            &loom_section,
        ),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            if path_occupied(&foreign_path)? {
                bail!(
                    "{} already exists; restore the pre-commit hook before installing Loom",
                    foreign_path.display()
                );
            }
            fs::write(&hook_path, plain)
                .with_context(|| format!("Failed to write hook: {}", hook_path.display()))?;
            make_executable(&hook_path)?;
            Ok(true)
        }
        Err(error) => {
            Err(error).with_context(|| format!("Failed to read hook: {}", hook_path.display()))
        }
    }
}

fn install_over_existing(
    hook: &Path,
    foreign: &Path,
    bytes: &[u8],
    plain: &str,
    chained: &str,
    section: &str,
) -> Result<bool> {
    if bytes == plain.as_bytes() || bytes == chained.as_bytes() {
        if bytes == plain.as_bytes() && path_occupied(foreign)? {
            bail!(
                "{} exists beside an unchained Loom hook; inspect it before repairing",
                foreign.display()
            );
        }
        if bytes == chained.as_bytes() && !foreign_hook_ready(foreign)? {
            bail!(
                "{} is missing or not executable; restore the original hook before repairing Loom",
                foreign.display()
            );
        }
        if has_installed_mode(hook)? {
            return Ok(false);
        }
        make_executable(hook)?;
        return Ok(true);
    }
    // The old installer produced exactly this hook when no foreign hook existed.
    let old_plain = format!("#!/usr/bin/env bash\n# Git pre-commit hook\n\n{section}");
    if bytes == old_plain.as_bytes() {
        fs::write(hook, plain)
            .with_context(|| format!("Failed to write hook: {}", hook.display()))?;
        make_executable(hook)?;
        return Ok(true);
    }
    if bytes
        .windows(LOOM_HOOK_START_MARKER.len())
        .any(|part| part == LOOM_HOOK_START_MARKER.as_bytes())
    {
        bail!(
            "{} contains an unrecognized Loom section; inspect and remove it before reinstalling",
            hook.display()
        );
    }
    chain_foreign_hook(hook, foreign, bytes, chained)
}

fn chain_foreign_hook(hook: &Path, foreign: &Path, bytes: &[u8], chained: &str) -> Result<bool> {
    if path_occupied(foreign)? {
        bail!(
            "{} already exists; move it aside before chaining the pre-commit hook",
            foreign.display()
        );
    }
    if !bytes.starts_with(b"#!") || !foreign_hook_ready(hook)? {
        bail!(
            "{} must have an executable shebang before Loom can chain it",
            hook.display()
        );
    }
    fs::rename(hook, foreign)
        .with_context(|| format!("Failed to preserve hook at {}", foreign.display()))?;
    if let Err(error) = fs::write(hook, chained) {
        let _ = fs::rename(foreign, hook);
        return Err(error).with_context(|| format!("Failed to write hook: {}", hook.display()));
    }
    make_executable(hook)?;
    Ok(true)
}

fn wrapper_content(loom_section: &str, chain: bool) -> String {
    let mut content =
        format!("#!/usr/bin/env bash\n# Loom pre-commit hook\nset -e\n\n{loom_section}\n");
    if chain {
        content.push_str("\n# The original runs with its own interpreter and exit status.\nexec \"$(dirname \"$0\")/pre-commit.loom-original\" \"$@\"\n");
    }
    content
}

fn foreign_hook_ready(path: &Path) -> Result<bool> {
    match fs::symlink_metadata(path) {
        Ok(metadata) => Ok(metadata.is_file() && metadata.permissions().mode() & 0o111 != 0),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(error) => {
            Err(error).with_context(|| format!("Failed to stat hook: {}", path.display()))
        }
    }
}

fn path_occupied(path: &Path) -> Result<bool> {
    match fs::symlink_metadata(path) {
        Ok(_) => Ok(true),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(error) => {
            Err(error).with_context(|| format!("Failed to inspect hook: {}", path.display()))
        }
    }
}

pub(super) fn has_installed_mode(path: &Path) -> Result<bool> {
    let metadata = fs::metadata(path)
        .with_context(|| format!("Failed to get metadata for hook: {}", path.display()))?;
    Ok(metadata.permissions().mode() & 0o777 == 0o755)
}

/// Make a hook executable (mode 0755)
fn make_executable(hook_path: &Path) -> Result<()> {
    let mut perms = fs::metadata(hook_path)
        .with_context(|| format!("Failed to get metadata for hook: {}", hook_path.display()))?
        .permissions();
    perms.set_mode(0o755);
    fs::set_permissions(hook_path, perms)
        .with_context(|| format!("Failed to set permissions on hook: {}", hook_path.display()))?;
    Ok(())
}

/// Extract the loom section from the full hook content
fn extract_loom_section(content: &str) -> String {
    let start = content.find(LOOM_HOOK_START_MARKER);
    let end = content.find(LOOM_HOOK_END_MARKER);

    match (start, end) {
        (Some(s), Some(e)) => content[s..e + LOOM_HOOK_END_MARKER.len()].to_string(),
        _ => {
            // Fallback: use the whole content (shouldn't happen with proper hook file)
            content.to_string()
        }
    }
}

/// Check if the pre-commit hook is installed
pub fn is_pre_commit_hook_installed(repo_root: &Path) -> bool {
    let hook_path = repo_root.join(".git/hooks/pre-commit");
    let section = extract_loom_section(PRE_COMMIT_HOOK_CONTENT);
    let plain = wrapper_content(&section, false);
    let chained = wrapper_content(&section, true);
    fs::read(&hook_path).is_ok_and(|content| {
        ((content == plain.as_bytes()
            && !path_occupied(&hook_path.with_file_name(FOREIGN_HOOK_NAME)).unwrap_or(true))
            || (content == chained.as_bytes()
                && foreign_hook_ready(&hook_path.with_file_name(FOREIGN_HOOK_NAME))
                    .unwrap_or(false)))
            && has_installed_mode(&hook_path).unwrap_or(false)
    })
}

/// `git config <scope> --includes --get core.hooksPath` in `repo_root` (`scope` is
/// `--worktree`, `--local`, `--global` or `--system`), or `None` when unset at that
/// scope or the read fails.
///
/// The reads are scoped because the git runner prepends `-c core.hooksPath=/dev/null`
/// to every command, which wins on precedence over an unscoped `config --get` and
/// over `rev-parse --git-path hooks`: an unscoped read always returns `/dev/null`.
/// A scoped read skips `include.path` files unless `--includes` is given, while git
/// applies a value set in one.
pub fn read_hooks_path_scope(repo_root: &Path, scope: &str) -> Option<String> {
    let args = ["config", scope, "--includes", "--get", "core.hooksPath"];
    let configured = super::run_git_checked(&args, repo_root).ok()?;
    let trimmed = configured.trim();
    (!trimmed.is_empty()).then(|| trimmed.to_string())
}

/// The effective `core.hooksPath`: the first of the worktree, local, global and
/// system scopes that sets it, which is git's own precedence order. `None` when no
/// scope sets it.
///
/// The worktree scope is `config.worktree`, which git reads only with
/// `extensions.worktreeConfig` on. With the extension off, a `--worktree` read
/// returns the local value in a repository without linked worktrees and fails in
/// one with them, which reads as unset: right, as git then reads no
/// `config.worktree`.
pub fn configured_hooks_path(repo_root: &Path) -> Option<String> {
    ["--worktree", "--local", "--global", "--system"]
        .into_iter()
        .find_map(|scope| read_hooks_path_scope(repo_root, scope))
}

#[cfg(test)]
mod reference_transaction_tests;

#[cfg(test)]
#[path = "hooks/tests.rs"]
mod tests;
