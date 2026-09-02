//! Integration tests for daemon and orchestrator configuration
//!
//! Tests verify that configuration options are properly applied and affect
//! orchestrator behavior as expected.
//!
//! ## Modules
//!
//! - `defaults` - Tests for default and custom configuration values
//! - `intervals` - Tests for poll and status update intervals
//! - `manual_mode` - Tests for manual mode orchestrator behavior
//! - `parallel_sessions` - Tests for parallel session configuration
//! - `stale_project_execution` - Regression: stale [project_execution] in config.toml is ignored
//! - `tests` - Remaining orchestrator tests (auto-merge, watch mode, etc.)

mod defaults;
mod intervals;
mod manual_mode;
mod parallel_sessions;
mod stale_project_execution;
mod tests;

use loom::plan::schema::{Implementers, StageDefinition};
use tempfile::TempDir;

/// Restores a process env var to its previous value on drop, on EVERY exit
/// path including a panic — so overriding a process-global var (like
/// `LOOM_HOME` below) can never leak a stale value into whichever test the
/// harness runs next.
pub(crate) struct EnvVarGuard {
    key: &'static str,
    original: Option<String>,
}

impl EnvVarGuard {
    pub(crate) fn set(key: &'static str, value: &str) -> Self {
        let original = std::env::var(key).ok();
        std::env::set_var(key, value);
        Self { key, original }
    }
}

impl Drop for EnvVarGuard {
    fn drop(&mut self) {
        match &self.original {
            Some(value) => std::env::set_var(self.key, value),
            None => std::env::remove_var(self.key),
        }
    }
}

/// Keep `read_terminal_config`/`read_context_config`'s user-config fallback
/// tier off the developer's real `~/.loom/config.toml`: the lib under
/// `loom/tests/` is compiled WITHOUT `cfg(test)`, so its per-thread test
/// redirect does not apply here. Points `LOOM_HOME` at a temp directory that
/// does not exist, so `crate::user_config::UserConfig::load()` sees "no user
/// config" and every affected test must be `#[serial]` — `LOOM_HOME` is
/// process-global env state.
pub(crate) fn isolate_user_config(temp: &TempDir) -> EnvVarGuard {
    let home = temp.path().join("no-such-loom-home");
    EnvVarGuard::set("LOOM_HOME", home.to_str().expect("temp path is UTF-8"))
}

/// Create a basic stage definition for testing
pub fn create_stage_def(id: &str, name: &str, deps: Vec<String>) -> StageDefinition {
    StageDefinition {
        id: id.to_string(),
        name: name.to_string(),
        description: Some(format!("Test stage {name}")),
        dependencies: deps,
        parallel_group: None,
        acceptance: vec![],
        setup: vec![],
        files: vec![],
        auto_merge: None,
        working_dir: ".".to_string(),
        sandbox: Default::default(),
        stage_type: None,
        artifacts: vec![],
        wiring: vec![],
        wiring_tests: vec![],
        dead_code_check: None,
        before_stage: vec![],
        after_stage: vec![],
        context_ceiling_tokens: None,
        removed_context_budget: None,
        plan_overview: None,
        execution_mode: None,
        bug_fix: None,
        regression_test: None,
        model: None,
        reasoning_effort: None,
        code_review: None,
        ultracode: false,
        implementers: Implementers::default(),
        subagent_timeout_secs: None,
    }
}
