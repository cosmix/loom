//! Sharing a failed build between the contracts of one run.
//!
//! Two contracts whose commands differ only in the test name, run from the same
//! directory, compile the same code, whatever the runner. When the compiler
//! rejects that code for the first, running the second repeats the work to
//! reach the same rejection, because no runner caches a failed build. Only a
//! rejection is shared (the adapter decides, see
//! `TestRunnerAdapter::compile_rejected`): a failing test says something about
//! its own test, and a pass is never shared. A build that succeeds is not
//! remembered here, since each runner's own incremental cache makes the repeat
//! cheap. The freeze and the completion check both use it.

use std::collections::HashMap;
use std::path::PathBuf;

/// What two contracts share when they run the same build: the run directory
/// and the command with the test name taken out
/// (`ContractRun::build`).
pub(crate) type BuildKey = (PathBuf, String);

/// The first rejected build of each [`BuildKey`], with the contract that hit it
/// and what the caller keeps of its run.
pub(crate) struct FailedBuilds<T> {
    failed: HashMap<BuildKey, (String, T)>,
}

impl<T> Default for FailedBuilds<T> {
    fn default() -> Self {
        Self {
            failed: HashMap::new(),
        }
    }
}

impl<T> FailedBuilds<T> {
    /// The contract whose run of `key` was rejected and what was kept of it.
    pub(crate) fn get(&self, key: &BuildKey) -> Option<(&str, &T)> {
        self.failed
            .get(key)
            .map(|(contract, kept)| (contract.as_str(), kept))
    }

    /// Remember that `contract`'s run of `key` was rejected; the first
    /// rejection of a key stays.
    pub(crate) fn record(&mut self, key: BuildKey, contract: String, kept: T) {
        self.failed.entry(key).or_insert((contract, kept));
    }
}
