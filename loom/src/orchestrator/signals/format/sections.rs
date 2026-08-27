use crate::codex::{
    CODEX_FORWARD_SENTINEL, CODEX_IMPLEMENTER_EFFORT, CODEX_IMPLEMENTER_MODEL_LUNA,
    CODEX_IMPLEMENTER_MODEL_TERRA,
};
use crate::handoff::git_handoff::{format_git_history_markdown, GitHistory};
use crate::models::session::Session;
use crate::models::stage::{Implementers, Stage, StageType};
use crate::models::worktree::Worktree;
use crate::skills::SkillMatch;

use super::super::retrieval::STAGE_QUERY_INPUTS;
use super::super::types::{DependencyStatus, EmbeddedContext};
use super::brief::format_knowledge_brief;
use super::helpers::append_stage_end_sequence;
use super::helpers::{
    append_budget_exceeded_box, extract_tasks_from_stage, format_dependency_outputs,
    format_dependency_table, format_structured_handoff,
};
use super::sandbox_section::format_sandbox_section;

/// SEMI-STABLE section: per-stage content (brief, facts), never per-session
pub(super) fn format_semi_stable_section(
    embedded_context: &EmbeddedContext,
    stage_type: StageType,
    stage_id: &str,
) -> String {
    let mut content = String::new();

    // Per-stage `## Knowledge Brief`, gated on retrieval having selected
    // anything. Labelled with the query's INPUT FIELDS, never `pack.query`
    // itself: that is the whole stage description, re-embedded a second time.
    if let Some(pack) = &embedded_context.context_pack {
        content.push_str(&format_knowledge_brief(pack, stage_id, STAGE_QUERY_INPUTS));
    }

    // Stage-type-aware reminder boxes
    match stage_type {
        StageType::Knowledge | StageType::IntegrationVerify | StageType::KnowledgeDistill => {
            // Knowledge, integration-verify, and knowledge-distill stages: CAN use both memory and knowledge
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
                "│  📝 SESSION MEMORY REQUIRED — RECORD AS YOU GO                    │\n",
            );
            content.push_str(
                "│                                                                    │\n",
            );
            content.push_str(
                "│  Record IMMEDIATELY when these happen (not at stage end):          │\n",
            );
            content.push_str(
                "│  Write each as ADVICE to your future self, not just a log entry.  │\n",
            );
            content.push_str(
                "│  - MISTAKE: tried X, failed → loom memory note \"mistake: ...\"     │\n",
            );
            content.push_str(
                "│  - DECISION: chose X over Y → loom memory decision \"...\"          │\n",
            );
            content.push_str(
                "│  - SURPRISE: unexpected behavior → loom memory note \"found: ...\"  │\n",
            );
            content.push_str(
                "│  - GOTCHA: trap for future agents → loom memory note \"gotcha: ...\"│\n",
            );
            content.push_str(
                "│                                                                    │\n",
            );
            content.push_str(
                "│  Do NOT record: procedural actions, obvious outcomes, task recaps  │\n",
            );
            content.push_str(
                "│                                                                    │\n",
            );
            content.push_str(
                "│  ⚠️  NEVER use 'loom knowledge' in implementation stages           │\n",
            );
            content.push_str(
                "│      Memory gets curated into knowledge by knowledge-distill       │\n",
            );
            content.push_str(
                "│                                                                    │\n",
            );
            content.push_str(
                "│  ⛔  NEVER use Claude Code's auto-memory system                   │\n",
            );
            content.push_str(
                "│      NEVER Write/Edit files under ~/.claude/projects/*/memory/    │\n",
            );
            content.push_str(
                "│      Use ONLY 'loom memory' commands — auto-memory is INVISIBLE   │\n",
            );
            content.push_str(
                "│      to loom and other stages. Anything saved there is LOST.      │\n",
            );
            content.push_str(
                "└────────────────────────────────────────────────────────────────────┘\n",
            );
            content.push_str("```\n\n");
        }
    }

    // Knowledge Management section with stage-type-aware content
    match stage_type {
        StageType::Knowledge | StageType::IntegrationVerify | StageType::KnowledgeDistill => {
            content.push_str("## Knowledge Management\n\n");

            if embedded_context.knowledge_tree_empty {
                // CRITICAL warning for a knowledge tree with no content at all
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
                    "4. **Conventions** - Note naming, file organization, test patterns\n",
                );
                content.push_str("5. **Coverage** - Cover ALL areas you touch — no sampling.\n\n");
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
            content.push_str("| Mistake/lesson | `loom knowledge update mistakes \"## What happened\\n\\n- Details\"` |\n");
            content.push_str("| Detail too long for a tier-1 file | `loom knowledge update <category>/<slug> \"## Section\\n\\n- Details\"` |\n\n");
            content.push_str(
                "**For long content, use stdin:** `loom knowledge update <file> - <<'EOF'`\n\n",
            );

            // Review document generation (knowledge-distill only)
            if stage_type == StageType::KnowledgeDistill {
                content.push_str("**Review Document:**\n\n");
                content.push_str("Generate a change summary for human reviewers:\n");
                content.push_str("```bash\nloom review\n```\n\n");

                // Documentation update reminder (knowledge-distill only)
                content.push_str("```text\n");
                content.push_str(
                    "┌────────────────────────────────────────────────────────────────────┐\n",
                );
                content.push_str(
                    "│  📄 DOCUMENTATION UPDATE REQUIRED                                  │\n",
                );
                content.push_str(
                    "│                                                                    │\n",
                );
                content.push_str(
                    "│  Update user-facing docs to reflect this plan's changes:           │\n",
                );
                content.push_str(
                    "│  - README.md: new commands, features, config, workflows            │\n",
                );
                content.push_str(
                    "│  - CONTRIBUTING.md: new dev patterns, build steps                  │\n",
                );
                content.push_str(
                    "│  - Other docs referencing changed functionality                    │\n",
                );
                content.push_str(
                    "│                                                                    │\n",
                );
                content.push_str(
                    "│  Only update relevant sections. Skip if no user-facing changes.    │\n",
                );
                content.push_str(
                    "│                                                                    │\n",
                );
                content.push_str(
                    "│  ⚠️  System knowledge → loom knowledge update                      │\n",
                );
                content.push_str(
                    "│  Do NOT modify the project's CLAUDE.md — it is the user's file.   │\n",
                );
                content.push_str(
                    "└────────────────────────────────────────────────────────────────────┘\n",
                );
                content.push_str("```\n\n");
            }
        }
        StageType::Standard => {
            // Standard implementation stages: Show MEMORY guidance instead
            content.push_str("## Stage Memory\n\n");
            content.push_str(
                "**Record insights AS THEY HAPPEN** — not at stage end. Curated later by the knowledge-distill stage.\n\n",
            );
            content.push_str("**Record IMMEDIATELY when:**\n\n");
            content.push_str("- You make a mistake or hit an error and fix it\n");
            content.push_str("- The user corrects your approach or gives feedback\n");
            content.push_str("- You choose between two or more approaches\n");
            content.push_str("- You discover something surprising or non-obvious about the code\n");
            content.push_str("- You find a gotcha that would trap future agents\n\n");
            content.push_str("**Do NOT record:** procedural actions (\"spawned agents\", \"read file\"), obvious outcomes (\"tests passed\"), task restatements\n\n");

            // Show memory commands table for Standard stages
            content.push_str("**Commands:**\n\n");
            content.push_str("| Trigger | Command | Example |\n");
            content.push_str("|---------|---------|--------|\n");
            content.push_str("| Mistake | `loom memory note \"mistake: ...\"` | `\"mistake: used wrong module path, fs::read vs std::fs::read\"` |\n");
            content.push_str("| Decision | `loom memory decision \"...\" --context \"...\"` | `\"chose serde_json\" --context \"need streaming, serde_yaml too slow\"` |\n");
            content.push_str("| Discovery | `loom memory note \"found: ...\"` | `\"found: config is loaded lazily in main.rs:45\"` |\n");
            content.push_str("| Gotcha | `loom memory note \"gotcha: ...\"` | `\"gotcha: stage IDs must be lowercase despite docs showing mixed\"` |\n");
            content.push_str("| File change | `loom memory change \"...\"` | `\"src/lib.rs - added new module export for feature X\"` |\n");
            content.push_str("| Question | `loom memory question \"...\"` | `\"should we deprecate the old API or keep both?\"` |\n");
            content.push_str("| List | `loom memory list` | Review your entries |\n\n");
        }
    }

    // Delegation decision framework (flat subagents vs hierarchy vs teams)
    content.push_str("## Delegation Choices\n\n");
    content
        .push_str("You have agent teams available (CLAUDE_CODE_EXPERIMENTAL_AGENT_TEAMS=1).\n\n");
    content.push_str("**When to use SUBAGENTS (Task tool, flat):**\n");
    content.push_str("- ~6 or fewer tasks with clear, concrete file assignments\n");
    content.push_str("- No inter-agent communication needed\n");
    content.push_str("- Fire-and-forget parallel work\n\n");
    content
        .push_str("**When to use a SUBAGENT HIERARCHY (coordinators → workers, 2-LEVEL CAP):**\n");
    content.push_str("- >~6 independent worker tasks, or results would bloat your context\n");
    content.push_str("- Work clusters into DISJOINT file territories\n");
    content.push_str("- Each coordinator subagent owns one territory, spawns sonnet workers BY TYPE, runs at most ONE narrowly-scoped check, returns a compact summary\n");
    content.push_str("- Workers NEVER spawn subagents\n\n");
    content.push_str("**When to use a `loom-advisor` (fable):**\n");
    content.push_str("- On a second failure on the same task, spawn one instead of a blind retry — narrow scope, full detail supplied by the orchestrator, advice returned\n");
    content.push_str("- Read-only: it diagnoses and recommends a next step; it never writes\n\n");
    content.push_str("**When to use AGENT TEAMS (~7x cost):**\n");
    content.push_str("- Tasks require discussion or iterative discovery\n");
    content.push_str("- Work scope may expand during execution\n");
    content.push_str("- Multiple review dimensions (security, quality, tests)\n");
    content.push_str("- Exploration across many code areas\n\n");
    content.push_str("**If you create a team:**\n");
    content.push_str("- Team name: loom-{stage_id} (using your stage ID)\n");
    content.push_str("- YOU are the only agent that may run git commit — and only at the END of the stage, after every teammate has returned and verification is green\n");
    content.push_str("- YOU are the only agent that may run loom stage complete\n");
    content.push_str("- Record teammate findings: loom memory note \"Teammate found: ...\"\n");
    content.push_str("- Keep your own context for coordination (aim for <40% utilization)\n");
    content.push_str("- Delegate implementation, do not implement yourself\n");
    content.push_str("- Shut down all teammates before completing the stage\n\n");

    // Codex implementer doctrine (semi-stable - gated on codex being one of the
    // stage's licensed lanes, whether or not it is the preferred one)
    if embedded_context.implementers.includes_codex() {
        content.push_str(&format_codex_implementers_section(
            &embedded_context.implementers,
            embedded_context.codex_available,
        ));
    }

    // Per-subagent response budget (semi-stable - gated on an explicit plan value).
    // The orchestrator measures the stage against this budget from the outside, so
    // the session has to be told the same number or it is held to a deadline it
    // cannot see. The general "how do I check on a subagent" doctrine (BLOCK-C) is
    // unconditional and already in this stage's stable prefix; this block only
    // layers the stage-specific number on top.
    if let Some(timeout_secs) = embedded_context.subagent_timeout_secs {
        content.push_str(&super::helpers::format_subagent_timeout_section(
            timeout_secs,
        ));
    }

    // Ultracode license (semi-stable - gated on the stage's ultracode flag)
    if embedded_context.ultracode {
        content.push_str("## Ultracode Mode\n\n");
        content.push_str(
            "This stage is licensed for ultracode workflow orchestration. Use the Workflow tool\n",
        );
        content.push_str(
            "for the large fan-out this stage was designed for; scale agent count to the work.\n",
        );
        content.push_str(
            "Workflow agents are subagents (Rule 5 applies — no commits, no stage completion);\n",
        );
        content.push_str(
            "the main agent runs acceptance criteria and commits. Do not exceed the stage's\n",
        );
        content.push_str("scope just because orchestration is available.\n");
        content.push_str(
            "Workflow fan-out spawns CLAUDE subagents only: the codex lane (gpt-5.6-terra /\n",
        );
        content.push_str(
            "gpt-5.6-luna) is NOT addressable from a Workflow script. On a stage licensed for\n",
        );
        content.push_str(
            "both, codex work goes through normal `loom-codex-forwarder` Agent spawns, outside\n",
        );
        content.push_str("the Workflow.\n\n");
    }

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
    content.push_str(&format!("- **Session**: {}\n", session.id));
    content.push_str(&format!("- **Stage**: {}\n", stage.id));
    if let Some(plan_id) = &stage.plan_id {
        content.push_str(&format!(
            "- **Plan**: {plan_id} (overview embedded below)\n"
        ));
    }
    content.push_str(&format!("- **Worktree**: {}\n", worktree.path.display()));
    content.push_str(&format!("- **Branch**: {}\n", worktree.branch));

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
        worktree.path.display()
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

    // Cross-stage change summary (integration-verify only)
    if let Some(summary) = &embedded_context.cross_stage_summary {
        content.push_str(summary);
        content.push_str("\n\n");
    }

    // Wiring checklist from stage memories
    if let Some(checklist) = &embedded_context.wiring_checklist {
        content.push_str(checklist);
        content.push_str("\n\n");
    }

    // Assignment section
    content.push_str("## Assignment\n\n");
    content.push_str(&format!("{}: ", stage.name));
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
            content.push_str(&format!("- [ ] {}\n", criterion.command()));
        }
    }
    content.push('\n');

    // Goal-backward verification criteria (if defined)
    if stage.has_any_goal_checks() {
        content.push_str("\n## Goal-Backward Verification\n\n");
        content.push_str("Beyond acceptance criteria, verify these OUTCOMES work:\n\n");

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

        // Wiring tests (wiring_tests field)
        if !stage.wiring_tests.is_empty() {
            content.push_str("### Wiring Tests (integration commands)\n\n");
            for test in &stage.wiring_tests {
                content.push_str(&format!("**{}:** `{}`\n", test.name, test.command));
                if let Some(desc) = &test.description {
                    content.push_str(&format!("  *{}*\n", desc));
                }
                let mut criteria = Vec::new();
                if let Some(code) = test.success_criteria.exit_code {
                    criteria.push(format!("exit code: {}", code));
                }
                if !test.success_criteria.stdout_contains.is_empty() {
                    criteria.push(format!(
                        "stdout contains: {}",
                        test.success_criteria.stdout_contains.join(", ")
                    ));
                }
                if !test.success_criteria.stdout_not_contains.is_empty() {
                    criteria.push(format!(
                        "stdout must NOT contain: {}",
                        test.success_criteria.stdout_not_contains.join(", ")
                    ));
                }
                if let Some(true) = test.success_criteria.stderr_empty {
                    criteria.push("stderr must be empty".to_string());
                }
                if !criteria.is_empty() {
                    content.push_str(&format!("  Success: {}\n", criteria.join("; ")));
                }
                content.push('\n');
            }
        }

        // Dead code check (dead_code_check field)
        if let Some(dead_code) = &stage.dead_code_check {
            content.push_str("### Dead Code Check\n\n");
            content.push_str(&format!("**Build command:** `{}`\n", dead_code.command));
            if !dead_code.fail_patterns.is_empty() {
                content.push_str(&format!(
                    "  Fail patterns: {}\n",
                    dead_code.fail_patterns.join(", ")
                ));
            }
            if !dead_code.ignore_patterns.is_empty() {
                content.push_str(&format!(
                    "  Ignore patterns: {}\n",
                    dead_code.ignore_patterns.join(", ")
                ));
            }
            content.push('\n');
        }

        content.push_str("Run `loom check <stage-id> --suggest` to check these automatically.\n\n");
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

    // Context-aware handoff reminders (independent of budget)
    if let Some(usage) = embedded_context.context_usage {
        if usage >= 75.0 {
            content.push_str("## COMPACTION IMMINENT\n\n");
            content.push_str("**Context is at critical level.** Run NOW:\n");
            content.push_str("```\nloom handoff --message \"what I was doing\"\n```\n\n");
        } else if usage >= 60.0 {
            content.push_str("## Context Preservation Reminder\n\n");
            content.push_str("Consider creating a handoff to preserve your working state:\n");
            content.push_str("```\nloom handoff --message \"current state\"\n```\n\n");
        }
    }

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
                append_budget_exceeded_box(&mut content);
            } else {
                content.push_str("**Approaching budget limit.** Prepare for handoff:\n");
                content.push_str("- `loom memory note` to record remaining observations\n");
                content.push_str("- `loom memory list` to verify insights captured\n");
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
    append_stage_end_sequence(&mut content);
    content.push('\n');

    // Embed stage memory at the END for maximum attention (Manus recitation pattern)
    content.push_str("## Stage Memory\n\n");
    if let Some(memory_content) = &embedded_context.memory_content {
        content.push_str("**YOUR WORKING MEMORY** - Notes and decisions from this stage:\n\n");
        content.push_str(memory_content);
        content.push('\n');
    } else {
        // CRITICAL: Show prominent prompt when memory is empty
        content.push_str("```\n");
        content.push_str("┌─────────────────────────────────────────────────────────────┐\n");
        content.push_str("│  ⚠️  NO MEMORY ENTRIES RECORDED — THIS IS A PROBLEM         │\n");
        content.push_str("│                                                             │\n");
        content.push_str("│  You should have been recording memories AS YOU WORKED:     │\n");
        content.push_str("│  - Every mistake/error you hit and how you fixed it         │\n");
        content.push_str("│  - Every non-obvious decision and WHY                       │\n");
        content.push_str("│  - Every surprise or gotcha in the code                     │\n");
        content.push_str("│                                                             │\n");
        content.push_str("│  BEFORE completing this stage, record what you learned:     │\n");
        content.push_str("│  BAD:  \"mistake: wrong path\"  (no context, useless)         │\n");
        content.push_str("│  GOOD: \"mistake: used loom/src/foo.rs in acceptance but     │\n");
        content.push_str("│    working_dir='loom' so path should be src/foo.rs.         │\n");
        content.push_str("│    Prevention: check working_dir before writing paths\"      │\n");
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
    content.push_str("- `loom memory change \"file.rs - description\"` - Record a file change\n");
    content.push_str("- `loom memory list` - Review your stage entries\n");
    content.push_str("- `loom memory show --all` - Show all stage memories\n\n");

    content
}

/// Format the codex-implementer doctrine block.
///
/// Emitted for any stage whose licensed lanes include [`Implementer::Codex`] —
/// gated on [`Implementers::includes_codex`], NOT on codex being the preferred
/// lane. A stage that spawns even one codex subagent needs the blast-radius
/// rules below, so a mixed stage carries them exactly as a codex-first stage
/// does. The models and effort are interpolated from [`CODEX_IMPLEMENTER_MODEL_TERRA`],
/// [`CODEX_IMPLEMENTER_MODEL_LUNA`], and [`CODEX_IMPLEMENTER_EFFORT`] rather than
/// repeated as literals — one source of truth for the lane's settings.
///
/// `codex_available` is [`crate::codex::codex_lane_available`] evaluated by the
/// caller: when the codex CLI or its plugin's companion runtime is missing on
/// this machine, the full doctrine below is replaced by a short fallback block
/// that forbids spawning `loom-codex-forwarder` and routes the codex tiers'
/// work to sonnet instead - the lane being licensed in the plan does not mean
/// it is installed on the machine actually running it.
pub(crate) fn format_codex_implementers_section(
    implementers: &Implementers,
    codex_available: bool,
) -> String {
    let mut content = String::new();

    content.push_str("## Codex Implementers\n\n");

    if !codex_available {
        content.push_str(&format!(
            "This stage lists codex in `implementers`, but the codex lane is UNAVAILABLE on this\n\
             machine - the codex CLI or its plugin's companion runtime is not installed. Do NOT\n\
             spawn `loom-codex-forwarder`. Route the codex tiers' work to sonnet\n\
             (`loom-software-engineer`) instead: {CODEX_IMPLEMENTER_MODEL_TERRA}'s tier (common\n\
             implementation, integration tests) and {CODEX_IMPLEMENTER_MODEL_LUNA}'s tier\n\
             (boilerplate, scaffolding, simple unit tests) alike. Every other rule of this signal\n\
             is unchanged.\n"
        ));
        return content;
    }

    content.push_str(&format!(
        "Implementation lanes licensed for this stage: {implementers}.\n"
    ));

    // The lane list is a per-SUBAGENT choice, not a per-stage mode. Say so
    // explicitly: the failure this wording exists to prevent is an orchestrator
    // reading one listed lane as "every subagent must be codex".
    if implementers.is_mixed() {
        content.push_str(&format!(
            "This stage MIXES lanes. Choose the lane PER SUBAGENT, not once for the whole stage:\n\
             reach for {} first on its tiers' work (terra: common implementation and integration\n\
             tests; luna: boilerplate, scaffolding, simple unit tests), and use the other lane\n\
             wherever the work calls for it. A single stage spawning codex implementers for one\n\
             file set and loom-software-engineer (sonnet) subagents for another is the intended\n\
             shape, not a contradiction.\n",
            implementers.preferred()
        ));
    } else {
        content.push_str(
            "Codex is the lane for this stage's terra- and luna-tier work. The Claude escalation\n\
             paths below still apply - they are not implementation lanes and never needed listing.\n",
        );
    }
    content.push_str(
        "Regardless of the list: YOU (the orchestrator) are opus, opus keeps the work that needs\n\
         architectural judgment, and loom-advisor (fable) is always available on a second failure.\n\
         Verification does NOT move - see below.\n\n",
    );
    content.push_str(
        "- Spawn codex implementation work with the Agent tool, subagent_type: \"loom-codex-forwarder\" -\n\
         loom's own forwarding shim. Do NOT spawn the plugin's codex:codex-rescue directly: plugin\n\
         agents' tools restriction is ignored by design, and such an unrestricted wrapper has been\n\
         observed implementing the task itself on sonnet instead of forwarding - a silent lane downgrade.\n",
    );
    content.push_str(&format!(
        "- THE FIRST LINE of every codex prompt is exactly \"{CODEX_FORWARD_SENTINEL}\". The\n\
         codex-forward-guard hook keys on that token to block a forwarder that reads or edits instead\n\
         of forwarding. Never put the token in a prompt for any other lane.\n"
    ));
    content.push_str(&format!(
        "- State the model and effort IN THE PROMPT TEXT: \"--model {CODEX_IMPLEMENTER_MODEL_TERRA} --effort {CODEX_IMPLEMENTER_EFFORT} <task>\"\n\
         for common implementation and integration tests, or \"--model {CODEX_IMPLEMENTER_MODEL_LUNA} --effort {CODEX_IMPLEMENTER_EFFORT} <task>\"\n\
         for boilerplate, scaffolding, and simple unit tests.\n"
    ));
    content.push_str("  The forwarder invokes only `~/.claude/hooks/loom/codex-forward.sh task '<task>' --model\n");
    content.push_str("  <model> --effort <effort> --write`; it single-quotes the task so shell operators remain data.\n");
    content.push_str("- STATE AN EXPLICIT BASH TIMEOUT IN THE PROMPT TEXT, e.g. \"make your single Bash call with an\n");
    content.push_str("  explicit timeout of 900000 ms\". The forwarder makes ONE Bash call and never raises the tool's\n");
    content.push_str("  120s default, so any longer codex run is backgrounded by the harness. When that happens the id\n");
    content.push_str("  the wrapper hands back is a CLAUDE CODE task id, not a codex job id - `codex-companion.mjs\n");
    content.push_str("  result <that id>` will not resolve it. Recover a stranded run with `codex-companion.mjs status\n");
    content.push_str("  --all` to get the real `task-*` id, and cancel runaways with `codex-companion.mjs cancel <id>`.\n");
    content.push_str("- SCOPE THE PROMPT OR IT WILL READ THE WHOLE KNOWLEDGE BASE. Codex does not run in Claude Code and\n");
    content.push_str("  never sees its tools: the OpenAI harness is shell-based BY DESIGN, so it reads by running perl/sed\n");
    content.push_str("  and writes by applying patches. That is not a misconfiguration and you cannot swap it for Read/Edit.\n");
    content.push_str("  The consequence is that reading is SLOW - it pages files in 160-line chunks - while it inherits\n");
    content.push_str("  CLAUDE.md's knowledge-first rule. Left to itself it sweeps all of doc/loom/knowledge/ first: measured at\n");
    content.push_str("  9m45s and still going on a four-question lookup, versus 54s for the same question once scoped.\n");
    content.push_str(
        "  Name the exact files it may open, and say plainly: do NOT read CLAUDE.md, do NOT read\n",
    );
    content.push_str(
        "  doc/loom/knowledge/, do NOT explore the repo, work only from what is named here.\n",
    );
    content.push_str("- THEREFORE YOU MUST FORCE-FEED IT. Forbidding exploration only works if you REPLACE what the\n");
    content.push_str("  exploration would have found - otherwise you have traded a slow agent for an ignorant one. It is\n");
    content.push_str(&format!(
        "  {CODEX_IMPLEMENTER_MODEL_TERRA} or {CODEX_IMPLEMENTER_MODEL_LUNA}, not an opus orchestrator: it will not infer your conventions, notice an\n"
    ));
    content.push_str("  adjacent helper it should reuse, or work out the shape you had in mind. Every codex prompt carries,\n");
    content.push_str("  inline and in full:\n");
    content.push_str("    * the exact file paths it owns (write) and may read, with nothing left to discovery;\n");
    content.push_str("    * exact symbol names and full signatures for anything it must call, implement or match - paste\n");
    content.push_str("      the signature, do not describe it;\n");
    content.push_str("    * the surrounding pattern to imitate, pasted as a snippet, when it must match existing style;\n");
    content.push_str(
        "    * every constraint that would otherwise live in knowledge or convention files;\n",
    );
    content.push_str(
        "    * step-by-step instructions with per-step acceptance, not a goal to figure out;\n",
    );
    content.push_str("    * the exact command that proves its slice works.\n");
    content.push_str("  If the prompt is short, it is almost certainly underspecified. A codex prompt that would fit a\n");
    content.push_str("  sonnet subagent is too thin - sonnet reads the repo to fill gaps, and you have just forbidden that.\n");
    content.push_str("- loom-codex-forwarder forwards with --write by default; do not ask for read-only when you want edits.\n");
    content.push_str("- PARALLEL FAN-OUT: you may run up to 6 codex implementers at once, each owning a DISJOINT file set,\n");
    content.push_str("  with the same file-ownership table you would write for sonnet subagents. Two codex agents writing\n");
    content.push_str("  one file is lost work, exactly as with any other subagent.\n");
    content.push_str("- MIXED FAN-OUT: codex and Claude subagents may run in the SAME wave. File ownership is what keeps\n");
    content.push_str("  them apart, and it is enforced across lanes, not within one - a codex agent and a sonnet agent\n");
    content.push_str("  writing one file is lost work just as surely as two codex agents. Put every subagent from every\n");
    content.push_str("  lane in ONE file-ownership table, and note each row's lane so you know which rules apply to it.\n");
    content.push_str("- Run parallel codex implementers in the FOREGROUND. Do NOT fan out --background jobs: the plugin\n");
    content.push_str("  tracks jobs in a shared state file written without a lock, and a background result is fetched\n");
    content.push_str("  through the very record a concurrent write can drop. Foreground results come back through stdout\n");
    content.push_str("  and do not depend on it.\n");
    content.push_str("- Do NOT use --resume-last under fan-out. It resolves \"the last job\" out of that same shared\n");
    content.push_str("  state file and can attach to a sibling's thread. Use fresh runs.\n");
    content.push_str("- A foreground codex run is ONE long Bash call: no PostToolUse fires, so the loom heartbeat goes\n");
    content.push_str("  stale and the daemon prints a spurious \"appears hung\" warning after 300s. That warning is\n");
    content.push_str("  ADVISORY ONLY - nothing is killed or retried. Ignore it.\n");
    content.push_str("- BLAST RADIUS. Codex runs with sandbox `workspace-write` and approval policy `never`: it edits\n");
    content.push_str("  anything under the git root without asking. In a loom worktree the git root IS the worktree,\n");
    content.push_str(
        "  so that is your isolation boundary - with two holes you must cover yourself:\n",
    );
    content.push_str("    * NEVER give a codex agent a path under `.work/`. It is a SYMLINK to orchestration state\n");
    content.push_str("      shared with every parallel stage; a write through it escapes worktree isolation and\n");
    content.push_str(
        "      corrupts other stages. `.work/` is yours via the loom CLI only (Rule 11).\n",
    );
    content.push_str("    * Loom's PreToolUse hooks (commit-filter, git-add-guard, the subagent guards) intercept\n");
    content.push_str("      CLAUDE CODE's Bash tool. They do NOT see commands codex runs inside its own session, so\n");
    content.push_str("      for codex those rules are prose, not enforcement. Tell every codex subagent it must not\n");
    content.push_str("      run git at all, and check `git status --short` after each run: anything staged, committed\n");
    content.push_str("      or touched outside that agent's assigned file set is YOUR problem to find, because no\n");
    content.push_str("      hook will.\n");
    content.push_str("- WHAT CODEX IS FOR: terra takes common implementation and integration tests (the sonnet\n");
    content.push_str("  tier); luna takes boilerplate, scaffolding, and simple unit tests. It does NOT take opus work\n");
    content.push_str("  (mainstream architecture, algorithm implementation, cross-cutting refactors, security-sensitive\n");
    content.push_str("  code), fable work (visual/UI design, a bug that survived a delegated fix attempt, extremely challenging algorithmic design), or\n");
    content.push_str("  loom-advisor's role on a second failure. Route each piece of work by what the work needs; the\n");
    content.push_str("  lane list says what is available, not what is mandatory. Sending a task to codex because the\n");
    content.push_str("  stage lists codex - rather than because the task fits a codex tier - is the misread this\n");
    content.push_str("  section exists to prevent.\n");
    content.push_str("- ACCEPT A CODEX REPORT ONLY WITH EVIDENCE. A genuine forward returns codex stdout followed by a\n");
    content.push_str("  \"--- LOOM-CODEX-EVIDENCE ---\" trailer listing companion state jobs/*.json paths. Verify the\n");
    content.push_str("  newest record for THIS worktree exists and its \"phase\" is \"done\". A report with no trailer -\n");
    content.push_str("  or edits in the tree with no matching job record - is a FAILED delegation: the wrapper did the\n");
    content.push_str("  work itself. Treat those edits as output from an unknown lane: revert and respawn the forwarder,\n");
    content.push_str(
        "  or keep them only after reviewing them as strictly as you would review sonnet output.\n",
    );
    content.push_str("- VERIFICATION STAYS WITH YOU (opus). Codex subagents implement and report; they never verify, never\n");
    content.push_str("  commit, and never run loom stage complete (Rule 5). YOU run the full build/test/lint gate, YOU run\n");
    content.push_str("  the six-dimension mini adversarial code review, and only THEN — after both — YOU commit, at the end of the stage. Never accept a codex agent's own\n");
    content.push_str(
        "  claim that its work is correct, and never have codex review its own output - use\n",
    );
    content.push_str("  loom-code-reviewer or your own reading.\n\n");

    content
}

/// Format task progression information for inclusion in signals
pub fn format_skill_recommendations(skills: &[SkillMatch]) -> String {
    let mut content = String::new();

    content.push_str("## Recommended Skills\n\n");

    // Partition skills into two classes with different framing:
    // - `detected`: language skills inferred from the files this stage edits.
    //   These are a DIRECTIVE — load them before writing code.
    // - `advisory`: skills matched from the task description. Invoke if relevant.
    let (detected, advisory): (Vec<&SkillMatch>, Vec<&SkillMatch>) = skills
        .iter()
        .partition(|s| s.matched_triggers.iter().any(|t| t == "project-language"));

    if !detected.is_empty() {
        content.push_str(
            "**Load these now — before editing any files.** Based on the file types this \
             stage will edit, invoke the Skill tool for each so your code follows the \
             project's language conventions:\n\n",
        );
        for skill in &detected {
            content.push_str(&format!("- `Skill(skill=\"{}\")`\n", skill.name));
        }
        content.push('\n');
    }

    if !advisory.is_empty() {
        content.push_str("These skills may also help with your task — invoke any that apply:\n\n");
        content.push_str("| Skill | Description | Invoke |\n");
        content.push_str("|-------|-------------|--------|\n");

        for skill in &advisory {
            // Truncate description if too long for table (UTF-8 safe)
            let desc = if skill.description.chars().count() > 60 {
                format!(
                    "{}...",
                    skill.description.chars().take(57).collect::<String>()
                )
            } else {
                skill.description.clone()
            };
            // Escape pipe characters in description and name
            let desc = desc.replace('|', "\\|");
            let name = skill.name.replace('|', "\\|");

            content.push_str(&format!(
                "| {} | {} | `Skill(skill=\"{}\")` |\n",
                name, desc, name
            ));
        }
        content.push('\n');

        // Show which triggers matched for transparency
        content.push_str("**Matched triggers:**\n");
        for skill in &advisory {
            if !skill.matched_triggers.is_empty() {
                let triggers = skill.matched_triggers.join(", ");
                content.push_str(&format!("- `{}`: {}\n", skill.name, triggers));
            }
        }
        content.push('\n');
    }

    content
}

#[cfg(test)]
mod skill_recommendation_tests {
    use super::format_skill_recommendations;
    use crate::skills::SkillMatch;

    fn detected(name: &str) -> SkillMatch {
        SkillMatch::new(
            name.to_string(),
            "Language expertise".to_string(),
            10.0,
            vec!["project-language".to_string()],
        )
    }

    fn advisory(name: &str, trigger: &str) -> SkillMatch {
        SkillMatch::new(
            name.to_string(),
            "Some advisory skill".to_string(),
            2.0,
            vec![trigger.to_string()],
        )
    }

    #[test]
    fn detected_skills_render_as_skill_tool_directive() {
        let out = format_skill_recommendations(&[detected("loom-rust")]);
        // Directive framing + an explicit Skill tool invocation the agent can run.
        assert!(out.contains("Load these now"), "missing directive: {out}");
        assert!(
            out.contains("Skill(skill=\"loom-rust\")"),
            "missing Skill tool call: {out}"
        );
    }

    #[test]
    fn advisory_skills_render_as_table_not_directive() {
        let out = format_skill_recommendations(&[advisory("loom-auth", "jwt")]);
        assert!(
            !out.contains("Load these now"),
            "should not be directive: {out}"
        );
        assert!(
            out.contains("may also help"),
            "missing advisory framing: {out}"
        );
        assert!(
            out.contains("Skill(skill=\"loom-auth\")"),
            "missing invoke column: {out}"
        );
        assert!(out.contains("jwt"), "missing matched trigger: {out}");
    }

    #[test]
    fn detected_and_advisory_are_partitioned() {
        let out =
            format_skill_recommendations(&[detected("loom-rust"), advisory("loom-auth", "jwt")]);
        // Detected directive comes before the advisory table.
        let load_pos = out.find("Load these now").expect("directive present");
        let advisory_pos = out.find("may also help").expect("advisory present");
        assert!(
            load_pos < advisory_pos,
            "directive should precede advisory: {out}"
        );
    }
}
