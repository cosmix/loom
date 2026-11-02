//! Git hook installation for loom
//!
//! Installs the pre-commit hook that prevents accidental commits of .loom/work/
//! and .worktrees/, and the reference-transaction hook that attests host moves
//! of a guarded target ([`install_reference_transaction_hook`]), in the directory
//! git runs hooks from (`location`).

use anyhow::{bail, Context, Result};
use std::fs;
use std::io::Write;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;

mod location;
mod reference_transaction;

pub use location::installable_hooks_dir;
use location::{active_hooks_dir, hooks_dir_for_install};
pub use reference_transaction::{
    install_reference_transaction_hook, is_reference_transaction_hook_installed,
    reference_transaction_hook_occupied, HookInstall,
};

/// Marker for loom's pre-commit hook section (for idempotent installation)
const LOOM_HOOK_START_MARKER: &str = "# LOOM_PRE_COMMIT_HOOK_START";
const LOOM_HOOK_END_MARKER: &str = "# LOOM_PRE_COMMIT_HOOK_END";
const FOREIGN_HOOK_NAME: &str = "pre-commit.loom-original";

/// The pre-commit hook script content (embedded from loom-hooks/git-pre-commit-hook.sh)
const PRE_COMMIT_HOOK_CONTENT: &str = include_str!("../../../loom-hooks/git-pre-commit-hook.sh");

/// Install the pre-commit hook in the directory git runs hooks from
///
/// This function:
/// 1. Refuses if `repo_root` has no `.git` directory - there is nothing to
///    install into - or if `core.hooksPath` names a directory loom must not
///    write to and the hook is not already there ([`installable_hooks_dir`])
/// 2. Creates the hooks directory if it doesn't exist
/// 3. Saves an executable foreign hook byte-for-byte as
///    `pre-commit.loom-original`, then installs a Bash hook that runs Loom
///    first and execs the original with its arguments and exit status
/// 4. Replaces an earlier Loom section in place, keeping every other byte
/// 5. Refuses ambiguous ownership or a hook without an executable shebang
/// 6. Repairs the executable mode of a current Loom hook
/// 7. Refuses a symlink at the hook path whose target is missing
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

    let Some(git_hooks_dir) =
        hooks_dir_for_install(repo_root, || is_pre_commit_hook_installed(repo_root))?
    else {
        return Ok(false);
    };
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
            install_where_absent(&hook_path, &foreign_path, &plain)
        }
        Err(error) => {
            Err(error).with_context(|| format!("Failed to read hook: {}", hook_path.display()))
        }
    }
}

/// Install where reading the hook found nothing. That is also what a symlink with
/// a missing target reads as, and it belongs to whoever made it: a plain write
/// would create the target, wherever the link points.
fn install_where_absent(hook: &Path, foreign: &Path, plain: &str) -> Result<bool> {
    if path_occupied(foreign)? {
        bail!(
            "{} already exists; restore the pre-commit hook before installing Loom",
            foreign.display()
        );
    }
    if path_occupied(hook)? {
        bail!(
            "{} is a symlink whose target is missing; restore the target or remove the link \
             before installing Loom",
            hook.display()
        );
    }
    create_hook(hook, plain.as_bytes())?;
    Ok(true)
}

fn install_over_existing(
    hook: &Path,
    foreign: &Path,
    bytes: &[u8],
    plain: &str,
    chained: &str,
    section: &str,
) -> Result<bool> {
    let upgraded = match upgraded_hook(bytes, section, plain) {
        HookSection::Absent => return chain_foreign_hook(hook, foreign, bytes, chained),
        HookSection::Malformed => bail!(
            "{} contains an unrecognized Loom section; inspect and remove it before reinstalling",
            hook.display()
        ),
        HookSection::Upgraded(upgraded) => upgraded,
    };
    if upgraded == bytes {
        return repair_current_hook(hook, foreign, bytes, plain);
    }
    require_companion(foreign, &upgraded, plain)?;
    if !fs::symlink_metadata(hook)
        .with_context(|| format!("Failed to stat hook: {}", hook.display()))?
        .is_file()
    {
        bail!(
            "{} holds an older Loom section but is not a regular file; update its target by hand",
            hook.display()
        );
    }
    replace_hook(hook, &upgraded)?;
    Ok(true)
}

/// Leave a hook holding the current section in place, repairing only its mode.
fn repair_current_hook(hook: &Path, foreign: &Path, bytes: &[u8], plain: &str) -> Result<bool> {
    require_companion(foreign, bytes, plain)?;
    if has_installed_mode(hook)? {
        return Ok(false);
    }
    make_executable(hook)?;
    Ok(true)
}

/// Refuse a hook whose relation to `pre-commit.loom-original` does not hold.
fn require_companion(foreign: &Path, content: &[u8], plain: &str) -> Result<()> {
    if content == plain.as_bytes() && path_occupied(foreign)? {
        bail!(
            "{} exists beside an unchained Loom hook; inspect it before repairing",
            foreign.display()
        );
    }
    if chains_original(content) && !foreign_hook_ready(foreign)? {
        bail!(
            "{} is missing or not executable; restore the original hook before repairing Loom",
            foreign.display()
        );
    }
    Ok(())
}

