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
/// some model's window, so 60% of the ceiling is Yellow whatever model is
/// running and whatever the ceiling happens to be.
///
/// The edges are computed from the constant rather than written out: this test
/// asserted absolute token counts derived from a 150,000-token default, and
/// every one of them became a different band the day that default moved.
#[test]
fn context_health_bands_are_fractions_of_the_ceiling() {
    let ceiling = DEFAULT_CONTEXT_CEILING_TOKENS;
    // Integer math, not floats: `(ceiling as f32 * 0.90) as u32` truncates one
    // token below the Red edge and would assert the opposite band.
    let percent = |n: u64| ((ceiling as u64 * n) / 100) as u32;

    assert_eq!(context_health(0, ceiling), ContextHealth::Green);
    assert_eq!(
        context_health(percent(60) - 1, ceiling),
        ContextHealth::Green
    );

    // 60% exactly is the first Yellow.
    assert_eq!(context_health(percent(60), ceiling), ContextHealth::Yellow);
    assert_eq!(
        context_health(percent(90) - 1, ceiling),
        ContextHealth::Yellow
    );

    // 90% exactly is the first Red.
    assert_eq!(context_health(percent(90), ceiling), ContextHealth::Red);
    assert_eq!(context_health(percent(200), ceiling), ContextHealth::Red);
}

/// A ceiling of 0 means the ceiling could not be resolved. Missing evidence is
/// not an emergency: reporting Red would hand off every session whose stage
/// failed to load.
#[test]
fn context_health_with_no_resolvable_ceiling_is_green() {
    assert_eq!(context_health(0, 0), ContextHealth::Green);
    assert_eq!(context_health(u32::MAX, 0), ContextHealth::Green);
}
