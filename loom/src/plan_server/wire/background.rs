//! The wire form of the plan chat's background tasks: the bridge's table row, flattened, with
//! the token figures of the agent it names.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::plan_session::bridge::background::{TaskKind, TaskState};

/// One background task. The first eleven fields are the bridge's `BackgroundTask`; the last
/// four come from the planner token pass and are `null` for a task it has no row for (a shell,
/// or an agent not read yet).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BackgroundTaskView {
    pub id: String,
    pub kind: TaskKind,
    pub label: Option<String>,
    pub agent_type: Option<String>,
    pub tool_use_id: Option<String>,
    pub state: TaskState,
    pub started_at: DateTime<Utc>,
    pub last_activity_at: DateTime<Utc>,
    pub steps: Option<u64>,
    pub last_tool: Option<String>,
    pub report_preview: Option<String>,
    pub model: Option<String>,
    /// Fresh tokens (input, cache writes, output) the agent used.
    pub total: Option<u64>,
    /// Fresh tokens per minute, as the dashboard's lane shows them.
    pub tokens_per_min: Option<u64>,
    pub active: Option<bool>,
}
