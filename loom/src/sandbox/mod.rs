//! Sandbox configuration and settings generation
//!
//! This module handles merging plan-level and stage-level sandbox configs,
//! and generating Claude Code settings files.

mod config;
mod settings;

pub use config::{
    default_mode_for, detect_path_escape, expand_env_vars, expand_paths, expand_tilde,
    is_legitimate_work_access, merge_config, validate_config, validate_paths, MergedSandboxConfig,
    PathEscapeAttempt,
};
pub use settings::{apply_default_mode, generate_settings_json, write_settings};
