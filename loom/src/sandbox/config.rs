use crate::plan::schema::{
    BackendType, FilesystemConfig, LinuxConfig, NetworkConfig, PermissionMode, SandboxConfig,
    StageSandboxConfig, StageType,
};
use anyhow::{bail, Result};
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
    /// Resolved Claude Code permission mode (stage > plan > stage-type default).
    pub permission_mode: PermissionMode,
}

/// Resolve the default `PermissionMode` for a stage type when no explicit
/// override is set at the plan or stage level.
///
/// `backend` is accepted for API symmetry with the rest of the sandbox
/// resolution path; the native backend is the only execution backend, so
/// the stage type alone determines the default.
///
/// Defaults:
/// - Knowledge / KnowledgeDistill → `AcceptEdits` — writes are scoped to
///   `doc/loom/knowledge/` and friction during knowledge curation hurts more
///   than it helps.
/// - Standard / IntegrationVerify → `Auto` — Claude's heuristics approve
///   safe edits while still prompting for destructive ones.
pub fn default_mode_for(stage_type: StageType, _backend: BackendType) -> PermissionMode {
    match stage_type {
        StageType::Knowledge | StageType::KnowledgeDistill => PermissionMode::AcceptEdits,
        StageType::Standard | StageType::IntegrationVerify => PermissionMode::Auto,
    }
}

/// Merge plan-level sandbox config with stage-level overrides.
///
/// Precedence for `permission_mode`: stage > plan > [`default_mode_for`].
/// `backend` is the resolved per-stage backend (after `resolve_stage_backend`)
/// — used only to compute the stage-type default. Explicit plan/stage values
/// take precedence and are passed through unchanged.
pub fn merge_config(
    plan_config: &SandboxConfig,
    stage_config: &StageSandboxConfig,
    stage_type: StageType,
    backend: BackendType,
) -> MergedSandboxConfig {
    let permission_mode = stage_config
        .permission_mode
        .or(plan_config.permission_mode)
        .unwrap_or_else(|| default_mode_for(stage_type, backend));

    MergedSandboxConfig {
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
        permission_mode,
    }
}

