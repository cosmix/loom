use crate::handoff::git_handoff::{format_git_history_markdown, GitHistory};
use crate::models::session::Session;
use crate::models::stage::Stage;
use crate::models::worktree::Worktree;

use super::types::{DependencyStatus, EmbeddedContext};

pub fn format_signal_content(
    session: &Session,
    stage: &Stage,
    worktree: &Worktree,
    dependencies_status: &[DependencyStatus],
    handoff_file: Option<&str>,
    git_history: Option<&GitHistory>,
    embedded_context: &EmbeddedContext,
) -> String {
    let mut content = String::new();

    content.push_str(&format!("# Signal: {}\n\n", session.id));

    // Worktree context - self-contained signal with strict isolation
    content.push_str("## Worktree Context\n\n");
    content.push_str(
        "You are in an **isolated git worktree**. This signal contains everything you need:\n\n",
    );
    content.push_str("- **Your stage assignment and acceptance criteria are below** - this file is self-contained\n");
    content.push_str("- **All context (plan overview, handoff, structure map) is embedded below** - reading main repo files is **FORBIDDEN**\n");
    content.push_str(
        "- **Commit to your worktree branch** - it will be merged after verification\n\n",
    );

    // Explicit isolation boundaries
    content.push_str("**Isolation Boundaries (STRICT):**\n\n");
    content.push_str("- You are **CONFINED** to this worktree - do not access files outside it\n");
    content.push_str("- All context you need is embedded below - reading main repo files is **FORBIDDEN**\n");
    content.push_str("- Git commands must target THIS worktree only - no `git -C`, no `cd ../..`\n\n");

    // Path boundaries subsection
    content.push_str("### Path Boundaries\n\n");
    content.push_str("| Type | Paths |\n");
    content.push_str("|------|-------|\n");
    content.push_str("| **ALLOWED** | `.` (this worktree), `.work/` (symlink to orchestration) |\n");
    content.push_str("| **FORBIDDEN** | `../..`, absolute paths to main repo, any path outside worktree |\n\n");

    // Add reminder to follow CLAUDE.md rules
    content.push_str("## Execution Rules\n\n");
    content.push_str("Follow your `~/.claude/CLAUDE.md` and project `CLAUDE.md` rules (both are symlinked into this worktree). Key reminders:\n\n");
    content.push_str("**Worktree Isolation (CRITICAL):**\n");
    content.push_str("- **STAY IN THIS WORKTREE** - never read files from main repo or other worktrees\n");
    content.push_str("- **All context is embedded above** - you have everything you need in this signal\n");
    content.push_str("- **No path escaping** - do not use `../..`, `cd` to parent directories, or absolute paths outside worktree\n\n");
    content.push_str("**Delegation & Efficiency:**\n");
    content.push_str(
        "- **Use PARALLEL subagents** - spawn multiple appropriate subagents concurrently when tasks are independent\n",
    );
    content.push_str("- **Use Skills** - invoke relevant skills wherever applicable\n");
    content.push_str("- **Use TodoWrite** to plan and track progress\n\n");
    content.push_str("**Completion:**\n");
    content.push_str("- **Verify acceptance criteria** before marking stage complete\n");
    content.push_str("- **Create handoff** if context exceeds 75%\n\n");
    content.push_str("**Git Staging (CRITICAL):**\n");
    content.push_str("- **ALWAYS use `git add <specific-files>`** - stage only files you modified\n");
    content.push_str("- **NEVER use `git add -A` or `git add .`** - these include `.work` which must NOT be committed\n");
    content.push_str("- `.work` is a symlink to shared orchestration state - never stage it\n\n");

    content.push_str("## Target\n\n");
    content.push_str(&format!("- **Session**: {}\n", session.id));
    content.push_str(&format!("- **Stage**: {}\n", stage.id));
    if let Some(plan_id) = &stage.plan_id {
        content.push_str(&format!(
            "- **Plan**: {plan_id} (overview embedded below)\n"
        ));
    }
    content.push_str(&format!("- **Worktree**: {}\n", worktree.path.display()));
    content.push_str(&format!("- **Branch**: {}\n", worktree.branch));
    content.push('\n');

    // Embed plan overview if available
    if let Some(plan_overview) = &embedded_context.plan_overview {
        content.push_str("## Plan Overview\n\n");
        content.push_str("<plan-overview>\n");
        content.push_str(plan_overview);
        content.push_str("\n</plan-overview>\n\n");
    }

    content.push_str("## Assignment\n\n");
    content.push_str(&format!("{}: ", stage.name));
    if let Some(desc) = &stage.description {
        content.push_str(desc);
    } else {
        content.push_str("(no description provided)");
    }
    content.push_str("\n\n");

    content.push_str("## Immediate Tasks\n\n");
    let tasks = extract_tasks_from_stage(stage);
    if tasks.is_empty() {
        content.push_str("1. Review stage acceptance criteria below\n");
        content.push_str("2. Implement required changes\n");
        content.push_str("3. Verify all acceptance criteria are met\n");
    } else {
        for (i, task) in tasks.iter().enumerate() {
            content.push_str(&format!("{}. {task}\n", i + 1));
        }
    }
    content.push('\n');

    if !dependencies_status.is_empty() {
        content.push_str("## Dependencies Status\n\n");
        content.push_str(&format_dependency_table(dependencies_status));
        content.push('\n');
    }

    // Embed handoff content if available (previous session context)
    if let Some(handoff_content) = &embedded_context.handoff_content {
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

    // Embed structure.md content if available
    if let Some(structure_content) = &embedded_context.structure_content {
        content.push_str("## Codebase Structure\n\n");
        content.push_str("<structure-map>\n");
        content.push_str(structure_content);
        content.push_str("\n</structure-map>\n\n");
    }

    // Git History from previous session (if resuming)
    if let Some(history) = git_history {
        content.push_str(&format_git_history_markdown(history));
        content.push('\n');
    }

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

    if !stage.files.is_empty() {
        content.push_str("## Files to Modify\n\n");
        for file in &stage.files {
            content.push_str(&format!("- {file}\n"));
        }
        content.push('\n');
    }

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
