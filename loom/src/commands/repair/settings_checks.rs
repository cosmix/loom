//! Repair checks over `.claude/settings.json`, `.claude/settings.local.json`,
//! and the codex CLI's own `~/.codex/config.toml`.

use std::path::Path;

use anyhow::Result;

use super::{DetectedIssue, RepairAction, Severity};
use crate::fs::permissions::constants::LOOM_HOOKS;
use crate::fs::permissions::{
    ensure_loom_hooks_local, loom_hook_scripts_needing_install, main_repo_settings_identity_drift,
    settings_json_has_hooks, settings_local_has_agent_teams_env,
    settings_local_has_worktree_isolation_disabled, settings_local_hook_drift,
};
use crate::sandbox::preflight::settings_local_loom_keys;

/// Every settings-file issue this repo currently has.
pub(super) fn check(repo_root: &Path) -> Vec<DetectedIssue> {
    let mut issues = Vec::new();

    // Only Loom-owned registrations belong in the local settings file.
    if settings_json_has_hooks(repo_root) {
        issues.push(DetectedIssue {
            action: RepairAction::Workspace(super::workspace::WorkspaceFix::HooksAndSettings),
            severity: Severity::Warning,
            description: "Hooks found in .claude/settings.json (should be in settings.local.json)"
                .to_string(),
            fix_description: "Migrate hooks from settings.json to settings.local.json".to_string(),
        });
    }

    issues.extend(codex_slash_tmp_issue());
    issues.extend(hook_scripts_issue());
    issues.extend(jq_missing_issue());
    issues.extend(identity_drift_issues(repo_root));

    if !repo_root.join(".claude/settings.local.json").exists() {
        issues.push(DetectedIssue {
            action: RepairAction::CleanSettings,
            severity: Severity::Info,
            description: "Settings not found (.claude/settings.local.json)".to_string(),
            fix_description: "Apply loom's hooks and env".to_string(),
        });
        return issues;
    }

    issues.extend(settings_local_drift_issue(repo_root));
    issues.extend(loom_written_keys_issue(repo_root));

    issues
}

/// Hook scripts on disk. The registrations in settings.local.json can be
/// perfect and every one of them a dead path: nothing else in repair reads
/// the installed hooks directory, so a missing/stale/non-executable script fails
/// silently at runtime instead of being caught here. Checked regardless of
/// whether settings.local.json exists.
fn hook_scripts_issue() -> Option<DetectedIssue> {
    let needing_install = loom_hook_scripts_needing_install();
    if needing_install.is_empty() {
        return None;
    }
    Some(DetectedIssue {
        action: RepairAction::Workspace(super::workspace::WorkspaceFix::HookScripts),
        severity: Severity::Warning,
        description: format!(
            "Loom hook scripts missing or outdated in {} ({} of {})",
            crate::assets::install::claude_hooks_dir_text(),
            needing_install.len(),
            LOOM_HOOKS.len()
        ),
        fix_description: "Reinstall loom hook scripts".to_string(),
    })
}

/// jq is what every hook script parses its Claude Code payload with. Without
/// it the blocking guards deny every tool call and the lifecycle hooks skip
/// silently, so a missing binary is a broken install even when every hook
/// file is present and current.
fn jq_missing_issue() -> Option<DetectedIssue> {
    if which::which("jq").is_ok() {
        return None;
    }
    Some(DetectedIssue {
        action: RepairAction::Manual,
        severity: Severity::Critical,
        description: "jq is not installed (every loom hook needs it to parse Claude Code payloads)"
            .to_string(),
        fix_description: "Install jq: apt install jq / brew install jq".to_string(),
    })
}

/// Stale per-session identity / dead LOOM_WORK_DIR pin. Checked regardless of
/// whether settings.local.json exists — it also covers settings.json.
fn identity_drift_issues(repo_root: &Path) -> Vec<DetectedIssue> {
    main_repo_settings_identity_drift(repo_root)
        .into_iter()
        .map(|path| DetectedIssue {
            action: RepairAction::IdentityDrift,
            severity: Severity::Warning,
            description: format!("Stale loom session env in {}", path.display()),
            fix_description: "Remove per-session identity env and any dead LOOM_WORK_DIR pin"
                .to_string(),
        })
        .collect()
}

