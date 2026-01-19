//! Task verification execution
//!
//! Runs verification rules defined in task definitions.
//! Verification is soft - it emits warnings but doesn't hard-block.

mod reporters;
mod runners;
mod types;

pub use reporters::summarize_verifications;
pub use runners::{run_single_verification, run_task_verifications};
pub use types::DEFAULT_VERIFICATION_TIMEOUT;
