use crate::handoff::git_handoff::{format_git_history_markdown, GitHistory};
use crate::models::session::Session;
use crate::models::stage::{Stage, StageType};
use crate::models::worktree::Worktree;
use crate::skills::SkillMatch;

use super::super::types::{DependencyStatus, EmbeddedContext, SandboxSummary};
use super::helpers::{
    extract_tasks_from_stage, format_dependency_outputs, format_dependency_table,
    format_structured_handoff,
};

/// SEMI-STABLE section: Changes per stage, not per session
/// Contains knowledge map references and facts
pub(super) fn format_semi_stable_section(
    embedded_context: &EmbeddedContext,
    stage_type: StageType,
) -> String {
    let mut content = String::new();

    // Knowledge reference via CLI (not embedded to save context tokens)
    if embedded_context.knowledge_has_content {
        content.push_str("## Knowledge Base\n\n");
        content.push_str("**Curated knowledge is available.** Read it BEFORE starting work:\n\n");
        content.push_str("```bash\n");
        content.push_str("loom knowledge show              # Show all knowledge\n");
        content.push_str("loom knowledge show architecture # Architecture overview\n");
        content.push_str("loom knowledge show entry-points # Key entry points\n");
        content.push_str("loom knowledge show patterns     # Architectural patterns\n");
        content.push_str("loom knowledge show conventions  # Coding conventions\n");
        content.push_str("loom knowledge show mistakes     # Lessons learned\n");
        content.push_str("```\n\n");
    }

    // Stage-type-aware reminder boxes
    match stage_type {
        StageType::Knowledge | StageType::IntegrationVerify | StageType::CodeReview => {
            // Knowledge and integration-verify stages: CAN use both memory and knowledge
            content.push_str("```text\n");
            content.push_str(
                "┌────────────────────────────────────────────────────────────────────┐\n",
            );
            content.push_str(
                "│  📝 KNOWLEDGE UPDATES REQUIRED                                     │\n",
            );
            content.push_str(
                "│                                                                    │\n",
            );
            content.push_str(
                "│  As you work, UPDATE doc/loom/knowledge/:                          │\n",
            );
            content.push_str(
                "│  - Entry points: Key files you discover                            │\n",
            );
            content.push_str(
                "│  - Patterns: Architectural patterns you find                       │\n",
            );
            content.push_str(
                "│  - Conventions: Coding conventions you learn                       │\n",
            );
            content.push_str(
                "│  - Mistakes: Errors you make and how to avoid them                 │\n",
            );
            content.push_str(
                "│                                                                    │\n",
            );
            content.push_str(
                "│  Command: loom knowledge update <file> \"content\"                   │\n",
            );
            content.push_str(
                "└────────────────────────────────────────────────────────────────────┘\n",
            );
            content.push_str("```\n\n");
        }
        StageType::Standard => {
            // Standard implementation stages: MEMORY ONLY, NO KNOWLEDGE UPDATES
            content.push_str("```text\n");
            content.push_str(
                "┌────────────────────────────────────────────────────────────────────┐\n",
            );
            content.push_str(
                "│  📝 SESSION MEMORY REQUIRED                                        │\n",
            );
            content.push_str(
                "│                                                                    │\n",
            );
            content.push_str(
                "│  As you work, record insights using MEMORY (NOT knowledge):        │\n",
            );
            content.push_str(
                "│  - Decisions: WHY you chose an approach                            │\n",
            );
            content.push_str(
                "│  - Discoveries: Patterns, gotchas, useful code locations           │\n",
            );
            content.push_str(
                "│  - Mistakes: What went wrong and how to avoid it                   │\n",
            );
            content.push_str(
                "│                                                                    │\n",
            );
            content.push_str(
                "│  Commands: loom memory note/decision/question                      │\n",
            );
            content.push_str(
                "│                                                                    │\n",
            );
            content.push_str(
                "│  ⚠️  NEVER use 'loom knowledge' in implementation stages           │\n",
            );
            content.push_str(
                "│      Memory gets promoted to knowledge by integration-verify       │\n",
            );
            content.push_str(
                "└────────────────────────────────────────────────────────────────────┘\n",
            );
            content.push_str("```\n\n");
        }
    }

    // Knowledge Management section with stage-type-aware content
    match stage_type {
        StageType::Knowledge | StageType::IntegrationVerify | StageType::CodeReview => {
            content.push_str("## Knowledge Management\n\n");

            if !embedded_context.knowledge_has_content {
                // CRITICAL warning for missing/empty knowledge
                content.push_str("```\n");
                content.push_str(
                    "┌────────────────────────────────────────────────────────────────────┐\n",
                );
                content.push_str(
                    "│  CRITICAL: KNOWLEDGE BASE IS EMPTY                                 │\n",
                );
                content.push_str(
                    "│                                                                    │\n",
                );
                content.push_str(
                    "│  Before implementing ANYTHING, you MUST explore and document:     │\n",
                );
                content.push_str(
                    "│                                                                    │\n",
                );
                content.push_str(
                    "│  1. Entry Points                                                   │\n",
                );
                content.push_str(
                    "│     - Main files, CLI entry, API endpoints                         │\n",
                );
                content.push_str(
                    "│                                                                    │\n",
                );
                content.push_str(
                    "│  2. Architectural Patterns                                         │\n",
                );
                content.push_str(
                    "│     - Error handling, state management, data flow                  │\n",
                );
                content.push_str(
                    "│                                                                    │\n",
                );
                content.push_str(
                    "│  3. Coding Conventions                                             │\n",
                );
                content.push_str(
                    "│     - Naming, file structure, testing patterns                     │\n",
                );
                content.push_str(
                    "│                                                                    │\n",
                );
                content.push_str(
                    "│  4. Mistakes and Lessons Learned                                   │\n",
                );
                content.push_str(
                    "│     - Document errors and how to avoid them                        │\n",
                );
                content.push_str(
                    "│                                                                    │\n",
                );
                content.push_str(
                    "│  This prevents wasted context on repeated exploration.             │\n",
                );
                content.push_str(
                    "└────────────────────────────────────────────────────────────────────┘\n",
                );
                content.push_str("```\n\n");

                content.push_str("**Exploration Order (hierarchical):**\n\n");
                content
                    .push_str("1. **Entry Points First** - Find main.rs, index.ts, app.py, etc.\n");
                content.push_str(
                    "2. **Core Modules** - Identify the key abstractions and data flow\n",
                );
                content.push_str(
                    "3. **Patterns** - Document error handling, logging, config approaches\n",
                );
                content.push_str(
                    "4. **Conventions** - Note naming, file organization, test patterns\n\n",
                );
            } else {
                // Standard instructions for established knowledge base
                content.push_str("**Extend the knowledge base** as you work:\n\n");
                content.push_str("- Check for undocumented modules in your working area\n");
                content.push_str("- Record new insights about system behavior\n");
                content.push_str("- Document edge cases and gotchas for future sessions\n\n");
            }

            // Show knowledge commands table for Knowledge and IntegrationVerify stages
            content.push_str("**Commands:**\n\n");
            content.push_str("| Discovery Type | Command |\n");
            content.push_str("|----------------|--------|\n");
            content.push_str("| Key entry point | `loom knowledge update entry-points \"## Section\\n\\n- path/file.rs - description\"` |\n");
            content.push_str("| Architectural pattern | `loom knowledge update patterns \"## Pattern Name\\n\\n- How it works\"` |\n");
            content.push_str("| Coding convention | `loom knowledge update conventions \"## Convention\\n\\n- Details\"` |\n");
            content.push_str("| Mistake/lesson | `loom knowledge update mistakes \"## What happened\\n\\n- Details\"` |\n\n");
        }
        StageType::Standard => {
            // Standard implementation stages: Show MEMORY guidance instead
            content.push_str("## Session Memory\n\n");
            content.push_str(
                "**Record insights as you work** (to be promoted later by integration-verify):\n\n",
            );
            content.push_str("- Decisions and their rationale\n");
            content.push_str("- Code patterns discovered during implementation\n");
            content.push_str("- Mistakes made and how to avoid them\n");
            content.push_str("- Important file locations and their purposes\n\n");

            // Show memory commands table for Standard stages
            content.push_str("**Commands:**\n\n");
            content.push_str("| Type | Command |\n");
            content.push_str("|------|--------|\n");
            content.push_str("| Note | `loom memory note \"observation or discovery\"` |\n");
            content.push_str(
                "| Decision | `loom memory decision \"choice made\" --context \"why\"` |\n",
            );
            content
                .push_str("| Question | `loom memory question \"open question to address\"` |\n");
            content.push_str("| List | `loom memory list` |\n\n");
        }
    }

    // Agent Teams decision framework
    content.push_str("## Agent Teams\n\n");
    content.push_str("You have agent teams available (CLAUDE_CODE_EXPERIMENTAL_AGENT_TEAMS=1).\n\n");
    content.push_str("**When to use SUBAGENTS (Task tool):**\n");
    content.push_str("- Tasks have clear, concrete file assignments\n");
    content.push_str("- No inter-agent communication needed\n");
    content.push_str("- Fire-and-forget parallel work\n\n");
    content.push_str("**When to use AGENT TEAMS:**\n");
    content.push_str("- Tasks require discussion or iterative discovery\n");
    content.push_str("- Work scope may expand during execution\n");
    content.push_str("- Multiple review dimensions (security, quality, tests)\n");
    content.push_str("- Exploration across many code areas\n\n");
    content.push_str("**If you create a team:**\n");
    content.push_str("- Team name: loom-{stage_id} (using your stage ID)\n");
    content.push_str("- YOU are the only agent that may run git commit\n");
    content.push_str("- YOU are the only agent that may run loom stage complete\n");
    content.push_str("- Record teammate findings: loom memory note \"Teammate found: ...\"\n");
    content.push_str("- Keep your own context for coordination (aim for <40% utilization)\n");
    content.push_str("- Delegate implementation, do not implement yourself\n");
    content.push_str("- Shut down all teammates before completing the stage\n\n");

    // Embed sandbox restrictions (semi-stable - based on stage config)
    if let Some(sandbox_summary) = &embedded_context.sandbox_summary {
        content.push_str(&format_sandbox_section(sandbox_summary));
    }

    // Embed skill recommendations (semi-stable - based on stage description)
    if !embedded_context.skill_recommendations.is_empty() {
        content.push_str(&format_skill_recommendations(
            &embedded_context.skill_recommendations,
        ));
    }

    content
}

