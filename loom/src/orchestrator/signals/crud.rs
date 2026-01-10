use anyhow::{bail, Context, Result};
use std::fs;
use std::path::Path;

use super::format::format_dependency_table;
use super::parse::parse_signal_content;
use super::types::{SignalContent, SignalUpdates};

pub fn update_signal(session_id: &str, updates: SignalUpdates, work_dir: &Path) -> Result<()> {
    let signal_path = work_dir.join("signals").join(format!("{session_id}.md"));

    if !signal_path.exists() {
        bail!("Signal file does not exist: {}", signal_path.display());
    }

    let content = fs::read_to_string(&signal_path).context("Failed to read signal file")?;

    let mut updated_content = content;

    if let Some(tasks) = updates.add_tasks {
        if !tasks.is_empty() {
            let task_section = tasks
                .iter()
                .enumerate()
                .map(|(i, task)| format!("{}. {}", i + 1, task))
                .collect::<Vec<_>>()
                .join("\n");

            if let Some(pos) = updated_content.find("## Immediate Tasks") {
                // Find the next section, or append at the end if this is the last section
                if let Some(next_section) = updated_content[pos..].find("\n\n## ") {
                    let insert_pos = pos + next_section;
                    updated_content.insert_str(insert_pos, &format!("\n{task_section}"));
                } else {
                    // No next section - append at the end of the file
                    updated_content.push_str(&format!("\n{task_section}\n"));
                }
            }
        }
    }

    if let Some(deps) = updates.update_dependencies {
        if !deps.is_empty() {
            let dep_table = format_dependency_table(&deps);
            if let Some(start) = updated_content.find("## Dependencies Status") {
                if let Some(table_start) = updated_content[start..].find("| Dependency") {
                    let abs_table_start = start + table_start;
                    if let Some(next_section) = updated_content[abs_table_start..].find("\n\n## ") {
                        let end_pos = abs_table_start + next_section;
                        updated_content.replace_range(abs_table_start..end_pos, &dep_table);
                    }
                }
            }
        }
    }

    if let Some(files) = updates.add_context_files {
        if !files.is_empty() {
            let file_list = files
                .iter()
                .map(|f| format!("- {f}"))
                .collect::<Vec<_>>()
                .join("\n");

            if let Some(pos) = updated_content.find("## Context Restoration") {
                if let Some(next_section) = updated_content[pos..].find("\n\n## ") {
                    let insert_pos = pos + next_section;
                    updated_content.insert_str(insert_pos, &format!("\n{file_list}"));
                }
            }
        }
    }

    fs::write(&signal_path, updated_content).context("Failed to update signal file")?;

    Ok(())
}

pub fn remove_signal(session_id: &str, work_dir: &Path) -> Result<()> {
    let signal_path = work_dir.join("signals").join(format!("{session_id}.md"));

    if signal_path.exists() {
        fs::remove_file(&signal_path)
            .with_context(|| format!("Failed to remove signal file: {}", signal_path.display()))?;
    }

    Ok(())
}

pub fn read_signal(session_id: &str, work_dir: &Path) -> Result<Option<SignalContent>> {
    let signal_path = work_dir.join("signals").join(format!("{session_id}.md"));

    if !signal_path.exists() {
        return Ok(None);
    }

    let content = fs::read_to_string(&signal_path).context("Failed to read signal file")?;

    let parsed = parse_signal_content(session_id, &content)?;
    Ok(Some(parsed))
}

pub fn list_signals(work_dir: &Path) -> Result<Vec<String>> {
    let signals_dir = work_dir.join("signals");

    if !signals_dir.exists() {
        return Ok(Vec::new());
    }

    let mut signals = Vec::new();

    for entry in fs::read_dir(signals_dir).context("Failed to read signals directory")? {
        let entry = entry.context("Failed to read directory entry")?;
        let path = entry.path();

        if path.extension().and_then(|s| s.to_str()) == Some("md") {
            if let Some(name) = path.file_stem().and_then(|s| s.to_str()) {
                signals.push(name.to_string());
            }
        }
    }

    signals.sort();
    Ok(signals)
}
