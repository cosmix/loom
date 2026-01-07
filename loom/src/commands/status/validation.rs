use crate::fs::work_dir::WorkDir;
use crate::parser::markdown::MarkdownDocument;
use anyhow::{Context, Result};
use colored::Colorize;
use std::collections::HashSet;
use std::fs;

pub fn validate_markdown_files(dir: &std::path::Path, entity_type: &str) -> Result<usize> {
    let mut issues = 0;

    if !dir.exists() {
        return Ok(0);
    }

    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();

        if path.is_file() && path.extension().is_some_and(|e| e == "md") {
            let content =
                fs::read_to_string(&path).with_context(|| format!("Failed to read {path:?}"))?;

            if let Err(e) = MarkdownDocument::parse(&content) {
                let file_name = path
                    .file_name()
                    .ok_or_else(|| anyhow::anyhow!("Path has no file name: {}", path.display()))?;
                println!(
                    "{} Failed to parse {} file: {:?}",
                    "ERROR:".red().bold(),
                    entity_type,
                    file_name
                );
                println!("  {e}");
                issues += 1;
            }
        }
    }

    Ok(issues)
}

pub fn validate_references(work_dir: &WorkDir) -> Result<usize> {
    let mut issues = 0;

    let track_ids = collect_track_ids(work_dir)?;
    let runner_ids = collect_runner_ids(work_dir)?;

    issues += validate_runner_track_refs(work_dir, &track_ids)?;
    issues += validate_signal_runner_refs(work_dir, &runner_ids)?;

    Ok(issues)
}

fn collect_track_ids(work_dir: &WorkDir) -> Result<HashSet<String>> {
    let mut ids = HashSet::new();
    let tracks_dir = work_dir.tracks_dir();

    if !tracks_dir.exists() {
        return Ok(ids);
    }

    for entry in fs::read_dir(&tracks_dir)? {
        let entry = entry?;
        let path = entry.path();

        if path.is_file() && path.extension().is_some_and(|e| e == "md") {
            if let Ok(content) = fs::read_to_string(&path) {
                if let Ok(doc) = MarkdownDocument::parse(&content) {
                    if let Some(id) = doc.get_frontmatter("id") {
                        ids.insert(id.clone());
                    }
                }
            }
        }
    }

    Ok(ids)
}

fn collect_runner_ids(work_dir: &WorkDir) -> Result<HashSet<String>> {
    let mut ids = HashSet::new();
    let runners_dir = work_dir.runners_dir();

    if !runners_dir.exists() {
        return Ok(ids);
    }

    for entry in fs::read_dir(&runners_dir)? {
        let entry = entry?;
        let path = entry.path();

        if path.is_file() && path.extension().is_some_and(|e| e == "md") {
            if let Ok(content) = fs::read_to_string(&path) {
                if let Ok(doc) = MarkdownDocument::parse(&content) {
                    if let Some(id) = doc.get_frontmatter("id") {
                        ids.insert(id.clone());
                    }
                }
            }
        }
    }

    Ok(ids)
}

fn validate_runner_track_refs(work_dir: &WorkDir, track_ids: &HashSet<String>) -> Result<usize> {
    let mut issues = 0;
    let runners_dir = work_dir.runners_dir();

    if !runners_dir.exists() {
        return Ok(0);
    }

    for entry in fs::read_dir(&runners_dir)? {
        let entry = entry?;
        let path = entry.path();

        if path.is_file() && path.extension().is_some_and(|e| e == "md") {
            if let Ok(content) = fs::read_to_string(&path) {
                if let Ok(doc) = MarkdownDocument::parse(&content) {
                    if let Some(assigned_track) = doc.get_frontmatter("assigned_track") {
                        if !track_ids.contains(assigned_track) {
                            let file_name = path.file_name().ok_or_else(|| {
                                anyhow::anyhow!("Path has no file name: {}", path.display())
                            })?;
                            println!(
                                "{} Runner references non-existent track: {}",
                                "WARNING:".yellow().bold(),
                                assigned_track
                            );
                            println!("  File: {file_name:?}");
                            issues += 1;
                        }
                    }
                }
            }
        }
    }

    Ok(issues)
}

fn validate_signal_runner_refs(work_dir: &WorkDir, runner_ids: &HashSet<String>) -> Result<usize> {
    let mut issues = 0;
    let signals_dir = work_dir.signals_dir();

    if !signals_dir.exists() {
        return Ok(0);
    }

    for entry in fs::read_dir(&signals_dir)? {
        let entry = entry?;
        let path = entry.path();

        if path.is_file() && path.extension().is_some_and(|e| e == "md") {
            if let Ok(content) = fs::read_to_string(&path) {
                if let Ok(doc) = MarkdownDocument::parse(&content) {
                    if let Some(target_runner) = doc.get_frontmatter("target_runner") {
                        if !runner_ids.contains(target_runner) {
                            let file_name = path.file_name().ok_or_else(|| {
                                anyhow::anyhow!("Path has no file name: {}", path.display())
                            })?;
                            println!(
                                "{} Signal targets non-existent runner: {}",
                                "WARNING:".yellow().bold(),
                                target_runner
                            );
                            println!("  File: {file_name:?}");
                            issues += 1;
                        }
                    }
                }
            }
        }
    }

    Ok(issues)
}
