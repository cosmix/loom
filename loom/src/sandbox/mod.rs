//! Sandbox configuration and settings generation
//!
//! This module handles merging plan-level and stage-level sandbox configs,
//! and generating Claude Code settings files.

mod config;
pub(crate) mod control_surfaces;
mod grant_paths;
mod package_caches;
mod settings;

pub(crate) use config::preflight;
pub use config::{
    default_mode_for, detect_path_escape, expand_env_vars, expand_paths, expand_tilde,
    is_legitimate_work_access, merge_config, validate_config, validate_paths, MergedSandboxConfig,
    PathEscapeAttempt, KNOWLEDGE_WRITE_GLOB,
};
pub use grant_paths::{missing_grant_paths, warn_missing_grants};
pub use package_caches::PACKAGE_MANAGER_CACHE_WRITE_PATHS;
pub use settings::{apply_default_mode, generate_settings_json};
pub(crate) use settings::{build_settings, target_is_worktree, validate_emittable, SettingsTarget};
