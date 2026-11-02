mod adjudication;
mod cache;
mod contract;
mod crud;
mod format;
mod generate;
mod helpers;
mod knowledge;
mod merge;
mod merge_discovery;
mod merge_launch;
#[cfg(test)]
pub(crate) use merge_launch::test_backend as merge_test_backend;
mod parse;
mod recovery;
mod recovery_format;
mod recovery_parsing;
mod recovery_types;
mod retrieval;
mod section_formatters;
mod types;
mod v2_section;

#[cfg(test)]
mod tests;

#[cfg(test)]
#[path = "tests_dependency_harness.rs"]
mod tests_dependency_harness;
#[cfg(test)]
#[path = "tests_size.rs"]
mod tests_size;

#[cfg(test)]
#[path = "tests_doctrine.rs"]
mod tests_doctrine;
#[cfg(test)]
#[path = "tests_doctrine_commands.rs"]
mod tests_doctrine_commands;
#[cfg(test)]
#[path = "tests_doctrine_gate.rs"]
mod tests_doctrine_gate;
#[cfg(test)]
#[path = "tests_doctrine_prefixes.rs"]
mod tests_doctrine_prefixes;
#[cfg(test)]
#[path = "tests_doctrine_v2.rs"]
mod tests_doctrine_v2;
#[cfg(test)]
#[path = "tests_doctrine_waiting.rs"]
mod tests_doctrine_waiting;
#[cfg(test)]
#[path = "tests_guidance_manifest.rs"]
mod tests_guidance_manifest;

// Re-export public types
pub use cache::SignalMetrics;
pub use recovery::generate_recovery_signal;
pub use recovery_parsing::read_recovery_signal;
pub use recovery_types::{LastHeartbeatInfo, RecoveryReason, RecoverySignalContent};
pub use types::{
    DependencyStatus, EmbeddedContext, MergeSignalContent, SignalContent, SignalUpdates,
};

// Re-export public functions
pub use cache::compute_hash;
pub use crud::{list_signals, read_signal, remove_signal, update_signal};
pub use format::{
    format_dependency_table, format_signal_with_metrics, format_skill_recommendations,
    FormattedSignal,
};
// Internal exports share prefix and brief formatting with their consumers.
pub use adjudication::generate_adjudication_signal;
pub(crate) use cache::stable_prefix_for;
pub use contract::generate_contract_signal;
pub(crate) use contract::scaffold_line;
pub(crate) use format::format_knowledge_brief;
pub use generate::{
    build_embedded_context_with_stage, generate_signal, generate_signal_with_metrics,
    generate_signal_with_skills, DEFAULT_MAX_SKILL_RECOMMENDATIONS,
};
pub use knowledge::generate_knowledge_signal;
pub use merge::{generate_merge_signal, read_merge_signal};
pub use merge_discovery::find_live_merge_session_for_stage;
pub(crate) use merge_discovery::find_live_merge_session_for_stage_unlocked;
mod merge_snapshot;
pub(crate) use merge::generate_merge_snapshot_signal;
pub(crate) use merge_launch::with_merge_resolver_lock;
pub(crate) use merge_launch::{publish_merge_session, write_ahead_merge_session};
pub(crate) use merge_snapshot::{load_merge_snapshot, publish_merge_snapshot};
