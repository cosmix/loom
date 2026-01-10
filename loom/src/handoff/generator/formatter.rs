//! Handoff markdown formatting.

use anyhow::Result;
use chrono::Utc;

use super::content::HandoffContent;
use crate::handoff::git_handoff::format_git_history_markdown;

/// Format HandoffContent into V2 format with YAML frontmatter and markdown body.
///
/// The V2 format includes:
/// - YAML frontmatter (between --- markers) with structured data for machine parsing
/// - Human-readable markdown body for context
pub fn format_handoff_markdown(content: &HandoffContent) -> Result<String> {
    let now = Utc::now();
    let date = now.format("%Y-%m-%d").to_string();

    let mut md = String::new();

    // V2 YAML frontmatter
    let v2 = content.to_v2();
    let yaml = v2.to_yaml()?;
    md.push_str("---\n");
    md.push_str(&yaml);
    md.push_str("---\n\n");

    // Title
    md.push_str(&format!("# Handoff: {}\n\n", content.stage_id));

    // Metadata (human-readable summary)
    md.push_str("## Metadata\n\n");
    md.push_str(&format!("- **Date**: {date}\n"));
    md.push_str(&format!("- **From**: {}\n", content.session_id));
    md.push_str("- **To**: (next session)\n");
    md.push_str(&format!("- **Stage**: {}\n", content.stage_id));
    if let Some(plan_id) = &content.plan_id {
        md.push_str(&format!("- **Plan**: {plan_id}\n"));
    }
    md.push_str(&format!(
        "- **Context**: {:.1}% (approaching threshold)\n\n",
        content.context_percent
    ));

    // Goals
    md.push_str("## Goals (What We're Building)\n\n");
    if content.goals.is_empty() {
        md.push_str("No goals specified.\n\n");
    } else {
        md.push_str(&content.goals);
        md.push_str("\n\n");
    }

    // Completed Work
    md.push_str("## Completed Work\n\n");
    if content.completed_work.is_empty() {
        md.push_str("- No work completed yet\n\n");
    } else {
        for item in &content.completed_work {
            md.push_str(&format!("- {item}\n"));
        }
        md.push('\n');
    }

    // Key Decisions Made
    md.push_str("## Key Decisions Made\n\n");
    if content.decisions.is_empty() {
        md.push_str("No decisions documented.\n\n");
    } else {
        md.push_str("| Decision | Rationale |\n");
        md.push_str("|----------|----------|\n");
        for (decision, rationale) in &content.decisions {
            // Escape pipe characters in table cells
            let decision_escaped = decision.replace('|', "\\|");
            let rationale_escaped = rationale.replace('|', "\\|");
            md.push_str(&format!("| {decision_escaped} | {rationale_escaped} |\n"));
        }
        md.push('\n');
    }

    // Current State
    md.push_str("## Current State\n\n");
    if let Some(branch) = &content.current_branch {
        md.push_str(&format!("- **Branch**: {branch}\n"));
    } else {
        md.push_str("- **Branch**: (unknown)\n");
    }
    if let Some(test_status) = &content.test_status {
        md.push_str(&format!("- **Tests**: {test_status}\n"));
    } else {
        md.push_str("- **Tests**: (not run)\n");
    }
    md.push_str("- **Files Modified**:\n");
    if content.files_modified.is_empty() {
        md.push_str("  - (none)\n");
    } else {
        for file in &content.files_modified {
            md.push_str(&format!("  - {file}\n"));
        }
    }
    md.push('\n');

    // Git History (if available)
    if let Some(history) = &content.git_history {
        md.push_str(&format_git_history_markdown(history));
        md.push('\n');
    }

    // Next Steps
    md.push_str("## Next Steps (Prioritized)\n\n");
    if content.next_steps.is_empty() {
        md.push_str("1. Review current state and determine next actions\n\n");
    } else {
        for (i, step) in content.next_steps.iter().enumerate() {
            md.push_str(&format!("{}. {step}\n", i + 1));
        }
        md.push('\n');
    }

    // Learnings / Patterns Identified
    md.push_str("## Learnings / Patterns Identified\n\n");
    if content.learnings.is_empty() {
        md.push_str("No learnings documented yet.\n");
    } else {
        for learning in &content.learnings {
            md.push_str(&format!("- {learning}\n"));
        }
    }

    Ok(md)
}

