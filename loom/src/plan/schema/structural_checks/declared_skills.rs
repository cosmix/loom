//! Validates the plan-level `skills:` field a stage may declare, per
//! `StageDefinition::skills` — the names a plan author is asserting the
//! stage's agents need, resolved the way the orchestrator resolves them by
//! default at runtime (`Orchestrator::load_skill_index`): every construction
//! site leaves `OrchestratorConfig.skills_dir` as `None`, so at runtime that
//! resolves to `~/.claude/skills` plus its sibling catalog, same as here.

use std::collections::HashSet;
use std::path::PathBuf;

use crate::skills::SkillIndex;

use super::super::types::StageDefinition;

/// Result of validating every stage's declared `skills:` list.
///
/// `errors` are `(stage_id, message)` pairs — `validate_structural_preflight`
/// routes these to `validate()`'s hard-error `ValidationError` channel, same
/// as an invalid acceptance criterion. `warnings` are plain messages folded
/// into the `structural` bucket, same severity as the plan's other advisory
/// checks.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct DeclaredSkillsReport {
    pub errors: Vec<(String, String)>,
    pub warnings: Vec<String>,
}

/// Validate every stage's `skills:` declarations.
///
/// Skipped entirely (no index load attempted) when no stage declares any
/// skill — the common case, and the reason this never costs a filesystem
/// walk for a plan that doesn't use the field.
pub fn check_declared_skills(stages: &[StageDefinition]) -> DeclaredSkillsReport {
    check_declared_skills_with_index(stages, load_skill_index().as_ref())
}

/// Same check, against an explicit index — the seam that keeps this
/// hermetic in tests and independent of the loader above.
fn check_declared_skills_with_index(
    stages: &[StageDefinition],
    index: Option<&SkillIndex>,
) -> DeclaredSkillsReport {
    let mut report = DeclaredSkillsReport::default();
    let declaring_stages: Vec<&StageDefinition> = stages
        .iter()
        .filter(|stage| !stage.skills.is_empty())
        .collect();
    if declaring_stages.is_empty() {
        return report;
    }

    for stage in &declaring_stages {
        check_empty_and_duplicate_names(stage, &mut report);
    }

    match index {
        Some(index) => {
            for stage in &declaring_stages {
                check_unknown_names(stage, index, &mut report);
            }
        }
        None => {
            let stage_ids: Vec<&str> = declaring_stages.iter().map(|s| s.id.as_str()).collect();
            report.warnings.push(format!(
                "No skill index could be loaded (looked under ~/.claude/skills) — declared \
                 skill names could not be validated for stage(s): {}",
                stage_ids.join(", ")
            ));
        }
    }

    report
}

fn check_empty_and_duplicate_names(stage: &StageDefinition, report: &mut DeclaredSkillsReport) {
    let mut seen = HashSet::new();
    for name in &stage.skills {
        if name.trim().is_empty() {
            report
                .errors
                .push((stage.id.clone(), "declares an empty skill name".to_string()));
            continue;
        }
        if !seen.insert(name.as_str()) {
            report.errors.push((
                stage.id.clone(),
                format!("declares skill '{name}' more than once"),
            ));
        }
    }
}

fn check_unknown_names(
    stage: &StageDefinition,
    index: &SkillIndex,
    report: &mut DeclaredSkillsReport,
) {
    for name in &stage.skills {
        if name.trim().is_empty() {
            continue;
        }
        if index.get_by_name(name).is_none() {
            report.errors.push((
                stage.id.clone(),
                format!("declares unknown skill '{name}': not found in the skill index"),
            ));
        }
    }
}

/// Resolve the skill index the way the orchestrator resolves it by default
/// at runtime (`Orchestrator::load_skill_index`): `OrchestratorConfig.skills_dir`
/// is `None` at every construction site, so runtime reads `~/.claude/skills`
/// plus its sibling catalog, nothing else.
fn load_skill_index() -> Option<SkillIndex> {
    let home_dir: PathBuf = dirs::home_dir()?.join(".claude").join("skills");
    if !home_dir.exists() {
        return None;
    }
    crate::skills::load_with_catalog(&home_dir)
        .ok()
        .filter(|index| !index.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use std::path::Path;
    use tempfile::TempDir;

    fn stage_with_skills(id: &str, skills: Vec<&str>) -> StageDefinition {
        let mut stage = crate::plan::schema::tests::make_stage(id, id);
        stage.skills = skills.into_iter().map(str::to_string).collect();
        stage
    }

    fn write_skill(dir: &Path, name: &str) {
        let skill_dir = dir.join(name);
        std::fs::create_dir_all(&skill_dir).unwrap();
        let mut f = std::fs::File::create(skill_dir.join("SKILL.md")).unwrap();
        writeln!(f, "---").unwrap();
        writeln!(f, "name: {name}").unwrap();
        writeln!(f, "description: test skill").unwrap();
        writeln!(f, "---").unwrap();
    }

    /// A fixture index built entirely under a TempDir — never touches the
    /// real home directory, unlike `load_skill_index()`.
    fn index_with_skills(names: &[&str]) -> SkillIndex {
        let temp = TempDir::new().unwrap();
        let skills_dir = temp.path().join("skills");
        std::fs::create_dir_all(&skills_dir).unwrap();
        for name in names {
            write_skill(&skills_dir, name);
        }
        crate::skills::load_from_roots(&skills_dir, &temp.path().join("no-such-catalog")).unwrap()
    }

    #[test]
    fn no_declarations_is_a_silent_no_op() {
        let stages = vec![crate::plan::schema::tests::make_stage("a", "a")];
        let report = check_declared_skills_with_index(&stages, None);
        assert!(report.errors.is_empty());
        assert!(report.warnings.is_empty());
    }

    #[test]
    fn known_skill_is_silent() {
        let index = index_with_skills(&["loom-rust"]);
        let stages = vec![stage_with_skills("a", vec!["loom-rust"])];
        let report = check_declared_skills_with_index(&stages, Some(&index));
        assert!(report.errors.is_empty(), "{:?}", report.errors);
        assert!(report.warnings.is_empty());
    }

    #[test]
    fn unknown_skill_with_loaded_index_errors() {
        let index = index_with_skills(&["loom-rust"]);
        let stages = vec![stage_with_skills("a", vec!["loom-does-not-exist"])];
        let report = check_declared_skills_with_index(&stages, Some(&index));
        assert_eq!(report.errors.len(), 1);
        assert_eq!(report.errors[0].0, "a");
        assert!(report.errors[0].1.contains("loom-does-not-exist"));
    }

    #[test]
    fn empty_skill_name_errors() {
        let stages = vec![stage_with_skills("a", vec!["  "])];
        let report = check_declared_skills_with_index(&stages, None);
        assert_eq!(report.errors.len(), 1);
        assert!(report.errors[0].1.contains("empty"));
    }

    #[test]
    fn duplicate_skill_name_errors() {
        let index = index_with_skills(&["loom-rust"]);
        let stages = vec![stage_with_skills("a", vec!["loom-rust", "loom-rust"])];
        let report = check_declared_skills_with_index(&stages, Some(&index));
        assert_eq!(report.errors.len(), 1);
        assert!(report.errors[0].1.contains("more than once"));
    }

    #[test]
    fn unresolvable_index_warns_once_naming_the_stage() {
        let stages = vec![stage_with_skills("a", vec!["loom-rust"])];
        let report = check_declared_skills_with_index(&stages, None);
        assert!(report.errors.is_empty());
        assert_eq!(report.warnings.len(), 1);
        assert!(report.warnings[0].contains('a'));
    }
}
