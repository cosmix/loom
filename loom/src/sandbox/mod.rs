//! Sandbox configuration and settings generation
//!
//! This module handles merging plan-level and stage-level sandbox configs,
//! and generating Claude Code settings files.

mod config;
mod settings;

pub use config::{expand_env_vars, expand_paths, expand_tilde, merge_config, MergedSandboxConfig};
pub use settings::{generate_settings_json, write_settings};
