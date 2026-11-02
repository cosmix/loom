//! Handoff markdown formatting.

use anyhow::Result;
use chrono::Utc;

use super::content::HandoffContent;
use crate::handoff::git_handoff::{format_git_history_markdown, table_cell};
use crate::handoff::schema::{CompletionCheckpoint, CompletionPhase, HandoffV2};
use crate::utils::format_thousands;

/// Format HandoffContent into V2 format with YAML frontmatter and markdown body.
///
/// The V2 format includes:
/// - YAML frontmatter (between --- markers) with structured data for machine parsing
/// - Human-readable markdown body for context
pub fn format_handoff_markdown(content: &HandoffContent) -> Result<String> {
    format_handoff_document(content, &content.to_v2())
}

pub(super) fn format_handoff_document(
    content: &HandoffContent,
    document: &HandoffV2,
) -> Result<String> {
    let date = Utc::now().format("%Y-%m-%d").to_string();

    let mut md = String::new();

    let yaml = document.to_yaml()?;
    md.push_str("---\n");
    md.push_str(&yaml);
    md.push_str("---\n\n");

    md.push_str(&format!("# Handoff: {}\n\n", content.stage_id));

    format_metadata_and_goals(&mut md, content, &date);
    format_work_and_decisions(&mut md, content);
    format_completion_checkpoint(&mut md, content.completion_checkpoint.as_ref());
    format_current_state(&mut md, content);

    if let Some(history) = &content.git_history {
        md.push_str(&format_git_history_markdown(history));
        md.push('\n');
    }

    format_next_steps(&mut md, content);

    if let Some(memory) = &content.memory_content {
        md.push_str(memory);
    }

    Ok(md)
}

fn format_metadata_and_goals(md: &mut String, content: &HandoffContent, date: &str) {
    md.push_str("## Metadata\n\n");
    md.push_str(&format!("- **Date**: {date}\n"));
    md.push_str(&format!("- **From**: {}\n", content.session_id));
    md.push_str("- **To**: (next session)\n");
    md.push_str(&format!("- **Stage**: {}\n", content.stage_id));
    if let Some(plan_id) = &content.plan_id {
        md.push_str(&format!("- **Plan**: {plan_id}\n"));
    }
    md.push_str(&format!(
        "- **Context**: {} tokens resident at handoff\n\n",
        format_thousands(content.context_tokens)
    ));

    md.push_str("## Goals (What We're Building)\n\n");
    if content.goals.is_empty() {
        md.push_str("No goals specified.\n\n");
    } else {
        md.push_str(&content.goals);
        md.push_str("\n\n");
    }
}

fn format_work_and_decisions(md: &mut String, content: &HandoffContent) {
    md.push_str("## Completed Work\n\n");
    if content.completed_work.is_empty() {
        md.push_str("- No work completed yet\n\n");
    } else {
        for item in &content.completed_work {
            md.push_str(&format!("- {item}\n"));
        }
        md.push('\n');
    }

    md.push_str("## Key Decisions Made\n\n");
    if content.decisions.is_empty() {
        md.push_str("No decisions documented.\n\n");
    } else {
        md.push_str("| Decision | Rationale |\n");
        md.push_str("|----------|----------|\n");
        for (decision, rationale) in &content.decisions {
            let decision_escaped = table_cell(decision);
            let rationale_escaped = table_cell(rationale);
            md.push_str(&format!("| {decision_escaped} | {rationale_escaped} |\n"));
        }
        md.push('\n');
    }
}

fn format_current_state(md: &mut String, content: &HandoffContent) {
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
}

fn format_next_steps(md: &mut String, content: &HandoffContent) {
    md.push_str("## Next Steps (Prioritized)\n\n");
    if content.next_steps.is_empty() {
        md.push_str("1. Review current state and determine next actions\n\n");
    } else {
        for (i, step) in content.next_steps.iter().enumerate() {
            md.push_str(&format!("{}. {step}\n", i + 1));
        }
        md.push('\n');
    }
}

fn format_completion_checkpoint(md: &mut String, checkpoint: Option<&CompletionCheckpoint>) {
    let Some(checkpoint) = checkpoint else {
        return;
    };
    md.push_str("## Completion Checkpoint\n\n");
    let phase = checkpoint
        .current_phase()
        .map_or("(none)", |phase| match phase {
            CompletionPhase::ToolFailed => "tool_failed",
            CompletionPhase::EvidenceMissing => "evidence_missing",
            CompletionPhase::VerifiedPendingAck => "verified_pending_ack",
            CompletionPhase::DaemonRejected => "daemon_rejected",
        });
    md.push_str(&format!("- **Current Phase**: {phase}\n"));
    if let Some(blocker) = &checkpoint.blocker {
        md.push_str(&format!(
            "- **Blocker**: {} ({})\n",
            blocker.short_fingerprint(),
            blocker.external_failure_code
        ));
    }
    md.push_str(&format!(
        "- **Repeat Count**: {}\n",
        checkpoint.repeat_count()
    ));
    md.push_str(&format!(
        "- **Accepted Receipt**: {}\n",
        if checkpoint.accepted.is_some() {
            "yes"
        } else {
            "no"
        }
    ));
    if checkpoint.conflict {
        md.push_str("- **Conflict**: yes\n");
    }
    if checkpoint.capacity_exhausted {
        md.push_str("- **Capacity Exhausted**: yes\n");
    }
    md.push('\n');
}
