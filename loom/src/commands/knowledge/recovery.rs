//! Operator-only migration for sessions launched before baseline recording.
use anyhow::{ensure, Context, Result};
use std::path::Path;

pub fn recover_baseline(stage_id: &str, commit: &str) -> Result<()> {
    let work_dir = crate::commands::common::work_dir_path()?;
    recover(&work_dir, stage_id, commit)
}

fn recover(work_dir: &Path, stage_id: &str, commit: &str) -> Result<()> {
    ensure!(
        crate::daemon::read_user_token(work_dir).is_some_and(|token| !token.is_empty()),
        "knowledge baseline recovery requires the operator user credential"
    );
    let stage = crate::verify::transitions::load_stage(stage_id, work_dir)?;
    ensure!(
        stage.stage_type == crate::models::stage::StageType::Knowledge,
        "stage is not a knowledge stage"
    );
    let session = stage
        .session
        .as_deref()
        .with_context(|| crate::verify::completion::knowledge::no_session_recovery(stage_id))?;
    let repo = crate::fs::work_dir::WorkDir::new(work_dir)?
        .main_project_root()
        .context("completion repository is unavailable")?;
    crate::verify::completion::knowledge_baseline::recover(work_dir, &repo, session, commit)?;
    println!("Recovered knowledge baseline for stage '{stage_id}'.");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recovery_requires_operator_credential_before_loading_stage() {
        let root = tempfile::tempdir().unwrap();
        let error = recover(root.path(), "missing-stage", "HEAD").unwrap_err();
        assert!(error.to_string().contains("operator user credential"));
    }
}
