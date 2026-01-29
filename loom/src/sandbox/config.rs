use crate::plan::schema::{
    FilesystemConfig, LinuxConfig, NetworkConfig, SandboxConfig, StageSandboxConfig, StageType,
};
use std::env;
use std::path::Path;

/// Result of path traversal validation
#[derive(Debug, Clone, PartialEq)]
pub enum PathEscapeAttempt {
    /// Path is safe - no escape detected
    Safe,
    /// Path attempts to escape via parent directory traversal
    ParentEscape {
        path: String,
        normalized_pattern: String,
    },
    /// Path attempts to access other worktrees
    WorktreeAccess { path: String },
    /// Path uses absolute path that may escape worktree
    AbsoluteEscape { path: String },
}

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

/// Check if a path attempts to escape the worktree boundary
///
/// This function detects:
/// - Parent directory traversal patterns (../, ../../, etc.)
/// - Direct access to .worktrees directory
/// - Absolute paths that escape the worktree
///
/// Note: This is a static check on path patterns. The actual sandbox enforcement
/// is done by Claude Code's sandbox based on the deny/allow rules we generate.
pub fn detect_path_escape(path: &str) -> PathEscapeAttempt {
    let path_trimmed = path.trim();

    // Check for parent directory escape patterns
    if contains_parent_escape(path_trimmed) {
        // Determine the type of escape
        if path_trimmed.contains(".worktrees") {
            return PathEscapeAttempt::WorktreeAccess {
                path: path.to_string(),
            };
        }
        return PathEscapeAttempt::ParentEscape {
            path: path.to_string(),
            normalized_pattern: normalize_parent_escape(path_trimmed),
        };
    }

    // Check for absolute paths that might escape worktree
    // Only flag if path starts with / and is not a standard system path
    if path_trimmed.starts_with('/') {
        let path_obj = Path::new(path_trimmed);
        // Allow /tmp, /dev, /proc for legitimate use
        let allowed_prefixes = ["/tmp", "/dev", "/proc", "/sys"];
        if !allowed_prefixes.iter().any(|p| path_trimmed.starts_with(p)) {
            // Check if it's not a home directory (those are handled separately)
            if !path_trimmed.starts_with("/home/") && !path_trimmed.starts_with("/Users/") {
                // Check if this could be an escape (starts with user's cwd parent)
                if let Ok(cwd) = env::current_dir() {
                    if let Some(parent) = cwd.parent() {
                        if path_obj.starts_with(parent) && !path_obj.starts_with(&cwd) {
                            return PathEscapeAttempt::AbsoluteEscape {
                                path: path.to_string(),
                            };
                        }
                    }
                }
            }
        }
    }

    PathEscapeAttempt::Safe
}

/// Check if path contains parent directory escape patterns
fn contains_parent_escape(path: &str) -> bool {
    // Patterns that indicate escape attempts
    let escape_patterns = [
        "../..", // Two levels up
        "../",   // Check if starts with parent escape
        "/..",   // Parent in middle of path
        "..\\",  // Windows-style
        "\\..",  // Windows-style in middle
    ];

    for pattern in escape_patterns {
        if path.contains(pattern) {
            return true;
        }
    }

    // Also check if path starts with ..
    path.starts_with("..")
}

/// Normalize parent escape patterns for reporting
fn normalize_parent_escape(path: &str) -> String {
    let mut normalized = path.to_string();

    // Count how many levels up the path goes
    let mut levels = 0;
    let mut current = path;
    while current.starts_with("../") || current == ".." {
        levels += 1;
        current = if current.len() > 3 { &current[3..] } else { "" };
    }

    if levels > 0 {
        normalized = format!("{} levels up from worktree", levels);
    }

    normalized
}

/// Validate all paths in a sandbox config and return any escape attempts detected
pub fn validate_paths(config: &MergedSandboxConfig) -> Vec<PathEscapeAttempt> {
    let mut escapes = Vec::new();

    // Check allow_write paths - these are the most sensitive since they grant write access
    for path in &config.filesystem.allow_write {
        let result = detect_path_escape(path);
        if result != PathEscapeAttempt::Safe {
            escapes.push(result);
        }
    }

    // Note: deny_read and deny_write typically contain escape patterns intentionally
    // (to block them), so we don't validate those here

    escapes
}

