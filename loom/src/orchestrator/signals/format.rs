use crate::handoff::git_handoff::{format_git_history_markdown, GitHistory};
use crate::handoff::schema::HandoffV2;
use crate::models::session::Session;
use crate::models::stage::Stage;
use crate::models::worktree::Worktree;

use super::cache::{generate_stable_prefix, SignalMetrics};
use super::types::{DependencyStatus, EmbeddedContext};

/// Result of formatting a signal with structured sections
pub struct FormattedSignal {
    /// The complete signal content
    pub content: String,
    /// Metrics about the signal sections
    pub metrics: SignalMetrics,
}

/// Format signal content using structured sections for KV-cache efficiency
///
/// Signal sections (Manus pattern):
/// 1. STABLE PREFIX - Fixed header that never changes (cached by KV-cache)
/// 2. SEMI-STABLE - Changes per stage (knowledge map, facts, learnings)
/// 3. DYNAMIC - Changes per session (current task, handoff, dependencies)
/// 4. RECITATION - At end for maximum attention (memory, immediate tasks)
pub fn format_signal_content(
    session: &Session,
    stage: &Stage,
    worktree: &Worktree,
    dependencies_status: &[DependencyStatus],
    handoff_file: Option<&str>,
    git_history: Option<&GitHistory>,
    embedded_context: &EmbeddedContext,
) -> String {
    let formatted = format_signal_with_metrics(
        session,
        stage,
        worktree,
        dependencies_status,
        handoff_file,
        git_history,
        embedded_context,
    );
    formatted.content
}

/// Format signal content with metrics about section sizes
pub fn format_signal_with_metrics(
    session: &Session,
    stage: &Stage,
    worktree: &Worktree,
    dependencies_status: &[DependencyStatus],
    handoff_file: Option<&str>,
    git_history: Option<&GitHistory>,
    embedded_context: &EmbeddedContext,
) -> FormattedSignal {
    // Build each section separately for metrics
    let header = format!("# Signal: {}\n\n", &session.id);
    let stable_prefix = generate_stable_prefix();
    let semi_stable = format_semi_stable_section(embedded_context);
    let dynamic = format_dynamic_section(
        session,
        stage,
        worktree,
        dependencies_status,
        handoff_file,
        git_history,
        embedded_context,
    );
    let recitation = format_recitation_section(stage, embedded_context);

    // Combine stable prefix with header for hash (header is session-specific but tiny)
    let stable_with_header = format!("{header}{stable_prefix}");

    let metrics =
        SignalMetrics::from_sections(&stable_with_header, &semi_stable, &dynamic, &recitation);

    let content = format!("{stable_with_header}{semi_stable}{dynamic}{recitation}");

    FormattedSignal { content, metrics }
}

