use crate::plan::schema::{
    FilesystemConfig, LinuxConfig, NetworkConfig, SandboxConfig, StageSandboxConfig, StageType,
};
use std::env;

/// Merged sandbox configuration for a specific stage
/// This is the final resolved config after merging plan-level defaults with stage overrides
#[derive(Debug, Clone)]
pub struct MergedSandboxConfig {
    pub enabled: bool,
    pub auto_allow: bool,
    pub allow_unsandboxed_escape: bool,
    pub excluded_commands: Vec<String>,
    pub filesystem: FilesystemConfig,
    pub network: NetworkConfig,
    pub linux: LinuxConfig,
}

/// Merge plan-level sandbox config with stage-level overrides
/// Stage values override plan values when present
pub fn merge_config(
    plan_config: &SandboxConfig,
    stage_config: &StageSandboxConfig,
    stage_type: StageType,
) -> MergedSandboxConfig {
    // Start with plan-level config
    let mut merged = MergedSandboxConfig {
        enabled: stage_config.enabled.unwrap_or(plan_config.enabled),
        auto_allow: stage_config.auto_allow.unwrap_or(plan_config.auto_allow),
        allow_unsandboxed_escape: stage_config
            .allow_unsandboxed_escape
            .unwrap_or(plan_config.allow_unsandboxed_escape),
        excluded_commands: {
            let mut commands = plan_config.excluded_commands.clone();
            commands.extend(stage_config.excluded_commands.clone());
            commands
        },
        filesystem: stage_config
            .filesystem
            .clone()
            .unwrap_or_else(|| plan_config.filesystem.clone()),
        network: stage_config
            .network
            .clone()
            .unwrap_or_else(|| plan_config.network.clone()),
        linux: stage_config
            .linux
            .clone()
            .unwrap_or_else(|| plan_config.linux.clone()),
    };

    // Special handling for Knowledge and IntegrationVerify stages
    // These stages need to write to doc/loom/knowledge/**
    if matches!(
        stage_type,
        StageType::Knowledge | StageType::IntegrationVerify
    ) {
        let knowledge_path = "doc/loom/knowledge/**".to_string();
        if !merged.filesystem.allow_write.contains(&knowledge_path) {
            merged.filesystem.allow_write.push(knowledge_path);
        }
    }

    merged
}

/// Expand ~ to home directory in paths
pub fn expand_tilde(path: &str) -> String {
    if path.starts_with("~/") {
        if let Ok(home) = env::var("HOME") {
            return path.replacen("~", &home, 1);
        }
    }
    path.to_string()
}

/// Expand ${ENV_VAR} patterns in strings
pub fn expand_env_vars(s: &str) -> String {
    // Use regex to find ${VAR} or $VAR patterns
    let re = regex::Regex::new(r"\$\{([A-Za-z_][A-Za-z0-9_]*)\}|\$([A-Za-z_][A-Za-z0-9_]*)")
        .expect("Invalid regex pattern");

    re.replace_all(s, |caps: &regex::Captures| {
        // ${VAR} form - group 1
        if let Some(var_name) = caps.get(1) {
            if let Ok(value) = env::var(var_name.as_str()) {
                return value;
            }
        }
        // $VAR form - group 2
        else if let Some(var_name) = caps.get(2) {
            if let Ok(value) = env::var(var_name.as_str()) {
                return value;
            }
        }
        // If variable not found, keep original
        caps.get(0).unwrap().as_str().to_string()
    })
    .to_string()
}

