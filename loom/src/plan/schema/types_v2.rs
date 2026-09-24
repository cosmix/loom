//! Plan schema types introduced with plan `version: 2`, and the
//! `reasoning_effort` deserializer. Everything here is re-exported from
//! `types.rs`, so importers name it through `crate::plan::schema`.

use serde::{Deserialize, Deserializer, Serialize};

use super::types::ALLOWED_REASONING_EFFORTS;

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
