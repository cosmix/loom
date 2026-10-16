use std::path::Path;

use anyhow::{ensure, Result};
use serde::Serialize;
use sha2::{Digest, Sha256};

use crate::git::branch::{branch_name_for_stage, get_branch_head};
use crate::git::runner::run_git_checked;
use crate::models::stage::{Stage, StageType};

pub const STAGE_ENVIRONMENT_POLICY: &str = "stage-host-allowlist-v2";

/// Versioned projection shared by evidence producers and consumers. Runtime
/// status/session/timestamps are deliberately absent.
#[derive(Serialize)]
struct GatePolicy<'a> {
    version: u32,
    stage_id: &'a str,
    plan_version: u32,
    stage_type: StageType,
    acceptance: &'a [crate::models::stage::AcceptanceCriterion],
    setup: &'a [String],
    working_dir: &'a Option<String>,
    artifacts: &'a [String],
    wiring: &'a [crate::models::stage::WiringCheck],
    wiring_tests: &'a [crate::models::stage::WiringTest],
    dead_code_check: &'a Option<crate::models::stage::DeadCodeCheck>,
    before_stage: &'a [crate::models::stage::TruthCheck],
    after_stage: &'a [crate::models::stage::TruthCheck],
    contracts: &'a [crate::plan::schema::ContractSpec],
    reachable: &'a [crate::plan::schema::ReachableCheck],
    code_review: &'a Option<crate::plan::schema::CodeReviewConfig>,
    sandbox: &'a crate::models::stage::StageSandboxConfig,
    harness: &'a [String],
    ratchet_files: &'a [String],
    files: &'a [String],
    dependencies: &'a [String],
    base_branch: &'a Option<String>,
    resolved_base: &'a Option<String>,
    worktree: &'a Option<String>,
    regression_test: &'a Option<crate::models::stage::RegressionTest>,
    bug_fix: Option<bool>,
    environment_policy: &'static str,
}

pub fn check_definition_hash(stage: &Stage) -> String {
    let policy = GatePolicy {
        version: 2,
        stage_id: &stage.id,
        plan_version: stage.plan_version,
        stage_type: stage.stage_type,
        acceptance: &stage.acceptance,
        setup: &stage.setup,
        working_dir: &stage.working_dir,
        artifacts: &stage.artifacts,
        wiring: &stage.wiring,
        wiring_tests: &stage.wiring_tests,
        dead_code_check: &stage.dead_code_check,
        before_stage: &stage.before_stage,
        after_stage: &stage.after_stage,
        contracts: &stage.contracts,
        reachable: &stage.reachable,
        code_review: &stage.code_review,
        sandbox: &stage.sandbox,
        harness: &stage.harness,
        ratchet_files: &stage.ratchet_files,
        files: &stage.files,
        dependencies: &stage.dependencies,
        base_branch: &stage.base_branch,
        resolved_base: &stage.resolved_base,
        worktree: &stage.worktree,
        regression_test: &stage.regression_test,
        bug_fix: stage.bug_fix,
        environment_policy: STAGE_ENVIRONMENT_POLICY,
    };
    let mut hash = Sha256::new();
    hash_json(&mut hash, &policy);
    hex::encode(hash.finalize())
}

pub fn stage_head_commit(stage_id: &str, repo_root: &Path) -> Result<String> {
    let commit = get_branch_head(&branch_name_for_stage(stage_id), repo_root)?;
    validated_commit(commit.trim())
}

pub fn worktree_head_commit(worktree: &Path) -> Result<String> {
    let commit = run_git_checked(&["rev-parse", "HEAD"], worktree)?;
    validated_commit(commit.trim())
}

pub fn expected_stage_commit(stage: &Stage, repo_root: &Path) -> Result<String> {
    if stage.stage_type == StageType::Knowledge {
        worktree_head_commit(repo_root)
    } else {
        stage_head_commit(&stage.id, repo_root)
    }
}

fn hash_json<T: Serialize>(hash: &mut Sha256, value: &T) {
    let bytes = serde_json::to_vec(value)
        .expect("stage verification definition fields must serialize to JSON");
    hash_field(hash, &bytes);
}

fn hash_field(hash: &mut Sha256, bytes: &[u8]) {
    let length = u64::try_from(bytes.len()).expect("field length must fit in u64");
    hash.update(length.to_be_bytes());
    hash.update(bytes);
}

fn validated_commit(commit: &str) -> Result<String> {
    ensure!(
        commit.len() == 40
            && commit
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte)),
        "git commit must be 40 lowercase hexadecimal characters"
    );
    Ok(commit.to_string())
}
