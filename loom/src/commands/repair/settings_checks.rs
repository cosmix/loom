//! Repair checks over `.claude/settings.json`, `.claude/settings.local.json`,
//! and the codex CLI's own `~/.codex/config.toml`.

use std::path::Path;

use anyhow::Result;

use super::{RepairIssue, Severity};
use crate::fs::permissions::constants::LOOM_HOOKS;
use crate::fs::permissions::{
    ensure_loom_hooks_local, loom_hook_scripts_needing_install, main_repo_settings_identity_drift,
    scrub_main_repo_settings_identity, settings_json_has_hooks, settings_local_has_agent_teams_env,
    settings_local_has_codex_sandbox, settings_local_has_worktree_isolation_disabled,
    settings_local_hook_drift,
};

/// Every settings-file issue this repo currently has.
pub(super) fn check(repo_root: &Path) -> Vec<RepairIssue> {
    let mut issues = Vec::new();

    // Hooks belong in settings.local.json, not the committed settings.json.
    if settings_json_has_hooks(repo_root) {
        issues.push(RepairIssue {
            severity: Severity::Warning,
            description: "Hooks found in .claude/settings.json (should be in settings.local.json)"
                .to_string(),
            fix_description: "Migrate hooks from settings.json to settings.local.json".to_string(),
        });
    }

    issues.extend(codex_slash_tmp_issue());
    issues.extend(hook_scripts_issue());
    issues.extend(identity_drift_issues(repo_root));

    if !repo_root.join(".claude/settings.local.json").exists() {
        issues.push(RepairIssue {
            severity: Severity::Info,
            description: "Settings not found (.claude/settings.local.json)".to_string(),
            fix_description: "Apply default sandbox settings and hooks".to_string(),
        });
        return issues;
    }

    issues.extend(settings_local_drift_issue(repo_root));

    // Checked apart from hooks/env: a settings file written before these
    // subprocess allowances existed is otherwise complete, so nothing else
    // here flags it and `--fix` would leave codex runs, or package-manager
    // installs, blocked.
    if !settings_local_has_codex_sandbox(repo_root) {
        issues.push(RepairIssue {
            severity: Severity::Warning,
            description: "Subprocess sandbox allowances missing from .claude/settings.local.json \
                 (codex lane, package-manager caches)"
                .to_string(),
            fix_description: "Grant the codex lane and package managers write access to their \
                               state/cache dirs"
                .to_string(),
        });
    }

    issues
}

/// Hook scripts on disk. The registrations in settings.local.json can be
/// perfect and every one of them a dead path: nothing else in repair reads
/// ~/.claude/hooks/loom, so a missing/stale/non-executable script fails
/// silently at runtime instead of being caught here. Checked regardless of
/// whether settings.local.json exists.
fn hook_scripts_issue() -> Option<RepairIssue> {
    let needing_install = loom_hook_scripts_needing_install();
    if needing_install.is_empty() {
        return None;
    }
    Some(RepairIssue {
        severity: Severity::Warning,
        description: format!(
            "Loom hook scripts missing or outdated in ~/.claude/hooks/loom ({} of {})",
            needing_install.len(),
            LOOM_HOOKS.len()
        ),
        fix_description: "Reinstall loom hook scripts".to_string(),
    })
}

/// Stale per-session identity / dead LOOM_WORK_DIR pin. Checked regardless of
/// whether settings.local.json exists — it also covers settings.json.
fn identity_drift_issues(repo_root: &Path) -> Vec<RepairIssue> {
    main_repo_settings_identity_drift(repo_root)
        .into_iter()
        .map(|path| RepairIssue {
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
fn settings_local_drift_issue(repo_root: &Path) -> Option<RepairIssue> {
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
    Some(RepairIssue {
        severity: Severity::Warning,
        description: format!(
            "Loom config drifted in .claude/settings.local.json ({})",
            reasons.join(", ")
        ),
        fix_description: "Rewrite loom hooks, env and worktree config in settings.local.json"
            .to_string(),
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
fn codex_slash_tmp_issue() -> Option<RepairIssue> {
    if cfg!(target_os = "linux") && crate::codex::codex_lane_status().is_ok() {
        if let Some(config_path) = crate::codex::codex_config_path() {
            if !crate::codex::codex_config_excludes_slash_tmp(&config_path) {
                return Some(RepairIssue {
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

/// Claim and repair the `.claude` settings-file issues. Returns `None` if
/// `description` names none of them, so `fix_issue` can fall through to its
/// remaining arms.
///
/// Order is load-bearing:
/// 1. "Settings not found (.claude/settings.local.json)" and "Stale
///    knowledge-directory deny in" both regenerate the sandbox settings and
///    rewrite hooks/env — matched together, ahead of the two arms below.
/// 2. `starts_with("Stale loom session env in")` must precede the generic
///    ".claude/settings.local.json" arm: a settings.local.json copy's
///    description names that file too, and the generic arm's
///    `fix_hooks_local` never touches settings.json, so the settings.json
///    copy would go unhealed if the generic arm claimed it first.
/// 3. The generic ".claude/settings.local.json" arm catches everything else
///    that names this file — missing hooks/env, missing codex sandbox
///    allowances — by rewriting it. The file-absent case is claimed by arm 1,
///    which runs first.
pub(super) fn fix_settings_issue(repo_root: &Path, description: &str) -> Option<Result<()>> {
    if description.contains("Settings not found (.claude/settings.local.json)")
        || description.contains("Stale knowledge-directory deny in")
    {
        return Some(
            super::sandbox_settings::fix_sandbox_settings(repo_root)
                .and_then(|()| fix_hooks_local(repo_root)),
        );
    }
    if description.starts_with("Stale loom session env in") {
        scrub_main_repo_settings_identity(repo_root);
        return Some(Ok(()));
    }
    if description.contains(".claude/settings.local.json") {
        return Some(fix_hooks_local(repo_root));
    }
    None
}
