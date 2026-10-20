//! Plan-authored check types a stage carries (shared by the plan schema and
//! the runtime `Stage`), and the plan identity `Stage::from_definition` stamps
//! onto every stage it builds.

use serde::{Deserialize, Serialize};
use std::time::Duration;

use crate::plan::parser::ParsedPlan;

/// The plan a stage is built from: its id, its `loom.version`, and the
/// plan-level `ratchet_files` copied onto every stage.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PlanIdentity<'a> {
    pub id: &'a str,
    pub version: u32,
    pub ratchet_files: &'a [String],
}

impl<'a> From<&'a ParsedPlan> for PlanIdentity<'a> {
    fn from(plan: &'a ParsedPlan) -> Self {
        Self {
            id: &plan.id,
            version: plan.metadata.loom.version,
            ratchet_files: &plan.metadata.loom.ratchet_files,
        }
    }
}

/// Plan version a persisted `Stage` without `plan_version` was built from.
pub(super) fn default_plan_version() -> u32 {
    1
}

/// Wiring check to verify component connections.
///
/// Used in goal-backward verification to ensure critical connections
/// between components are in place.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WiringCheck {
    /// Source file path (relative to working_dir)
    pub source: String,
    /// What to check for (grep pattern)
    pub pattern: String,
    /// Human-readable description of what this verifies
    pub description: String,
    /// Match `pattern` as a literal string rather than a regex.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub literal: bool,
}

/// Enhanced truth check with extended success criteria beyond exit code.
///
/// All extended fields are optional for backward compatibility.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TruthCheck {
    /// Shell command to execute
    pub command: String,
    /// Strings that must appear in stdout
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub stdout_contains: Vec<String>,
    /// Strings that must NOT appear in stdout
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub stdout_not_contains: Vec<String>,
    /// Whether stderr must be empty (default: false, meaning stderr is ignored)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stderr_empty: Option<bool>,
    /// Expected exit code (default: 0)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub exit_code: Option<i32>,
    /// Seconds the command may run before it is killed and fails (default:
    /// the runner's timeout for this kind of check)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timeout_secs: Option<u64>,
    /// Human-readable description of what this truth verifies
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

impl TruthCheck {
    /// How long the command may run: `timeout_secs` when set, else `default`.
    pub fn timeout(&self, default: Duration) -> Duration {
        self.timeout_secs.map_or(default, Duration::from_secs)
    }
}

/// Unified acceptance criterion - either a simple shell command or an extended check.
///
/// In YAML, simple criteria are plain strings, extended criteria are objects:
/// ```yaml
/// acceptance:
///   - "cargo test"                           # Simple
///   - command: "loom --help"                  # Extended
///     stdout_contains: ["Usage:"]
///     exit_code: 0
/// ```
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum AcceptanceCriterion {
    /// Simple shell command - succeeds if exit code is 0
    Simple(String),
    /// Extended check with output validation (reuses TruthCheck structure)
    Extended(TruthCheck),
}

impl AcceptanceCriterion {
    /// Get the shell command string for this criterion
    pub fn command(&self) -> &str {
        match self {
            AcceptanceCriterion::Simple(cmd) => cmd,
            AcceptanceCriterion::Extended(check) => &check.command,
        }
    }

    /// Whether this is an extended criterion with output validation
    pub fn is_extended(&self) -> bool {
        matches!(self, AcceptanceCriterion::Extended(_))
    }
}

impl std::fmt::Display for AcceptanceCriterion {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.command())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn timeout_secs_round_trips_through_yaml_and_overrides_the_default() {
        let criterion: AcceptanceCriterion =
            crate::yaml::from_str("command: cargo test\ntimeout_secs: 900\n").unwrap();
        let AcceptanceCriterion::Extended(check) = &criterion else {
            panic!("an object criterion is extended: {criterion:?}");
        };
        assert_eq!(check.timeout_secs, Some(900));
        assert_eq!(
            check.timeout(Duration::from_secs(30)),
            Duration::from_secs(900)
        );
        let yaml = crate::yaml::to_string(&criterion).unwrap();
        assert!(yaml.contains("timeout_secs: 900"), "{yaml}");
        let reparsed: AcceptanceCriterion = crate::yaml::from_str(&yaml).unwrap();
        assert_eq!(reparsed, criterion);
    }

    #[test]
    fn an_unset_timeout_is_the_default_and_is_not_serialized() {
        let criterion: AcceptanceCriterion =
            crate::yaml::from_str("command: cargo test\n").unwrap();
        let AcceptanceCriterion::Extended(check) = &criterion else {
            panic!("an object criterion is extended: {criterion:?}");
        };
        assert_eq!(
            check.timeout(Duration::from_secs(30)),
            Duration::from_secs(30)
        );
        let yaml = crate::yaml::to_string(&criterion).unwrap();
        assert!(!yaml.contains("timeout_secs"), "{yaml}");
    }
}
