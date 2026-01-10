use anyhow::{bail, Context, Result};
use std::fs;
use std::path::PathBuf;

use crate::fs::work_dir::WorkDir;
use crate::models::runner::Runner;

use super::serialization::runner_from_markdown;

/// Generate a unique runner ID based on type and existing runners
pub fn generate_runner_id(work_dir: &WorkDir, runner_type: &str) -> Result<String> {
    let prefix = role_prefix(runner_type);
    let runners_dir = work_dir.runners_dir();

    let entries = fs::read_dir(&runners_dir).with_context(|| "Failed to read runners directory")?;

    let mut max_num = 0;
    for entry in entries {
        let entry = entry?;
        let path = entry.path();

        if let Some(filename) = path.file_stem().and_then(|s| s.to_str()) {
            if filename.starts_with(prefix) {
                if let Some(num_str) = filename
                    .strip_prefix(prefix)
                    .and_then(|s| s.strip_prefix('-'))
                {
                    if let Ok(num) = num_str.parse::<u32>() {
                        max_num = max_num.max(num);
                    }
                }
            }
        }
    }

    Ok(format!("{}-{:03}", prefix, max_num + 1))
}

/// Get the ID prefix for a runner type
pub fn role_prefix(runner_type: &str) -> &'static str {
    match runner_type {
        "software-engineer" => "se",
        "senior-software-engineer" => "sse",
        "tech-lead" => "tl",
        "architect" => "arch",
        "devops" => "do",
        "qa" => "qa",
        "product-manager" => "pm",
        "designer" => "des",
        _ => "runner",
    }
}

/// Load a runner from a file path
pub fn load_runner(path: &PathBuf) -> Result<Runner> {
    let filename = path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("runner file");

    let content = fs::read_to_string(path)
        .with_context(|| format!("Failed to read runner file: {filename}"))?;

    runner_from_markdown(&content)
}

/// Find a runner file by ID
pub fn find_runner_file(work_dir: &WorkDir, id: &str) -> Result<PathBuf> {
    let runner_path = work_dir.runners_dir().join(format!("{id}.md"));

    if !runner_path.exists() {
        bail!("Runner '{id}' does not exist");
    }

    Ok(runner_path)
}

/// Truncate a string to a maximum length with ellipsis
/// Uses character count instead of byte count to ensure UTF-8 safety
pub fn truncate(s: &str, max_chars: usize) -> String {
    if s.chars().count() <= max_chars {
        s.to_string()
    } else {
        let truncated: String = s.chars().take(max_chars.saturating_sub(3)).collect();
        format!("{truncated}...")
    }
}
