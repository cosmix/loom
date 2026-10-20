//! Plan schema types introduced with plan `version: 2`, and the
//! `reasoning_effort` deserializer. Its items are re-exported through
//! `crate::plan::schema` (`ProvisionEntry` directly from `mod.rs`), so importers
//! name them there.

use serde::{Deserialize, Deserializer, Serialize};

use crate::claude::CLAUDE_MODELS;

use super::types::ALLOWED_REASONING_EFFORTS;

/// A command the daemon runs on the host, in `<worktree>/<working_dir>`, each time a
/// stage leaves the queue for a session (plan `version: 2`). It must be idempotent and
/// may write only files git ignores.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProvisionEntry {
    /// Directory to run in, relative to the repository root; no `..`.
    pub working_dir: String,
    /// Shell command, run with `sh -c`.
    pub command: String,
}

/// A behavioural contract: one named test the stage's contract phase writes
/// before implementation, and that must fail on a plausible wrong implementation.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ContractSpec {
    /// `^[a-z0-9][a-z0-9-]*$`, unique within the stage.
    pub id: String,
    /// Test file, relative to `working_dir`, no `..`.
    pub file: String,
    /// Exact test name the runner adapter selects.
    pub test: String,
    /// Runner adapter name; `None` means detected from the project.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub runner: Option<String>,
    /// Directory the contract runs from, relative to the worktree root; `None`
    /// means detected (nearest runner config, else `working_dir`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cwd: Option<String>,
    /// What the test sets up.
    pub scenario: String,
    /// The plausible wrong implementation the test must fail on.
    pub rejects: String,
}

/// A reachability requirement: a new unit must be reached from an entry point
/// in the code graph.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReachableCheck {
    /// The new unit that must be reached.
    pub symbol: String,
    /// The entry point it must be reached from.
    pub from: String,
    /// Minimum path confidence, `0.0..=1.0`; `None` means 0.0 (as `loom map --impact`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub min_confidence: Option<f32>,
    pub description: String,
}

/// Serde deserializer for `StageDefinition::reasoning_effort`.
///
/// Accepts the allowed set verbatim; rejects anything else (including values
/// containing whitespace, semicolons, shell metacharacters). Returns
/// `Ok(None)` when the field is omitted.
pub(super) fn deserialize_reasoning_effort<'de, D>(
    deserializer: D,
) -> Result<Option<String>, D::Error>
where
    D: Deserializer<'de>,
{
    use serde::de::Error as _;
    let opt = <Option<String>>::deserialize(deserializer)?;
    match opt {
        None => Ok(None),
        Some(s) => {
            if ALLOWED_REASONING_EFFORTS.contains(&s.as_str()) {
                Ok(Some(s))
            } else {
                Err(D::Error::custom(format!(
                    "invalid reasoning_effort '{s}'. Allowed values: {}",
                    ALLOWED_REASONING_EFFORTS.join(", ")
                )))
            }
        }
    }
}

/// Serde deserializer for `StageDefinition::model`.
///
/// The stage model must be one of the aliases the `[models]` config accepts,
/// so a typo fails when the plan is parsed.
pub(super) fn deserialize_stage_model<'de, D>(deserializer: D) -> Result<Option<String>, D::Error>
where
    D: Deserializer<'de>,
{
    use serde::de::Error as _;
    let opt = <Option<String>>::deserialize(deserializer)?;
    match opt {
        None => Ok(None),
        Some(s) => {
            if CLAUDE_MODELS.contains(&s.as_str()) {
                Ok(Some(s))
            } else {
                Err(D::Error::custom(format!(
                    "invalid model '{s}'. Allowed values: {}",
                    CLAUDE_MODELS.join(", ")
                )))
            }
        }
    }
}
