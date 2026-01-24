use anyhow::Result;
use colored::Colorize;
use std::fs;

use crate::fs::work_dir::WorkDir;
use crate::models::failure::FailureType;
use crate::models::stage::{Stage, StageStatus};
use crate::parser::frontmatter::parse_from_markdown;

pub fn display_stages(work_dir: &WorkDir) -> Result<()> {
    let stages_dir = work_dir.stages_dir();
    if !stages_dir.exists() {
        return Ok(());
    }

    let mut stages = Vec::new();
    for entry in fs::read_dir(&stages_dir)? {
        let entry = entry?;
        let path = entry.path();

        if path.is_file() && path.extension().is_some_and(|e| e == "md") {
            if let Ok(content) = fs::read_to_string(&path) {
                if let Ok(stage) = parse_stage_from_markdown(&content) {
                    stages.push(stage);
                }
            }
        }
    }

    if stages.is_empty() {
        return Ok(());
    }

    println!("\n{}", "Active Stages".bold());

    let status_order = [
        (StageStatus::Completed, "✓", "Completed"),
        (StageStatus::Executing, "▶", "Executing"),
        (StageStatus::Queued, "○", "Ready"),
        (StageStatus::WaitingForInput, "?", "Waiting for Input"),
        (StageStatus::NeedsHandoff, "↻", "Needs Handoff"),
        (StageStatus::MergeConflict, "⚡", "Merge Conflict"),
        (
            StageStatus::CompletedWithFailures,
            "✗",
            "Completed with Failures",
        ),
        (StageStatus::MergeBlocked, "⚠", "Merge Blocked"),
        (StageStatus::Blocked, "✗", "Blocked"),
        (StageStatus::WaitingForDeps, "·", "Pending"),
        (StageStatus::Skipped, "⊘", "Skipped"),
    ];

    let max_id_len = stages.iter().map(|s| s.id.len()).max().unwrap_or(0);

    for (status, icon, label) in status_order {
        let matching: Vec<_> = stages.iter().filter(|s| s.status == status).collect();
        if matching.is_empty() {
            continue;
        }

        let header = format!("{icon} {label} ({})", matching.len());
        let colored_header = match status {
            StageStatus::Completed => header.green(),
            StageStatus::Executing => header.blue(),
            StageStatus::Queued => header.cyan(),
            StageStatus::WaitingForInput => header.magenta(),
            StageStatus::NeedsHandoff => header.yellow(),
            StageStatus::MergeConflict => header.yellow(),
            StageStatus::CompletedWithFailures => header.red(),
            StageStatus::MergeBlocked => header.red(),
            StageStatus::Blocked => header.red(),
            StageStatus::WaitingForDeps => header.dimmed(),
            StageStatus::Skipped => header.dimmed().strikethrough(),
        };
        println!("  {colored_header}");

        for stage in matching {
            let padded_id = format!("{:width$}", stage.id, width = max_id_len);
            let held_indicator = if stage.held {
                " [HELD]".yellow()
            } else {
                "".normal()
            };

            let status_suffix = if stage.status == StageStatus::Blocked {
                let max = stage.max_retries.unwrap_or(3);
                let failure_label = stage
                    .failure_info
                    .as_ref()
                    .map(|i| match i.failure_type {
                        FailureType::SessionCrash => "crash",
                        FailureType::TestFailure => "test",
                        FailureType::BuildFailure => "build",
                        FailureType::CodeError => "code",
                        FailureType::Timeout => "timeout",
                        FailureType::ContextExhausted => "context",
                        FailureType::UserBlocked => "user",
                        FailureType::MergeConflict => "merge",
                        FailureType::InfrastructureError => "infra",
                        FailureType::Unknown => "error",
                    })
                    .unwrap_or("error");

                format!(
                    " [{}] ({}/{} retries)",
                    failure_label, stage.retry_count, max
                )
                .red()
            } else {
                "".normal()
            };

            println!(
                "    {}  {}{}{}",
                padded_id.dimmed(),
                stage.name,
                held_indicator,
                status_suffix
            );
        }
        println!();
    }

    Ok(())
}

pub fn parse_stage_from_markdown(content: &str) -> Result<Stage> {
    parse_from_markdown(content, "Stage")
}
