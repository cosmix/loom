//! `loom hook reviewer-model`: the model and effort every `loom-code-reviewer`
//! spawn runs with, for the spawn-guard shell hook.

use anyhow::Result;

use crate::fs::work_dir::config_sections::resolve_code_reviewer_followup_model_effort;
use crate::fs::work_dir::{resolve_code_reviewer_model_effort, WorkDir};
use crate::verify::review::store;
use std::path::Path;

use super::target::non_empty_env;

/// Print `model=<model> effort=<effort>`. The work dir comes from
/// `LOOM_WORK_DIR` when set, else the current directory (a worktree's
/// `.loom/work` symlink resolves to the main workspace); with no work dir the
/// user tier and the built-in apply. A config that fails to parse is an error.
pub fn reviewer_model() -> Result<()> {
    println!("{}", render(&resolve()?));
    Ok(())
}

fn resolve() -> Result<(String, String)> {
    let hint = non_empty_env("LOOM_WORK_DIR").unwrap_or_else(|| ".".to_string());
    let work_dir = WorkDir::new(hint).ok();
    resolve_for(
        work_dir.as_ref().map(WorkDir::root),
        non_empty_env("LOOM_STAGE_ID").as_deref(),
    )
}

fn resolve_for(work_dir: Option<&Path>, stage: Option<&str>) -> Result<(String, String)> {
    let followup = work_dir.zip(stage).is_some_and(|(dir, stage)| {
        store::load_rounds(dir, stage)
            .map(|rounds| rounds.iter().any(|round| round.is_well_formed()))
            .unwrap_or(false)
    });
    if followup {
        resolve_code_reviewer_followup_model_effort(work_dir)
    } else {
        resolve_code_reviewer_model_effort(work_dir)
    }
}

fn render((model, effort): &(String, String)) -> String {
    format!("model={model} effort={effort}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn renders_one_model_effort_line() {
        assert_eq!(
            render(&("opus".to_string(), "high".to_string())),
            "model=opus effort=high"
        );
    }

    fn record(work: &Path, number: u32, malformed: Option<&str>) {
        let round = store::ReviewRound {
            version: store::RECORD_VERSION,
            round: number,
            agent_id: format!("reviewer-{number}"),
            harvested_at: chrono::Utc::now(),
            fingerprint: "sha256:test".into(),
            base: "base".into(),
            behaviour: Default::default(),
            files: Default::default(),
            malformed: malformed.map(str::to_string),
            findings: vec![],
            resolved: vec![],
            unresolved: vec![],
            suggestion_memory_ids: vec![],
        };
        store::write_round(work, "s1", &round).unwrap();
    }

    #[test]
    fn only_a_well_formed_round_selects_the_followup_model() {
        let temp = tempfile::tempdir().unwrap();
        let work = temp.path();
        let user = work.join("user.toml");
        std::fs::write(&user, "[models]\ncode_reviewer_model = \"opus\"\n").unwrap();
        let _redirect = crate::user_config::redirect_user_config(user);
        assert_eq!(resolve_for(Some(work), Some("s1")).unwrap().0, "opus");
        record(work, 1, Some("missing block"));
        assert_eq!(resolve_for(Some(work), Some("s1")).unwrap().0, "opus");
        record(work, 2, None);
        assert_eq!(resolve_for(Some(work), Some("s1")).unwrap().0, "sonnet");
        std::fs::write(
            work.join("config.toml"),
            "[models]\ncode_reviewer_followup_model = \"haiku\"\ncode_reviewer_effort = \"low\"\n",
        )
        .unwrap();
        assert_eq!(
            resolve_for(Some(work), Some("s1")).unwrap(),
            ("haiku".into(), "low".into())
        );
        assert_eq!(resolve_for(Some(work), None).unwrap().0, "opus");
        assert_eq!(resolve_for(None, Some("s1")).unwrap().0, "opus");
        std::fs::write(work.join("reviews/s1/round-2.json"), "invalid").unwrap();
        assert_eq!(resolve_for(Some(work), Some("s1")).unwrap().0, "opus");
    }
}