fn chains_original(content: &[u8]) -> bool {
    !occurrences(content, FOREIGN_HOOK_NAME.as_bytes()).is_empty()
}

/// A hook's Loom section, read through its markers.
#[derive(Debug, PartialEq, Eq)]
enum HookSection {
    /// No marker: the hook is foreign.
    Absent,
    /// The markers do not bound exactly one section.
    Malformed,
    /// The hook as it should read with the current section.
    Upgraded(Vec<u8>),
}

/// `content` with its Loom section replaced by `section`, every other byte kept.
/// The first installer's wrapper around the section becomes the current one,
/// because that wrapper has no `set -e`.
fn upgraded_hook(content: &[u8], section: &str, plain: &str) -> HookSection {
    let starts = occurrences(content, LOOM_HOOK_START_MARKER.as_bytes());
    let ends = occurrences(content, LOOM_HOOK_END_MARKER.as_bytes());
    let (start, end) = match (starts.as_slice(), ends.as_slice()) {
        ([], []) => return HookSection::Absent,
        ([start], [end]) if start < end => (*start, *end + LOOM_HOOK_END_MARKER.len()),
        _ => return HookSection::Malformed,
    };
    let upgraded = [&content[..start], section.as_bytes(), &content[end..]].concat();
    let first_wrapper = format!("#!/usr/bin/env bash\n# Git pre-commit hook\n\n{section}");
    HookSection::Upgraded(if upgraded == first_wrapper.as_bytes() {
        plain.as_bytes().to_vec()
    } else {
        upgraded
    })
}

fn occurrences(content: &[u8], needle: &[u8]) -> Vec<usize> {
    content
        .windows(needle.len())
        .enumerate()
        .filter(|(_, window)| *window == needle)
        .map(|(index, _)| index)
        .collect()
}

/// A complete, executable copy of `content` beside `hook`, so git never runs a
/// partial one and a failed write leaves the foreign bytes where they were. It is
/// closed here: the kernel refuses to execute a file open for writing.
fn staged_hook(hook: &Path, content: &[u8]) -> Result<tempfile::TempPath> {
    let parent = hook.parent().context("A hook path has no parent")?;
    let mut staged = tempfile::NamedTempFile::new_in(parent)
        .with_context(|| format!("Failed to stage hook in {}", parent.display()))?;
    staged
        .write_all(content)
        .with_context(|| format!("Failed to write hook: {}", hook.display()))?;
    make_executable(staged.path())?;
    Ok(staged.into_temp_path())
}

fn replace_hook(hook: &Path, content: &[u8]) -> Result<()> {
    staged_hook(hook, content)?
        .persist(hook)
        .with_context(|| format!("Failed to write hook: {}", hook.display()))?;
    Ok(())
}

/// Put a new hook at `hook`, refusing anything already there, a link included.
fn create_hook(hook: &Path, content: &[u8]) -> Result<()> {
    staged_hook(hook, content)?
        .persist_noclobber(hook)
        .with_context(|| format!("Failed to write hook: {}", hook.display()))?;
    Ok(())
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
    if let Err(error) = create_hook(hook, chained.as_bytes()) {
        let _ = fs::rename(foreign, hook);
        return Err(error);
    }
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

/// Check if the pre-commit hook is installed where git runs it
pub fn is_pre_commit_hook_installed(repo_root: &Path) -> bool {
    let hook_path = active_hooks_dir(repo_root).join("pre-commit");
    let section = extract_loom_section(PRE_COMMIT_HOOK_CONTENT);
    let plain = wrapper_content(&section, false);
    let foreign = hook_path.with_file_name(FOREIGN_HOOK_NAME);
    fs::read(&hook_path).is_ok_and(|content| {
        matches!(upgraded_hook(&content, &section, &plain), HookSection::Upgraded(current) if current == content)
            && require_companion(&foreign, &content, &plain).is_ok()
            && has_installed_mode(&hook_path).unwrap_or(false)
    })
}

/// `git config <scope> --includes --type=path --get core.hooksPath` in `repo_root`
/// (`scope` is `--worktree`, `--local`, `--global` or `--system`), or `None` when
/// unset at that scope or the read fails. `--type=path` expands `~/` as git does
/// when it reads the value to run a hook.
///
/// The reads are scoped because the git runner prepends `-c core.hooksPath=/dev/null`
/// to every command, which wins on precedence over an unscoped `config --get` and
/// over `rev-parse --git-path hooks`: an unscoped read always returns `/dev/null`.
/// A scoped read skips `include.path` files unless `--includes` is given, while git
/// applies a value set in one.
pub fn read_hooks_path_scope(repo_root: &Path, scope: &str) -> Option<String> {
    let args = [
        "config",
        scope,
        "--includes",
        "--type=path",
        "--get",
        "core.hooksPath",
    ];
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
mod location_tests;

#[cfg(test)]
mod reference_transaction_tests;

#[cfg(test)]
#[path = "hooks/tests.rs"]
mod tests;
