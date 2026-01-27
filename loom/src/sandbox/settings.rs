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
    // If sandboxing is disabled, set dangerouslyDisableSandbox
    if !config.enabled {
        return json!({
            "dangerouslyDisableSandbox": true
        });
    }

    let mut permissions = json!({});

    // Build deny permissions (filesystem)
    let mut deny = Vec::new();

    // Add deny_read paths
    for path in &config.filesystem.deny_read {
        deny.push(json!({
            "type": "read",
            "path": path
        }));
    }

    // Add deny_write paths
    for path in &config.filesystem.deny_write {
        deny.push(json!({
            "type": "write",
            "path": path
        }));
    }

    // Build allow permissions (filesystem allow_write exceptions)
    let mut allow = Vec::new();

    for path in &config.filesystem.allow_write {
        allow.push(json!({
            "type": "write",
            "path": path
        }));
    }

    // Add network permissions
    if !config.network.allowed_domains.is_empty() || !config.network.additional_domains.is_empty() {
        let mut domains = config.network.allowed_domains.clone();
        domains.extend(config.network.additional_domains.clone());

        for domain in domains {
            allow.push(json!({
                "type": "network",
                "domain": domain
            }));
        }
    }

    // Add local binding permission if enabled
    if config.network.allow_local_binding {
        allow.push(json!({
            "type": "network",
            "binding": "local"
        }));
    }

    // Add Unix socket permission if enabled
    if config.network.allow_unix_sockets {
        allow.push(json!({
            "type": "socket",
            "protocol": "unix"
        }));
    }

    // Build permissions object
    if !allow.is_empty() {
        permissions["allow"] = json!(allow);
    }
    if !deny.is_empty() {
        permissions["deny"] = json!(deny);
    }

    // Build final settings object
    let mut settings = json!({
        "permissions": permissions,
        "dangerouslyDisableSandbox": false
    });

    // Add Linux-specific settings if configured
    if config.linux.enable_weaker_nested {
        settings["linux"] = json!({
            "enableWeakerNested": true
        });
    }

    // Add excluded commands if any
    if !config.excluded_commands.is_empty() {
        settings["excludedCommands"] = json!(config.excluded_commands);
    }

    // Add allow_unsandboxed_escape if enabled
    if config.allow_unsandboxed_escape {
        settings["allowUnsandboxedEscape"] = json!(true);
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
        assert_eq!(json["dangerouslyDisableSandbox"], true);
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
        assert_eq!(json["dangerouslyDisableSandbox"], false);

        let deny = json["permissions"]["deny"].as_array().unwrap();
        assert_eq!(deny.len(), 2);
        assert_eq!(deny[0]["type"], "read");
        assert_eq!(deny[0]["path"], "~/.ssh/**");

        let allow = json["permissions"]["allow"].as_array().unwrap();
        assert_eq!(allow.len(), 1);
        assert_eq!(allow[0]["type"], "write");
        assert_eq!(allow[0]["path"], "src/**");
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

        let allow = json["permissions"]["allow"].as_array().unwrap();
        // 2 domains + local binding + unix sockets
        assert_eq!(allow.len(), 4);

        // Check domain permissions
        assert!(allow
            .iter()
            .any(|p| p["type"] == "network" && p["domain"] == "*.github.com"));
        assert!(allow
            .iter()
            .any(|p| p["type"] == "network" && p["domain"] == "api.example.com"));

        // Check binding permission
        assert!(allow
            .iter()
            .any(|p| p["type"] == "network" && p["binding"] == "local"));

        // Check unix socket permission
        assert!(allow
            .iter()
            .any(|p| p["type"] == "socket" && p["protocol"] == "unix"));
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
        let excluded = json["excludedCommands"].as_array().unwrap();
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
        assert_eq!(json["allowUnsandboxedEscape"], true);
    }
}
