//! Markdown section formatters shared by the merge and
//! knowledge signal generators.
//!
//! These render the small, self-contained sections ("## Target",
//! "## Execution Rules", "## Stage Context", "## Conflicting Files") that
//! appear verbatim across those signal types. Every function here formats
//! content for the OUTGOING signal; `helpers.rs` parses a signal file back in.

use crate::context::untrusted::multiline_safe;
use crate::models::stage::Stage;

/// Most refusal lines the handback section lists; the rest stay in the stage file.
const MAX_HANDBACK_LINES: usize = 40;

/// Format the "## Target" markdown section for conflict-type signals.
///
/// Used by the merge signal generator.
/// Standard stage signals have a more complex target section (with working_dir,
/// execution path, etc.) and use their own formatter in `format/sections.rs`.
pub(super) fn format_target_section(
    session_id: &str,
    stage_id: &str,
    source_branch: Option<&str>,
    target_branch: &str,
) -> String {
    let mut content = String::new();

    content.push_str("## Target\n\n");
    content.push_str(&format!("- **Session**: {session_id}\n"));
    content.push_str(&format!("- **Stage**: {stage_id}\n"));
    if let Some(branch) = source_branch {
        content.push_str(&format!("- **Source Branch**: {branch}\n"));
    }
    content.push_str(&format!("- **Target Branch**: {target_branch}\n"));
    content.push('\n');

    content
}

/// Format the "## Execution Rules" section for conflict resolution signals.
///
/// The `preserve_intent` parameter controls the wording:
/// - `"BOTH branches"` for merge signals
pub(super) fn format_execution_rules_section(preserve_intent: &str) -> String {
    let mut content = String::new();

    content.push_str("## Execution Rules\n\n");
    content.push_str("Follow your `~/.claude/CLAUDE.md` rules. Key reminders:\n");
    content.push_str("- **Do NOT modify code** beyond what's needed for conflict resolution\n");
    content.push_str(&format!(
        "- **Preserve intent from {preserve_intent}** where possible\n"
    ));
    content.push_str("- **Ask the user** if unclear how to resolve a conflict\n");
    content.push_str("- **Use TodoWrite** to track resolution progress\n\n");

    content
}

/// Format the "## Stage Context" section showing stage name and description.
///
/// Returns an empty string if the stage has no description.
/// Shared across merge signal generators.
pub(super) fn format_stage_context_section(stage: &Stage) -> String {
    if let Some(desc) = &stage.description {
        format!("## Stage Context\n\n**{}**: {}\n\n", stage.name, desc)
    } else {
        String::new()
    }
}

/// Format the "## Conflicting Files" section as a bullet list of backtick-wrapped paths.
///
/// Shows a fallback message when no files are listed.
/// Used by the merge signal generator.
pub(super) fn format_conflicting_files_section(files: &[String]) -> String {
    let mut content = String::new();

    content.push_str("## Conflicting Files\n\n");
    if files.is_empty() {
        content
            .push_str("_No specific files listed - run `git status` to see current conflicts_\n");
    } else {
        for file in files {
            content.push_str(&format!("- `{file}`\n"));
        }
    }
    content.push('\n');

    content
}

/// Format the "## Target" markdown section for knowledge stage signals.
///
/// Knowledge stages run in the main repo (no worktree / no source branch), so
/// their Target section uses Type and Directory fields instead of branches.
pub(super) fn format_knowledge_target_section(
    session_id: &str,
    stage_id: &str,
    plan_id: Option<&str>,
    repo_root: &str,
) -> String {
    let mut content = String::new();

    content.push_str("## Target\n\n");
    content.push_str(&format!("- **Session**: {session_id}\n"));
    content.push_str(&format!("- **Stage**: {stage_id}\n"));
    content.push_str("- **Type**: Knowledge (no worktree)\n");
    if let Some(plan) = plan_id {
        content.push_str(&format!("- **Plan**: {plan}\n"));
    }
    content.push_str(&format!("- **Directory**: {repo_root}\n"));
    content.push('\n');

    content
}

