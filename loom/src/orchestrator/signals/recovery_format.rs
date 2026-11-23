//! Formatting for recovery signal markdown files.

use crate::models::stage::Stage;

use super::cache::stable_prefix_for;
use super::format::budget::push_model_budget;
use super::format::format_recitation_section;
use super::recovery_sections::{push_recovery_assignment, push_semi_stable};
use super::recovery_types::{LastHeartbeatInfo, RecoverySignalContent};
use super::types::EmbeddedContext;
use super::wrap::wrap_untrusted;
use crate::context::untrusted::inline_safe;

/// Render the "### Last Known State" block from the previous session's last
/// heartbeat.
fn format_last_known_state(hb: &LastHeartbeatInfo) -> String {
    let mut section = String::from("### Last Known State\n\n");
    section.push_str(&format!(
        "- **Timestamp**: {}\n",
        hb.timestamp.format("%Y-%m-%d %H:%M:%S UTC")
    ));
    if let Some(tokens) = hb.context_tokens {
        section.push_str(&format!("- **Context Tokens**: {tokens} tokens\n"));
    }
    if let Some(ref tool) = hb.last_tool {
        section.push_str(&format!("- **Last Tool**: {}\n", inline_safe(tool)));
    }
    if let Some(ref activity) = hb.activity {
        section.push_str(&format!("- **Activity**: {}\n", inline_safe(activity)));
    }
    section.push('\n');
    section
}

/// Render the "## Recovery Context" / "### Last Known State" / "### Recovery
/// Actions" block: what triggered this recovery, the previous session's last
/// heartbeat, and the suggested next actions.
fn format_recovery_header(content: &RecoverySignalContent) -> String {
    let mut header = String::new();

    header.push_str("## Recovery Context\n\n");
    header
        .push_str("**This is a RECOVERY session.** The previous session encountered an issue.\n\n");
    header.push_str(&format!("- **Reason**: {}\n", content.reason));
    header.push_str(&format!(
        "- **Previous Session**: {}\n",
        content.previous_session_id
    ));
    header.push_str(&format!(
        "- **Recovery Attempt**: #{}\n",
        content.recovery_attempt
    ));
    header.push_str(&format!(
        "- **Detected At**: {}\n",
        content.detected_at.format("%Y-%m-%d %H:%M:%S UTC")
    ));

    if let Some(ref crash_path) = content.crash_report_path {
        header.push_str(&format!(
            "- **Crash Report**: {}\n",
            inline_safe(&crash_path.display().to_string())
        ));
    }

    header.push('\n');

    if let Some(ref hb) = content.last_heartbeat {
        header.push_str(&format_last_known_state(hb));
    }

    header.push_str("### Recovery Actions\n\n");
    // Actions are written by loom or typed by the operator (multi-line answers
    // reach the signal verbatim); heartbeat text is made inline-safe where an
    // action is built from it (`RecoverySignalContent::for_hung`).
    for (i, action) in content.recovery_actions.iter().enumerate() {
        header.push_str(&format!("{}. {action}\n", i + 1));
    }
    header.push('\n');

    header
}

/// Format a recovery signal as markdown
pub fn format_recovery_signal(
    content: &RecoverySignalContent,
    stage: &Stage,
    embedded_context: &EmbeddedContext,
) -> String {
    let mut signal = String::new();

    // Header
    signal.push_str(&format!("# Recovery Signal: {}\n\n", content.session_id));

    signal.push_str(&format_recovery_header(content));

    // Full stable prefix for this stage type: worktree context, isolation and path boundaries, the
    // "## Execution Rules" header (a pointer at ~/.claude/CLAUDE.md plus the knowledge-consumption
    // contract), the subagent no-verify block, and — for code stages (Standard / IntegrationVerify)
    // — the mini adversarial code review, plus commit-timing and completion rules. Built outside
    // the KV-cache path, so without this a resumed stage would miss all of that guidance.
    signal.push_str(&stable_prefix_for(stage.stage_type));

    // Semi-stable per-stage blocks (brief, memory reminder, codex lane, response
    // budget, ultracode, sandbox): this signal embeds them nowhere else, so a
    // resumed stage would lose what its fresh signal carried.
    push_semi_stable(&mut signal, stage, embedded_context);

    push_model_budget(embedded_context.model_budget.as_deref(), &mut signal);
    append_target_information(&mut signal, content, stage);

    push_recovery_assignment(&mut signal, stage);

    // Embedded context - handoff
    if let Some(ref handoff) = embedded_context.handoff_content {
        signal.push_str("## Previous Session Handoff\n\n");
        signal.push_str(&wrap_untrusted("handoff", handoff));
    }

    // Embedded context - plan overview
    if let Some(ref overview) = embedded_context.plan_overview {
        signal.push_str("## Plan Overview\n\n");
        signal.push_str(&wrap_untrusted("plan-overview", overview));
    }

    signal.push_str(&format_recitation_section(stage, embedded_context));
    signal
}

fn append_target_information(signal: &mut String, content: &RecoverySignalContent, stage: &Stage) {
    // Target information
    signal.push_str("## Target\n\n");
    signal.push_str(&format!("- **Session**: {}\n", content.session_id));
    signal.push_str(&format!("- **Stage**: {}\n", content.stage_id));
    if let Some(ref plan_id) = stage.plan_id {
        signal.push_str(&format!("- **Plan**: {plan_id}\n"));
    }
    if let Some(ref worktree) = stage.worktree {
        signal.push_str(&format!("- **Worktree**: {worktree}\n"));
    }
    signal.push_str(&format!("- **Branch**: loom/{}\n", content.stage_id));
    signal.push('\n');
}
