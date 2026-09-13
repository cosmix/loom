use anyhow::{ensure, Context, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
pub mod job_record;
mod load;
pub mod locator;
pub mod marker;
pub mod transcript;
pub use load::{fold_observations, load_receipts, ReceiptLoad};
pub const FORWARD_RECEIPT_SCHEMA: u16 = 1;
pub const RECEIPTS_FILE_NAME: &str = "forward-receipts.jsonl";
const FORWARD_RECEIPT_FIELDS: [&str; 16] = [
    "schema",
    "receipt_id",
    "parent_session_id",
    "agent_id",
    "tool_use_id",
    "stage_id",
    "loom_session_id",
    "backend",
    "backend_id",
    "state",
    "observed_at",
    "exit_code",
    "codex_thread_id",
    "locator",
    "model",
    "effort",
];
pub fn is_safe_id(value: &str) -> bool {
    let bytes = value.as_bytes();
    (1..=128).contains(&bytes.len())
        && bytes[0].is_ascii_alphanumeric()
        && bytes
            .iter()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-'))
}
pub fn receipts_path(work_dir: &Path, stage_id: &str) -> Result<PathBuf> {
    ensure!(is_safe_id(stage_id), "unsafe forward receipt stage id");
    Ok(work_dir
        .join("subagents")
        .join(stage_id)
        .join(RECEIPTS_FILE_NAME))
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ForwardBackend {
    Companion,
    Direct,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ForwardState {
    Queued,
    Running,
    Succeeded,
    Failed,
    Canceled,
    Unknown,
}
impl ForwardState {
    pub fn is_terminal(self) -> bool {
        matches!(self, Self::Succeeded | Self::Failed | Self::Canceled)
    }
    pub fn label(self) -> &'static str {
        match self {
            Self::Queued => "queued",
            Self::Running => "running",
            Self::Succeeded => "succeeded",
            Self::Failed => "failed",
            Self::Canceled => "canceled",
            Self::Unknown => "unknown",
        }
    }
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ForwardIdentity {
    pub parent_session_id: String,
    pub agent_id: String,
    pub tool_use_id: String,
    pub stage_id: String,
    pub loom_session_id: String,
}
impl ForwardIdentity {
    pub fn new(
        parent_session_id: impl Into<String>,
        agent_id: impl Into<String>,
        tool_use_id: impl Into<String>,
        stage_id: impl Into<String>,
        loom_session_id: impl Into<String>,
    ) -> Result<Self> {
        let identity = Self {
            parent_session_id: parent_session_id.into(),
            agent_id: agent_id.into(),
            tool_use_id: tool_use_id.into(),
            stage_id: stage_id.into(),
            loom_session_id: loom_session_id.into(),
        };
        identity.validate()?;
        Ok(identity)
    }
    pub fn receipt_id(&self) -> String {
        let canonical = format!(
            "loom.forward-receipt.v1\0{}\0{}\0{}\0{}\0{}",
            self.parent_session_id,
            self.agent_id,
            self.tool_use_id,
            self.stage_id,
            self.loom_session_id
        );
        hex::encode(Sha256::digest(canonical.as_bytes()))
    }
    fn validate(&self) -> Result<()> {
        let values = [
            &self.parent_session_id,
            &self.agent_id,
            &self.tool_use_id,
            &self.stage_id,
            &self.loom_session_id,
        ];
        ensure!(
            values.iter().all(|value| is_safe_id(value)),
            "unsafe forward identity"
        );
        Ok(())
    }
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ForwardObservation {
    pub schema: u16,
    pub receipt_id: String,
    pub parent_session_id: String,
    pub agent_id: String,
    pub tool_use_id: String,
    pub stage_id: String,
    pub loom_session_id: String,
    pub backend: ForwardBackend,
    pub backend_id: String,
    pub state: ForwardState,
    pub observed_at: DateTime<Utc>,
    pub exit_code: Option<i32>,
    pub codex_thread_id: Option<String>,
    pub locator: Option<String>,
    pub model: Option<String>,
    pub effort: Option<String>,
}
impl ForwardObservation {
    pub fn decode_line(line: &str) -> Result<Self> {
        let value: serde_json::Value =
            serde_json::from_str(line).context("invalid forward receipt JSON")?;
        ensure!(
            value.as_object().is_some_and(|fields| {
                fields.len() == FORWARD_RECEIPT_FIELDS.len()
                    && FORWARD_RECEIPT_FIELDS
                        .iter()
                        .all(|field| fields.contains_key(*field))
            }),
            "forward receipt fields are incomplete"
        );
        let timestamp = value
            .get("observed_at")
            .and_then(serde_json::Value::as_str)
            .and_then(|value| DateTime::parse_from_rfc3339(value).ok());
        ensure!(
            timestamp.is_some_and(|value| value.offset().local_minus_utc() == 0),
            "observed_at is not RFC3339 UTC"
        );
        let observation: Self =
            serde_json::from_value(value).context("invalid forward receipt JSON")?;
        observation.validate()?;
        Ok(observation)
    }
    pub fn encode_line(&self) -> Result<String> {
        self.validate()?;
        serde_json::to_string(self).context("failed to encode forward receipt")
    }
    fn identity(&self) -> Result<ForwardIdentity> {
        ForwardIdentity::new(
            self.parent_session_id.clone(),
            self.agent_id.clone(),
            self.tool_use_id.clone(),
            self.stage_id.clone(),
            self.loom_session_id.clone(),
        )
    }
    fn validate(&self) -> Result<()> {
        ensure!(
            self.schema == FORWARD_RECEIPT_SCHEMA,
            "unsupported forward receipt schema"
        );
        let identity = self.identity()?;
        ensure!(
            self.receipt_id == identity.receipt_id(),
            "forward receipt id mismatch"
        );
        ensure!(is_safe_id(&self.backend_id), "unsafe backend id");
        ensure!(
            self.codex_thread_id.as_deref().is_none_or(is_safe_id),
            "unsafe codex thread id"
        );
        ensure!(
            self.state != ForwardState::Unknown,
            "unknown is not an observation state"
        );
        ensure!(
            self.model.as_ref().is_none_or(|value| value.len() <= 64),
            "model exceeds 64 bytes"
        );
        ensure!(
            self.effort.as_ref().is_none_or(|value| value.len() <= 64),
            "effort exceeds 64 bytes"
        );
        if let Some(locator) = &self.locator {
            ensure!(
                locator.len() <= 4096 && Path::new(locator).is_absolute(),
                "unsafe locator"
            );
        }
        ensure!(
            self.state.is_terminal() || self.exit_code.is_none(),
            "nonterminal observation has exit code"
        );
        Ok(())
    }
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ForwardReceipt {
    pub identity: ForwardIdentity,
    pub receipt_id: String,
    pub backend: ForwardBackend,
    pub backend_id: String,
    pub state: ForwardState,
    pub observed_at: DateTime<Utc>,
    pub exit_code: Option<i32>,
    pub codex_thread_id: Option<String>,
    #[serde(skip)]
    pub locator: Option<String>,
    pub model: Option<String>,
    pub effort: Option<String>,
}
impl ForwardReceipt {
    fn from_first(observation: ForwardObservation) -> Self {
        let identity = ForwardIdentity {
            parent_session_id: observation.parent_session_id.clone(),
            agent_id: observation.agent_id.clone(),
            tool_use_id: observation.tool_use_id.clone(),
            stage_id: observation.stage_id.clone(),
            loom_session_id: observation.loom_session_id.clone(),
        };
        let invalid = observation.state.is_terminal() || invalid_direct(&observation);
        Self {
            identity,
            receipt_id: observation.receipt_id,
            backend: observation.backend,
            backend_id: observation.backend_id,
            state: if invalid {
                ForwardState::Unknown
            } else {
                observation.state
            },
            observed_at: observation.observed_at,
            exit_code: observation.exit_code,
            codex_thread_id: observation.codex_thread_id,
            locator: observation.locator,
            model: observation.model,
            effort: observation.effort,
        }
    }
    fn apply(&mut self, observation: ForwardObservation) {
        self.observed_at = observation.observed_at;
        if self.state == ForwardState::Unknown {
            return;
        }
        if self.backend != observation.backend
            || self.backend_id != observation.backend_id
            || invalid_direct(&observation)
            || merge_conflicts(&mut self.codex_thread_id, observation.codex_thread_id)
            || merge_conflicts(&mut self.locator, observation.locator)
        {
            self.state = ForwardState::Unknown;
            return;
        }
        keep_first(&mut self.model, observation.model);
        keep_first(&mut self.effort, observation.effort);
        if observation.state == ForwardState::Unknown {
            self.state = ForwardState::Unknown;
        } else if self.state.is_terminal() {
            let repeat = observation.state == self.state && observation.exit_code == self.exit_code;
            if observation.state.is_terminal() && !repeat {
                self.state = ForwardState::Unknown;
            }
        } else {
            self.state = observation.state;
            self.exit_code = observation.exit_code;
        }
    }
}
fn invalid_direct(observation: &ForwardObservation) -> bool {
    observation.backend == ForwardBackend::Direct
        && (observation.locator.is_some()
            || observation
                .codex_thread_id
                .as_deref()
                .is_some_and(|thread_id| thread_id != observation.backend_id))
}
fn merge_conflicts(current: &mut Option<String>, next: Option<String>) -> bool {
    match (current.as_ref(), next) {
        (Some(current), Some(next)) => current != &next,
        (None, Some(next)) => {
            *current = Some(next);
            false
        }
        _ => false,
    }
}
fn keep_first(current: &mut Option<String>, next: Option<String>) {
    if current.is_none() {
        *current = next;
    }
}
#[cfg(test)]
#[path = "forward_receipt/tests.rs"]
mod tests;
