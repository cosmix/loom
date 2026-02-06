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
//! - `tests` - Remaining orchestrator tests (auto-merge, backend types, etc.)

mod defaults;
mod intervals;
mod manual_mode;
mod parallel_sessions;
mod tests;

use loom::plan::schema::StageDefinition;

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
        stage_type: loom::plan::schema::StageType::default(),
        truths: vec![],
        artifacts: vec![],
        wiring: vec![],
        truth_checks: vec![],
        wiring_tests: vec![],
        dead_code_check: None,
        context_budget: None,
        execution_mode: None,
    }
}
