use anyhow::{bail, Context, Result};
use chrono::{DateTime, Utc};

use crate::models::constants::DEFAULT_CONTEXT_LIMIT;
use crate::models::keys::frontmatter;
use crate::models::runner::{Runner, RunnerStatus};
use crate::parser::markdown::MarkdownDocument;

/// Convert a Runner to markdown format
pub fn runner_to_markdown(runner: &Runner) -> Result<String> {
    let assigned_track = runner.assigned_track.as_deref().unwrap_or("");

    let status_str = match runner.status {
        RunnerStatus::Idle => "idle",
        RunnerStatus::Active => "active",
        RunnerStatus::Blocked => "blocked",
        RunnerStatus::Archived => "archived",
    };

    let created_date = runner.created_at.format("%Y-%m-%d").to_string();
    let context_pct = runner.context_health();

    let track_display = runner.assigned_track.as_deref().unwrap_or("none");

    let markdown = format!(
        r#"---
id: {}
name: {}
runner_type: {}
status: {}
assigned_track: {}
context_tokens: {}
context_limit: {}
created_at: {}
last_active: {}
---

# Runner: {}

## Identity

- **Role**: {}
- **Created**: {}

## Assignment

- **Track**: {}
- **Status**: {}

## Session History

| Date       | Action    | Context % | Notes      |
| ---------- | --------- | --------- | ---------- |
| {}         | created   | 0%        | Initial    |
| {}         | {}        | {:.1}%    | Current    |
"#,
        runner.id,
        runner.name,
        runner.runner_type,
        status_str,
        assigned_track,
        runner.context_tokens,
        runner.context_limit,
        runner.created_at.to_rfc3339(),
        runner.last_active.to_rfc3339(),
        runner.id,
        runner.runner_type,
        created_date,
        track_display,
        status_str,
        created_date,
        runner.last_active.format("%Y-%m-%d"),
        status_str,
        context_pct
    );

    Ok(markdown)
}

/// Parse a Runner from markdown content
pub fn runner_from_markdown(content: &str) -> Result<Runner> {
    let doc = MarkdownDocument::parse(content).context("Failed to parse markdown document")?;

    let id = doc
        .get_frontmatter(frontmatter::ID)
        .ok_or_else(|| anyhow::anyhow!("Missing '{}' in frontmatter", frontmatter::ID))?
        .to_string();

    let name = doc
        .get_frontmatter(frontmatter::NAME)
        .ok_or_else(|| anyhow::anyhow!("Missing '{}' in frontmatter", frontmatter::NAME))?
        .to_string();

    let runner_type = doc
        .get_frontmatter(frontmatter::RUNNER_TYPE)
        .ok_or_else(|| anyhow::anyhow!("Missing '{}' in frontmatter", frontmatter::RUNNER_TYPE))?
        .to_string();

    let status_str = doc
        .get_frontmatter(frontmatter::STATUS)
        .ok_or_else(|| anyhow::anyhow!("Missing '{}' in frontmatter", frontmatter::STATUS))?;

    let status = match status_str.as_str() {
        "idle" => RunnerStatus::Idle,
        "active" => RunnerStatus::Active,
        "blocked" => RunnerStatus::Blocked,
        "archived" => RunnerStatus::Archived,
        _ => bail!("Invalid status: {status_str}"),
    };

    let assigned_track_str = doc
        .get_frontmatter(frontmatter::ASSIGNED_TRACK)
        .map(|s| s.to_string());
    let assigned_track = if assigned_track_str.as_deref() == Some("") {
        None
    } else {
        assigned_track_str
    };

    let context_tokens = doc
        .get_frontmatter(frontmatter::CONTEXT_TOKENS)
        .and_then(|s| s.parse::<u32>().ok())
        .unwrap_or(0);

    let context_limit = doc
        .get_frontmatter(frontmatter::CONTEXT_LIMIT)
        .and_then(|s| s.parse::<u32>().ok())
        .unwrap_or(DEFAULT_CONTEXT_LIMIT);

    let created_at = doc
        .get_frontmatter(frontmatter::CREATED_AT)
        .and_then(|s| s.parse::<DateTime<Utc>>().ok())
        .ok_or_else(|| {
            anyhow::anyhow!(
                "Missing or invalid '{}' in frontmatter",
                frontmatter::CREATED_AT
            )
        })?;

    let last_active = doc
        .get_frontmatter(frontmatter::LAST_ACTIVE)
        .and_then(|s| s.parse::<DateTime<Utc>>().ok())
        .ok_or_else(|| {
            anyhow::anyhow!(
                "Missing or invalid '{}' in frontmatter",
                frontmatter::LAST_ACTIVE
            )
        })?;

    Ok(Runner {
        id,
        name,
        runner_type,
        status,
        assigned_track,
        context_tokens,
        context_limit,
        created_at,
        last_active,
    })
}
