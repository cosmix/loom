//! Tests for criteria configuration

use std::time::Duration;

use crate::verify::criteria::config::{CriteriaConfig, DEFAULT_COMMAND_TIMEOUT};

#[test]
fn test_criteria_config_default() {
    let config = CriteriaConfig::default();
    assert_eq!(config.command_timeout, DEFAULT_COMMAND_TIMEOUT);
    assert_eq!(config.command_timeout, Duration::from_secs(300));
}

#[test]
fn test_criteria_config_with_timeout() {
    let config = CriteriaConfig::with_timeout(Duration::from_secs(60));
    assert_eq!(config.command_timeout, Duration::from_secs(60));
}
