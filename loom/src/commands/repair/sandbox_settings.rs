//! `.claude/settings.local.json` sandbox regeneration, the stale
//! doc/loom/knowledge deny check/fix, and the stale token-deny-shape one.

use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

use super::{RepairIssue, Severity};
use crate::fs::permissions::state_root::{self, is_parent_glob_token_deny, is_token_read_deny};

/// Check 13: stale doc/loom/knowledge deny in .claude/settings.local.json.
///
/// A settings file written before the knowledge-directory sandbox grant
/// existed can carry `Edit(doc/loom/knowledge/**)` / `Write(doc/loom/knowledge/**)`
/// in `permissions.deny` alongside the (shadowed, harmless) `allowWrite`
/// grant — deny wins, so the `loom knowledge update` CLI subprocess stays
/// blocked for that checkout until the file is regenerated.
/// `write_settings`'s scrub (`merge_existing_permissions`) heals this
/// automatically on regeneration, but nothing today prompts for one; this
/// check is that prompt. Checked in both the main repo and every worktree,
/// since each has its own settings.local.json.
pub(super) fn check_stale_knowledge_denies(repo_root: &Path) -> Vec<RepairIssue> {
    existing_settings_local_files(repo_root)
        .into_iter()
        .filter(|settings_path| settings_local_has_stale_knowledge_deny(settings_path))
        .map(|settings_path| RepairIssue {
            severity: Severity::Warning,
            description: format!(
                "Stale knowledge-directory deny in {}",
                settings_path.display()
            ),
            fix_description: "Regenerate sandbox settings so the knowledge grant is not \
                               shadowed by a stale deny"
                .to_string(),
        })
        .collect()
}

/// Every `.claude/settings.local.json` that currently exists: the main
/// repo's, plus one per worktree that already has its own. A worktree
/// without a settings file yet gets one when its own stage session starts,
/// so it is not a repair issue and is skipped here.
fn existing_settings_local_files(repo_root: &Path) -> Vec<PathBuf> {
    let mut paths = Vec::new();

    let main_settings = repo_root.join(".claude/settings.local.json");
    if main_settings.exists() {
        paths.push(main_settings);
    }

    if let Ok(entries) = fs::read_dir(repo_root.join(".worktrees")) {
        for entry in entries.flatten() {
            let settings_path = entry.path().join(".claude/settings.local.json");
            if entry.path().is_dir() && settings_path.exists() {
                paths.push(settings_path);
            }
        }
    }

    paths
}

/// Whether a `permissions.deny` entry names the knowledge directory, in
/// either the enforced `Edit(...)` form or the inert-but-OS-leaking
/// `Write(...)` form (see `sandbox::settings::merge_existing_permissions`'s
/// doc comment for why both matter). Shared by the detector
/// (`settings_local_has_stale_knowledge_deny`) and the worktree scalpel
/// (`strip_stale_knowledge_denies`) so the two can never drift apart on what
/// counts as "stale".
fn is_knowledge_dir_deny_entry(entry: &str) -> bool {
    entry.starts_with("Edit(doc/loom/knowledge") || entry.starts_with("Write(doc/loom/knowledge")
}

/// Whether a `.claude/settings.local.json` file at `path` carries a stale
/// `permissions.deny` entry for the knowledge directory. Any parse failure
/// is treated as "no issue" — this is a diagnostic nudge, not a validator,
/// and `loom repair` already has other checks for a malformed settings file.
fn settings_local_has_stale_knowledge_deny(path: &Path) -> bool {
    let Ok(content) = fs::read_to_string(path) else {
        return false;
    };
    let Ok(value) = serde_json::from_str::<serde_json::Value>(&content) else {
        return false;
    };
    value["permissions"]["deny"]
        .as_array()
        .map(|deny| {
            deny.iter()
                .any(|entry| entry.as_str().is_some_and(is_knowledge_dir_deny_entry))
        })
        .unwrap_or(false)
}

/// Apply default sandbox settings to `target`'s `.claude/settings.local.json`.
///
/// `target` may be the main repo root or a worktree root — `write_settings`
/// resolves `target_is_worktree` itself from the path it is given, so a
/// fresh `merge_config` per target (rather than reusing one merged config
/// across targets) keeps that resolution correct for whichever `target` this
/// call was given.
///
/// Always passes `&Implementers::default()` (claude-only), matching the
/// existing main-repo behavior: this repairs the DEFAULT sandbox, not
/// whatever lane a stage's own plan may have licensed. See the doc comment
/// on `sandbox::settings::preserve_unowned_keys` for why the claude-only
/// default here is deliberate — restoring an actual codex license is a
/// separate concern from repairing a broken default.
fn write_default_sandbox_settings(target: &Path) -> Result<()> {
    use crate::models::stage::Implementers;
    use crate::plan::schema::{SandboxConfig, StageSandboxConfig, StageType};
    let mut merged = crate::sandbox::merge_config(
        &SandboxConfig::default(),
        &StageSandboxConfig::default(),
        StageType::Standard,
        &Implementers::default(),
    );
    crate::sandbox::expand_paths(&mut merged);
    crate::sandbox::write_settings(&merged, target)?;
    Ok(())
}

