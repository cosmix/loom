//! Tests for the merged sandbox summary.

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
        build_sandbox_summary(&stage, &work).missing_allow_write,
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

    assert!(build_sandbox_summary(&stage, &work)
        .missing_allow_write
        .is_empty());
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
        build_sandbox_summary(&stage, &work).missing_allow_write,
        vec![missing_path]
    );
}

#[test]
fn summary_inherits_plan_and_respects_network_override() {
    let temp = TempDir::new().unwrap();
    let work = init_work(&temp);
    let mut plan = SandboxConfig::default();
    plan.filesystem.deny_read = vec!["secret/**".into()];
    plan.filesystem.deny_write = vec!["protected/**".into()];
    plan.filesystem.allow_write = vec!["output/**".into()];
    plan.network.allowed_domains = vec!["example.com".into()];
    plan.excluded_commands = vec!["tool".into()];
    write_plan_sandbox(&work, &plan).unwrap();
    let mut stage = stage_with_sandbox(StageSandboxConfig::default());
    let summary = build_sandbox_summary(&stage, &work);
    assert_eq!(summary.deny_read, ["secret/**"]);
    assert_eq!(summary.deny_write, ["protected/**"]);
    assert_eq!(summary.allow_write, ["output/**"]);
    assert_eq!(summary.allowed_domains, ["example.com"]);
    assert_eq!(summary.excluded_commands, ["tool"]);
    stage.sandbox.network = Some(Default::default());
    assert!(build_sandbox_summary(&stage, &work)
        .allowed_domains
        .is_empty());
}

#[test]
fn summary_matches_launcher_grants_for_a_codex_stage_and_escaping_paths() {
    use crate::codex::{CODEX_SANDBOX_DOMAINS, CODEX_SANDBOX_WRITE_PATHS};
    use crate::models::stage::{Implementer, Implementers};

    let temp = TempDir::new().unwrap();
    let work = init_work(&temp);
    let mut plan = SandboxConfig::default();
    plan.filesystem.allow_write = vec!["output/**".into(), "../../escape/**".into()];
    plan.network.allowed_domains = vec![CODEX_SANDBOX_DOMAINS[0].into()];
    write_plan_sandbox(&work, &plan).unwrap();
    let mut stage = stage_with_sandbox(StageSandboxConfig::default());

    let claude_only = build_sandbox_summary(&stage, &work);
    assert_eq!(claude_only.allow_write, ["output/**"]);
    assert_eq!(claude_only.allowed_domains, [CODEX_SANDBOX_DOMAINS[0]]);

    stage.implementers = Implementers::new(vec![Implementer::Codex]);
    let codex = build_sandbox_summary(&stage, &work);
    assert_eq!(codex.allowed_domains, CODEX_SANDBOX_DOMAINS);
    for path in CODEX_SANDBOX_WRITE_PATHS {
        assert!(codex.allow_write.iter().any(|granted| granted == path));
    }
}
