use super::config::MergedSandboxConfig;
use anyhow::{Context, Result};
use serde_json::{json, Value};
use std::collections::HashSet;
use std::fs;
use std::path::Path;

/// Write Claude Code settings.local.json to worktree .claude/ directory
pub fn write_settings(config: &MergedSandboxConfig, worktree_path: &Path) -> Result<()> {
    let claude_dir = worktree_path.join(".claude");

    // Create .claude/ directory if it doesn't exist
    fs::create_dir_all(&claude_dir)
        .with_context(|| format!("Failed to create .claude directory at {:?}", claude_dir))?;

    let settings_path = claude_dir.join("settings.local.json");

    // Read existing settings if they exist
    let existing_settings = if settings_path.exists() {
        let content = fs::read_to_string(&settings_path)
            .with_context(|| format!("Failed to read existing settings at {:?}", settings_path))?;
        serde_json::from_str(&content)
            .with_context(|| format!("Failed to parse existing settings at {:?}", settings_path))?
    } else {
        json!({})
    };

    // Generate new sandbox settings
    let mut settings_json = generate_settings_json(config);

    // Merge existing permissions into the new settings
    merge_existing_permissions(&mut settings_json, &existing_settings);

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

    // Add Bash permissions for excluded commands.
    // This complements sandbox.excludedCommands (which exempts from OS-level sandbox)
    // by also auto-approving the permission prompt for these commands.
    for cmd in &config.excluded_commands {
        let cmd_trimmed = cmd.trim();
        if cmd_trimmed.is_empty() {
            continue;
        }
        allow.push(json!(format!("Bash({}:*)", cmd_trimmed)));
    }

    // Add narrow Read permissions for orchestration state files agents need.
    // The main settings.json grants broader Read(.work/**) via LOOM_PERMISSIONS_WORKTREE,
    // but settings.local.json uses these narrow grants as defense-in-depth.
    allow.push(json!("Read(.work/signals/**)"));
    allow.push(json!("Read(.work/handoffs/**)"));
    allow.push(json!("Read(.work/config.toml)"));

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

/// Merge existing permissions from an old settings file into new settings
///
/// This preserves user-approved permissions that were granted in a previous settings file,
/// while still applying sandbox-generated permissions. Only `permissions.allow` and
/// `permissions.deny` are merged - sandbox/network/linux config always comes from the generator.
///
/// Uses HashSet for deduplication to avoid duplicate permissions in the merged result.
fn merge_existing_permissions(new_settings: &mut Value, existing_settings: &Value) {
    // Extract existing permissions if they exist
    let existing_permissions = existing_settings.get("permissions");
    if existing_permissions.is_none() || existing_permissions.unwrap().is_null() {
        return; // No permissions to merge
    }

    let existing_allow = existing_permissions
        .and_then(|p| p.get("allow"))
        .and_then(|a| a.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(String::from))
                .collect::<Vec<String>>()
        })
        .unwrap_or_default();

    let existing_deny = existing_permissions
        .and_then(|p| p.get("deny"))
        .and_then(|d| d.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(String::from))
                .collect::<Vec<String>>()
        })
        .unwrap_or_default();

    // Get or create permissions block in new settings
    let new_permissions = new_settings
        .as_object_mut()
        .and_then(|obj| obj.get_mut("permissions"))
        .and_then(|p| p.as_object_mut());

    if new_permissions.is_none() {
        return; // New settings has no permissions block, nothing to merge into
    }

    let new_permissions = new_permissions.unwrap();

    // Merge allow permissions
    if !existing_allow.is_empty() {
        let new_allow = new_permissions
            .entry("allow")
            .or_insert_with(|| json!([]))
            .as_array_mut();

        if let Some(new_allow_arr) = new_allow {
            // Collect all permissions into a HashSet for deduplication
            let mut all_allow: HashSet<String> = new_allow_arr
                .iter()
                .filter_map(|v| v.as_str().map(String::from))
                .collect();

            // Add existing permissions
            for perm in existing_allow {
                all_allow.insert(perm);
            }

            // Replace array with deduplicated permissions
            *new_allow_arr = all_allow.into_iter().map(|s| json!(s)).collect();
        }
    }

    // Merge deny permissions
    if !existing_deny.is_empty() {
        let new_deny = new_permissions
            .entry("deny")
            .or_insert_with(|| json!([]))
            .as_array_mut();

        if let Some(new_deny_arr) = new_deny {
            // Collect all permissions into a HashSet for deduplication
            let mut all_deny: HashSet<String> = new_deny_arr
                .iter()
                .filter_map(|v| v.as_str().map(String::from))
                .collect();

            // Add existing permissions
            for perm in existing_deny {
                all_deny.insert(perm);
            }

            // Replace array with deduplicated permissions
            *new_deny_arr = all_deny.into_iter().map(|s| json!(s)).collect();
        }
    }
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
        // Now includes: Write(src/**) + Read(.work/signals/**) + Read(.work/handoffs/**) + Read(.work/config.toml)
        assert_eq!(allow.len(), 4);
        assert_eq!(allow[0], "Write(src/**)");
        assert_eq!(allow[1], "Read(.work/signals/**)");
        assert_eq!(allow[2], "Read(.work/handoffs/**)");
        assert_eq!(allow[3], "Read(.work/config.toml)");
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

    #[test]
    fn test_generate_settings_excluded_commands_get_bash_allow() {
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
        let allow = json["permissions"]["allow"].as_array().unwrap();

        // Should have Bash(loom:*) and Bash(git:*) in allow
        let allow_strs: Vec<&str> = allow.iter().map(|v| v.as_str().unwrap()).collect();
        assert!(
            allow_strs.contains(&"Bash(loom:*)"),
            "Should have Bash(loom:*) in allow, got: {:?}",
            allow_strs
        );
        assert!(
            allow_strs.contains(&"Bash(git:*)"),
            "Should have Bash(git:*) in allow, got: {:?}",
            allow_strs
        );
    }

    #[test]
    fn test_generate_settings_includes_work_dir_read_allows() {
        let config = MergedSandboxConfig {
            enabled: true,
            auto_allow: true,
            allow_unsandboxed_escape: false,
            excluded_commands: vec![],
            filesystem: FilesystemConfig::default(),
            network: NetworkConfig::default(),
            linux: LinuxConfig::default(),
        };

        let json = generate_settings_json(&config);
        let allow = json["permissions"]["allow"].as_array().unwrap();

        let allow_strs: Vec<&str> = allow.iter().map(|v| v.as_str().unwrap()).collect();
        assert!(
            allow_strs.contains(&"Read(.work/signals/**)"),
            "Should allow reading signals, got: {:?}",
            allow_strs
        );
        assert!(
            allow_strs.contains(&"Read(.work/handoffs/**)"),
            "Should allow reading handoffs, got: {:?}",
            allow_strs
        );
        assert!(
            allow_strs.contains(&"Read(.work/config.toml)"),
            "Should allow reading config, got: {:?}",
            allow_strs
        );
    }

    #[test]
    fn test_no_path_in_both_allow_and_deny() {
        use crate::plan::schema::{SandboxConfig, StageSandboxConfig, StageType};
        use crate::sandbox::merge_config;

        // Test all stage types
        for stage_type in [
            StageType::Standard,
            StageType::Knowledge,
            StageType::IntegrationVerify,
            StageType::CodeReview,
        ] {
            let plan = SandboxConfig::default();
            let stage = StageSandboxConfig::default();
            let merged = merge_config(&plan, &stage, stage_type);
            let json = generate_settings_json(&merged);

            let permissions = &json["permissions"];
            if permissions.is_null() {
                continue;
            }

            let allow = permissions["allow"]
                .as_array()
                .map(|a| a.to_vec())
                .unwrap_or_default();
            let deny = permissions["deny"]
                .as_array()
                .map(|a| a.to_vec())
                .unwrap_or_default();

            // Compare full permission strings (e.g. "Read(.work/signals/**)")
            // to detect true conflicts where the same permission type + path
            // appears in both allow and deny.
            let allow_strs: Vec<&str> = allow.iter().filter_map(|v| v.as_str()).collect();
            let deny_strs: Vec<&str> = deny.iter().filter_map(|v| v.as_str()).collect();

            for entry in &allow_strs {
                assert!(
                    !deny_strs.contains(entry),
                    "Stage type {:?}: '{}' appears in both allow and deny",
                    stage_type,
                    entry
                );
            }
        }
    }

    #[test]
    fn test_write_settings_preserves_existing_permissions() {
        use tempfile::TempDir;

        let temp_dir = TempDir::new().unwrap();
        let worktree_path = temp_dir.path();

        // Create existing settings.local.json with user-approved permissions
        let claude_dir = worktree_path.join(".claude");
        fs::create_dir_all(&claude_dir).unwrap();
        let settings_path = claude_dir.join("settings.local.json");

        let existing_settings = json!({
            "permissions": {
                "allow": [
                    "Read(~/.ssh/config)",
                    "Bash(docker:*)"
                ],
                "deny": [
                    "Write(~/.bashrc)"
                ]
            }
        });
        fs::write(
            &settings_path,
            serde_json::to_string_pretty(&existing_settings).unwrap(),
        )
        .unwrap();

        // Now call write_settings with sandbox config
        let config = MergedSandboxConfig {
            enabled: true,
            auto_allow: true,
            allow_unsandboxed_escape: false,
            excluded_commands: vec![],
            filesystem: FilesystemConfig {
                deny_read: vec![],
                deny_write: vec![],
                allow_write: vec!["src/**".to_string()],
            },
            network: NetworkConfig::default(),
            linux: LinuxConfig::default(),
        };

        write_settings(&config, worktree_path).unwrap();

        // Read the result
        let result_content = fs::read_to_string(&settings_path).unwrap();
        let result: Value = serde_json::from_str(&result_content).unwrap();

        // Verify sandbox-generated permissions are present
        let allow = result["permissions"]["allow"].as_array().unwrap();
        let allow_strs: Vec<&str> = allow.iter().filter_map(|v| v.as_str()).collect();
        assert!(allow_strs.contains(&"Write(src/**)"));
        assert!(allow_strs.contains(&"Read(.work/signals/**)"));

        // Verify existing permissions are preserved
        assert!(allow_strs.contains(&"Read(~/.ssh/config)"));
        assert!(allow_strs.contains(&"Bash(docker:*)"));

        let deny = result["permissions"]["deny"].as_array().unwrap();
        let deny_strs: Vec<&str> = deny.iter().filter_map(|v| v.as_str()).collect();
        assert!(deny_strs.contains(&"Write(~/.bashrc)"));
    }

    #[test]
    fn test_write_settings_deduplicates() {
        use tempfile::TempDir;

        let temp_dir = TempDir::new().unwrap();
        let worktree_path = temp_dir.path();

        // Create existing settings.local.json with overlapping permissions
        let claude_dir = worktree_path.join(".claude");
        fs::create_dir_all(&claude_dir).unwrap();
        let settings_path = claude_dir.join("settings.local.json");

        let existing_settings = json!({
            "permissions": {
                "allow": [
                    "Read(.work/signals/**)",  // This will also be generated by sandbox
                    "Read(custom/path/**)"
                ]
            }
        });
        fs::write(
            &settings_path,
            serde_json::to_string_pretty(&existing_settings).unwrap(),
        )
        .unwrap();

        // Call write_settings
        let config = MergedSandboxConfig {
            enabled: true,
            auto_allow: true,
            allow_unsandboxed_escape: false,
            excluded_commands: vec![],
            filesystem: FilesystemConfig::default(),
            network: NetworkConfig::default(),
            linux: LinuxConfig::default(),
        };

        write_settings(&config, worktree_path).unwrap();

        // Read the result
        let result_content = fs::read_to_string(&settings_path).unwrap();
        let result: Value = serde_json::from_str(&result_content).unwrap();

        let allow = result["permissions"]["allow"].as_array().unwrap();
        let allow_strs: Vec<String> = allow
            .iter()
            .filter_map(|v| v.as_str().map(String::from))
            .collect();

        // Count occurrences of the overlapping permission
        let signal_count = allow_strs
            .iter()
            .filter(|s| *s == "Read(.work/signals/**)")
            .count();
        assert_eq!(
            signal_count, 1,
            "Read(.work/signals/**) should appear exactly once"
        );

        // Verify custom permission is preserved
        assert!(allow_strs.contains(&"Read(custom/path/**)".to_string()));
    }

    #[test]
    fn test_write_settings_no_existing_file() {
        use tempfile::TempDir;

        let temp_dir = TempDir::new().unwrap();
        let worktree_path = temp_dir.path();

        // Call write_settings with no existing file
        let config = MergedSandboxConfig {
            enabled: true,
            auto_allow: true,
            allow_unsandboxed_escape: false,
            excluded_commands: vec![],
            filesystem: FilesystemConfig {
                deny_read: vec!["~/.ssh/**".to_string()],
                deny_write: vec![],
                allow_write: vec!["src/**".to_string()],
            },
            network: NetworkConfig::default(),
            linux: LinuxConfig::default(),
        };

        write_settings(&config, worktree_path).unwrap();

        // Read the result
        let settings_path = worktree_path.join(".claude/settings.local.json");
        let result_content = fs::read_to_string(&settings_path).unwrap();
        let result: Value = serde_json::from_str(&result_content).unwrap();

        // Verify expected permissions (same as before, no existing to merge)
        let allow = result["permissions"]["allow"].as_array().unwrap();
        let allow_strs: Vec<&str> = allow.iter().filter_map(|v| v.as_str()).collect();
        assert!(allow_strs.contains(&"Write(src/**)"));
        assert!(allow_strs.contains(&"Read(.work/signals/**)"));

        let deny = result["permissions"]["deny"].as_array().unwrap();
        let deny_strs: Vec<&str> = deny.iter().filter_map(|v| v.as_str()).collect();
        assert!(deny_strs.contains(&"Read(~/.ssh/**)"));
    }

    #[test]
    fn test_merge_existing_permissions_empty() {
        // Existing file has no permissions block
        let existing = json!({
            "sandbox": {
                "enabled": true
            }
        });

        let config = MergedSandboxConfig {
            enabled: true,
            auto_allow: true,
            allow_unsandboxed_escape: false,
            excluded_commands: vec![],
            filesystem: FilesystemConfig {
                allow_write: vec!["src/**".to_string()],
                deny_read: vec![],
                deny_write: vec![],
            },
            network: NetworkConfig::default(),
            linux: LinuxConfig::default(),
        };

        let mut new_settings = generate_settings_json(&config);
        let original_allow_count = new_settings["permissions"]["allow"]
            .as_array()
            .unwrap()
            .len();

        // Merge should be a no-op
        merge_existing_permissions(&mut new_settings, &existing);

        let after_allow_count = new_settings["permissions"]["allow"]
            .as_array()
            .unwrap()
            .len();
        assert_eq!(original_allow_count, after_allow_count);
    }
}
