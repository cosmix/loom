use crate::handoff::schema::HandoffV2;
use crate::models::stage::Stage;

use super::super::types::DependencyStatus;

/// Format the per-subagent response-budget block.
///
/// Emitted only for stages that set `subagent_timeout_secs` explicitly, so a
/// plan that never opts in gets a byte-identical signal to before this field
/// existed. It lives here rather than in `sections.rs` because both the
/// semi-stable path and the recovery path emit it, and `sections.rs` is already
/// well over the file-size ceiling.
///
/// The block states the budget and what to DO when it elapses. The orchestrator
/// side is advisory — it prints a warning and nothing more — so the only thing
/// that can actually act on a silent subagent is the agent holding the stage.
pub(crate) fn format_subagent_timeout_section(timeout_secs: u64) -> String {
    let mut content = String::new();

    content.push_str("## Subagent Response Budget\n\n");
    content.push_str(&format!(
        "This stage's heartbeat budget is {timeout_secs}s: the orchestrator warns when the session goes\n"
    ));
    content.push_str(
        "that long with no tool activity. The warning is ADVISORY - it never kills or retries anything.\n",
    );
    content.push_str(
        "Treat the budget as a check-in cadence for your bounded liveness checks, NOT as a deadline on\n",
    );
    content.push_str(
        "any subagent's work. When a check fires and the subagent is still alive (task running, files\n",
    );
    content.push_str(
        "changing, output growing), re-arm the check and keep waiting - slow is not dead. Take over or\n",
    );
    content.push_str(
        "re-assign ONLY on positive evidence of death: the task failed or was killed, or several\n",
    );
    content.push_str(
        "consecutive checks show zero liveness AND no result. Elapsed time alone is NEVER that evidence.\n",
    );
    content.push_str(
        "Restarting live work forfeits the tokens it already spent and risks two agents writing the same\n",
    );
    content.push_str("files. Never complete the stage while any subagent is still out.\n\n");

    content
}

/// Append the "BUDGET EXCEEDED - WRAP UP NOW" box shown in the recitation
/// section once context usage reaches the stage's budget.
///
/// Extracted from `format_recitation_section` (`sections.rs`), which is
/// already well over the file-size ceiling.
pub(super) fn append_budget_exceeded_box(content: &mut String) {
    content.push_str("```\n");
    content.push_str("┌──────────────────────────────────────────────────────────┐\n");
    content.push_str("│  🛑 BUDGET EXCEEDED - WRAP UP NOW                        │\n");
    content.push_str("│  1. loom memory list (verify insights captured)          │\n");
    content.push_str("│  2. Stage SETTLED (subagents returned, defects fixed,    │\n");
    content.push_str("│     everything committed)?                               │\n");
    content.push_str("│     YES -> loom stage complete <stage-id>                │\n");
    content.push_str("│     NO  -> loom handoff --message \"<state>\" and STOP.    │\n");
    content.push_str("│            Do NOT complete an unsettled stage.           │\n");
    content.push_str("└──────────────────────────────────────────────────────────┘\n");
    content.push_str("```\n");
}

/// Format a table showing dependency status for inclusion in signals
pub fn format_dependency_table(deps: &[DependencyStatus]) -> String {
    let mut table = String::new();
    table.push_str("| Dependency | Status |\n");
    table.push_str("|------------|--------|\n");

    for dep in deps {
        let name = &dep.name;
        let status = &dep.status;
        table.push_str(&format!("| {name} | {status} |\n"));
    }

    table
}

/// Format outputs from dependency stages for inclusion in signals.
///
/// This produces a clear, structured format that agents can easily parse:
/// ```text
/// ### From stage-name
///
/// - **key**: value
///   > Description of what this output represents
/// ```
pub(super) fn format_dependency_outputs(deps: &[&DependencyStatus]) -> String {
    let mut content = String::new();

    for dep in deps {
        content.push_str(&format!("### From {}\n\n", dep.name));

        for output in &dep.outputs {
            // Format value based on type
            let value_str = match &output.value {
                serde_json::Value::String(s) => format!("`\"{s}\"`"),
                serde_json::Value::Null => "`null`".to_string(),
                serde_json::Value::Bool(b) => format!("`{b}`"),
                serde_json::Value::Number(n) => format!("`{n}`"),
                serde_json::Value::Array(_) | serde_json::Value::Object(_) => {
                    let json = serde_json::to_string(&output.value).unwrap_or_default();
                    format!("```json\n{json}\n```")
                }
            };

            content.push_str(&format!("- **{}**: {}\n", output.key, value_str));
            content.push_str(&format!("  > {}\n\n", output.description));
        }
    }

    content
}

/// Extract task list from stage definition
pub(super) fn extract_tasks_from_stage(stage: &Stage) -> Vec<String> {
    let mut tasks = Vec::new();

    if let Some(desc) = &stage.description {
        tasks.extend(extract_tasks_from_description(desc));
    }

    if tasks.is_empty() && !stage.acceptance.is_empty() {
        for criterion in &stage.acceptance {
            tasks.push(criterion.command().to_string());
        }
    }

    tasks
}