/// The "## Merge Resolver Handback" section for a stage whose resolver's
/// committed merge failed completion verification, or `None` without one.
/// Each non-empty line of the refusal is a bullet; the text came from command
/// output, so it is neutralised with `multiline_safe`, which keeps long lines
/// whole. The `- ` prefix stops a line that starts with `## ` from forming a
/// heading. The review sentence appears only for stages the review gate covers.
pub(super) fn format_merge_handback_section(stage: &Stage) -> Option<String> {
    let refusal = stage.merge.handback_refusal.as_deref()?;
    let mut content = String::from(
        "\n## Merge Resolver Handback\n\n\
         The merge of the target branch into this branch is committed, but its content failed \
         completion verification, so loom stopped the resolver and gave the stage to you. Do \
         not merge the target again. Fix only the failures listed here, or dispute them. \
         Record any other problem, including ones in the stage memory, with `loom memory note` \
         and leave it. Then run `loom stage complete` as usual",
    );
    if crate::verify::review::gate::covers(stage) {
        content.push_str("; it needs a review round that matches the tree");
    }
    content.push_str(
        ". If a listed line names no problem in the tree (a daemon or file error), run `loom stage \
         complete` again; it verifies from scratch.\n\n",
    );
    for line in multiline_safe(refusal)
        .lines()
        .filter(|line| !line.trim().is_empty())
        .take(MAX_HANDBACK_LINES)
    {
        content.push_str(&format!("- {line}\n"));
    }
    Some(content)
}

/// Append the handback section to `content` when the stage has a refusal.
pub(super) fn append_merge_handback_section(content: &mut String, stage: &Stage) {
    if let Some(section) = format_merge_handback_section(stage) {
        super::helpers::ensure_trailing_newline(content);
        content.push_str(&section);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_target_section_with_source() {
        let section = format_target_section("session-1", "stage-1", Some("loom/stage-1"), "main");
        assert!(section.contains("## Target"));
        assert!(section.contains("- **Session**: session-1"));
        assert!(section.contains("- **Stage**: stage-1"));
        assert!(section.contains("- **Source Branch**: loom/stage-1"));
        assert!(section.contains("- **Target Branch**: main"));
    }

    #[test]
    fn test_format_target_section_without_source() {
        let section = format_target_section("session-1", "stage-1", None, "loom/_base/stage-1");
        assert!(!section.contains("Source Branch"));
        assert!(section.contains("- **Target Branch**: loom/_base/stage-1"));
    }

    #[test]
    fn test_format_execution_rules_both() {
        let rules = format_execution_rules_section("BOTH branches");
        assert!(rules.contains("Preserve intent from BOTH branches"));
        assert!(rules.contains("Do NOT modify code"));
    }

    #[test]
    fn test_format_execution_rules_all() {
        let rules = format_execution_rules_section("ALL branches");
        assert!(rules.contains("Preserve intent from ALL branches"));
    }

    #[test]
    fn test_format_stage_context_with_description() {
        let mut stage = Stage::new("My Stage".to_string(), Some("Description here".to_string()));
        stage.id = "my-stage".to_string();
        let section = format_stage_context_section(&stage);
        assert!(section.contains("## Stage Context"));
        assert!(section.contains("**My Stage**: Description here"));
    }

    #[test]
    fn test_format_stage_context_no_description() {
        let mut stage = Stage::new("My Stage".to_string(), None);
        stage.id = "my-stage".to_string();
        let section = format_stage_context_section(&stage);
        assert!(section.is_empty());
    }

    #[test]
    fn test_format_conflicting_files() {
        let files = vec!["src/main.rs".to_string(), "src/lib.rs".to_string()];
        let section = format_conflicting_files_section(&files);
        assert!(section.contains("## Conflicting Files"));
        assert!(section.contains("- `src/main.rs`"));
        assert!(section.contains("- `src/lib.rs`"));
    }

    #[test]
    fn test_format_conflicting_files_empty() {
        let section = format_conflicting_files_section(&[]);
        assert!(section.contains("_No specific files listed"));
    }
}
