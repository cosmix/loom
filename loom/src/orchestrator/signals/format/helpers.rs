use crate::handoff::schema::HandoffV2;
use crate::models::stage::Stage;

use super::super::types::DependencyStatus;

/// Format the per-subagent response-budget block.
///
/// Emitted only for stages that set `subagent_timeout_secs` explicitly, by both
/// the semi-stable path and the recovery path.
///
/// This carries the stage's specific number and just enough framing to keep it
/// from being misread as the watch's `--timeout`: the budget is the IDLE
/// threshold death is judged against, while one identity-bound watch waits for
/// 3600 seconds in the background. The full doctrine for what to DO with it is
/// never generated into a signal: BLOCK-C is static-only, pinned byte-for-byte
/// in `skills/loom-orchestration/SKILL.md` "## Rule 6 — Subagents" by
/// `tests_doctrine_waiting.rs`, and reaches the agent through that skill,
/// loaded when the orchestrator spawns its first subagent, so repeating it
/// here would just duplicate it in the same signal.
pub(crate) fn format_subagent_timeout_section(timeout_secs: u64) -> String {
    format!(
        "## Subagent Response Budget\n\n\
         This stage's advisory heartbeat budget is {timeout_secs}s. Run ONE \
         `loom subagents watch --worker ... --timeout 3600` through the Bash tool's \
         `run_in_background`, naming every worker in that single owned wait; never issue another \
         watch. The budget is the idle threshold for a stall, \
         never a deadline on the subagent's own work: only idle time past {timeout_secs}s with no \
         transcript growth signals a stall, and a stall is no proof of death. ADVISORY ONLY: the orchestrator's own \
         hung warning never kills or retries anything — recovery stays with you.\n\n"
    )
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
pub(in crate::orchestrator::signals) fn extract_tasks_from_stage(stage: &Stage) -> Vec<String> {
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
        } else if let Some(task) = super::super::helpers::numbered_item(trimmed) {
            tasks.push(task.trim().to_string());
        }
    }

    tasks
}

/// Format a V2 structured handoff for inclusion in signals
pub(super) fn format_structured_handoff(handoff: &HandoffV2) -> String {
    let mut content = String::new();

    content.push_str(&format!(
        "**Previous Session**: {} | **Context**: {} tokens\n\n",
        handoff.session_id, handoff.context_tokens
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
    content.push_str("**Stage end sequence (in this order, nothing skipped):** every subagent returned → format → `loom check <stage-id>` ONCE, beside the final review round → ONE fix worker takes every finding and every failed criterion → once it returns, `loom check <stage-id> --criterion <n>` for each criterion that failed (none failed: no check) → one re-review → `loom stage commit` (orchestrator only, one logical commit per concern) → `loom stage complete <stage-id>`. Run no `loom check` between the last review round and `loom stage complete`: completion runs every criterion. A commit before this point is premature.\n");
}

/// Append the package-manager-cache carve-out note to the sandbox section,
/// shown whenever the sandbox is enabled (`format/sandbox_section.rs`).
pub(super) fn append_package_cache_note(content: &mut String) {
    content.push_str("**Package-manager caches:** your home directory grants write access to the cargo and rustup caches only. Every other ecosystem (bun, npm, pnpm, yarn, deno, uv, pip, go, gradle, maven) writes its cache into this stage's own cache root, which the launcher exports through the tools' own environment variables (`XDG_CACHE_HOME`, `BUN_INSTALL_CACHE_DIR`, `UV_CACHE_DIR`, `GOMODCACHE`, ...); keep them as exported and never point a tool back at your own caches, which are read-only here. So `bun install`, `cargo add`, `uv sync` and `go get` work inside this sandbox. One limit: relocating `CARGO_HOME` or `RUSTUP_HOME` is not covered, and a toolchain install under a read-only directory fails. `EROFS` / `Read-only file system` from a package manager means a tool reached a real cache or a toolchain directory: block the stage with `loom stage block <stage-id> \"<path> is read-only in the sandbox\"`; do not work around it.\n\n");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_multi_digit_numbered_tasks() {
        let text = (1..=12)
            .map(|i| format!("{i}. task {i}\n"))
            .collect::<String>();
        let tasks = extract_tasks_from_description(&text);
        assert_eq!(tasks.len(), 12);
        assert_eq!(tasks[11], "task 12");
    }

    #[test]
    fn the_package_cache_note_names_loom_stage_block() {
        let mut note = String::new();
        append_package_cache_note(&mut note);

        assert!(note.starts_with("**Package-manager caches:**"));
        assert!(note.contains("loom stage block"));
        assert!(!note.contains("report it as a blocker"));
    }

    #[test]
    fn the_package_cache_note_states_the_per_session_caches() {
        let mut note = String::new();
        append_package_cache_note(&mut note);

        assert!(note.starts_with("**Package-manager caches:**"));
        assert!(note.contains("read-only"));
        assert!(note.contains("CARGO_HOME"));
    }

    #[test]
    fn the_package_cache_note_no_longer_grants_the_real_caches() {
        let mut note = String::new();
        append_package_cache_note(&mut note);

        for retired in ["are writable", "allow_write entry", "does not exist yet"] {
            assert!(!note.contains(retired), "{retired}");
        }
    }
}