/// Extract tasks from markdown description text
///
/// Recognizes:
/// - Bullet lists (- task or * task)
/// - Numbered lists (1. task or 1) task)
pub fn extract_tasks_from_description(description: &str) -> Vec<String> {
    let mut tasks = Vec::new();

    for line in description.lines() {
        let trimmed = line.trim();

        if trimmed.starts_with("- ") || trimmed.starts_with("* ") {
            tasks.push(trimmed[2..].trim().to_string());
        } else if let Some(rest) = trimmed.strip_prefix(|c: char| c.is_ascii_digit()) {
            if let Some(task) = rest.strip_prefix(". ").or_else(|| rest.strip_prefix(") ")) {
                tasks.push(task.trim().to_string());
            }
        }
    }

    tasks
}

/// Format a V2 structured handoff for inclusion in signals
pub(super) fn format_structured_handoff(handoff: &HandoffV2) -> String {
    let mut content = String::new();

    content.push_str(&format!(
        "**Previous Session**: {} | **Context**: {:.1}%\n\n",
        handoff.session_id, handoff.context_percent
    ));

    // Completed tasks
    if !handoff.completed_tasks.is_empty() {
        content.push_str("### Completed Tasks\n\n");
        for task in &handoff.completed_tasks {
            content.push_str(&format!("- {}\n", task.description));
            if !task.files.is_empty() {
                for file in &task.files {
                    content.push_str(&format!("  - `{file}`\n"));
                }
            }
        }
        content.push('\n');
    }

    // Key decisions
    if !handoff.key_decisions.is_empty() {
        content.push_str("### Key Decisions\n\n");
        content.push_str("| Decision | Rationale |\n");
        content.push_str("|----------|----------|\n");
        for decision in &handoff.key_decisions {
            let dec_escaped = decision.decision.replace('|', "\\|");
            let rat_escaped = decision.rationale.replace('|', "\\|");
            content.push_str(&format!("| {dec_escaped} | {rat_escaped} |\n"));
        }
        content.push('\n');
    }

    // Discovered facts
    if !handoff.discovered_facts.is_empty() {
        content.push_str("### Discovered Facts\n\n");
        for fact in &handoff.discovered_facts {
            content.push_str(&format!("- {fact}\n"));
        }
        content.push('\n');
    }

    // Open questions
    if !handoff.open_questions.is_empty() {
        content.push_str("### Open Questions\n\n");
        for question in &handoff.open_questions {
            content.push_str(&format!("- {question}\n"));
        }
        content.push('\n');
    }

    // Next actions (prioritized)
    if !handoff.next_actions.is_empty() {
        content.push_str("### Next Actions (Prioritized)\n\n");
        for (i, action) in handoff.next_actions.iter().enumerate() {
            content.push_str(&format!("{}. {action}\n", i + 1));
        }
        content.push('\n');
    }

    // Git state
    if handoff.branch.is_some()
        || !handoff.commits.is_empty()
        || !handoff.uncommitted_files.is_empty()
    {
        content.push_str("### Git State\n\n");
        if let Some(branch) = &handoff.branch {
            content.push_str(&format!("- **Branch**: {branch}\n"));
        }
        if !handoff.commits.is_empty() {
            content.push_str("- **Commits**:\n");
            for commit in &handoff.commits {
                content.push_str(&format!("  - `{}` {}\n", commit.hash, commit.message));
            }
        }
        if !handoff.uncommitted_files.is_empty() {
            content.push_str("- **Uncommitted Changes**:\n");
            for file in &handoff.uncommitted_files {
                content.push_str(&format!("  - {file}\n"));
            }
        }
        content.push('\n');
    }

    // Files read for context
    if !handoff.files_read.is_empty() {
        content.push_str("### Files Read for Context\n\n");
        for file_ref in &handoff.files_read {
            let ref_str = file_ref.to_ref_string();
            content.push_str(&format!("- `{ref_str}` - {}\n", file_ref.purpose));
        }
        content.push('\n');
    }

    // Files modified
    if !handoff.files_modified.is_empty() {
        content.push_str("### Files Modified\n\n");
        for file in &handoff.files_modified {
            content.push_str(&format!("- `{file}`\n"));
        }
        content.push('\n');
    }

    content
}

/// Append the "stage end sequence" recap emitted at the end of "## Immediate
/// Tasks" in the recitation section — the order commits must follow, restated
/// at maximum attention (Manus recitation pattern) alongside the task list.
pub(super) fn append_stage_end_sequence(content: &mut String) {
    content.push('\n');
    content.push_str("**Stage end sequence (in this order, nothing skipped):** every subagent returned → full gate green → adversarial review returned and every finding fixed → gate green again → commit (orchestrator only, one logical commit per concern) → `loom stage complete <stage-id>`. A commit before this point is premature.\n");
}

/// Append the package-manager-cache carve-out note to the sandbox section,
/// shown whenever the sandbox is enabled (`format/sandbox_section.rs`).
pub(super) fn append_package_cache_note(content: &mut String) {
    content.push_str("**Package-manager caches:** the per-user caches of bun, npm, pnpm, yarn, deno, cargo, rustup, uv, pip and go under your home directory are writable, so `bun install`, `cargo add`, `uv sync`, `go get` and their peers work inside this sandbox. Two limits: a cache directory that does not exist yet at session start is NOT bound (the sandbox skips missing paths), and a cache relocated by an env var (`XDG_CACHE_HOME`, `CARGO_HOME`, `BUN_INSTALL_CACHE_DIR`, ...) is not covered — either one surfaces as `EROFS` / `Read-only file system` from the package manager. That is a sandbox limit, not a bug in your change: STOP and report it as a blocker (it needs a plan-level `sandbox.filesystem.allow_write` entry); do not work around it.\n\n");
}
