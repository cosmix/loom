//! `loom container build` — build the image for the current project's fingerprint.

use anyhow::{Context, Result};
use colored::Colorize;
use std::path::PathBuf;

use crate::fs::work_dir::WorkDir;
use crate::orchestrator::terminal::container::{fingerprint as fp, image, runtime as rt};

pub fn execute(fingerprint_override: Option<String>) -> Result<()> {
    let work_dir = WorkDir::new(".")?;
    let project_root: PathBuf = work_dir
        .project_root()
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| PathBuf::from("."));

    let runtime = rt::detect_runtime("auto").context("Container runtime detection failed")?;

    let fingerprint = match fingerprint_override {
        Some(f) => f,
        None => fp::compute_fingerprint(&project_root, &[]),
    };

    println!("{} Building image", "→".cyan().bold());
    println!("  Runtime:     {}", runtime);
    println!("  Fingerprint: {}", fingerprint);

    let digest = image::ensure_image(&fingerprint, runtime, false)
        .with_context(|| format!("Failed to ensure image for fingerprint {fingerprint}"))?;

    println!("{} Image ready", "✓".green().bold());
    println!("  Digest: {}", digest.dimmed());
    Ok(())
}
