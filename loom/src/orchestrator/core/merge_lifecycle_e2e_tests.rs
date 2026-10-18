//! The merge lifecycle driven through the daemon's own entry points with real
//! git: auto-merge, the blocked-merge retry, `--resolved` relayed through the
//! inbox, the resolver's exit, and the leftover sweep. Every scenario asserts,
//! after each step, what the operator's main checkout must keep: no
//! `MERGE_HEAD`, the same `HEAD` and bytes unless the step is the
//! fast-forward of the target.

#[path = "merge_lifecycle_e2e_tests_blocked.rs"]
mod merge_lifecycle_e2e_tests_blocked;
#[path = "merge_lifecycle_e2e_tests_conflict.rs"]
mod merge_lifecycle_e2e_tests_conflict;
#[path = "merge_lifecycle_e2e_tests_guard_contracts.rs"]
mod merge_lifecycle_e2e_tests_guard_contracts;
#[path = "merge_lifecycle_e2e_tests_support.rs"]
mod merge_lifecycle_e2e_tests_support;
#[path = "merge_lifecycle_e2e_tests_sync.rs"]
mod merge_lifecycle_e2e_tests_sync;
