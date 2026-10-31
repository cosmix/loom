use crate::fs::work_dir::WorkDir;
use crate::parser::markdown::parse_frontmatter_strings;
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
        if !dir.exists() {
            continue;
        }

        for entry in fs::read_dir(&dir)? {
            let entry = entry?;
            let path = entry.path();

            if path.is_file() && path.extension().is_some_and(|e| e == "md") {
                if let Ok(content) = fs::read_to_string(&path) {
                    if parse_frontmatter_strings(&content).is_err() {
                        let file_name = path.file_name().ok_or_else(|| {
                            anyhow::anyhow!("Path has no file name: {}", path.display())
                        })?;
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
        }
    }

    Ok(issues)
}

// Runner and track diagnostics removed - these concepts no longer exist