/// Remove stale doc/loom/knowledge `permissions.deny` entries from a
/// worktree's `.claude/settings.local.json`, in place — every other key
/// (the rest of `permissions`, the `sandbox` block, plugin keys, anything
/// else) is left exactly as it was.
///
/// Unlike the main repo (regenerated wholesale by
/// `write_default_sandbox_settings`), a worktree's settings file is the
/// sandbox of a possibly LIVE stage session, and it legitimately differs
/// from the default: a codex-licensed stage carries `~/.codex` write grants
/// and the codex domains, and any stage carries its plan's own
/// `allow_write` entries. Regenerating it from `SandboxConfig::default()`
/// would silently narrow a running stage's sandbox mid-session — a bigger
/// hazard than the stale deny being healed, and one the stage would not
/// recover from until its next respawn. So only the offending entries are
/// stripped.
fn strip_stale_knowledge_denies(path: &Path) -> Result<()> {
    let content =
        fs::read_to_string(path).with_context(|| format!("Failed to read {}", path.display()))?;
    let mut value: serde_json::Value = serde_json::from_str(&content)
        .with_context(|| format!("Failed to parse {}", path.display()))?;

    if let Some(deny) = value
        .pointer_mut("/permissions/deny")
        .and_then(|d| d.as_array_mut())
    {
        deny.retain(|entry| !entry.as_str().is_some_and(is_knowledge_dir_deny_entry));
    }

    let updated = serde_json::to_string_pretty(&value)
        .with_context(|| format!("Failed to serialize {}", path.display()))?;
    fs::write(path, updated).with_context(|| format!("Failed to write {}", path.display()))?;
    Ok(())
}

/// Apply default sandbox settings to the main repo, and scalpel the stale
/// doc/loom/knowledge deny out of every worktree that carries one.
///
/// The main repo's settings file is loom's own default sandbox (used for
/// knowledge-type stages and general repair), so full regeneration is
/// correct there — see `write_default_sandbox_settings`. A worktree's
/// settings file is not: see `strip_stale_knowledge_denies` for why it gets
/// a targeted fix instead of regeneration. A worktree with no stale deny,
/// or no settings file yet, is left untouched. `pub(super)`: also called by
/// `settings_checks::fix_settings_issue`.
pub(super) fn fix_sandbox_settings(repo_root: &Path) -> Result<()> {
    write_default_sandbox_settings(repo_root)?;

    if let Ok(entries) = fs::read_dir(repo_root.join(".worktrees")) {
        for entry in entries.flatten() {
            let settings_path = entry.path().join(".claude/settings.local.json");
            if entry.path().is_dir() && settings_local_has_stale_knowledge_deny(&settings_path) {
                strip_stale_knowledge_denies(&settings_path)?;
            }
        }
    }

    Ok(())
}

/// Check 14: token deny rules in a shape that prompts on every search.
///
/// Claude Code's `deniedPathInsideDirectory` check refuses `rg`, `grep`,
/// `diff`, `git`, `cp` and `mv` over any directory that contains a `Read(...)`
/// deny rule's location — the rule's path up to its first wildcard — and that
/// refusal is bypass-immune and not classifier-approvable. A settings file
/// written before loom globbed the project directory out of its token denies
/// (`state_root::token_read_denies`) puts that location inside the project, so
/// every search from the project root stalls auto mode on an operator prompt.
/// Checked in the main repo's two settings files and in every worktree's.
pub(super) fn check_stale_token_denies(repo_root: &Path) -> Vec<RepairIssue> {
    token_deny_settings_files(repo_root)
        .into_iter()
        .filter(|settings_path| has_stale_token_deny(settings_path))
        .map(|settings_path| RepairIssue {
            severity: Severity::Warning,
            description: format!("Stale token deny shape in {}", settings_path.display()),
            fix_description: "Rewrite the daemon token deny rules so rg and grep stop \
                              prompting for approval"
                .to_string(),
        })
        .collect()
}

