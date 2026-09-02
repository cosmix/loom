//! Old unprefixed `$HOME/.claude` skills, agents, and skill-name references
//! left behind by a pre-`loom-` install.

use std::fs;

use anyhow::{Context, Result};

use super::{RepairIssue, Severity};

/// Loom-specific skill names referenced in settings.json that may need prefix migration.
const LOOM_SKILL_NAMES: &[&str] = &[
    "accessibility",
    "api-design",
    "api-documentation",
    "argocd",
    "auth",
    "background-jobs",
    "before-after",
    "caching",
    "ci-cd",
    "code-migration",
    "code-review",
    "concurrency",
    "crossplane",
    "data-validation",
    "data-visualization",
    "database-design",
    "dead-code-check",
    "debugging",
    "dependency-scan",
    "diagramming",
    "docker",
    "documentation",
    "e2e-testing",
    "error-handling",
    "event-driven",
    "feature-flags",
    "fluxcd",
    "git-workflow",
    "golang",
    "grafana",
    "i18n",
    "istio",
    "karpenter",
    "kubernetes",
    "kustomize",
    "logging-observability",
    "md-tables",
    "model-evaluation",
    "performance-testing",
    "prometheus",
    "prompt-engineering",
    "python",
    "rate-limiting",
    "react",
    "refactoring",
    "rust",
    "search",
    "security-audit",
    "security-scan",
    "serialization",
    "sql-optimization",
    "technical-writing",
    "terraform",
    "test-strategy",
    "testing",
    "threat-model",
    "typescript",
    "webhooks",
    "wiring-test",
];

/// Check 7: old unprefixed skills. Check 8: old unprefixed agents. Check 10:
/// settings.json references to old-style skill names.
pub(super) fn check() -> Vec<RepairIssue> {
    let mut issues = Vec::new();
    issues.extend(check_old_skills());
    issues.extend(check_old_agents());
    issues.extend(check_old_skill_refs());
    issues
}

/// Check 7: old unprefixed skills that have a loom- counterpart.
fn check_old_skills() -> Vec<RepairIssue> {
    let mut issues = Vec::new();
    let Some(home) = dirs::home_dir() else {
        return issues;
    };
    let skills_dir = home.join(".claude/skills");
    let Ok(entries) = fs::read_dir(&skills_dir) else {
        return issues;
    };
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().to_string();
        if name.starts_with("loom-") || !entry.path().is_dir() {
            continue;
        }
        let prefixed = skills_dir.join(format!("loom-{}", name));
        if prefixed.is_dir() {
            issues.push(RepairIssue {
                severity: Severity::Warning,
                description: format!(
                    "Old unprefixed skill '{}' found (superseded by 'loom-{}')",
                    name, name
                ),
                fix_description: format!(
                    "Remove ~/.claude/skills/{} (loom-{} already installed)",
                    name, name
                ),
            });
        }
    }
    issues
}

/// Check 8: old unprefixed agents that have a loom- counterpart.
fn check_old_agents() -> Vec<RepairIssue> {
    let mut issues = Vec::new();
    let Some(home) = dirs::home_dir() else {
        return issues;
    };
    let agents_dir = home.join(".claude/agents");
    let Ok(entries) = fs::read_dir(&agents_dir) else {
        return issues;
    };
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().to_string();
        if name.starts_with("loom-") || !name.ends_with(".md") {
            continue;
        }
        let prefixed = agents_dir.join(format!("loom-{}", name));
        if prefixed.exists() {
            let bare = name.trim_end_matches(".md");
            issues.push(RepairIssue {
                severity: Severity::Warning,
                description: format!(
                    "Old unprefixed agent '{}' found (superseded by 'loom-{}')",
                    bare, bare
                ),
                fix_description: format!(
                    "Remove ~/.claude/agents/{} (loom-{} already installed)",
                    name, name
                ),
            });
        }
    }
    issues
}

/// Check 10: settings.json references old-style skill names.
fn check_old_skill_refs() -> Vec<RepairIssue> {
    let mut issues = Vec::new();
    let Some(home) = dirs::home_dir() else {
        return issues;
    };
    let settings_path = home.join(".claude/settings.json");
    let Ok(content) = fs::read_to_string(&settings_path) else {
        return issues;
    };
    let has_old_refs = LOOM_SKILL_NAMES
        .iter()
        .any(|name| content.contains(&format!("Skill({}", name)));
    if has_old_refs {
        issues.push(RepairIssue {
            severity: Severity::Info,
            description: "Settings.json references old-style skill names".to_string(),
            fix_description: "Update skill references from 'name' to 'loom-name' in settings"
                .to_string(),
        });
    }
    issues
}

/// Remove an old unprefixed skill directory (loom- version already installed).
pub(super) fn fix_old_skill(description: &str) -> Result<()> {
    let name = description
        .strip_prefix("Old unprefixed skill '")
        .and_then(|s| s.split('\'').next())
        .with_context(|| format!("Cannot parse skill name from: {}", description))?;

    let home = dirs::home_dir().context("Cannot determine home directory")?;
    let old_path = home.join(".claude/skills").join(name);
    fs::remove_dir_all(&old_path)
        .with_context(|| format!("Failed to remove {}", old_path.display()))?;
    Ok(())
}

/// Remove an old unprefixed agent file (loom- version already installed).
pub(super) fn fix_old_agent(description: &str) -> Result<()> {
    let name = description
        .strip_prefix("Old unprefixed agent '")
        .and_then(|s| s.split('\'').next())
        .with_context(|| format!("Cannot parse agent name from: {}", description))?;

    let home = dirs::home_dir().context("Cannot determine home directory")?;
    let old_path = home.join(".claude/agents").join(format!("{}.md", name));
    fs::remove_file(&old_path)
        .with_context(|| format!("Failed to remove {}", old_path.display()))?;
    Ok(())
}

/// Update old-style skill references in the global settings.json.
///
/// Replaces `Skill({name}` with `Skill(loom-{name}` for each loom-specific
/// skill that does not already have the `loom-` prefix.
pub(super) fn fix_settings_skill_refs() -> Result<()> {
    let home = dirs::home_dir().context("Cannot determine home directory")?;
    let settings_path = home.join(".claude/settings.json");
    let mut content = fs::read_to_string(&settings_path)
        .with_context(|| format!("Failed to read {}", settings_path.display()))?;

    for name in LOOM_SKILL_NAMES {
        let old_ref = format!("Skill({}", name);
        let new_ref = format!("Skill(loom-{}", name);
        content = content.replace(&old_ref, &new_ref);
    }

    fs::write(&settings_path, &content)
        .with_context(|| format!("Failed to write {}", settings_path.display()))?;
    Ok(())
}
