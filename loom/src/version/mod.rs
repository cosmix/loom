pub mod derive;

/// The release tag this binary was built from, embedded at compile time by
/// `build.rs` (`LOOM_VERSION`) and checked against the pushed tag by the
/// release workflow's `verify-version` job.
pub const VERSION: &str = env!("LOOM_VERSION");

/// [`VERSION`] with a leading `v`, as the dashboards print it.
pub const LABEL: &str = concat!("v", env!("LOOM_VERSION"));
