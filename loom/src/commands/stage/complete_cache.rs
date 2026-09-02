//! `--no-cache` bypass for the acceptance-pass cache.

/// Disable the acceptance-pass cache for this process when `--no-cache` was
/// passed. Single-threaded at this point in a one-shot CLI invocation — no
/// concurrent reader of the environment exists yet — so a plain process-wide
/// set is safe, the same reasoning `daemon::server::environment::apply`
/// relies on. `CriteriaConfig::default()` (built inside
/// `run_acceptance_with_display`) reads this once.
pub(super) fn bypass_acceptance_cache(no_cache: bool) {
    if no_cache {
        std::env::set_var("LOOM_ACCEPTANCE_CACHE", "0");
    }
}