/// SEMI-STABLE section: Changes per stage, not per session
/// Contains knowledge map references, facts, and learnings from learning loop
fn format_semi_stable_section(embedded_context: &EmbeddedContext) -> String {
    let mut content = String::new();

    // Embed knowledge summary (curated entry points, patterns, conventions)
    if let Some(knowledge_summary) = &embedded_context.knowledge_summary {
        content.push_str("<knowledge>\n");
        content.push_str(knowledge_summary);
        content.push_str("\n</knowledge>\n\n");
    }

    // Knowledge Management section with conditional urgency
    content.push_str("## Knowledge Management\n\n");

    if !embedded_context.knowledge_exists || embedded_context.knowledge_is_empty {
        // CRITICAL warning for missing/empty knowledge
        content.push_str("```\n");
        content
            .push_str("┌────────────────────────────────────────────────────────────────────┐\n");
        content
            .push_str("│  CRITICAL: KNOWLEDGE BASE IS EMPTY                                 │\n");
        content
            .push_str("│                                                                    │\n");
        content.push_str("│  Before implementing ANYTHING, you MUST explore and document:     │\n");
        content
            .push_str("│                                                                    │\n");
        content
            .push_str("│  1. Entry Points                                                   │\n");
        content
            .push_str("│     - Main files, CLI entry, API endpoints                         │\n");
        content
            .push_str("│                                                                    │\n");
        content
            .push_str("│  2. Architectural Patterns                                         │\n");
        content
            .push_str("│     - Error handling, state management, data flow                  │\n");
        content
            .push_str("│                                                                    │\n");
        content
            .push_str("│  3. Coding Conventions                                             │\n");
        content
            .push_str("│     - Naming, file structure, testing patterns                     │\n");
        content
            .push_str("│                                                                    │\n");
        content
            .push_str("│  This prevents wasted context on repeated exploration.             │\n");
        content
            .push_str("└────────────────────────────────────────────────────────────────────┘\n");
        content.push_str("```\n\n");

        content.push_str("**Exploration Order (hierarchical):**\n\n");
        content.push_str("1. **Entry Points First** - Find main.rs, index.ts, app.py, etc.\n");
        content.push_str("2. **Core Modules** - Identify the key abstractions and data flow\n");
        content.push_str("3. **Patterns** - Document error handling, logging, config approaches\n");
        content.push_str("4. **Conventions** - Note naming, file organization, test patterns\n\n");
    } else {
        // Standard instructions for established knowledge base
        content.push_str("**Extend the knowledge base** as you work:\n\n");
        content.push_str("- Check for undocumented modules in your working area\n");
        content.push_str("- Record new insights about system behavior\n");
        content.push_str("- Document edge cases and gotchas for future sessions\n\n");
    }

    // Always show commands table at the end
    content.push_str("**Commands:**\n\n");
    content.push_str("| Discovery Type | Command |\n");
    content.push_str("|----------------|--------|\n");
    content.push_str("| Key entry point | `loom knowledge update entry-points \"## Section\\n\\n- path/file.rs - description\"` |\n");
    content.push_str("| Architectural pattern | `loom knowledge update patterns \"## Pattern Name\\n\\n- How it works\"` |\n");
    content.push_str("| Coding convention | `loom knowledge update conventions \"## Convention\\n\\n- Details\"` |\n\n");

    // Embed facts for this stage (semi-stable - facts accumulate but rarely change)
    if let Some(facts_content) = &embedded_context.facts_content {
        content.push_str("## Known Facts\n\n");
        content.push_str("These facts were recorded by previous stages or this stage. High-confidence facts from other stages are included.\n\n");
        content.push_str(facts_content);
        content.push('\n');
        content.push_str(
            "To add a new fact: `loom fact set <key> <value> [--confidence high|medium|low]`\n\n",
        );
    }

    // Embed learnings from learning loop (semi-stable - accumulated wisdom)
    if let Some(learnings_content) = &embedded_context.learnings_content {
        content.push_str("## Recent Learnings\n\n");
        content.push_str("**REVIEW THESE BEFORE STARTING** - Lessons from previous sessions:\n\n");
        content.push_str(learnings_content);
        content.push_str("To record a new learning:\n");
        content.push_str("- `loom learn mistake \"description\" [--correction \"fix\"]`\n");
        content.push_str("- `loom learn pattern \"description\"`\n");
        content.push_str("- `loom learn convention \"description\"`\n\n");
    }

    content
}

