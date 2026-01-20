//! Plan configuration operations
//!
//! This module handles reading and writing plan source path configuration
//! in config.toml.

use anyhow::{Context, Result};
use std::fs;
use std::path::{Path, PathBuf};

use crate::fs::work_dir::WorkDir;

/// Get the plan source path from config.toml
pub fn get_plan_source_path(work_dir: &WorkDir) -> Result<Option<PathBuf>> {
    let config_path = work_dir.root().join("config.toml");

    if !config_path.exists() {
        return Ok(None);
    }

    let config_content =
        std::fs::read_to_string(&config_path).context("Failed to read config.toml")?;

    let config: toml::Value =
        toml::from_str(&config_content).context("Failed to parse config.toml")?;

    let source_path = config
        .get("plan")
        .and_then(|p| p.get("source_path"))
        .and_then(|s| s.as_str())
        .map(PathBuf::from);

    Ok(source_path)
}

/// Update the plan source path in config.toml
pub fn update_plan_source_path(work_dir: &WorkDir, new_path: &Path) -> Result<()> {
    let config_path = work_dir.root().join("config.toml");

    let config_content =
        std::fs::read_to_string(&config_path).context("Failed to read config.toml")?;

    let mut config: toml::Value =
        toml::from_str(&config_content).context("Failed to parse config.toml")?;

    if let Some(plan) = config.get_mut("plan") {
        if let Some(table) = plan.as_table_mut() {
            table.insert(
                "source_path".to_string(),
                toml::Value::String(new_path.display().to_string()),
            );
        }
    }

    // Serialize back to TOML with proper formatting
    let new_content = toml::to_string_pretty(&config).context("Failed to serialize config")?;
    fs::write(&config_path, new_content).context("Failed to write config.toml")?;

    Ok(())
}