/// Validate that a merged sandbox config is safe for execution.
///
/// `bypass-permissions` is rejected unconditionally: it disables every Claude
/// Code permission prompt, granting the agent unrestricted access to the host
/// filesystem. No execution backend makes this safe, so it is refused
/// regardless of the resolved backend.
///
/// `backend` is accepted for API symmetry with the rest of the sandbox
/// resolution path; it does not affect the outcome.
pub fn validate_config(merged: &MergedSandboxConfig, _backend: BackendType) -> Result<()> {
    if merged.permission_mode == PermissionMode::BypassPermissions {
        bail!(
            "permission_mode=bypass-permissions is not permitted: it disables all \
             Claude Code permission prompts and grants unrestricted access to the \
             host filesystem. Choose a different permission_mode (auto, accept-edits, \
             plan, or default)."
        );
    }
    Ok(())
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
///
/// Only expands environment variables (`${VAR}`), NOT tildes (`~`).
/// Tilde paths are passed through to Claude Code's settings file as-is.
/// Claude Code's OS-level sandbox mangles absolute paths by prepending
/// the project root, so we must NOT expand `~` to absolute form here.
pub fn expand_paths(config: &mut MergedSandboxConfig) {
    // Only expand env vars — NOT tildes.
    // Claude Code handles ~ in permission patterns, and expanding tildes
    // causes the OS sandbox to create invalid paths like:
    //   /project/root/Users/user/.ssh (instead of /Users/user/.ssh)
    for path in &mut config.filesystem.deny_read {
        *path = expand_env_vars(path);
    }
    for path in &mut config.filesystem.deny_write {
        *path = expand_env_vars(path);
    }
    for path in &mut config.filesystem.allow_write {
        *path = expand_env_vars(path);
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
            permission_mode: None,
        };

        let stage = StageSandboxConfig {
            enabled: Some(false),
            auto_allow: None,
            allow_unsandboxed_escape: Some(true),
            excluded_commands: vec!["git".to_string()],
            filesystem: None,
            network: None,
            linux: None,
            permission_mode: None,
        };

        let merged = merge_config(&plan, &stage, StageType::Standard, BackendType::Native);

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
            permission_mode: Some(PermissionMode::Auto),
        };

        let stage = StageSandboxConfig::default();

        let merged = merge_config(&plan, &stage, StageType::Knowledge, BackendType::Native);

        // Knowledge stage should NOT have doc/loom/knowledge/** in allow_write
        // (knowledge stages use `loom knowledge update` CLI which runs outside sandbox)
        assert!(!merged
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
            permission_mode: Some(PermissionMode::Auto),
        };

        let stage = StageSandboxConfig::default();

        let merged = merge_config(
            &plan,
            &stage,
            StageType::IntegrationVerify,
            BackendType::Native,
        );

        // IntegrationVerify stage should NOT have doc/loom/knowledge/** in allow_write
        // (uses `loom knowledge update` CLI which runs outside sandbox)
        assert!(!merged
            .filesystem
            .allow_write
            .contains(&"doc/loom/knowledge/**".to_string()));
    }

    // =========================================================================
    // Permission Mode resolution tests
    // =========================================================================

    #[test]
    fn test_default_mode_for_stage_type_native() {
        assert_eq!(
            default_mode_for(StageType::Standard, BackendType::Native),
            PermissionMode::Auto
        );
        assert_eq!(
            default_mode_for(StageType::IntegrationVerify, BackendType::Native),
            PermissionMode::Auto
        );
        assert_eq!(
            default_mode_for(StageType::Knowledge, BackendType::Native),
            PermissionMode::AcceptEdits
        );
        assert_eq!(
            default_mode_for(StageType::KnowledgeDistill, BackendType::Native),
            PermissionMode::AcceptEdits
        );
    }

    #[test]
    fn test_merge_config_permission_mode_precedence() {
        let plan = SandboxConfig {
            permission_mode: Some(PermissionMode::Plan),
            ..SandboxConfig::default()
        };

        // Stage override beats plan override
        let stage_override = StageSandboxConfig {
            permission_mode: Some(PermissionMode::AcceptEdits),
            ..StageSandboxConfig::default()
        };
        let merged = merge_config(
            &plan,
            &stage_override,
            StageType::Standard,
            BackendType::Native,
        );
        assert_eq!(merged.permission_mode, PermissionMode::AcceptEdits);

        // No stage override: plan wins over default
        let merged = merge_config(
            &plan,
            &StageSandboxConfig::default(),
            StageType::Standard,
            BackendType::Native,
        );
        assert_eq!(merged.permission_mode, PermissionMode::Plan);

        // No plan / no stage override: stage type default
        let plan_default = SandboxConfig::default();
        let merged = merge_config(
            &plan_default,
            &StageSandboxConfig::default(),
            StageType::Standard,
            BackendType::Native,
        );
        assert_eq!(merged.permission_mode, PermissionMode::Auto);

        let merged = merge_config(
            &plan_default,
            &StageSandboxConfig::default(),
            StageType::Knowledge,
            BackendType::Native,
        );
        assert_eq!(merged.permission_mode, PermissionMode::AcceptEdits);
    }

    #[test]
    fn test_validate_config_rejects_bypass_permissions_unconditionally() {
        // Build a MergedSandboxConfig with a specific permission mode.
        let make = |mode: PermissionMode| MergedSandboxConfig {
            enabled: true,
            auto_allow: true,
            allow_unsandboxed_escape: false,
            excluded_commands: vec![],
            filesystem: FilesystemConfig::default(),
            network: NetworkConfig::default(),
            linux: LinuxConfig::default(),
            permission_mode: mode,
        };

        // Every non-bypass mode is accepted.
        for mode in [
            PermissionMode::Default,
            PermissionMode::AcceptEdits,
            PermissionMode::Auto,
            PermissionMode::Plan,
        ] {
            assert!(validate_config(&make(mode), BackendType::Native).is_ok());
        }

        // BypassPermissions is rejected unconditionally, regardless of backend.
        let bypass = make(PermissionMode::BypassPermissions);
        let err = validate_config(&bypass, BackendType::Native).unwrap_err();
        assert!(
            err.to_string().contains("bypass-permissions"),
            "error must name the rejected mode, got: {err}"
        );
        assert!(
            err.to_string().contains("not permitted"),
            "error must explain the mode is not permitted, got: {err}"
        );
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
            permission_mode: PermissionMode::Auto,
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
            BackendType::Native,
        );
        assert!(merged.enabled, "Merged config should have sandbox enabled");
    }
}
