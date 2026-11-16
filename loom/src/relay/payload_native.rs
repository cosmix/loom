//! Lead-issued commands for native Codex workers.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::native_worker::model::Delivery;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case", deny_unknown_fields)]
pub enum WorkerControlPayload {
    Start {
        unit_id: String,
        brief_path: PathBuf,
        brief_sha256: String,
        model: String,
        effort: String,
    },
    Message {
        unit_id: String,
        invocation_id: String,
        message_id: String,
        delivery: Delivery,
        text: String,
    },
    Cancel {
        unit_id: String,
        invocation_id: String,
        reason: String,
    },
}
