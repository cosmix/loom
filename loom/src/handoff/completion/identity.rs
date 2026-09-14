use std::path::Path;

use anyhow::{ensure, Result};
use serde::Serialize;
use sha2::{Digest, Sha256};

use crate::git::branch::{branch_name_for_stage, get_branch_head};
use crate::git::runner::run_git_checked;
use crate::models::stage::{Stage, StageType};

pub const STAGE_ENVIRONMENT_POLICY: &str = "stage-host-allowlist-v2";

pub fn check_definition_hash(stage: &Stage) -> String {
    let mut hash = Sha256::new();
    hash_field(&mut hash, b"loom-check-definition-v1");
    hash_json(&mut hash, &stage.id);
    hash_json(&mut hash, &stage.acceptance);
    hash_json(&mut hash, &stage.setup);
    hash_json(&mut hash, &stage.working_dir);
    hash_json(&mut hash, &stage.artifacts);
    hash_json(&mut hash, &stage.wiring);
    hash_json(&mut hash, &stage.wiring_tests);
    hash_json(&mut hash, &stage.dead_code_check);
    hash_json(&mut hash, &stage.before_stage);
    hash_json(&mut hash, &stage.after_stage);
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