/// Aggregate every way settings.local.json's hooks/env/worktree config can
/// drift from canonical into a single issue: `settings_local_has_hooks` used
/// to only check for presence of *a* `hooks` key, so a file carrying one
/// stale registration silently passed and `--fix` never ran.
fn settings_local_drift_issue(repo_root: &Path) -> Option<DetectedIssue> {
    let mut reasons: Vec<String> = Vec::new();
    let drift = settings_local_hook_drift(repo_root);
    if !drift.missing.is_empty() {
        reasons.push(format!(
            "{} hook registration(s) missing",
            drift.missing.len()
        ));
    }
    if !drift.obsolete.is_empty() {
        reasons.push(format!(
            "{} obsolete hook registration(s)",
            drift.obsolete.len()
        ));
    }
    if !settings_local_has_agent_teams_env(repo_root) {
        reasons.push("agent teams env var missing".to_string());
    }
    if !settings_local_has_worktree_isolation_disabled(repo_root) {
        reasons.push("worktree.bgIsolation not \"none\"".to_string());
    }
    if reasons.is_empty() {
        return None;
    }
    Some(DetectedIssue {
        action: RepairAction::RewriteLocal,
        severity: Severity::Warning,
        description: format!(
            "Loom config drifted in .claude/settings.local.json ({})",
            reasons.join(", ")
        ),
        fix_description: "Rewrite loom hooks, env and worktree config in settings.local.json"
            .to_string(),
    })
}

/// Keys loom used to write into settings.local.json that every session's
/// capsule carries now (plan section 12): the sandbox block, state-directory
/// and plan reads, handoff and `allowWrite` edit rules, the session hook
/// registrations and `env.LOOM_WORK_DIR`. `loom run` warns on the same list.
fn loom_written_keys_issue(repo_root: &Path) -> Option<DetectedIssue> {
    let keys = settings_local_loom_keys(repo_root);
    if keys.is_empty() {
        return None;
    }
    Some(DetectedIssue {
        action: RepairAction::CleanSettings,
        severity: Severity::Warning,
        description: format!(
            "Loom-written keys in .claude/settings.local.json ({})",
            keys.summary()
        ),
        fix_description: "Strip them; every session's capsule carries them now".to_string(),
    })
}

/// Codex's own workspace-write sandbox must exclude /tmp. On Linux the codex
/// CLI wraps every exec in its own bubblewrap sandbox and masks `.git` under
/// every writable root; with `/tmp` among the default roots, bwrap must
/// create the missing `/tmp/.git` mountpoint, which the outer stage sandbox's
/// read-only `/tmp` refuses — every forward dies with `bwrap: Can't mkdir
/// /tmp/.git: Read-only file system` before the model runs a single command.
/// Only checked when the lane is installed; the config is irrelevant without
/// it. See `crate::codex` for the full story.
fn codex_slash_tmp_issue() -> Option<DetectedIssue> {
    if cfg!(target_os = "linux") && crate::codex::codex_lane_status().is_ok() {
        if let Some(config_path) = crate::codex::codex_config_path() {
            if !crate::codex::codex_config_excludes_slash_tmp(&config_path) {
                return Some(DetectedIssue {
            action: RepairAction::CodexSlashTmp,
                    severity: Severity::Warning,
                    description: "codex workspace-write sandbox claims /tmp \
                         (sandbox_workspace_write.exclude_slash_tmp not set in ~/.codex/config.toml)"
                        .to_string(),
                    fix_description:
                        "Set sandbox_workspace_write.exclude_slash_tmp = true in ~/.codex/config.toml"
                            .to_string(),
                });
            }
        }
    }
    None
}

/// Set `sandbox_workspace_write.exclude_slash_tmp = true` in `~/.codex/config.toml`.
pub(super) fn fix_codex_slash_tmp() -> anyhow::Result<bool> {
    match crate::codex::codex_config_path() {
        Some(config_path) => {
            crate::codex::ensure_codex_config_excludes_slash_tmp(&config_path)?;
            Ok(true)
        }
        None => Ok(false),
    }
}

/// Configure hooks and env in settings.local.json
pub(super) fn fix_hooks_local(repo_root: &Path) -> Result<()> {
    ensure_loom_hooks_local(repo_root)?;
    Ok(())
}
