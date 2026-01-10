//! Tests for poll and status update interval configuration

use loom::orchestrator::OrchestratorConfig;
use std::time::Duration;

#[test]
fn test_poll_interval_configuration() {
    // Test different poll intervals can be configured
    let configs = vec![
        Duration::from_millis(100),
        Duration::from_millis(500),
        Duration::from_secs(1),
        Duration::from_secs(5),
        Duration::from_secs(30),
    ];

    for poll_interval in configs {
        let config = OrchestratorConfig {
            poll_interval,
            ..Default::default()
        };
        assert_eq!(config.poll_interval, poll_interval);
    }
}

#[test]
fn test_status_update_interval_configuration() {
    // Test different status update intervals can be configured
    let intervals = vec![
        Duration::from_secs(5),
        Duration::from_secs(10),
        Duration::from_secs(30),
        Duration::from_secs(60),
    ];

    for interval in intervals {
        let config = OrchestratorConfig {
            status_update_interval: interval,
            ..Default::default()
        };
        assert_eq!(config.status_update_interval, interval);
    }
}
