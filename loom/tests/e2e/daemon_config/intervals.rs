//! Default poll and status update intervals

use loom::orchestrator::OrchestratorConfig;
use std::time::Duration;

#[test]
fn test_poll_interval_default() {
    assert_eq!(
        OrchestratorConfig::default().poll_interval,
        Duration::from_secs(5)
    );
}

#[test]
fn test_status_update_interval_default() {
    assert_eq!(
        OrchestratorConfig::default().status_update_interval,
        Duration::from_secs(30)
    );
}
