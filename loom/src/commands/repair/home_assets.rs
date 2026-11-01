//! Old unprefixed skills and agents in the installed Claude root
//! (`assets::install::default_paths`), and skill-name references in
//! `$HOME/.claude/settings.json`, left behind by a pre-`loom-` install.

use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

use super::{DetectedIssue, RepairAction, Severity};

mod ownership;

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
pub(super) fn check() -> Vec<DetectedIssue> {
    let mut issues = Vec::new();
    issues.extend(check_old_skills());
    issues.extend(check_old_agents());
    issues.extend(check_old_skill_refs());
    issues
}

/// The installed Claude root's `subdir`, where `install-assets` placed loom's
/// skills and agents.
fn installed_dir(subdir: &str) -> Result<PathBuf> {
    Ok(crate::assets::install::default_paths()?
        .claude_dir
        .join(subdir))
}

/// Check 7: old unprefixed skills that have a loom- counterpart.
fn check_old_skills() -> Vec<DetectedIssue> {
    let Ok(skills_dir) = installed_dir("skills") else {
        return Vec::new();
    };
    check_old_skills_at(&skills_dir)
}

pub(super) fn check_old_skills_at(skills_dir: &std::path::Path) -> Vec<DetectedIssue> {
    let mut issues = Vec::new();
    let Ok(entries) = fs::read_dir(skills_dir) else {
        return issues;
    };
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().to_string();
        if name.starts_with("loom-") || !entry.path().is_dir() {
            continue;
        }
        let prefixed = skills_dir.join(format!("loom-{}", name));
        if prefixed.is_dir() {
            let owned = ownership::skill(&entry.path(), &name);
            issues.push(DetectedIssue {
                action: if owned {
                    RepairAction::OldSkill(entry.path())
                } else {
                    RepairAction::Manual
                },
                severity: Severity::Warning,
                description: if owned {
                    format!("Shipped unprefixed skill '{}' found", name)
                } else {
                    format!("Ambiguous unprefixed skill '{}' found", name)
                },
                fix_description: if owned {
                    format!("Back up {} before migration", entry.path().display())
                } else {
                    "Preserve this skill; its content does not prove Loom ownership".into()
                },
            });
        }
    }
    issues
}

/// Check 8: old unprefixed agents that have a loom- counterpart.
fn check_old_agents() -> Vec<DetectedIssue> {
    let Ok(agents_dir) = installed_dir("agents") else {
        return Vec::new();
    };
    check_old_agents_at(&agents_dir)
}

pub(super) fn check_old_agents_at(agents_dir: &std::path::Path) -> Vec<DetectedIssue> {
    let mut issues = Vec::new();
    let Ok(entries) = fs::read_dir(agents_dir) else {
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
            let owned = ownership::agent(&entry.path(), bare);
            issues.push(DetectedIssue {
                action: if owned {
                    RepairAction::OldAgent(entry.path())
                } else {
                    RepairAction::Manual
                },
                severity: Severity::Warning,
                description: if owned {
                    format!("Shipped unprefixed agent '{}' found", bare)
                } else {
                    format!("Ambiguous unprefixed agent '{}' found", bare)
                },
                fix_description: if owned {
                    format!("Back up {} before migration", entry.path().display())
                } else {
                    "Preserve this agent; its content does not prove Loom ownership".into()
                },
            });
        }
    }
    issues
}

/// Check 10: settings.json references old-style skill names.
fn check_old_skill_refs() -> Vec<DetectedIssue> {
    let Some(home) = dirs::home_dir() else {
        return Vec::new();
    };
    check_old_skill_refs_at(&home.join(".claude/settings.json"))
}

