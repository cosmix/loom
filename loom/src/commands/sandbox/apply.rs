use anyhow::{Context, Result};
use std::env;

use crate::plan::schema::{SandboxConfig, StageSandboxConfig, StageType};
use crate::sandbox::{expand_paths, merge_config, write_settings};

/// Apply sandbox settings to .claude/settings.local.json for the current project
pub fn execute() -> Result<()> {
    let current_dir = env::current_dir().context("Failed to get current directory")?;

    let config = SandboxConfig::default();
    let stage_config = StageSandboxConfig::default();
    let mut merged = merge_config(&config, &stage_config, StageType::Standard);
    expand_paths(&mut merged);

    write_settings(&merged, &current_dir)?;

    println!(
        "Sandbox settings written to {}",
        current_dir
            .join(".claude/settings.local.json")
            .display()
    );

    Ok(())
}
