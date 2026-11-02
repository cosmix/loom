//! Metadata fixtures for dependency tests; transitions remain in each test.
#![allow(dead_code)]

use loom::models::stage::{Stage, StageStatus, StageType};
use loom::verify::transitions::save_stage;
use std::path::Path;

pub fn stage(
    id: &str,
    name: &str,
    description: Option<&str>,
    status: StageStatus,
    dependencies: &[&str],
) -> Stage {
    let mut stage = Stage::new(name.to_string(), description.map(str::to_string));
    stage.id = id.to_string();
    stage.status = status;
    for dependency in dependencies {
        stage.add_dependency((*dependency).to_string());
    }
    stage
}

/// Knowledge stages have no Git branch, so metadata tests can isolate dependency rules.
pub fn knowledge_stage(id: &str, name: &str, status: StageStatus) -> Stage {
    let mut stage = stage(id, name, Some(&format!("Test stage {name}")), status, &[]);
    stage.stage_type = StageType::Knowledge;
    stage
}

pub fn save_three_stage_chain(work_dir: &Path) {
    for (id, name, description, status, dependencies) in [
        (
            "stage-1",
            "Stage 1",
            "First stage",
            StageStatus::Queued,
            &[][..],
        ),
        (
            "stage-2",
            "Stage 2",
            "Second stage",
            StageStatus::WaitingForDeps,
            &["stage-1"][..],
        ),
        (
            "stage-3",
            "Stage 3",
            "Third stage",
            StageStatus::WaitingForDeps,
            &["stage-2"][..],
        ),
    ] {
        save_stage(
            &stage(id, name, Some(description), status, dependencies),
            work_dir,
        )
        .expect("save dependency fixture");
    }
}
