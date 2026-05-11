//! `loom container doctor` — diagnose the container backend.

use anyhow::Result;
use colored::Colorize;
use std::fs;
use std::path::PathBuf;
use std::process::Command;

use crate::fs::work_dir::{self as wdapi, WorkDir};
use crate::orchestrator::terminal::container::{fingerprint as fp, image, runtime as rt};

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

    // Per finding #15: warn on rootless Podman capability constraints.
    if runtime == rt::Runtime::Podman {
        let rootless = Command::new("podman")
            .args(["info", "--format", "{{.Host.Security.Rootless}}"])
            .output()
            .ok()
            .and_then(|o| {
                if o.status.success() {
                    Some(String::from_utf8_lossy(&o.stdout).trim().to_string())
                } else {
                    None
                }
            });
        if rootless.as_deref() == Some("true") {
            println!(
                "  {} Rootless Podman detected — NET_ADMIN/NET_RAW are emulated. \
                 The firewall script may need slirp4netns >= 1.2.3 for iptables \
                 inside rootless containers; if it fails, switch to root Podman or Docker.",
                "⚠".yellow().bold()
            );
        }
    }
    if runtime == rt::Runtime::AppleContainer {
        println!(
            "  {} Apple Container has limited Linux capability emulation. \
             If the firewall script fails, fall back to Docker or rootful Podman.",
            "⚠".yellow().bold()
        );
    }

    // Current project fingerprint
    let work_dir = WorkDir::new(".")?;
    let project_root: PathBuf = work_dir
        .project_root()
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| PathBuf::from("."));
    let current_fp = fp::compute_fingerprint(&project_root, &[]);
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
    let inspect = Command::new(runtime.binary())
        .args(["image", "inspect", &format!("loom/base:{current_fp}")])
        .output();
    match inspect {
        Ok(o) if o.status.success() => {
            println!(
                "  {} Image loom/base:{} present",
                "✓".green().bold(),
                current_fp
            );
        }
        _ => {
            println!(
                "  {} Image loom/base:{} not present (run `loom container build`)",
                "✗".red().bold(),
                current_fp
            );
        }
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
