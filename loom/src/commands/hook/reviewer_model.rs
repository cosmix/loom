//! `loom hook reviewer-model`: the model and effort every `loom-code-reviewer`
//! spawn runs with, for the spawn-guard shell hook.

use anyhow::Result;

use crate::fs::work_dir::{resolve_code_reviewer_model_effort, WorkDir};

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
    resolve_code_reviewer_model_effort(work_dir.as_ref().map(WorkDir::root))
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
}
