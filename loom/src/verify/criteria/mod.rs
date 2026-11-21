//! Acceptance Criteria Execution Module
//!
//! This module executes shell commands defined as acceptance criteria in loom plans.
//!
//! # Trust Model
//!
//! Plan files (containing acceptance criteria and setup commands) follow the same trust
//! model as Makefiles, shell scripts, or CI/CD configuration files. They are considered
//! trusted project artifacts that are:
//!
//! - Version controlled alongside application code
//! - Reviewed as part of the normal code review process
//! - Authored by project maintainers or approved contributors
//!
//! Users should treat plan files with the same caution as any executable script:
//! do not run plans from untrusted sources without reviewing their contents.
//!
//! # Security Controls
//!
//! While plan files are trusted, this module implements the following controls to
//! limit the impact of runaway or misbehaving commands:
//!
//! - **Command Timeout**: All commands have a configurable timeout (default 5 minutes)
//!   to prevent indefinite hangs from blocking the orchestration pipeline.
//!
//! - **Explicit Shell Invocation**: Commands are executed via `sh -c` (Unix) or
//!   `cmd /C` (Windows) with the command passed as a single argument, avoiding
//!   shell injection through improper argument splitting. A command can also be
//!   expressed as [`CommandSpec::Program`], which involves no shell at all.
//!
//! - **Isolated Working Directory**: Commands can be scoped to a specific worktree
//!   directory, limiting their filesystem context.
//!
//! - **Host Confinement**: Commands run inside an OS boundary with an allowlisted
//!   environment, restricted filesystem access, and denied network access.
//!   Missing host containment fails before repository code runs. A plan or
//!   stage can opt out with `sandbox.command_confinement: inherit`. See
//!   [`resolve_confinement`] for the level resolution and [`spawn_confined`]
//!   for the spawn primitive.
//!
//! # Timeout Behavior
//!
//! When a command exceeds its timeout:
//! - The process is terminated (SIGKILL on Unix, TerminateProcess on Windows)
//! - The criterion is marked as failed with a timeout-specific error message
//! - Subsequent criteria continue to execute (fail-fast is not the default)

pub(crate) mod assertions;
mod cache;
mod cache_contract;
mod cache_executable;
mod cache_fingerprint;
mod cache_git_state;
mod cache_ignore;
mod config;
mod confine;
mod criterion_eval;
#[cfg(test)]
mod environment;
#[cfg(test)]
mod environment_contract_tests;
mod escapees;
mod executor;
mod live_output;
mod memory_journals;
pub(crate) mod output;
#[cfg(target_os = "linux")]
pub(crate) mod pid_namespace;
mod probe;
mod process_table;
mod result;
mod runner;
mod runner_results;
#[cfg(test)]
mod seatbelt_grant_contract_tests;
mod spawn_observer;
mod zero_tests;

#[cfg(test)]
mod tests;

// Re-export public types and functions
pub use cache::CachePolicy;
pub use config::{
    extended_criterion_timeout, CriteriaConfig, CriterionObserver, DEFAULT_CHECK_TIMEOUT,
    DEFAULT_COMMAND_TIMEOUT, DEFAULT_EXTENDED_CRITERION_TIMEOUT,
};
pub(crate) use confine::host::NetworkAccess;
#[cfg(test)]
pub(crate) use confine::{confined_policy, host_launcher, ConfinedPolicy};
pub(crate) use confine::{confined_stage_cache, stage_cache_for, stage_network};
#[cfg(target_os = "linux")]
pub(crate) use confine::{
    git_dirs,
    host::{bind_read_roots, bubblewrap, read_roots},
    CargoScratch,
};
#[cfg(target_os = "macos")]
pub(crate) use confine::{
    git_dirs,
    host::{read_roots, NetworkAccess},
};
pub use confine::{
    plan_confinement, plan_local_binding, resolve_confinement, spawn_confined, CommandSpec,
    ConfinedChild,
};
pub(crate) use escapees::lead_own_session;
pub(crate) use executor::{run_provision_with_timeout, run_spec_in_stage_network};
pub use executor::{
    run_single_criterion, run_single_criterion_with_timeout, run_spec_in_optional_stage,
    run_spec_in_optional_tree, run_spec_in_stage, run_spec_with_timeout,
};
pub use live_output::{LiveOutput, LiveWriterOpener};
pub use memory_journals::{memory_journals_grant, MemoryJournals};
pub(crate) use output::{format_criterion_result, running_block};
pub(in crate::verify) use probe::{CachedRun, CriteriaProbe, ProbeRun, ProbeRunner};
pub use result::{AcceptanceResult, CriterionResult};
pub use runner::{run_acceptance, run_acceptance_with_config};
pub use spawn_observer::{set_spawn_observer, GroupAnchor, SpawnObserver, SpawnObserverGuard};
pub(crate) use zero_tests::has_test_summary;
