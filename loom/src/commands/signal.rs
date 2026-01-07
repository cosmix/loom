use crate::fs::work_dir::WorkDir;
use crate::validation::{validate_id, validate_message, validate_name};
use anyhow::{bail, Context, Result};
use colored::Colorize;
use std::fs;
use std::path::PathBuf;

/// Set a signal for a runner
pub fn set(runner_id: String, signal_type: String, message: String, priority: u8) -> Result<()> {
    // Validate inputs before any file operations
    validate_id(&runner_id).context("Invalid runner ID")?;
    validate_name(&signal_type).context("Invalid signal type")?;
    validate_message(&message).context("Invalid signal message")?;

    let work_dir = WorkDir::new(".")?;
    work_dir.load()?;

    let (runner_type, assigned_track) = get_runner_info(&work_dir, &runner_id)?;

    let markdown = signal_to_markdown(
        &runner_id,
        &runner_type,
        assigned_track.as_deref(),
        &signal_type,
        &message,
        priority,
    );

    let signal_path = work_dir.signals_dir().join(format!("{runner_id}.md"));
    fs::write(&signal_path, markdown)
        .with_context(|| format!("Failed to write signal file for runner {runner_id}"))?;

    println!(
        "{} Signal set for runner {}",
        "✓".green().bold(),
        runner_id.cyan().bold()
    );
    println!("  Type: {}", signal_type.yellow());
    println!("  Priority: {priority}");
    println!("  Message: {message}");

    Ok(())
}

/// Show signals for a runner (or all signals)
pub fn show(runner_id: Option<String>) -> Result<()> {
    // Validate ID if provided before any file operations
    if let Some(ref id) = runner_id {
        validate_id(id).context("Invalid runner ID")?;
    }

    let work_dir = WorkDir::new(".")?;
    work_dir.load()?;

    if let Some(id) = runner_id {
        show_single_signal(&work_dir, &id)
    } else {
        show_all_signals(&work_dir)
    }
}

/// Clear a signal
pub fn clear(signal_id: String) -> Result<()> {
    // Validate ID before any file operations
    validate_id(&signal_id).context("Invalid signal ID")?;

    let work_dir = WorkDir::new(".")?;
    work_dir.load()?;

    let signal_path = work_dir.signals_dir().join(format!("{signal_id}.md"));

    if !signal_path.exists() {
        bail!("Signal file not found for runner: {signal_id}");
    }

    fs::remove_file(&signal_path)
        .with_context(|| format!("Failed to remove signal file for runner {signal_id}"))?;

    println!(
        "{} Signal cleared for runner {}",
        "✓".green().bold(),
        signal_id.cyan().bold()
    );

    Ok(())
}

fn get_runner_info(work_dir: &WorkDir, runner_id: &str) -> Result<(String, Option<String>)> {
    let runner_path = work_dir.runners_dir().join(format!("{runner_id}.md"));

    if !runner_path.exists() {
        bail!("Runner not found: {runner_id}. Create the runner first with 'loom runner create'");
    }

    let content = fs::read_to_string(&runner_path)
        .with_context(|| format!("Failed to read runner file: {runner_id}"))?;

    let runner_type = extract_field(&content, "Role").unwrap_or_else(|| "unknown".to_string());

    let assigned_track = extract_field(&content, "Track");

    Ok((runner_type, assigned_track))
}

fn extract_field(content: &str, field: &str) -> Option<String> {
    for line in content.lines() {
        if line.contains(&format!("**{field}**:")) {
            if let Some(value) = line.split(':').nth(1) {
                return Some(value.trim().to_string());
            }
        }
    }
    None
}

fn signal_to_markdown(
    runner_id: &str,
    runner_type: &str,
    track: Option<&str>,
    signal_type: &str,
    message: &str,
    priority: u8,
) -> String {
    let track_line = if let Some(track_id) = track {
        format!("- **Track**: {track_id}\n")
    } else {
        "- **Track**: none\n".to_string()
    };

    format!(
        r#"# Signal: {}

## Target

- **Runner**: {}
- **Role**: {}
{}
## Signal

- **Type**: {}
- **Priority**: {}

## Work

{}

### Immediate Tasks

1. Review the signal message above
2. Load relevant context files
3. Execute the requested work

### Context Restoration

- `.work/runners/{}.md` - Runner information
{}
### Acceptance Criteria

- [ ] Signal work completed
- [ ] All tests pass
- [ ] No IDE diagnostics
"#,
        runner_id,
        runner_id,
        runner_type,
        track_line,
        signal_type,
        priority,
        message,
        runner_id,
        if let Some(track_id) = track {
            format!("- `.work/tracks/{track_id}.md` - Track overview\n")
        } else {
            String::new()
        }
    )
}

fn show_single_signal(work_dir: &WorkDir, runner_id: &str) -> Result<()> {
    let signal_path = work_dir.signals_dir().join(format!("{runner_id}.md"));

    if !signal_path.exists() {
        println!(
            "{} No signal found for runner {}",
            "i".blue().bold(),
            runner_id.cyan()
        );
        return Ok(());
    }

    let content = fs::read_to_string(&signal_path)
        .with_context(|| format!("Failed to read signal file for runner {runner_id}"))?;

    println!("{}", "Signal Details".bold().underline());
    println!();
    println!("{content}");

    Ok(())
}

fn show_all_signals(work_dir: &WorkDir) -> Result<()> {
    let signals_dir = work_dir.signals_dir();

    let entries = fs::read_dir(&signals_dir).context("Failed to read signals directory")?;

    let mut signal_files: Vec<PathBuf> = entries
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.extension().and_then(|s| s.to_str()) == Some("md"))
        .collect();

    if signal_files.is_empty() {
        println!("{} No signals found", "i".blue().bold());
        return Ok(());
    }

    signal_files.sort();

    println!("{}", "Active Signals".bold().underline());
    println!();
    println!(
        "{:<15} {:<20} {:<10} {}",
        "Runner".bold(),
        "Type".bold(),
        "Priority".bold(),
        "Message Preview".bold()
    );
    println!("{}", "-".repeat(80));

    for signal_file in signal_files {
        let runner_id = signal_file
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("unknown");

        let content = fs::read_to_string(&signal_file)
            .with_context(|| format!("Failed to read signal file: {signal_file:?}"))?;

        let signal_type = extract_field(&content, "Type").unwrap_or_else(|| "unknown".to_string());
        let priority = extract_field(&content, "Priority").unwrap_or_else(|| "0".to_string());

        let message_preview = extract_work_section(&content)
            .map(|msg| {
                let trimmed = msg.trim();
                if trimmed.chars().count() > 40 {
                    let truncated: String = trimmed.chars().take(37).collect();
                    format!("{truncated}...")
                } else {
                    trimmed.to_string()
                }
            })
            .unwrap_or_else(|| "No message".to_string());

        println!(
            "{:<15} {:<20} {:<10} {}",
            runner_id.cyan(),
            signal_type.yellow(),
            priority,
            message_preview
        );
    }

    Ok(())
}

fn extract_work_section(content: &str) -> Option<String> {
    let mut in_work_section = false;
    let mut work_content = String::new();

    for line in content.lines() {
        if line.starts_with("## Work") {
            in_work_section = true;
            continue;
        }

        if in_work_section {
            if line.starts_with("##") || line.starts_with("###") {
                break;
            }
            if !line.is_empty() {
                if !work_content.is_empty() {
                    work_content.push(' ');
                }
                work_content.push_str(line.trim());
            }
        }
    }

    if work_content.is_empty() {
        None
    } else {
        Some(work_content)
    }
}