/// Expand all paths in the config
pub fn expand_paths(config: &mut MergedSandboxConfig) {
    // Expand filesystem paths
    for path in &mut config.filesystem.deny_read {
        *path = expand_env_vars(&expand_tilde(path));
    }
    for path in &mut config.filesystem.deny_write {
        *path = expand_env_vars(&expand_tilde(path));
    }
    for path in &mut config.filesystem.allow_write {
        *path = expand_env_vars(&expand_tilde(path));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_expand_tilde() {
        let home = env::var("HOME").unwrap();
        assert_eq!(expand_tilde("~/test"), format!("{}/test", home));
        assert_eq!(expand_tilde("/absolute/path"), "/absolute/path");
        assert_eq!(expand_tilde("relative/path"), "relative/path");
    }

    #[test]
    fn test_expand_env_vars() {
        env::set_var("TEST_VAR", "test_value");
        assert_eq!(expand_env_vars("${TEST_VAR}/path"), "test_value/path");
        assert_eq!(expand_env_vars("$TEST_VAR/path"), "test_value/path");
        assert_eq!(
            expand_env_vars("prefix/${TEST_VAR}/suffix"),
            "prefix/test_value/suffix"
        );
        assert_eq!(expand_env_vars("no_vars_here"), "no_vars_here");
        env::remove_var("TEST_VAR");
    }

    #[test]
    fn test_expand_env_vars_overlapping_names() {
        env::set_var("TEST_HOME", "/home");
        env::set_var("TEST_HOME_DIR", "/home/user/dir");

        // The longer variable name should NOT have its prefix replaced
        let result = expand_env_vars("$TEST_HOME_DIR/foo");
        assert_eq!(result, "/home/user/dir/foo");

        // Both should work independently
        let result = expand_env_vars("$TEST_HOME and $TEST_HOME_DIR");
        assert_eq!(result, "/home and /home/user/dir");

        // Test with ${} form as well
        let result = expand_env_vars("${TEST_HOME_DIR}/foo");
        assert_eq!(result, "/home/user/dir/foo");

        let result = expand_env_vars("${TEST_HOME} and ${TEST_HOME_DIR}");
        assert_eq!(result, "/home and /home/user/dir");

        // Clean up
        env::remove_var("TEST_HOME");
        env::remove_var("TEST_HOME_DIR");
    }

    #[test]
    fn test_expand_env_vars_undefined() {
        // Make sure these variables are not defined
        env::remove_var("UNDEFINED_VAR_TEST");
        env::remove_var("ALSO_UNDEFINED");

        // Undefined variables should be preserved as-is
        let result = expand_env_vars("$UNDEFINED_VAR_TEST/path");
        assert_eq!(result, "$UNDEFINED_VAR_TEST/path");

        let result = expand_env_vars("${UNDEFINED_VAR_TEST}/path");
        assert_eq!(result, "${UNDEFINED_VAR_TEST}/path");

        // Mix of defined and undefined
        env::set_var("DEFINED_VAR", "value");
        let result = expand_env_vars("$DEFINED_VAR/$ALSO_UNDEFINED");
        assert_eq!(result, "value/$ALSO_UNDEFINED");

        env::remove_var("DEFINED_VAR");
    }

    #[test]
    fn test_merge_config_stage_overrides() {
        let plan = SandboxConfig {
            enabled: true,
            auto_allow: true,
            allow_unsandboxed_escape: false,
            excluded_commands: vec!["loom".to_string()],
            filesystem: FilesystemConfig::default(),
            network: NetworkConfig::default(),
            linux: LinuxConfig::default(),
        };

        let stage = StageSandboxConfig {
            enabled: Some(false),
            auto_allow: None,
            allow_unsandboxed_escape: Some(true),
            excluded_commands: vec!["git".to_string()],
            filesystem: None,
            network: None,
            linux: None,
        };

        let merged = merge_config(&plan, &stage, StageType::Standard);

        assert!(!merged.enabled); // Overridden
        assert!(merged.auto_allow); // From plan
        assert!(merged.allow_unsandboxed_escape); // Overridden
        assert_eq!(merged.excluded_commands, vec!["loom", "git"]); // Merged
    }

    #[test]
    fn test_merge_config_knowledge_stage() {
        let plan = SandboxConfig {
            enabled: true,
            auto_allow: true,
            allow_unsandboxed_escape: false,
            excluded_commands: vec!["loom".to_string()],
            filesystem: FilesystemConfig {
                deny_read: vec![],
                deny_write: vec!["doc/loom/knowledge/**".to_string()],
                allow_write: vec![],
            },
            network: NetworkConfig::default(),
            linux: LinuxConfig::default(),
        };

        let stage = StageSandboxConfig::default();

        let merged = merge_config(&plan, &stage, StageType::Knowledge);

        // Knowledge stage should have doc/loom/knowledge/** in allow_write
        assert!(merged
            .filesystem
            .allow_write
            .contains(&"doc/loom/knowledge/**".to_string()));
    }

    #[test]
    fn test_merge_config_integration_verify_stage() {
        let plan = SandboxConfig {
            enabled: true,
            auto_allow: true,
            allow_unsandboxed_escape: false,
            excluded_commands: vec!["loom".to_string()],
            filesystem: FilesystemConfig::default(),
            network: NetworkConfig::default(),
            linux: LinuxConfig::default(),
        };

        let stage = StageSandboxConfig::default();

        let merged = merge_config(&plan, &stage, StageType::IntegrationVerify);

        // IntegrationVerify stage should have doc/loom/knowledge/** in allow_write
        assert!(merged
            .filesystem
            .allow_write
            .contains(&"doc/loom/knowledge/**".to_string()));
    }
}
