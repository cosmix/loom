//! Provider observations and session discovery shared by live and offline consumers.
pub(crate) mod claude;
pub(crate) mod codex_discovery;
pub(crate) mod ledger;
pub mod resolve;
pub mod sessions;
pub(crate) const SYNTHETIC_MODEL: &str = "<synthetic>";

/// Later timestamps win; input order breaks ties for equal timestamps.
pub(crate) fn supersedes<T: Ord>(current: T, incoming: T) -> bool {
    incoming >= current
}
