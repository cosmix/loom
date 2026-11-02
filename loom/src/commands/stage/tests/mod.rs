//! Tests for stage commands

#[cfg(test)]
mod session;

#[cfg(test)]
mod state;

#[cfg(test)]
pub(super) mod complete;

#[cfg(test)]
mod complete_router;

#[cfg(test)]
mod progressive_complete;

// Shared test utilities
use crate::models::stage::{Stage, StageStatus};
use std::fs;
use std::path::{Path, PathBuf};
use tempfile::TempDir;

pub(crate) fn create_test_stage(id: &str, status: StageStatus) -> Stage {
    Stage {
        id: id.to_string(),
        name: format!("Stage {id}"),
        status,
        ..Stage::default()
    }
}

pub(crate) fn setup_work_dir() -> TempDir {
    use crate::fs::work_dir::WorkDir;

    let temp_dir = TempDir::new().unwrap();
    let work_dir = WorkDir::new(temp_dir.path()).unwrap();
    work_dir.initialize().unwrap();
    temp_dir
}

/// The shared failing-signer repository with the checkout identity a landing
/// demands.
pub(crate) fn failing_signer_repo() -> (TempDir, PathBuf, String) {
    let fixture = crate::stage_service::merge_test_fixtures::failing_signer_repo();
    fs::create_dir_all(&fixture.1).unwrap();
    crate::git::worktree::capture_main_identity(fixture.0.path(), &fixture.1).unwrap();
    fixture
}

pub(crate) fn save_test_stage(work_dir: &Path, stage: &Stage) {
    let yaml = crate::yaml::to_string(stage).unwrap();
    let content = format!("---\n{yaml}---\n\n# Stage: {}\n", stage.name);

    let stages_dir = work_dir.join("stages");
    fs::create_dir_all(&stages_dir).unwrap();

    let stage_path = stages_dir.join(format!("00-{}.md", stage.id));
    fs::write(stage_path, content).unwrap();
}
