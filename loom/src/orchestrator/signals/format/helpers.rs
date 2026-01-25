use crate::handoff::schema::HandoffV2;
use crate::models::stage::Stage;

use super::super::types::DependencyStatus;

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
            tasks.push(criterion.clone());
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
pub(super) fn format_task_progression(task_state: &crate::checkpoints::TaskState) -> String {
    let mut content = String::new();

    if task_state.tasks.is_empty() {
        return content;
    }

    content.push_str("## Task Progression\n\n");

    // Current task
    if let Some(current) = task_state.current_task() {
        content.push_str(&format!(
            "**Current Task**: `{}` - {}\n\n",
            current.id, current.instruction
        ));
    }

    // Task status table
    content.push_str("| Task | Status | Instruction |\n");
    content.push_str("|------|--------|-------------|\n");

    for task in &task_state.tasks {
        let status = if task_state.completed_tasks.contains_key(&task.id) {
            "Completed"
        } else if task_state.is_task_unlocked(&task.id) {
            "Unlocked"
        } else {
            "Locked"
        };

        let instruction = task.instruction.replace('|', "\\|");
        content.push_str(&format!("| {} | {} | {} |\n", task.id, status, instruction));
    }
    content.push('\n');

    // Verification notes for current task
    if let Some(current) = task_state.current_task() {
        if !current.verification.is_empty() {
            content.push_str("**Verification for current task:**\n\n");
            for rule in &current.verification {
                match rule {
                    crate::checkpoints::VerificationRule::FileExists { path } => {
                        content.push_str(&format!("- File must exist: `{path}`\n"));
                    }
                    crate::checkpoints::VerificationRule::Contains { path, pattern } => {
                        content
                            .push_str(&format!("- `{path}` must contain pattern: `{pattern}`\n"));
                    }
                    crate::checkpoints::VerificationRule::Command {
                        cmd,
                        expected_exit_code,
                    } => {
                        content.push_str(&format!(
                            "- Command `{cmd}` must exit with code {expected_exit_code}\n"
                        ));
                    }
                    crate::checkpoints::VerificationRule::OutputSet { key } => {
                        content.push_str(&format!("- Output `{key}` must be set\n"));
                    }
                }
            }
            content.push('\n');
        }
    }

    // Checkpoint instructions
    content.push_str("**To complete a task**, run:\n");
    content.push_str("```bash\n");
    content.push_str("loom checkpoint create <task-id> --status completed\n");
    content.push_str("```\n\n");

    content
}