/// Every existing settings file that can carry a token deny: the main repo's
/// `settings.local.json` and `settings.json`, plus both spellings in each
/// worktree. `git::worktree::settings` writes a worktree's `settings.json`,
/// `sandbox::write_settings` its `settings.local.json`, so both can hold a
/// stale shape.
fn token_deny_settings_files(repo_root: &Path) -> Vec<PathBuf> {
    let mut paths = Vec::new();
    let mut push_existing = |path: PathBuf| {
        if path.exists() {
            paths.push(path);
        }
    };

    push_existing(repo_root.join(".claude/settings.local.json"));
    push_existing(repo_root.join(".claude/settings.json"));

    if let Ok(entries) = fs::read_dir(repo_root.join(".worktrees")) {
        for entry in entries.flatten().filter(|entry| entry.path().is_dir()) {
            push_existing(entry.path().join(".claude/settings.local.json"));
            push_existing(entry.path().join(".claude/settings.json"));
        }
    }

    paths
}

/// A token deny loom would no longer write: any spelling other than the
/// current parent-glob one. Shared by the detector and the fix so the two
/// cannot drift on what counts as stale.
fn is_stale_token_deny(entry: &str) -> bool {
    is_token_read_deny(entry) && !is_parent_glob_token_deny(entry)
}

/// Whether a settings file at `path` carries a stale token deny. Any read or
/// parse failure is treated as "no issue", the same diagnostic-nudge posture
/// as `settings_local_has_stale_knowledge_deny`.
fn has_stale_token_deny(path: &Path) -> bool {
    let Ok(content) = fs::read_to_string(path) else {
        return false;
    };
    let Ok(value) = serde_json::from_str::<serde_json::Value>(&content) else {
        return false;
    };
    value["permissions"]["deny"]
        .as_array()
        .map(|deny| {
            deny.iter()
                .any(|entry| entry.as_str().is_some_and(is_stale_token_deny))
        })
        .unwrap_or(false)
}

/// Fix for check 14. The main repo's `settings.local.json` is regenerated —
/// `carry_forward_denies` prunes the stale entries and the generator emits the
/// current shape. Every other stale file is treated with the scalpel
/// `strip_stale_knowledge_denies` documents: the offending entries are removed
/// and, for a worktree file, the correct rules pushed back, with every other
/// key untouched. The main repo's `settings.json` only gets the removal; its
/// `settings.local.json` is where loom writes the rules.
pub(super) fn fix_stale_token_denies(repo_root: &Path) -> Result<()> {
    write_default_sandbox_settings(repo_root)?;

    let main_local = repo_root.join(".claude/settings.local.json");
    for settings_path in token_deny_settings_files(repo_root) {
        if settings_path == main_local || !has_stale_token_deny(&settings_path) {
            continue;
        }
        let worktree = owning_worktree(repo_root, &settings_path);
        rewrite_token_denies(&settings_path, worktree.as_deref())?;
    }

    Ok(())
}

/// The worktree root a settings file belongs to, or `None` when it is one of
/// the main repo's own files.
fn owning_worktree(repo_root: &Path, settings_path: &Path) -> Option<PathBuf> {
    let root = settings_path.parent()?.parent()?;
    (root != repo_root).then(|| root.to_path_buf())
}

/// Strip stale token denies from `path` in place and, for a worktree file,
/// push the current parent-glob rules back. A worktree whose state-root
/// symlink no longer resolves keeps the removal and gets no replacement — the
/// next stage spawn rewrites the file anyway.
fn rewrite_token_denies(path: &Path, worktree: Option<&Path>) -> Result<()> {
    let content =
        fs::read_to_string(path).with_context(|| format!("Failed to read {}", path.display()))?;
    let mut value: serde_json::Value = serde_json::from_str(&content)
        .with_context(|| format!("Failed to parse {}", path.display()))?;

    if let Some(deny) = value
        .pointer_mut("/permissions/deny")
        .and_then(|entries| entries.as_array_mut())
    {
        deny.retain(|entry| !entry.as_str().is_some_and(is_stale_token_deny));
        for rule in current_token_denies(worktree) {
            if !deny
                .iter()
                .any(|entry| entry.as_str() == Some(rule.as_str()))
            {
                deny.push(serde_json::Value::String(rule));
            }
        }
    }

    let updated = serde_json::to_string_pretty(&value)
        .with_context(|| format!("Failed to serialize {}", path.display()))?;
    fs::write(path, updated).with_context(|| format!("Failed to write {}", path.display()))?;
    Ok(())
}

/// The rules a worktree's settings file should carry, empty for a main-repo
/// file or a symlink that no longer resolves.
fn current_token_denies(worktree: Option<&Path>) -> Vec<String> {
    worktree
        .and_then(state_root::resolve_state_root)
        .map(|resolved| state_root::token_read_denies(&resolved.to_string_lossy()).to_vec())
        .unwrap_or_default()
}
