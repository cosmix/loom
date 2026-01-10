use anyhow::Result;

use crate::models::track::Track;
use crate::parser::markdown::MarkdownDocument;

use super::helpers::{parse_datetime, status_from_string, status_to_string};

/// Convert a Track to markdown format
pub fn track_to_markdown(track: &Track) -> Result<String> {
    let mut output = String::new();

    output.push_str("---\n");
    output.push_str(&format!("id: {}\n", track.id));
    output.push_str(&format!("name: {}\n", track.name));
    output.push_str(&format!("status: {}\n", status_to_string(&track.status)));

    if let Some(ref runner) = track.assigned_runner {
        output.push_str(&format!("assigned_runner: {runner}\n"));
    }

    if let Some(ref parent) = track.parent_track {
        output.push_str(&format!("parent_track: {parent}\n"));
    }

    output.push_str(&format!("created_at: {}\n", track.created_at.to_rfc3339()));
    output.push_str(&format!("updated_at: {}\n", track.updated_at.to_rfc3339()));

    if let Some(closed_at) = track.closed_at {
        output.push_str(&format!("closed_at: {}\n", closed_at.to_rfc3339()));
    }

    output.push_str("---\n\n");
    output.push_str(&format!("# {}\n\n", track.name));

    if let Some(ref desc) = track.description {
        output.push_str("## Description\n\n");
        output.push_str(desc);
        output.push_str("\n\n");
    }

    if !track.child_tracks.is_empty() {
        output.push_str("## Child Tracks\n\n");
        for child in &track.child_tracks {
            output.push_str(&format!("- {child}\n"));
        }
        output.push('\n');
    }

    if let Some(ref reason) = track.close_reason {
        output.push_str("## Close Reason\n\n");
        output.push_str(reason);
        output.push('\n');
    }

    Ok(output)
}

/// Parse a Track from markdown content
pub fn track_from_markdown(content: &str) -> Result<Track> {
    let doc = MarkdownDocument::parse(content)?;

    let id = doc
        .get_frontmatter("id")
        .ok_or_else(|| anyhow::anyhow!("Missing 'id' in frontmatter"))?
        .clone();

    let name = doc
        .get_frontmatter("name")
        .ok_or_else(|| anyhow::anyhow!("Missing 'name' in frontmatter"))?
        .clone();

    let status_str = doc
        .get_frontmatter("status")
        .ok_or_else(|| anyhow::anyhow!("Missing 'status' in frontmatter"))?;
    let status = status_from_string(status_str)?;

    let assigned_runner = doc.get_frontmatter("assigned_runner").cloned();
    let parent_track = doc.get_frontmatter("parent_track").cloned();

    let created_at = doc
        .get_frontmatter("created_at")
        .ok_or_else(|| anyhow::anyhow!("Missing 'created_at' in frontmatter"))?;
    let created_at = parse_datetime(created_at)?;

    let updated_at = doc
        .get_frontmatter("updated_at")
        .ok_or_else(|| anyhow::anyhow!("Missing 'updated_at' in frontmatter"))?;
    let updated_at = parse_datetime(updated_at)?;

    let closed_at = doc
        .get_frontmatter("closed_at")
        .map(|s| parse_datetime(s))
        .transpose()?;

    let description = doc.get_section("Description").map(|s| s.trimmed_content());

    let child_tracks = doc
        .get_section("Child Tracks")
        .map(|s| {
            s.trimmed_content()
                .lines()
                .filter_map(|line| line.trim().strip_prefix("- ").map(|s| s.to_string()))
                .collect()
        })
        .unwrap_or_default();

    let close_reason = doc.get_section("Close Reason").map(|s| s.trimmed_content());

    Ok(Track {
        id,
        name,
        description,
        status,
        assigned_runner,
        parent_track,
        child_tracks,
        created_at,
        updated_at,
        closed_at,
        close_reason,
    })
}
