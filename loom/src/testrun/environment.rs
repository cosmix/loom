//! The installed environments the adapters declare
//! ([`TestRunnerAdapter::required_environment`]): what a run of a package
//! needs on disk and cannot build with the network denied.
//!
//! [`TestRunnerAdapter::required_environment`]: super::TestRunnerAdapter::required_environment

use std::path::Path;

use crate::skills::project::declares_dependencies;

/// `node_modules`, when the JS package at `package_dir` declares dependencies
/// (an unreadable or unparsable manifest counts as declaring them).
pub(super) fn node_modules(root: &Path, package_dir: &Path) -> Option<&'static str> {
    declares_dependencies(root, package_dir).then_some("node_modules")
}

/// `.venv`, when the package at `package_dir` has a `uv.lock`: `uv run` runs
/// its tests from that environment.
pub(super) fn uv_venv(package_dir: &Path) -> Option<&'static str> {
    package_dir.join("uv.lock").is_file().then_some(".venv")
}
