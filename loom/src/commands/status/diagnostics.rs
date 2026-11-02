use super::validation::scan_markdown;
use crate::fs::work_dir::WorkDir;
use anyhow::Result;
use colored::Colorize;
use std::fs;

pub fn check_directory_structure(work_dir: &WorkDir) -> Result<usize> {
    let mut issues = 0;
    let required_dirs = vec![
        ("signals", work_dir.signals_dir()),
        ("handoffs", work_dir.handoffs_dir()),
        ("archive", work_dir.archive_dir()),
    ];

    for (name, path) in required_dirs {
        if !path.exists() {
            // Auto-create missing directories
            if let Err(e) = fs::create_dir_all(&path) {
                println!(
                    "{} Failed to create missing directory {}: {}",
                    "ERROR:".red().bold(),
                    name,
                    e
                );
                issues += 1;
            }
        }
    }

    Ok(issues)
}

pub fn check_parsing_errors(work_dir: &WorkDir) -> Result<usize> {
    let mut issues = 0;
    let dirs = vec![
        ("signals", work_dir.signals_dir()),
        ("handoffs", work_dir.handoffs_dir()),
    ];

    for (entity_type, dir) in dirs {
        for (path, parsed) in scan_markdown(&dir)? {
            if parsed.is_err() {
                let file_name = path
                    .file_name()
                    .ok_or_else(|| anyhow::anyhow!("Path has no file name: {}", path.display()))?;
                println!(
                    "{} Invalid {} file: {:?}",
                    "WARNING:".yellow().bold(),
                    entity_type,
                    file_name
                );
                println!(
                    "  {} Check frontmatter and markdown syntax",
                    "Fix:".yellow()
                );
                issues += 1;
            }
        }
    }

    Ok(issues)
}
