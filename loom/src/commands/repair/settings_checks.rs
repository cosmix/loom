//! Repair checks over `.claude/settings.json` and `.claude/settings.local.json`.

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