pub(super) fn check_old_skill_refs_at(settings_path: &std::path::Path) -> Vec<DetectedIssue> {
    let mut issues = Vec::new();
    let Ok(content) = fs::read_to_string(settings_path) else {
        return issues;
    };
    let Ok(mut value) = serde_json::from_str::<serde_json::Value>(&content) else {
        return issues;
    };
    let has_old_refs = migrate_skill_refs(&mut value);
    if has_old_refs {
        issues.push(DetectedIssue {
            action: RepairAction::SkillRefs,
            severity: Severity::Info,
            description: "Settings.json references old-style skill names".to_string(),
            fix_description: "Update skill references from 'name' to 'loom-name' in settings"
                .to_string(),
        });
    }
    issues
}

/// Move a demonstrably shipped old skill to a unique, recoverable backup.
pub(super) fn fix_old_skill(path: &Path) -> Result<()> {
    let name = path
        .file_name()
        .and_then(|name| name.to_str())
        .context("Invalid skill path")?;
    anyhow::ensure!(
        ownership::skill(path, name),
        "{} is not a shipped Loom skill",
        path.display()
    );
    back_up_legacy_asset(path)
}

/// Move a demonstrably shipped old agent to a unique, recoverable backup.
pub(super) fn fix_old_agent(path: &Path) -> Result<()> {
    let name = path
        .file_stem()
        .and_then(|name| name.to_str())
        .context("Invalid agent path")?;
    anyhow::ensure!(
        ownership::agent(path, name),
        "{} is not a shipped Loom agent",
        path.display()
    );
    back_up_legacy_asset(path)
}

fn back_up_legacy_asset(path: &Path) -> Result<()> {
    let name = path
        .file_name()
        .context("Invalid asset path")?
        .to_string_lossy();
    let backup = path.with_file_name(format!(
        "{name}.loom-backup-{}",
        crate::relay::new_request_id()
    ));
    anyhow::ensure!(
        fs::symlink_metadata(&backup)
            .is_err_and(|error| error.kind() == std::io::ErrorKind::NotFound),
        "Backup path {} already exists or cannot be inspected",
        backup.display()
    );
    fs::rename(path, &backup).with_context(|| {
        format!(
            "Failed to back up {} to {}",
            path.display(),
            backup.display()
        )
    })
}

/// Update old-style skill references in the global settings.json.
///
pub(super) fn fix_settings_skill_refs() -> Result<()> {
    let home = dirs::home_dir().context("Cannot determine home directory")?;
    let settings_path = home.join(".claude/settings.json");
    fix_settings_skill_refs_at(&settings_path)
}

pub(super) fn fix_settings_skill_refs_at(settings_path: &Path) -> Result<()> {
    let content = fs::read_to_string(settings_path)
        .with_context(|| format!("Failed to read {}", settings_path.display()))?;
    let mut value: serde_json::Value = serde_json::from_str(&content)
        .with_context(|| format!("Failed to parse {}", settings_path.display()))?;
    if migrate_skill_refs(&mut value) {
        fs::write(settings_path, serde_json::to_vec_pretty(&value)?)
            .with_context(|| format!("Failed to write {}", settings_path.display()))?;
    }
    Ok(())
}

fn migrate_skill_refs(value: &mut serde_json::Value) -> bool {
    let Some(permissions) = value.get_mut("permissions") else {
        return false;
    };
    let mut changed = false;
    for field in ["allow", "ask", "deny"] {
        let Some(rules) = permissions
            .get_mut(field)
            .and_then(serde_json::Value::as_array_mut)
        else {
            continue;
        };
        for rule in rules {
            let Some(text) = rule.as_str() else { continue };
            let Some(inner) = text
                .strip_prefix("Skill(")
                .and_then(|rest| rest.strip_suffix(')'))
            else {
                continue;
            };
            let (name, argument) = inner.find(':').map_or((inner, ""), |at| inner.split_at(at));
            if LOOM_SKILL_NAMES.contains(&name) {
                *rule = serde_json::Value::String(format!("Skill(loom-{name}{argument})"));
                changed = true;
            }
        }
    }
    changed
}
