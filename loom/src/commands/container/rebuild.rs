//! `loom container rebuild` — rebuild image(s), bypassing the cache.

use anyhow::{Context, Result};
use colored::Colorize;
use std::fs;
use std::path::PathBuf;

use crate::fs::work_dir::WorkDir;
use crate::orchestrator::terminal::container::{fingerprint as fp, image, runtime as rt};

pub fn execute(fingerprint_override: Option<String>, all: bool) -> Result<()> {
    let runtime = rt::detect_runtime("auto").context("Container runtime detection failed")?;

    if all {
        let cache_root = image::cache_dir()?;
        if !cache_root.exists() {
            println!(
                "{} No cached images at {}",
                "→".dimmed(),
                cache_root.display()
            );
            return Ok(());
        }
        let entries = fs::read_dir(&cache_root)
            .with_context(|| format!("Failed to read cache dir {}", cache_root.display()))?;
        let mut count = 0usize;
        for e in entries.flatten() {
            if e.file_type().map(|t| t.is_dir()).unwrap_or(false) {
                let fp_str = e.file_name().to_string_lossy().to_string();
                println!("{} Rebuilding {}", "→".cyan().bold(), fp_str);
                let _ = image::ensure_image(&fp_str, runtime, true)
                    .with_context(|| format!("Rebuild failed for {fp_str}"))?;
                count += 1;
            }
        }
        println!(
            "{} Rebuilt {count} cached fingerprint(s)",
            "✓".green().bold()
        );
        return Ok(());
    }

    let work_dir = WorkDir::new(".")?;
    let project_root: PathBuf = work_dir
        .project_root()
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| PathBuf::from("."));
    let fingerprint = fingerprint_override.unwrap_or_else(|| {
        let working_dirs = fp::plan_working_dirs(work_dir.root());
        fp::compute_fingerprint(&project_root, &working_dirs)
    });

    println!("{} Rebuilding image", "→".cyan().bold());
    println!("  Fingerprint: {}", fingerprint);
    let digest = image::ensure_image(&fingerprint, runtime, true)?;
    println!("{} Image rebuilt", "✓".green().bold());
    println!("  Digest: {}", digest.dimmed());
    Ok(())
}
