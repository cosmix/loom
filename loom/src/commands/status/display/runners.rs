use anyhow::Result;
use colored::Colorize;
use std::fs;

use crate::fs::work_dir::WorkDir;
use crate::models::constants::DEFAULT_CONTEXT_LIMIT;
use crate::models::keys::frontmatter;
use crate::models::runner::{Runner, RunnerStatus};
use crate::parser::markdown::MarkdownDocument;

pub fn load_runners(work_dir: &WorkDir) -> Result<(Vec<Runner>, usize)> {
    let runners_dir = work_dir.runners_dir();
    let mut runners = Vec::new();
    let mut count = 0;

    if !runners_dir.exists() {
        return Ok((runners, 0));
    }

    for entry in fs::read_dir(&runners_dir)? {
        let entry = entry?;
        let path = entry.path();

        if path.is_file() && path.extension().is_some_and(|e| e == "md") {
            count += 1;
            if let Ok(content) = fs::read_to_string(&path) {
                if let Ok(doc) = MarkdownDocument::parse(&content) {
                    if let Some(runner) = parse_runner_from_doc(&doc) {
                        runners.push(runner);
                    }
                }
            }
        }
    }

    Ok((runners, count))
}

fn parse_runner_from_doc(doc: &MarkdownDocument) -> Option<Runner> {
    let id = doc.get_frontmatter(frontmatter::ID)?.clone();
    let name = doc.get_frontmatter(frontmatter::NAME)?.clone();
    let runner_type = doc.get_frontmatter(frontmatter::RUNNER_TYPE)?.clone();

    let context_tokens = doc
        .get_frontmatter(frontmatter::CONTEXT_TOKENS)
        .and_then(|s| s.parse().ok())
        .unwrap_or(0);
    let context_limit = doc
        .get_frontmatter(frontmatter::CONTEXT_LIMIT)
        .and_then(|s| s.parse().ok())
        .unwrap_or(DEFAULT_CONTEXT_LIMIT);

    Some(Runner {
        id,
        name,
        runner_type,
        status: RunnerStatus::Idle,
        assigned_track: doc.get_frontmatter(frontmatter::ASSIGNED_TRACK).cloned(),
        context_tokens,
        context_limit,
        created_at: chrono::Utc::now(),
        last_active: chrono::Utc::now(),
    })
}

pub fn display_runner_health(runner: &Runner) {
    let health = runner.context_usage_percent();
    let health_str = format!("{health:.1}%");
    let context_tokens = runner.context_tokens;
    let context_limit = runner.context_limit;
    let status_str = format!("{context_tokens}/{context_limit} tokens");

    let colored_health = if health < 60.0 {
        health_str.green()
    } else if health < 75.0 {
        health_str.yellow()
    } else {
        health_str.red()
    };

    println!(
        "  {} [{}] {}",
        runner.name,
        colored_health,
        status_str.dimmed()
    );
}
