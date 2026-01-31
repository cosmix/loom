use super::config::MergedSandboxConfig;
use anyhow::{Context, Result};
use serde_json::{json, Value};
use std::fs;
use std::path::Path;

/// Write Claude Code settings.local.json to worktree .claude/ directory
pub fn write_settings(config: &MergedSandboxConfig, worktree_path: &Path) -> Result<()> {
    let claude_dir = worktree_path.join(".claude");

    // Create .claude/ directory if it doesn't exist
    fs::create_dir_all(&claude_dir)
        .with_context(|| format!("Failed to create .claude directory at {:?}", claude_dir))?;

    let settings_path = claude_dir.join("settings.local.json");
    let settings_json = generate_settings_json(config);

    // Write settings file with pretty formatting
    let settings_string = serde_json::to_string_pretty(&settings_json)
        .context("Failed to serialize settings JSON")?;

    fs::write(&settings_path, settings_string)
        .with_context(|| format!("Failed to write settings to {:?}", settings_path))?;

    Ok(())
}

/// Generate Claude Code settings JSON from sandbox config
pub fn generate_settings_json(config: &MergedSandboxConfig) -> Value {
    let mut settings = json!({});

    // Build sandbox block with native sandbox configuration
    let mut sandbox = json!({
        "enabled": config.enabled
    });

    // Add autoAllowBashIfSandboxed if enabled
    if config.auto_allow {
        sandbox["autoAllowBashIfSandboxed"] = json!(true);
    }

    // Add excluded commands if any
    if !config.excluded_commands.is_empty() {
        sandbox["excludedCommands"] = json!(config.excluded_commands);
    }

    // Add allowUnsandboxedCommands if enabled
    if config.allow_unsandboxed_escape {
        sandbox["allowUnsandboxedCommands"] = json!(true);
    }

    // Add network configuration
    let mut network = json!({});
    let mut domains = config.network.allowed_domains.clone();
    domains.extend(config.network.additional_domains.clone());
    if !domains.is_empty() {
        network["allowedDomains"] = json!(domains);
    }
    if config.network.allow_local_binding {
        network["allowLocalBinding"] = json!(true);
    }
    // Only add network block if it has content
    if network.as_object().is_some_and(|o| !o.is_empty()) {
        sandbox["network"] = network;
    }

    settings["sandbox"] = sandbox;

    // Build permissions block for file tool restrictions (Read/Write/Edit prompting)
    // These still work for prompting even though they don't provide OS-level isolation
    let mut permissions = json!({});
    let mut deny: Vec<Value> = Vec::new();
    let mut allow: Vec<Value> = Vec::new();

    // Add deny_read paths (prompts before allowing Read tool on these)
    for path in &config.filesystem.deny_read {
        deny.push(json!(format!("Read({})", path)));
    }

    // Add deny_write paths (prompts before allowing Write/Edit tools on these)
    for path in &config.filesystem.deny_write {
        deny.push(json!(format!("Write({})", path)));
    }

    // Add allow_write paths as exceptions
    for path in &config.filesystem.allow_write {
        allow.push(json!(format!("Write({})", path)));
    }

    if !allow.is_empty() {
        permissions["allow"] = json!(allow);
    }
    if !deny.is_empty() {
        permissions["deny"] = json!(deny);
    }
    if permissions.as_object().is_some_and(|o| !o.is_empty()) {
        settings["permissions"] = permissions;
    }

    // Add Linux-specific settings if configured
    if config.linux.enable_weaker_nested {
        settings["linux"] = json!({
            "enableWeakerNested": true
        });
    }

    settings
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::plan::schema::{FilesystemConfig, LinuxConfig, NetworkConfig};

    #[test]
    fn test_generate_settings_disabled() {
        let config = MergedSandboxConfig {
            enabled: false,
            auto_allow: true,
            allow_unsandboxed_escape: false,
            excluded_commands: vec![],
            filesystem: FilesystemConfig::default(),
            network: NetworkConfig::default(),
            linux: LinuxConfig::default(),
        };

        let json = generate_settings_json(&config);
        // Sandbox block should have enabled: false
        assert_eq!(json["sandbox"]["enabled"], false);
        // dangerouslyDisableSandbox should NOT be present (deprecated)
        assert!(json.get("dangerouslyDisableSandbox").is_none());
    }

    #[test]
    fn test_generate_settings_with_filesystem() {
        let config = MergedSandboxConfig {
            enabled: true,
            auto_allow: true,
            allow_unsandboxed_escape: false,
            excluded_commands: vec![],
            filesystem: FilesystemConfig {
                deny_read: vec!["~/.ssh/**".to_string()],
                deny_write: vec![".work/**".to_string()],
                allow_write: vec!["src/**".to_string()],
            },
            network: NetworkConfig::default(),
            linux: LinuxConfig::default(),
        };

        let json = generate_settings_json(&config);
        // Sandbox enabled in sandbox block
        assert_eq!(json["sandbox"]["enabled"], true);
        assert_eq!(json["sandbox"]["autoAllowBashIfSandboxed"], true);

        // Permissions for file tool restrictions
        let deny = json["permissions"]["deny"].as_array().unwrap();
        assert_eq!(deny.len(), 2);
        assert_eq!(deny[0], "Read(~/.ssh/**)");
        assert_eq!(deny[1], "Write(.work/**)");

        let allow = json["permissions"]["allow"].as_array().unwrap();
        assert_eq!(allow.len(), 1);
        assert_eq!(allow[0], "Write(src/**)");
    }

    #[test]
    fn test_generate_settings_with_network() {
        let config = MergedSandboxConfig {
            enabled: true,
            auto_allow: true,
            allow_unsandboxed_escape: false,
            excluded_commands: vec![],
            filesystem: FilesystemConfig::default(),
            network: NetworkConfig {
                allowed_domains: vec!["*.github.com".to_string()],
                additional_domains: vec!["api.example.com".to_string()],
                allow_local_binding: true,
                allow_unix_sockets: true,
            },
            linux: LinuxConfig::default(),
        };

        let json = generate_settings_json(&config);

        // Network config is now in sandbox.network block
        let network = &json["sandbox"]["network"];
        let domains = network["allowedDomains"].as_array().unwrap();
        assert_eq!(domains.len(), 2);
        assert!(domains.iter().any(|d| d == "*.github.com"));
        assert!(domains.iter().any(|d| d == "api.example.com"));
        assert_eq!(network["allowLocalBinding"], true);
    }

    #[test]
    fn test_generate_settings_with_linux_config() {
        let config = MergedSandboxConfig {
            enabled: true,
            auto_allow: true,
            allow_unsandboxed_escape: false,
            excluded_commands: vec![],
            filesystem: FilesystemConfig::default(),
            network: NetworkConfig::default(),
            linux: LinuxConfig {
                enable_weaker_nested: true,
            },
        };

        let json = generate_settings_json(&config);
        assert_eq!(json["linux"]["enableWeakerNested"], true);
    }

    #[test]
    fn test_generate_settings_with_excluded_commands() {
        let config = MergedSandboxConfig {
            enabled: true,
            auto_allow: true,
            allow_unsandboxed_escape: false,
            excluded_commands: vec!["loom".to_string(), "git".to_string()],
            filesystem: FilesystemConfig::default(),
            network: NetworkConfig::default(),
            linux: LinuxConfig::default(),
        };

        let json = generate_settings_json(&config);
        // Excluded commands are now in sandbox block
        let excluded = json["sandbox"]["excludedCommands"].as_array().unwrap();
        assert_eq!(excluded.len(), 2);
        assert_eq!(excluded[0], "loom");
        assert_eq!(excluded[1], "git");
    }

    #[test]
    fn test_generate_settings_with_unsandboxed_escape() {
        let config = MergedSandboxConfig {
            enabled: true,
            auto_allow: true,
            allow_unsandboxed_escape: true,
            excluded_commands: vec![],
            filesystem: FilesystemConfig::default(),
            network: NetworkConfig::default(),
            linux: LinuxConfig::default(),
        };

        let json = generate_settings_json(&config);
        // allowUnsandboxedCommands is now in sandbox block
        assert_eq!(json["sandbox"]["allowUnsandboxedCommands"], true);
    }
}