/// Format HandoffContent into legacy V1 format (prose only, no YAML frontmatter).
///
/// This function is provided for backward compatibility with systems that
/// cannot parse V2 format.
#[allow(dead_code)]
pub fn format_handoff_markdown_v1(content: &HandoffContent) -> Result<String> {
    let now = Utc::now();
    let date = now.format("%Y-%m-%d").to_string();

    let mut md = String::new();

    // Title
    md.push_str(&format!("# Handoff: {}\n\n", content.stage_id));

    // Metadata
    md.push_str("## Metadata\n\n");
    md.push_str(&format!("- **Date**: {date}\n"));
    md.push_str(&format!("- **From**: {}\n", content.session_id));
    md.push_str("- **To**: (next session)\n");
    md.push_str(&format!("- **Stage**: {}\n", content.stage_id));
    if let Some(plan_id) = &content.plan_id {
        md.push_str(&format!("- **Plan**: {plan_id}\n"));
    }
    md.push_str(&format!(
        "- **Context**: {:.1}% (approaching threshold)\n\n",
        content.context_percent
    ));

    // Goals
    md.push_str("## Goals (What We're Building)\n\n");
    if content.goals.is_empty() {
        md.push_str("No goals specified.\n\n");
    } else {
        md.push_str(&content.goals);
        md.push_str("\n\n");
    }

    // Completed Work
    md.push_str("## Completed Work\n\n");
    if content.completed_work.is_empty() {
        md.push_str("- No work completed yet\n\n");
    } else {
        for item in &content.completed_work {
            md.push_str(&format!("- {item}\n"));
        }
        md.push('\n');
    }

    // Key Decisions Made
    md.push_str("## Key Decisions Made\n\n");
    if content.decisions.is_empty() {
        md.push_str("No decisions documented.\n\n");
    } else {
        md.push_str("| Decision | Rationale |\n");
        md.push_str("|----------|----------|\n");
        for (decision, rationale) in &content.decisions {
            let decision_escaped = decision.replace('|', "\\|");
            let rationale_escaped = rationale.replace('|', "\\|");
            md.push_str(&format!("| {decision_escaped} | {rationale_escaped} |\n"));
        }
        md.push('\n');
    }

    // Current State
    md.push_str("## Current State\n\n");
    if let Some(branch) = &content.current_branch {
        md.push_str(&format!("- **Branch**: {branch}\n"));
    } else {
        md.push_str("- **Branch**: (unknown)\n");
    }
    if let Some(test_status) = &content.test_status {
        md.push_str(&format!("- **Tests**: {test_status}\n"));
    } else {
        md.push_str("- **Tests**: (not run)\n");
    }
    md.push_str("- **Files Modified**:\n");
    if content.files_modified.is_empty() {
        md.push_str("  - (none)\n");
    } else {
        for file in &content.files_modified {
            md.push_str(&format!("  - {file}\n"));
        }
    }
    md.push('\n');

    // Git History (if available)
    if let Some(history) = &content.git_history {
        md.push_str(&format_git_history_markdown(history));
        md.push('\n');
    }

    // Next Steps
    md.push_str("## Next Steps (Prioritized)\n\n");
    if content.next_steps.is_empty() {
        md.push_str("1. Review current state and determine next actions\n\n");
    } else {
        for (i, step) in content.next_steps.iter().enumerate() {
            md.push_str(&format!("{}. {step}\n", i + 1));
        }
        md.push('\n');
    }

    // Learnings / Patterns Identified
    md.push_str("## Learnings / Patterns Identified\n\n");
    if content.learnings.is_empty() {
        md.push_str("No learnings documented yet.\n");
    } else {
        for learning in &content.learnings {
            md.push_str(&format!("- {learning}\n"));
        }
    }

    Ok(md)
}
