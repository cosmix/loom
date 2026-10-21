use crate::models::stage::NetworkConfig;

#[test]
fn default_network_config_denies_unix_sockets() {
    assert!(NetworkConfig::default().allow_unix_sockets.is_empty());
    assert!(!NetworkConfig::default().allow_all_unix_sockets);
}
