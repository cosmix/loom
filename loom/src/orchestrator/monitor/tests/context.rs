//! Monitor configuration defaults and the context-health bands.

use crate::models::constants::DEFAULT_CONTEXT_CEILING_TOKENS;
use crate::orchestrator::monitor::{context_health, ContextHealth, MonitorConfig};
use std::path::PathBuf;
use std::time::Duration;

#[test]
fn test_monitor_config_default() {
    let config = MonitorConfig::default();
    assert_eq!(config.poll_interval, Duration::from_secs(5));
    assert_eq!(config.work_dir, PathBuf::from(".work"));
    assert_eq!(
        config.context.ceiling_tokens,
        DEFAULT_CONTEXT_CEILING_TOKENS
    );
}

/// The band edges, exactly. Bands are fractions of the STAGE's ceiling, not of
/// some model's window, so 90_000 of 150_000 is Yellow whatever model is
/// running.
#[test]
fn context_health_bands_are_fractions_of_the_ceiling() {
    let ceiling = DEFAULT_CONTEXT_CEILING_TOKENS; // 150_000

    assert_eq!(context_health(0, ceiling), ContextHealth::Green);
    assert_eq!(context_health(89_999, ceiling), ContextHealth::Green);

    // 60% exactly is the first Yellow.
    assert_eq!(context_health(90_000, ceiling), ContextHealth::Yellow);
    assert_eq!(context_health(134_999, ceiling), ContextHealth::Yellow);

    // 90% exactly is the first Red.
    assert_eq!(context_health(135_000, ceiling), ContextHealth::Red);
    assert_eq!(context_health(400_000, ceiling), ContextHealth::Red);
}

/// A ceiling of 0 means the ceiling could not be resolved. Missing evidence is
/// not an emergency: reporting Red would hand off every session whose stage
/// failed to load.
#[test]
fn context_health_with_no_resolvable_ceiling_is_green() {
    assert_eq!(context_health(0, 0), ContextHealth::Green);
    assert_eq!(context_health(u32::MAX, 0), ContextHealth::Green);
}
