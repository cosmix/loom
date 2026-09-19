use crate::models::stage::NetworkConfig;

#[test]
fn default_network_config_denies_unix_sockets_for_completion_broker_integrity() {
    assert!(NetworkConfig::default().allow_unix_sockets.is_empty());
    assert!(!NetworkConfig::default().allow_all_unix_sockets);
}
