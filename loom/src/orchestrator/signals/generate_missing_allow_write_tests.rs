//! Tests for `missing_allow_write_from_merged`, split out of `generate.rs`
//! to keep that file under the size ceiling.

use super::*;
use crate::fs::work_dir::write_plan_sandbox;
use crate::models::stage::{FilesystemConfig, StageSandboxConfig, StageStatus};
use crate::plan::schema::SandboxConfig;
use tempfile::TempDir;

fn init_work(temp: &TempDir) -> PathBuf {
    let work = temp.path().join(".loom").join("work");
    std::fs::create_dir_all(&work).unwrap();
    work
}

fn stage_with_sandbox(sandbox: StageSandboxConfig) -> Stage {
    Stage {
        id: "test-stage".to_string(),
        name: "Test Stage".to_string(),
        status: StageStatus::Queued,
        stage_type: StageType::Standard,
        sandbox,
        ..Stage::default()
    }
}

#[test]
fn missing_plan_level_grant_is_reported() {
    let temp = TempDir::new().unwrap();
    let work = init_work(&temp);
    let missing_path = temp.path().join("does-not-exist").display().to_string();

    let mut plan_sandbox = SandboxConfig::default();
    plan_sandbox.filesystem.allow_write = vec![missing_path.clone()];
    write_plan_sandbox(&work, &plan_sandbox).unwrap();

    let stage = stage_with_sandbox(StageSandboxConfig::default());

    assert_eq!(
        missing_allow_write_from_merged(&work, &stage),
        vec![missing_path]
    );
}

#[test]
fn existing_plan_level_grant_is_not_reported() {
    let temp = TempDir::new().unwrap();
    let work = init_work(&temp);
    let existing_path = temp.path().display().to_string();

    let mut plan_sandbox = SandboxConfig::default();
    plan_sandbox.filesystem.allow_write = vec![existing_path];
    write_plan_sandbox(&work, &plan_sandbox).unwrap();

    let stage = stage_with_sandbox(StageSandboxConfig::default());

    assert!(missing_allow_write_from_merged(&work, &stage).is_empty());
}

#[test]
fn missing_stage_level_grant_is_reported_with_no_plan_sandbox() {
    let temp = TempDir::new().unwrap();
    let work = init_work(&temp);
    let missing_path = temp.path().join("stage-missing").display().to_string();

    let stage = stage_with_sandbox(StageSandboxConfig {
        filesystem: Some(FilesystemConfig {
            allow_write: vec![missing_path.clone()],
            ..FilesystemConfig::default()
        }),
        ..StageSandboxConfig::default()
    });

    assert_eq!(
        missing_allow_write_from_merged(&work, &stage),
        vec![missing_path]
    );
}
