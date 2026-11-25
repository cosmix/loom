//! Stage state manipulation
//! Usage: `loom stage <id> [complete|block|reset|merge|retry|...]`

pub(crate) mod acceptance_runner;
mod adjudicate;
pub(crate) mod admin_proof;
pub(crate) mod amend;
pub mod commit;
pub mod complete;
pub(crate) mod contracts;
pub(crate) mod criterion_output;
mod dispute_criteria;
mod dispute_kinds;
mod dispute_transport;
mod human_review;
mod integrity_status;
mod knowledge_complete;
mod merge;
mod merge_resolver;
pub(crate) mod native_gate;
#[cfg(test)]
pub(crate) use merge_resolver::{
    spawn_merge_resolver as test_spawn_merge_resolver, MergeResolverResult,
};
pub mod merge_verify;
mod output;
mod progressive_complete;
pub(crate) mod recover;
mod review_status;
pub(crate) mod session;
mod skip_retry;
mod state;
pub(crate) mod verify_host;

#[cfg(test)]
mod amend_contract_tests;
#[cfg(test)]
mod landing_note_contract_tests;
#[cfg(test)]
mod native_gate_contract_tests;
#[cfg(test)]
mod tests;

// Re-export public API
pub use adjudicate::{adjudicate, record_verdict, AdjudicateOutcome};
pub use complete::complete;
pub use contracts::{
    freeze as contracts_freeze, restore as contracts_restore, show as contracts_show,
};
pub use dispute_criteria::dispute_criteria;
pub use dispute_kinds::{file_dispute, DisputeFiling, DisputeTarget};
pub(crate) use dispute_kinds::{parse_replacements, read_replacements};
pub use human_review::human_review;
pub use integrity_status::review_integrity;
pub use merge::merge;
pub use output::{
    get as output_get, list as output_list, remove as output_remove, set as output_set,
};
pub use review_status::review_status;
pub(crate) use review_status::stage_worktree_and_target;
pub use skip_retry::{retry, skip};
pub use state::{block, hold, release, reset, resume_from_waiting, waiting};