/// DYNAMIC section: Changes per session
/// Contains current task, handoff, dependencies, git history
fn format_dynamic_section(
    session: &Session,
    stage: &Stage,
    worktree: &Worktree,
    dependencies_status: &[DependencyStatus],
    handoff_file: Option<&str>,
    git_history: Option<&GitHistory>,
    embedded_context: &EmbeddedContext,
) -> String {
    let mut content = String::new();

    // Target section (session-specific)
    content.push_str("## Target\n\n");
    content.push_str(&format!("- **Session**: {}\n", &session.id));
    content.push_str(&format!("- **Stage**: {}\n", &stage.id));
    if let Some(plan_id) = &stage.plan_id {
        content.push_str(&format!(
            "- **Plan**: {plan_id} (overview embedded below)\n"
        ));
    }
    content.push_str(&format!("- **Worktree**: {}\n", &worktree.path.display()));
    content.push_str(&format!("- **Branch**: {}\n", &worktree.branch));
    content.push('\n');

    // Embed plan overview if available
    if let Some(plan_overview) = &embedded_context.plan_overview {
        content.push_str("## Plan Overview\n\n");
        content.push_str("<plan-overview>\n");
        content.push_str(plan_overview);
        content.push_str("\n</plan-overview>\n\n");
    }

    // Assignment section
    content.push_str("## Assignment\n\n");
    content.push_str(&format!("{}: ", &stage.name));
    if let Some(desc) = &stage.description {
        content.push_str(desc);
    } else {
        content.push_str("(no description provided)");
    }
    content.push_str("\n\n");

    // Dependencies status (dynamic - status changes)
    if !dependencies_status.is_empty() {
        content.push_str("## Dependencies Status\n\n");
        content.push_str(&format_dependency_table(dependencies_status));
        content.push('\n');

        // Include outputs from completed dependencies
        let deps_with_outputs: Vec<_> = dependencies_status
            .iter()
            .filter(|d| !d.outputs.is_empty())
            .collect();

        if !deps_with_outputs.is_empty() {
            content.push_str("## Dependency Outputs\n\n");
            content.push_str(&format_dependency_outputs(&deps_with_outputs));
            content.push('\n');
        }
    }

    // Embed handoff content if available (previous session context)
    if let Some(parsed) = &embedded_context.parsed_handoff {
        // V2 structured handoff: show structured summary
        content.push_str("## Previous Session Handoff (Structured)\n\n");
        content.push_str(&format_structured_handoff(parsed));
        content.push('\n');
    } else if let Some(handoff_content) = &embedded_context.handoff_content {
        // V1 prose handoff: embed raw content
        content.push_str("## Previous Session Handoff\n\n");
        content.push_str(
            "**READ THIS CAREFULLY** - This contains context from the previous session:\n\n",
        );
        content.push_str("<handoff>\n");
        content.push_str(handoff_content);
        content.push_str("\n</handoff>\n\n");
    } else if let Some(handoff) = handoff_file {
        // Fallback reference if content couldn't be read
        content.push_str("## Context Restoration\n\n");
        content.push_str(&format!(
            "- `.work/handoffs/{handoff}.md` - **READ THIS FIRST** - Previous session handoff\n\n"
        ));
    }

    // Git History from previous session (if resuming)
    if let Some(history) = git_history {
        content.push_str(&format_git_history_markdown(history));
        content.push('\n');
    }

    // Acceptance Criteria (stage-specific but part of dynamic for ordering)
    content.push_str("## Acceptance Criteria\n\n");
    if stage.acceptance.is_empty() {
        content.push_str("- [ ] Implementation complete\n");
        content.push_str("- [ ] Code reviewed and tested\n");
    } else {
        for criterion in &stage.acceptance {
            content.push_str(&format!("- [ ] {criterion}\n"));
        }
    }
    content.push('\n');

    // Files to modify
    if !stage.files.is_empty() {
        content.push_str("## Files to Modify\n\n");
        for file in &stage.files {
            content.push_str(&format!("- {file}\n"));
        }
        content.push('\n');
    }

    content
}

/// RECITATION section: At end for maximum attention (Manus pattern)
/// Contains immediate tasks, task progression, and session memory
fn format_recitation_section(stage: &Stage, embedded_context: &EmbeddedContext) -> String {
    let mut content = String::new();

    // Task progression section (if task state is available)
    if let Some(task_state) = &embedded_context.task_state {
        content.push_str(&format_task_progression(task_state));
    }

    // Immediate tasks - recited at end for attention
    content.push_str("## Immediate Tasks\n\n");
    let tasks = extract_tasks_from_stage(stage);
    if tasks.is_empty() {
        content.push_str("1. Review stage acceptance criteria above\n");
        content.push_str("2. Implement required changes\n");
        content.push_str("3. Verify all acceptance criteria are met\n");
    } else {
        for (i, task) in tasks.iter().enumerate() {
            content.push_str(&format!("{}. {task}\n", i + 1));
        }
    }
    content.push('\n');

    // Embed session memory at the END for maximum attention (Manus recitation pattern)
    // Recent notes, decisions, and questions stay in agent's working memory
    if let Some(memory_content) = &embedded_context.memory_content {
        content.push_str("## Session Memory\n\n");
        content.push_str("**YOUR WORKING MEMORY** - Notes and decisions from this session:\n\n");
        content.push_str(memory_content);
        content.push_str("To record to memory:\n");
        content.push_str("- `loom memory note \"observation\"`\n");
        content.push_str("- `loom memory decision \"choice\" --context \"rationale\"`\n");
        content.push_str("- `loom memory question \"open question\"`\n\n");
    }

    content
}

/// Format task progression information for inclusion in signals
fn format_task_progression(task_state: &crate::checkpoints::TaskState) -> String {
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
fn format_dependency_outputs(deps: &[&DependencyStatus]) -> String {
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

fn extract_tasks_from_stage(stage: &Stage) -> Vec<String> {
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

pub(super) fn extract_tasks_from_description(description: &str) -> Vec<String> {
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
fn format_structured_handoff(handoff: &HandoffV2) -> String {
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
