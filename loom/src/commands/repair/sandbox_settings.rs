//! `.claude/settings.local.json` sandbox regeneration, the stale
//! doc/loom/knowledge deny check/fix, and the `Read(...)` deny check/fix.

use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

use super::{RepairIssue, Severity};
use crate::fs::permissions::state_root::is_loom_written_read_deny;

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

/// Check 14: any `Read(...)` deny rule in `permissions.deny`.
///
/// Claude Code's Bash path validator refuses `rg`, `grep`, `diff`, `git`,
/// `cp` and `mv` on a relative path issued after a `cd` in the same compound
/// command whenever ANY settings file carries ANY `Read(...)` deny rule —
/// bypass-immune, not classifier-approvable, and independent of the rule's
/// path shape (see the `state_root` module doc comment). Loom itself writes
/// no `Read(...)` deny at all any more: the daemon token files are instead
/// denied to Bash at the OS level (`sandbox.filesystem.denyRead`,
/// `state_root::token_deny_paths`) and to the native file tools by the
/// `hooks/credential-guard.sh` PreToolUse hook. A settings file written
/// before this change can still carry the `Read(...)` rules loom itself used
/// to write (`state_root::is_loom_written_read_deny`) — those get an
/// automated fix. An operator's own `Read(...)` rule is flagged too, so the
/// prompting hazard is visible, but loom never removes it. Checked in the
/// main repo's two settings files, every worktree's, and — operator-authored
/// only, loom never writes there — `~/.claude/settings.json`.
pub(super) fn check_read_denies(repo_root: &Path) -> Vec<RepairIssue> {
    let mut issues = Vec::new();

    for settings_path in read_deny_settings_files(repo_root) {
        issues.extend(read_deny_issues_for(&settings_path, true));
    }
    if let Some(home_settings) = home_settings_json() {
        issues.extend(read_deny_issues_for(&home_settings, false));
    }

    issues
}

/// Every existing settings file loom itself may have written a `Read(...)`
/// deny into: the main repo's `settings.local.json` and `settings.json`,
/// plus both spellings in each worktree. `git::worktree::settings` writes a
/// worktree's `settings.json`, `sandbox::write_settings` its
/// `settings.local.json`, so both can hold a rule.
fn read_deny_settings_files(repo_root: &Path) -> Vec<PathBuf> {
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

/// `~/.claude/settings.json`, checked for the operator-authored case only —
/// loom never writes to this file, so any `Read(...)` deny found there is
/// always the operator's own, whatever it looks like.
fn home_settings_json() -> Option<PathBuf> {
    let path = dirs::home_dir()?.join(".claude/settings.json");
    path.exists().then_some(path)
}

/// The `permissions.deny` entries in a settings file that start with
/// `Read(`. Any read or parse failure is treated as "no issue", the same
/// diagnostic-nudge posture as `settings_local_has_stale_knowledge_deny`.
fn read_deny_entries(path: &Path) -> Vec<String> {
    let Ok(content) = fs::read_to_string(path) else {
        return Vec::new();
    };
    let Ok(value) = serde_json::from_str::<serde_json::Value>(&content) else {
        return Vec::new();
    };
    value["permissions"]["deny"]
        .as_array()
        .map(|deny| {
            deny.iter()
                .filter_map(|entry| entry.as_str())
                .filter(|entry| entry.starts_with("Read("))
                .map(String::from)
                .collect()
        })
        .unwrap_or_default()
}

/// The read-deny issues for one settings file. `classify_loom_written`
/// selects whether an entry matching `is_loom_written_read_deny` is split
/// out into its own automatically-fixable issue (every other settings file)
/// or reported as operator-authored regardless of shape (`~/.claude/settings.json`,
/// which loom never writes to).
fn read_deny_issues_for(settings_path: &Path, classify_loom_written: bool) -> Vec<RepairIssue> {
    let mut issues = Vec::new();
    let entries = read_deny_entries(settings_path);
    let (loom_written, operator): (Vec<String>, Vec<String>) = if classify_loom_written {
        entries
            .into_iter()
            .partition(|entry| is_loom_written_read_deny(entry))
    } else {
        (Vec::new(), entries)
    };

    if !loom_written.is_empty() {
        issues.push(RepairIssue {
            severity: Severity::Warning,
            description: format!("Read deny rule in {}", settings_path.display()),
            fix_description: "Claude Code prompts for approval on every relative-path rg, \
                               grep, diff, git, cp or mv issued after a cd while this rule \
                               exists"
                .to_string(),
        });
    }
    for entry in operator {
        issues.push(RepairIssue {
            severity: Severity::Warning,
            description: format!(
                "Operator-authored Read deny rule {entry} in {}",
                settings_path.display()
            ),
            fix_description: "loom does not remove operator-authored deny rules; Claude Code \
                               keeps prompting for approval on every relative-path rg, grep, \
                               diff, git, cp or mv issued after a cd until it is removed by \
                               hand"
                .to_string(),
        });
    }

    issues
}

/// Fix for check 14. The main repo's `settings.local.json` is regenerated —
/// `carry_forward_denies` now drops every `Read(...)` deny outright, so the
/// generator emits none. Every other file that carries a loom-written
/// `Read(...)` deny (`state_root::is_loom_written_read_deny`) gets the
/// scalpel `strip_loom_read_denies` documents: those entries removed, every
/// other key — including any operator-authored `Read(...)` rule — left
/// exactly as it was, with nothing pushed back. `~/.claude/settings.json` and
/// an operator's own rule anywhere are never touched — `check_read_denies`
/// only warns about those.
pub(super) fn fix_read_denies(repo_root: &Path) -> Result<()> {
    write_default_sandbox_settings(repo_root)?;

    let main_local = repo_root.join(".claude/settings.local.json");
    for settings_path in read_deny_settings_files(repo_root) {
        if settings_path == main_local {
            continue;
        }
        let has_loom_written = read_deny_entries(&settings_path)
            .iter()
            .any(|entry| is_loom_written_read_deny(entry));
        if has_loom_written {
            strip_loom_read_denies(&settings_path)?;
        }
    }

    Ok(())
}

/// Strip loom-written `Read(...)` deny entries from `path` in place, leaving
/// every other key — including any operator-authored `Read(...)` rule —
/// exactly as it was. Mirrors `strip_stale_knowledge_denies`; nothing is
/// pushed back, since the cure for these entries is their absence (see the
/// `state_root` module doc comment for why).
fn strip_loom_read_denies(path: &Path) -> Result<()> {
    let content =
        fs::read_to_string(path).with_context(|| format!("Failed to read {}", path.display()))?;
    let mut value: serde_json::Value = serde_json::from_str(&content)
        .with_context(|| format!("Failed to parse {}", path.display()))?;

    if let Some(deny) = value
        .pointer_mut("/permissions/deny")
        .and_then(|entries| entries.as_array_mut())
    {
        deny.retain(|entry| !entry.as_str().is_some_and(is_loom_written_read_deny));
    }

    let updated = serde_json::to_string_pretty(&value)
        .with_context(|| format!("Failed to serialize {}", path.display()))?;
    fs::write(path, updated).with_context(|| format!("Failed to write {}", path.display()))?;
    Ok(())
}