/// Check if a path is a legitimate .work directory access via symlink
///
/// In worktrees, .work is a symlink to ../../.work (shared orchestration state).
/// Access to .work/ directly (not ../..work) is legitimate.
pub fn is_legitimate_work_access(path: &str) -> bool {
    let path_trimmed = path.trim();

    // Direct access to .work/ is fine - it's a symlink
    if path_trimmed.starts_with(".work/") || path_trimmed == ".work" {
        // But not if it's trying to escape through the symlink's target
        if !path_trimmed.contains("../") {
            return true;
        }
    }

    false
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

    // =========================================================================
    // Sandbox Hardening Tests
    // =========================================================================

    #[test]
    fn test_default_deny_read_contains_worktree_escape_patterns() {
        let config = FilesystemConfig::default();

        // Verify worktree escape patterns are in deny_read
        assert!(
            config.deny_read.contains(&"../../**".to_string()),
            "deny_read should contain ../../** to prevent parent escape"
        );
        assert!(
            config.deny_read.contains(&"../.worktrees/**".to_string()),
            "deny_read should contain ../.worktrees/** to prevent worktree access"
        );

        // Verify credential directories are still there
        assert!(config.deny_read.contains(&"~/.ssh/**".to_string()));
        assert!(config.deny_read.contains(&"~/.aws/**".to_string()));
    }

    #[test]
    fn test_default_deny_write_contains_worktree_escape_patterns() {
        let config = FilesystemConfig::default();

        // Verify worktree escape patterns are in deny_write
        assert!(
            config.deny_write.contains(&"../../**".to_string()),
            "deny_write should contain ../../** to prevent parent escape"
        );

        // Verify orchestration state protection
        assert!(
            config.deny_write.contains(&".work/stages/**".to_string()),
            "deny_write should protect .work/stages/**"
        );
        assert!(
            config.deny_write.contains(&".work/sessions/**".to_string()),
            "deny_write should protect .work/sessions/**"
        );
    }

    #[test]
    fn test_detect_path_escape_parent_traversal() {
        // Two levels up should be detected
        let result = detect_path_escape("../../some/path");
        assert!(
            matches!(result, PathEscapeAttempt::ParentEscape { .. }),
            "../../some/path should be detected as parent escape"
        );

        // Three levels up should be detected
        let result = detect_path_escape("../../../some/path");
        assert!(
            matches!(result, PathEscapeAttempt::ParentEscape { .. }),
            "../../../some/path should be detected as parent escape"
        );

        // One level up should be detected
        let result = detect_path_escape("../sibling");
        assert!(
            matches!(result, PathEscapeAttempt::ParentEscape { .. }),
            "../sibling should be detected as parent escape"
        );

        // Mid-path traversal should be detected
        let result = detect_path_escape("some/path/../../../escape");
        assert!(
            matches!(result, PathEscapeAttempt::ParentEscape { .. }),
            "Mid-path traversal should be detected"
        );
    }

    #[test]
    fn test_detect_path_escape_worktree_access() {
        // Direct worktree access attempt
        let result = detect_path_escape("../.worktrees/other-stage");
        assert!(
            matches!(result, PathEscapeAttempt::WorktreeAccess { .. }),
            "../.worktrees access should be detected"
        );

        // Nested worktree access
        let result = detect_path_escape("../../.worktrees/stage/src");
        assert!(
            matches!(result, PathEscapeAttempt::WorktreeAccess { .. }),
            "../../.worktrees access should be detected"
        );
    }

    #[test]
    fn test_detect_path_escape_safe_paths() {
        // Normal relative paths are safe
        assert_eq!(detect_path_escape("src/main.rs"), PathEscapeAttempt::Safe);
        assert_eq!(detect_path_escape("./src/main.rs"), PathEscapeAttempt::Safe);
        assert_eq!(detect_path_escape("tests/unit/"), PathEscapeAttempt::Safe);

        // System paths are safe
        assert_eq!(detect_path_escape("/tmp/cache"), PathEscapeAttempt::Safe);
        assert_eq!(detect_path_escape("/dev/null"), PathEscapeAttempt::Safe);
    }

    #[test]
    fn test_legitimate_work_access_via_symlink() {
        // Direct .work/ access is legitimate
        assert!(is_legitimate_work_access(".work/signals/session.md"));
        assert!(is_legitimate_work_access(".work/config.toml"));
        assert!(is_legitimate_work_access(".work"));

        // But not escape through .work
        assert!(!is_legitimate_work_access(".work/../../../escape"));

        // Not other paths
        assert!(!is_legitimate_work_access("src/main.rs"));
        assert!(!is_legitimate_work_access("../../.work"));
    }

    #[test]
    fn test_validate_paths_detects_escape_in_allow_write() {
        let config = MergedSandboxConfig {
            enabled: true,
            auto_allow: true,
            allow_unsandboxed_escape: false,
            excluded_commands: vec![],
            filesystem: FilesystemConfig {
                deny_read: vec![],
                deny_write: vec![],
                // Malicious: trying to allow writing to parent
                allow_write: vec!["../../malicious/**".to_string(), "src/**".to_string()],
            },
            network: NetworkConfig::default(),
            linux: LinuxConfig::default(),
        };

        let escapes = validate_paths(&config);

        // Should detect the escape attempt
        assert_eq!(escapes.len(), 1);
        assert!(matches!(
            &escapes[0],
            PathEscapeAttempt::ParentEscape { path, .. } if path == "../../malicious/**"
        ));
    }

    #[test]
    fn test_sandbox_enabled_by_default() {
        let config = SandboxConfig::default();
        assert!(config.enabled, "Sandbox should be enabled by default");

        let merged = merge_config(
            &SandboxConfig::default(),
            &StageSandboxConfig::default(),
            StageType::Standard,
        );
        assert!(merged.enabled, "Merged config should have sandbox enabled");
    }
}
