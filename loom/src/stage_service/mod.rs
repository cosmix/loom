//! Stage application services shared by RPC and durable spool adapters.
mod contracts;
mod control_block;
mod dispute;
mod dispute_kinds;
mod dispute_store;
pub(crate) mod observer;
pub(crate) mod ownership;
mod receipts;
mod types;
pub(crate) use contracts::handle_freeze_contracts;
pub(crate) use control_block::handle_block_stage;
pub use dispute::handle_dispute_criteria;
pub(crate) use dispute::handle_dispute_criteria_identified;
pub(crate) use dispute_kinds::handle_file_dispute;
pub(crate) use dispute_kinds::handle_file_dispute_identified;
use serde::{Deserialize, Serialize};
pub use types::{QueuedRequest, StageRequest};
pub const MAX_REQUEST_BYTES: usize = 64 * 1024;
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Response {
    Ok,
    Error { message: String },
    DisputeCreated { id: u32 },
    ContractsFrozen { files: usize },
}

/// How one contract test ran when the contract session froze it.
///
/// `outcome` is `failed`, `build_failed` or `exit_nonzero_unverified`: a
/// contract is frozen only while it fails.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContractRunReport {
    pub contract_id: String,
    pub adapter: Option<String>,
    pub outcome: String,
    pub exit_code: Option<i32>,
}
