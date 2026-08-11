//! Repair checks over `.claude/settings.json`, `.claude/settings.local.json`,
//! and the codex CLI's own `~/.codex/config.toml`.

use std::path::Path;

use super::{RepairIssue, Severity};
use crate::fs::permissions::{
    settings_json_has_hooks, settings_local_has_codex_sandbox, settings_local_has_hooks,
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

    if !repo_root.join(".claude/settings.local.json").exists() {
        issues.push(RepairIssue {
            severity: Severity::Info,
            description: "Settings not found (.claude/settings.local.json)".to_string(),
            fix_description: "Apply default sandbox settings and hooks".to_string(),
        });
        return issues;
    }

    if !settings_local_has_hooks(repo_root) {
        issues.push(RepairIssue {
            severity: Severity::Info,
            description: "Hooks/env missing from .claude/settings.local.json".to_string(),
            fix_description: "Configure hooks and env in settings.local.json".to_string(),
        });
    }

    // Checked apart from hooks/env: a settings file written before the codex
    // lane had sandbox allowances is otherwise complete, so nothing else here
    // flags it and `--fix` would leave every codex run blocked.
    if !settings_local_has_codex_sandbox(repo_root) {
        issues.push(RepairIssue {
            severity: Severity::Warning,
            description: "Codex sandbox allowances missing from .claude/settings.local.json"
                .to_string(),
            fix_description: "Grant the codex lane write access to its state dirs".to_string(),
        });
    }

    issues
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
