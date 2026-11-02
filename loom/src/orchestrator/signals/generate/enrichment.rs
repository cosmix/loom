use crate::models::stage::{Stage, StageType};
use crate::orchestrator::signals::retrieval::DependencyChange;
use std::collections::BTreeMap;

/// True when `stage` is an enrichment target: integration-verify or
/// knowledge-distill with at least one dependency to summarize.
pub(super) fn wants_stage_enrichment(stage: &Stage) -> bool {
    matches!(
        stage.stage_type,
        StageType::IntegrationVerify | StageType::KnowledgeDistill
    ) && !stage.dependencies.is_empty()
}

/// Build a cross-stage change summary: aggregates each dependency's file
/// assignments and metadata into a bird's-eye view for integration-verify agents.
pub(super) fn build_cross_stage_summary(
    stage: &Stage,
    changes: &[DependencyChange],
) -> Option<String> {
    if !wants_stage_enrichment(stage) {
        return None;
    }

    let mut summary = String::from("## Cross-Stage Changes\n\n");
    let mut has_content = false;
    let mut all_files: BTreeMap<String, Vec<String>> = BTreeMap::new();

    for change in changes {
        let dep_stage = &change.stage;
        let dep_id = &dep_stage.id;
        has_content = true;
        summary.push_str(&format!(
            "### Stage: {} ({})\n",
            dep_stage.name,
            format_stage_status(&dep_stage.status)
        ));
        summary.push_str(&format!("Branch: loom/{dep_id}\n"));
        if !change.paths.is_empty() {
            summary.push_str(if change.range.is_some() {
                "Changed paths:\n"
            } else {
                "Declared paths:\n"
            });
            for file in &change.paths {
                summary.push_str(&format!("- {file}\n"));
                all_files
                    .entry(file.clone())
                    .or_default()
                    .push(dep_id.clone());
            }
        }
        summary.push('\n');
    }

    if !has_content {
        return None;
    }

    append_file_concerns(&mut summary, &all_files);

    Some(summary)
}

/// Format a stage status for display
fn format_stage_status(status: &crate::models::stage::StageStatus) -> &'static str {
    use crate::models::stage::StageStatus;
    match status {
        StageStatus::Completed => "completed",
        StageStatus::Executing => "executing",
        StageStatus::Queued => "queued",
        StageStatus::WaitingForDeps => "waiting",
        StageStatus::Blocked => "blocked",
        StageStatus::NeedsHandoff => "needs-handoff",
        StageStatus::WaitingForInput => "waiting-for-input",
        StageStatus::MergeConflict => "merge-conflict",
        StageStatus::Skipped => "skipped",
        StageStatus::CompletedWithFailures => "completed-with-failures",
        StageStatus::MergeBlocked => "merge-blocked",
        StageStatus::NeedsHumanReview => "needs-human-review",
        StageStatus::NeedsAdjudication => "needs-adjudication",
    }
}

fn append_file_concerns(summary: &mut String, all_files: &BTreeMap<String, Vec<String>>) {
    // Identify files touched by multiple stages
    let multi_stage_files: Vec<_> = all_files
        .iter()
        .filter(|(_, stages)| stages.len() > 1)
        .collect();

    if !multi_stage_files.is_empty() {
        summary.push_str("### Potential Concerns\n");
        for (file, stages) in &multi_stage_files {
            summary.push_str(&format!(
                "- `{}` changed or assigned by {} stages — verify no conflicts\n",
                file,
                stages.len()
            ));
        }
        summary.push('\n');
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reports_changed_and_declared_paths_without_claiming_additions() {
        let stage = Stage {
            stage_type: StageType::IntegrationVerify,
            dependencies: vec!["a".into(), "b".into()],
            ..Stage::default()
        };
        let change = |id: &str, paths: &[&str], observed: bool| DependencyChange {
            stage: Stage {
                id: id.into(),
                name: id.into(),
                ..Stage::default()
            },
            range: observed.then(|| ("base".into(), "head".into())),
            paths: paths.iter().map(|path| (*path).into()).collect(),
        };
        let summary = build_cross_stage_summary(
            &stage,
            &[
                change("a", &["shared.rs", "pkg-a/src/lib.rs"], true),
                change("b", &["shared.rs", "pkg-b/src/lib.rs"], false),
            ],
        )
        .unwrap();
        assert!(summary.contains("Changed paths:"));
        assert!(summary.contains("Declared paths:"));
        assert!(summary.contains("`shared.rs` changed or assigned by 2 stages"));
        assert!(!summary.contains("`pkg-a/src/lib.rs` changed or assigned"));
        assert!(!summary.contains("new file"));
    }

    #[test]
    fn fallback_paths_respect_working_directories_and_normalization() {
        let temp = tempfile::TempDir::new().unwrap();
        let work = temp.path().join(".loom/work");
        std::fs::create_dir_all(work.join("stages")).unwrap();
        for (id, dir, files) in [
            ("a", "pkg-a", vec!["src/lib.rs", "../shared/./lib.rs"]),
            ("b", "pkg-b", vec!["src/lib.rs", "../shared/lib.rs"]),
        ] {
            let dependency = Stage {
                id: id.into(),
                name: id.into(),
                working_dir: Some(dir.into()),
                files: files.into_iter().map(str::to_string).collect(),
                ..Stage::default()
            };
            crate::verify::transitions::save_stage(&dependency, &work).unwrap();
        }
        let stage = Stage {
            stage_type: StageType::IntegrationVerify,
            dependencies: vec!["a".into(), "b".into()],
            ..Stage::default()
        };
        let changes = crate::orchestrator::signals::retrieval::dependency_changes(&work, &stage);
        let summary = build_cross_stage_summary(&stage, &changes).unwrap();
        assert!(summary.contains("`shared/lib.rs` changed or assigned by 2 stages"));
        assert!(!summary.contains("`pkg-a/src/lib.rs` changed or assigned"));
        assert!(!summary.contains("`pkg-b/src/lib.rs` changed or assigned"));
    }
}
