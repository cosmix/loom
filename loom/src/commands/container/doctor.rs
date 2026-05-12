//! `loom container doctor` — diagnose the container backend.

use anyhow::Result;
use colored::Colorize;
use std::fs;
use std::path::PathBuf;
use std::process::Command;

use crate::fs::work_dir::{self as wdapi, WorkDir};
use crate::orchestrator::terminal::container::{fingerprint as fp, image, probe, runtime as rt};

pub fn execute() -> Result<()> {
    println!("{}", "Container backend doctor".bold());
    println!("{}", "─".repeat(40).dimmed());

    let runtime = match rt::detect_runtime("auto") {
        Ok(r) => {
            println!("  {} Runtime: {}", "✓".green().bold(), r);
            r
        }
        Err(e) => {
            println!("  {} Runtime: {}", "✗".red().bold(), e);
            return Ok(());
        }
    };

    // Runtime version
    if let Ok(out) = Command::new(runtime.binary()).arg("--version").output() {
        let ver = String::from_utf8_lossy(&out.stdout);
        println!("  {} Version: {}", "ℹ".cyan(), ver.trim());
    }

    // Current project fingerprint
    let work_dir = WorkDir::new(".")?;
    let project_root: PathBuf = work_dir
        .project_root()
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| PathBuf::from("."));
    let working_dirs = fp::plan_working_dirs(work_dir.root());
    let current_fp = fp::compute_fingerprint(&project_root, &working_dirs);
    println!("  {} Current fingerprint: {}", "ℹ".cyan(), current_fp);

    // Pinned digest (if any)
    if let Ok(Some(project)) = wdapi::read_project_execution(work_dir.root()) {
        println!("  {} Configured backend: {}", "ℹ".cyan(), project.backend);
        if let Some(c) = project.container.as_ref() {
            println!("  {} Pinned digest:       {}", "ℹ".cyan(), c.image_digest);
            println!("  {} Pinned fingerprint:  {}", "ℹ".cyan(), c.fingerprint);
        }
    } else {
        println!(
            "  {} No [project_execution] in .work/config.toml",
            "ℹ".dimmed()
        );
    }

    // Image presence
    let image_ref = format!("loom/base:{current_fp}");
    let inspect = Command::new(runtime.binary())
        .args(["image", "inspect", &image_ref])
        .output();
    let image_present = matches!(&inspect, Ok(o) if o.status.success());
    if image_present {
        println!(
            "  {} Image loom/base:{} present",
            "✓".green().bold(),
            current_fp
        );
    } else {
        println!(
            "  {} Image loom/base:{} not present (run `loom container build`)",
            "✗".red().bold(),
            current_fp
        );
    }

    // Firewall enforcement probe. Replaces the earlier
    // rootless-Podman / Apple-Container blanket warnings with an
    // authoritative smoke test against the actual runtime + image.
    // `doctor` itself stays exit 0 — the report differentiates
    // "warn" (probe ran but firewall did not block) from "fail"
    // (probe could not run at all).
    if image_present {
        match probe::run_firewall_smoke_test(runtime, &image_ref) {
            Ok(result) if result.enforced => {
                println!(
                    "  {} Firewall enforcement: blocked outbound",
                    "✓".green().bold()
                );
            }
            Ok(result) => {
                println!(
                    "  {} Firewall enforcement: NOT enforcing on this runtime. \
                     Container egress is not filtered — pass \
                     --allow-insecure-runtime to `loom init` to acknowledge.\n      {}",
                    "✗".red().bold(),
                    result.diagnostic.lines().next().unwrap_or("").dimmed()
                );
            }
            Err(e) => {
                println!(
                    "  {} Firewall enforcement: probe failed to run: {}",
                    "⚠".yellow().bold(),
                    e
                );
            }
        }
    } else {
        println!(
            "  {} Firewall enforcement: skipped (image not present)",
            "ℹ".dimmed()
        );
    }

    // Cache size + entries
    let cache = image::cache_dir()?;
    if cache.exists() {
        let mut total: u64 = 0;
        let mut fps: Vec<String> = Vec::new();
        for e in fs::read_dir(&cache).into_iter().flatten().flatten() {
            if e.file_type().map(|t| t.is_dir()).unwrap_or(false) {
                fps.push(e.file_name().to_string_lossy().to_string());
            }
            if let Ok(meta) = e.metadata() {
                total += meta.len();
            }
        }
        println!("  {} Cache dir: {}", "ℹ".cyan(), cache.display());
        println!("    Size: {} bytes", total);
        println!("    Cached fingerprints: {}", fps.len());
        for f in fps {
            println!("      - {}", f);
        }
    } else {
        println!(
            "  {} Cache dir not yet created at {}",
            "ℹ".dimmed(),
            cache.display()
        );
    }

    Ok(())
}