/// DYNAMIC section: Changes per session
/// Contains current task, handoff, dependencies, git history
pub(super) fn format_dynamic_section(
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

    // Add working_dir and computed execution path
    let working_dir = stage.working_dir.as_deref().unwrap_or(".");
    content.push_str(&format!("- **working_dir**: `{working_dir}`\n"));
    let execution_path = if working_dir == "." {
        worktree.path.display().to_string()
    } else {
        format!("{}/{}", worktree.path.display(), working_dir)
    };
    content.push_str(&format!("- **Execution Path**: `{execution_path}`\n"));
    content.push('\n');

    // Execution path reminder box
    content.push_str("```text\n");
    content.push_str("┌────────────────────────────────────────────────────────────────────┐\n");
    content.push_str("│  📍 WHERE COMMANDS EXECUTE                                         │\n");
    content.push_str("│                                                                    │\n");
    content.push_str(&format!(
        "│  Acceptance criteria run from: {}{}│\n",
        execution_path,
        " ".repeat(39_usize.saturating_sub(execution_path.len()))
    ));
    content.push_str(&format!(
        "│  Formula: WORKTREE + working_dir = {}{}│\n",
        working_dir,
        " ".repeat(29_usize.saturating_sub(working_dir.len()))
    ));
    content.push_str("│                                                                    │\n");
    content.push_str("│  If cargo/npm fails with 'not found', check working_dir setting.   │\n");
    content.push_str("└────────────────────────────────────────────────────────────────────┘\n");
    content.push_str("```\n\n");

    // Add worktree root directory reminder (defense-in-depth)
    content.push_str(&format!(
        "**IMPORTANT:** Before running `loom stage complete`, ensure you are at the worktree root: `cd {}`\n\n",
        &worktree.path.display()
    ));

    // Worktree Isolation section with explicit boundaries
    content.push_str("## Worktree Isolation\n\n");

    // Show both relative and absolute paths
    let relative_path = format!(".worktrees/{}/", stage.id);
    content.push_str(&format!("You are working in: `{relative_path}`\n\n"));

    // Try to get absolute path for clarity
    if let Ok(absolute_path) = worktree.path.canonicalize() {
        content.push_str(&format!(
            "**Absolute path:** `{}`\n\n",
            absolute_path.display()
        ));
    }

    content.push_str("**ALLOWED:**\n");
    content.push_str("- Files within this worktree\n");
    content.push_str("- `.work/` directory (via symlink)\n");
    content.push_str("- Reading `CLAUDE.md` (symlinked)\n");
    content.push_str("- Using loom CLI commands\n\n");

    content.push_str("**FORBIDDEN:**\n");
    content.push_str("- Path traversal (`../../`, `../.worktrees/`)\n");
    content.push_str("- Git operations targeting main repo (`git -C`, `--work-tree`)\n");
    content.push_str("- Direct modification of `.work/stages/` or `.work/sessions/`\n");
    content.push_str("- Attempting to merge your own branch (loom handles merges)\n\n");

    content.push_str(
        "If you need something outside your worktree, **STOP** and explain what you need.\n",
    );
    content.push_str("The orchestrator will handle cross-worktree operations.\n\n");

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

    // Reminder about working_dir for acceptance criteria
    let working_dir = stage.working_dir.as_deref().unwrap_or(".");
    content.push_str(&format!(
        "**Note:** These commands will run from working_dir: `{working_dir}`\n\n"
    ));

    if stage.acceptance.is_empty() {
        content.push_str("- [ ] Implementation complete\n");
        content.push_str("- [ ] Code reviewed and tested\n");
    } else {
        for criterion in &stage.acceptance {
            content.push_str(&format!("- [ ] {criterion}\n"));
        }
    }
    content.push('\n');

    // Goal-backward verification criteria (if defined)
    let has_goal_checks =
        !stage.truths.is_empty() || !stage.artifacts.is_empty() || !stage.wiring.is_empty();

    if has_goal_checks {
        content.push_str("\n## Goal-Backward Verification\n\n");
        content.push_str("Beyond acceptance criteria, verify these OUTCOMES work:\n\n");

        if !stage.truths.is_empty() {
            content.push_str("### Truths (observable behaviors - must return exit 0)\n\n");
            for truth in &stage.truths {
                content.push_str(&format!("```bash\n{truth}\n```\n\n"));
            }
        }

        if !stage.artifacts.is_empty() {
            content.push_str("### Artifacts (files must exist with real implementation)\n\n");
            for artifact in &stage.artifacts {
                content.push_str(&format!("- `{artifact}`\n"));
            }
            content.push('\n');
        }

        if !stage.wiring.is_empty() {
            content.push_str("### Wiring (critical connections to verify)\n\n");
            for check in &stage.wiring {
                content.push_str(&format!(
                    "- **{}**: pattern `{}` in `{}`\n",
                    check.description, check.pattern, check.source
                ));
            }
            content.push('\n');
        }

        content
            .push_str("Run `loom verify <stage-id> --suggest` to check these automatically.\n\n");
    }

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
pub(super) fn format_recitation_section(
    stage: &Stage,
    embedded_context: &EmbeddedContext,
) -> String {
    let mut content = String::new();

    // Context Budget Warning (high attention position - before tasks)
    if let (Some(usage), Some(budget)) = (
        embedded_context.context_usage,
        embedded_context.context_budget,
    ) {
        if usage >= budget * 0.8 {
            // 80% of budget
            content.push_str("## ⚠️ CONTEXT BUDGET WARNING\n\n");
            content.push_str(&format!(
                "Current usage: **{usage:.0}%** | Budget: **{budget:.0}%**\n\n",
            ));

            if usage >= budget {
                content.push_str("```\n");
                content.push_str("┌─────────────────────────────────────────────────┐\n");
                content.push_str("│  🛑 BUDGET EXCEEDED - HANDOFF REQUIRED         │\n");
                content.push_str("│  Run: loom memory promote all mistakes          │\n");
                content.push_str("│  Then: loom stage complete <stage-id>           │\n");
                content.push_str("└─────────────────────────────────────────────────┘\n");
                content.push_str("```\n");
            } else {
                content.push_str("**Approaching budget limit.** Prepare for handoff:\n");
                content.push_str("- `loom memory note` to record remaining observations\n");
                content.push_str("- `loom memory promote all mistakes` before completing\n");
            }
            content.push('\n');
        }
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
    content.push_str("## Session Memory\n\n");
    if let Some(memory_content) = &embedded_context.memory_content {
        content.push_str("**YOUR WORKING MEMORY** - Notes and decisions from this session:\n\n");
        content.push_str(memory_content);
        content.push('\n');
    } else {
        // CRITICAL: Show prominent prompt when memory is empty
        content.push_str("```\n");
        content.push_str("┌─────────────────────────────────────────────────────────────┐\n");
        content.push_str("│  ⚠️  NO MEMORY ENTRIES RECORDED FOR THIS SESSION            │\n");
        content.push_str("│                                                             │\n");
        content.push_str("│  Memory is MANDATORY. Record as you work:                   │\n");
        content.push_str("│  - Decisions: WHY you chose an approach                     │\n");
        content.push_str("│  - Discoveries: Patterns, gotchas, useful code locations    │\n");
        content.push_str("│  - Mistakes: What went wrong and how to avoid it            │\n");
        content.push_str("│                                                             │\n");
        content.push_str("│  Empty memory = lost learning = repeated mistakes           │\n");
        content.push_str("└─────────────────────────────────────────────────────────────┘\n");
        content.push_str("```\n\n");
    }
    content.push_str("**Memory Commands:**\n");
    content.push_str("- `loom memory note \"observation\"` - Record a discovery\n");
    content.push_str(
        "- `loom memory decision \"choice\" --context \"rationale\"` - Record a decision\n",
    );
    content.push_str("- `loom memory question \"open question\"` - Record an open question\n");
    content.push_str("- `loom memory list` - Review your session entries\n");
    content.push_str("- `loom memory promote all mistakes` - Promote insights to knowledge (BEFORE completing)\n\n");

    content
}

/// Format sandbox restrictions for agent awareness
fn format_sandbox_section(summary: &SandboxSummary) -> String {
    let mut content = String::new();

    if !summary.enabled {
        content.push_str("## Sandbox Status\n\n");
        content.push_str("**Sandbox is DISABLED** for this stage.\n\n");
        return content;
    }

    content.push_str("## Sandbox Restrictions\n\n");
    content.push_str("The following restrictions are in effect for this session:\n\n");

    // Filesystem restrictions
    if !summary.deny_read.is_empty() || !summary.deny_write.is_empty() {
        content.push_str("### Filesystem\n\n");

        if !summary.deny_read.is_empty() {
            content.push_str("**Cannot Read:**\n");
            for path in &summary.deny_read {
                content.push_str(&format!("- `{}`\n", path));
            }
            content.push('\n');
        }

        if !summary.deny_write.is_empty() {
            content.push_str("**Cannot Write:**\n");
            for path in &summary.deny_write {
                content.push_str(&format!("- `{}`\n", path));
            }
            content.push('\n');
        }

        if !summary.allow_write.is_empty() {
            content.push_str("**Exceptions (CAN Write):**\n");
            for path in &summary.allow_write {
                content.push_str(&format!("- `{}`\n", path));
            }
            content.push('\n');
        }
    }

    // Network restrictions
    if !summary.allowed_domains.is_empty() {
        content.push_str("### Network\n\n");
        content.push_str("**Allowed Domains:**\n");
        for domain in &summary.allowed_domains {
            content.push_str(&format!("- `{}`\n", domain));
        }
        content.push('\n');
    } else {
        content.push_str("### Network\n\n");
        content.push_str("**No network access allowed.**\n\n");
    }

    // Excluded commands
    if !summary.excluded_commands.is_empty() {
        content.push_str("### Excluded Commands\n\n");
        content.push_str("These commands bypass sandbox restrictions:\n");
        for cmd in &summary.excluded_commands {
            content.push_str(&format!("- `{}`\n", cmd));
        }
        content.push('\n');
    }

    content
}

/// Format task progression information for inclusion in signals
pub fn format_skill_recommendations(skills: &[SkillMatch]) -> String {
    let mut content = String::new();

    content.push_str("## Recommended Skills\n\n");
    content.push_str("Based on your task, these skills may be helpful:\n\n");

    content.push_str("| Skill | Description | Invoke |\n");
    content.push_str("|-------|-------------|--------|\n");

    for skill in skills {
        // Truncate description if too long for table
        let desc = if skill.description.len() > 60 {
            format!("{}...", &skill.description[..57])
        } else {
            skill.description.clone()
        };
        // Escape pipe characters in description
        let desc = desc.replace('|', "\\|");

        content.push_str(&format!(
            "| {} | {} | `/{}`|\n",
            skill.name, desc, skill.name
        ));
    }
    content.push('\n');

    // Show which triggers matched for transparency
    content.push_str("**Matched triggers:**\n");
    for skill in skills {
        if !skill.matched_triggers.is_empty() {
            let triggers = skill.matched_triggers.join(", ");
            content.push_str(&format!("- `{}`: {}\n", skill.name, triggers));
        }
    }
    content.push('\n');

    content
}
