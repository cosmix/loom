//! Formatting for recovery signal markdown files.

use crate::models::stage::Stage;

use super::recovery_types::RecoverySignalContent;
use super::types::EmbeddedContext;

/// Format a recovery signal as markdown
pub fn format_recovery_signal(
    content: &RecoverySignalContent,
    stage: &Stage,
    embedded_context: &EmbeddedContext,
) -> String {
    let mut signal = String::new();

    // Header
    signal.push_str(&format!("# Recovery Signal: {}\n\n", &content.session_id));

    // Recovery context
    signal.push_str("## Recovery Context\n\n");
    signal
        .push_str("**This is a RECOVERY session.** The previous session encountered an issue.\n\n");
    signal.push_str(&format!("- **Reason**: {}\n", &content.reason));
    signal.push_str(&format!(
        "- **Previous Session**: {}\n",
        &content.previous_session_id
    ));
    signal.push_str(&format!(
        "- **Recovery Attempt**: #{}\n",
        &content.recovery_attempt
    ));
    signal.push_str(&format!(
        "- **Detected At**: {}\n",
        content.detected_at.format("%Y-%m-%d %H:%M:%S UTC")
    ));

    if let Some(ref crash_path) = content.crash_report_path {
        signal.push_str(&format!("- **Crash Report**: {}\n", crash_path.display()));
    }

    signal.push('\n');

    // Last heartbeat info
    if let Some(ref hb) = content.last_heartbeat {
        signal.push_str("### Last Known State\n\n");
        signal.push_str(&format!(
            "- **Timestamp**: {}\n",
            hb.timestamp.format("%Y-%m-%d %H:%M:%S UTC")
        ));
        if let Some(pct) = hb.context_percent {
            signal.push_str(&format!("- **Context Usage**: {pct:.1}%\n"));
        }
        if let Some(ref tool) = hb.last_tool {
            signal.push_str(&format!("- **Last Tool**: {tool}\n"));
        }
        if let Some(ref activity) = hb.activity {
            signal.push_str(&format!("- **Activity**: {activity}\n"));
        }
        signal.push('\n');
    }

    // Recovery actions
    signal.push_str("### Recovery Actions\n\n");
    for (i, action) in content.recovery_actions.iter().enumerate() {
        signal.push_str(&format!("{}. {action}\n", i + 1));
    }
    signal.push('\n');

    // Worktree context
    signal.push_str("## Worktree Context\n\n");
    signal.push_str(
        "You are in an **isolated git worktree**. This signal contains everything you need:\n\n",
    );
    signal.push_str("- **Your stage assignment and acceptance criteria are below** - this file is self-contained\n");
    signal.push_str("- **All context (plan overview, handoff, knowledge) is embedded below** - reading main repo files is **FORBIDDEN**\n");
    signal.push_str(
        "- **Commit to your worktree branch** - it will be merged after verification\n\n",
    );

    // Target information
    signal.push_str("## Target\n\n");
    signal.push_str(&format!("- **Session**: {}\n", &content.session_id));
    signal.push_str(&format!("- **Stage**: {}\n", &content.stage_id));
    if let Some(ref plan_id) = stage.plan_id {
        signal.push_str(&format!("- **Plan**: {plan_id}\n"));
    }
    if let Some(ref worktree) = stage.worktree {
        signal.push_str(&format!("- **Worktree**: {worktree}\n"));
    }
    signal.push_str(&format!("- **Branch**: loom/{}\n", &content.stage_id));
    signal.push('\n');

    // Assignment from stage
    signal.push_str("## Assignment\n\n");
    signal.push_str(&format!("{}\n\n", &stage.name));
    if let Some(ref desc) = stage.description {
        signal.push_str(&format!("{desc}\n\n"));
    }

    // Acceptance criteria
    if !stage.acceptance.is_empty() {
        signal.push_str("## Acceptance Criteria\n\n");
        for criteria in &stage.acceptance {
            signal.push_str(&format!("- [ ] {criteria}\n"));
        }
        signal.push('\n');
    }

    // Files to modify
    if !stage.files.is_empty() {
        signal.push_str("## Files to Modify\n\n");
        for file in &stage.files {
            signal.push_str(&format!("- {file}\n"));
        }
        signal.push('\n');
    }

    // Embedded context - handoff
    if let Some(ref handoff) = embedded_context.handoff_content {
        signal.push_str("## Previous Session Handoff\n\n");
        signal.push_str("<handoff>\n");
        signal.push_str(handoff);
        signal.push_str("\n</handoff>\n\n");
    }

    // Embedded context - plan overview
    if let Some(ref overview) = embedded_context.plan_overview {
        signal.push_str("## Plan Overview\n\n");
        signal.push_str("<plan-overview>\n");
        signal.push_str(overview);
        signal.push_str("\n</plan-overview>\n\n");
    }

    signal
}
