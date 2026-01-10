use crate::fs::work_dir::WorkDir;
use crate::parser::markdown::MarkdownDocument;
use anyhow::Result;
use colored::Colorize;
use std::collections::HashSet;
use std::fs;

pub fn check_directory_structure(work_dir: &WorkDir) -> Result<usize> {
    let mut issues = 0;
    let required_dirs = vec![
        ("runners", work_dir.runners_dir()),
        ("tracks", work_dir.tracks_dir()),
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
        ("runners", work_dir.runners_dir()),
        ("tracks", work_dir.tracks_dir()),
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
                    if MarkdownDocument::parse(&content).is_err() {
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

pub fn check_stuck_runners(work_dir: &WorkDir) -> Result<usize> {
    let mut issues = 0;
    let runners_dir = work_dir.runners_dir();

    if !runners_dir.exists() {
        return Ok(0);
    }

    let signal_targets = collect_signal_targets(work_dir)?;

    for entry in fs::read_dir(&runners_dir)? {
        let entry = entry?;
        let path = entry.path();

        if path.is_file() && path.extension().is_some_and(|e| e == "md") {
            if let Ok(content) = fs::read_to_string(&path) {
                if let Ok(doc) = MarkdownDocument::parse(&content) {
                    let status = doc.get_frontmatter("status");
                    let runner_id = doc.get_frontmatter("id");

                    if let (Some(status), Some(id)) = (status, runner_id) {
                        if status == "Active" && !signal_targets.contains(id) {
                            println!(
                                "{} Runner '{}' is Active but has no signals",
                                "INFO:".cyan().bold(),
                                doc.get_frontmatter("name").unwrap_or(&id.clone())
                            );
                            println!("  {} May be stuck or waiting for input", "Note:".cyan());
                            issues += 1;
                        }
                    }
                }
            }
        }
    }

    Ok(issues)
}

fn collect_signal_targets(work_dir: &WorkDir) -> Result<HashSet<String>> {
    let mut targets = HashSet::new();
    let signals_dir = work_dir.signals_dir();

    if !signals_dir.exists() {
        return Ok(targets);
    }

    for entry in fs::read_dir(&signals_dir)? {
        let entry = entry?;
        let path = entry.path();

        if path.is_file() && path.extension().is_some_and(|e| e == "md") {
            if let Ok(content) = fs::read_to_string(&path) {
                if let Ok(doc) = MarkdownDocument::parse(&content) {
                    if let Some(target) = doc.get_frontmatter("target_runner") {
                        targets.insert(target.clone());
                    }
                }
            }
        }
    }

    Ok(targets)
}

pub fn check_orphaned_tracks(work_dir: &WorkDir) -> Result<usize> {
    let mut issues = 0;
    let tracks_dir = work_dir.tracks_dir();

    if !tracks_dir.exists() {
        return Ok(0);
    }

    for entry in fs::read_dir(&tracks_dir)? {
        let entry = entry?;
        let path = entry.path();

        if path.is_file() && path.extension().is_some_and(|e| e == "md") {
            if let Ok(content) = fs::read_to_string(&path) {
                if let Ok(doc) = MarkdownDocument::parse(&content) {
                    let assigned_runner = doc.get_frontmatter("assigned_runner");
                    let status = doc.get_frontmatter("status");

                    if let Some(status) = status {
                        if status == "Active" && assigned_runner.is_none() {
                            println!(
                                "{} Track '{}' is Active but has no runner",
                                "INFO:".cyan().bold(),
                                doc.get_frontmatter("name")
                                    .unwrap_or(&"Unknown".to_string())
                            );
                            println!(
                                "  {} Consider assigning a runner or closing",
                                "Suggestion:".cyan()
                            );
                            issues += 1;
                        }
                    }
                }
            }
        }
    }

    Ok(issues)
}
