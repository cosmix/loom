//! Effective gate inputs beyond the persisted stage definition.
use crate::models::stage::Stage;
use anyhow::Result;
use serde::Serialize;
use std::path::Path;

#[derive(Serialize)]
struct EffectivePolicy {
    version: u32,
    stage: String,
    config: Option<String>,
    source_plan: Option<String>,
    frozen_contracts: Option<String>,
    amendment_revision: u32,
}

pub fn effective_policy_hash(stage: &Stage, work_dir: &Path) -> Result<String> {
    let source = crate::fs::resolve_source_path(work_dir)?;
    let policy = EffectivePolicy {
        version: 1,
        stage: super::identity::check_definition_hash(stage),
        config: digest_file(&work_dir.join("config.toml"))?,
        source_plan: source.as_deref().map(digest_file).transpose()?.flatten(),
        frozen_contracts: digest_file(
            &work_dir
                .join("contracts")
                .join(&stage.id)
                .join("freeze.json"),
        )?,
        amendment_revision: crate::plan::amendment::count_amendments_for_stage(
            work_dir, &stage.id,
        )?,
    };
    // An absent plan/config retains the stage-only policy identity for standalone stages.
    if policy.config.is_none()
        && policy.source_plan.is_none()
        && policy.frozen_contracts.is_none()
        && policy.amendment_revision == 0
    {
        return Ok(policy.stage);
    }
    Ok(crate::utils::sha256_hex(&serde_json::to_vec(&policy)?))
}

fn digest_file(path: &Path) -> Result<Option<String>> {
    match std::fs::read(path) {
        Ok(bytes) => Ok(Some(crate::utils::sha256_hex(&bytes))),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error.into()),
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn file_digest_is_pinned() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("policy");
        std::fs::write(&path, b"abc").unwrap();
        assert_eq!(
            super::digest_file(&path).unwrap().unwrap(),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }
}
