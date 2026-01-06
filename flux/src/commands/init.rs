use crate::fs::work_dir::WorkDir;
use anyhow::{Context, Result};
use std::fs;
use std::path::PathBuf;

/// Initialize the .work/ directory structure
pub fn execute(plan_path: Option<PathBuf>) -> Result<()> {
    let work_dir = WorkDir::new(".")?;
    work_dir.initialize()?;

    if let Some(path) = plan_path {
        initialize_with_plan(&work_dir, &path)?;
        println!(
            "Initialized .work/ directory structure with plan from {}",
            path.display()
        );
    } else {
        println!("Initialized .work/ directory structure");
    }

    Ok(())
}

/// Initialize with a plan file
fn initialize_with_plan(work_dir: &WorkDir, plan_path: &std::path::Path) -> Result<()> {
    // Validate plan file exists
    if !plan_path.exists() {
        anyhow::bail!("Plan file does not exist: {}", plan_path.display());
    }

    // Create config.toml to track the active plan
    let config_content = format!(
        "# Flux Configuration\n# Generated from plan: {}\n\n[plan]\nsource_path = \"{}\"\n",
        plan_path.display(),
        plan_path.display()
    );

    let config_path = work_dir.root().join("config.toml");
    fs::write(&config_path, config_content).context("Failed to write config.toml")?;

    // Note: Actual plan parsing and stage creation will be implemented in Phase 3
    // For now, we just create the config file to indicate a plan is active

    Ok(())
}
