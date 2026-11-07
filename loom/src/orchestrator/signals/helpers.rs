//! Shared helpers for signal generation, formatting, and parsing.
//!
//! Owns signal publication and the constrained Markdown readers the signal
//! kinds share (section, field and numbered-list recognizers).

use anyhow::{Context, Result};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

// Shared knowledge retrieval and delivery helpers.
pub(super) use super::retrieval::{
    ensure_trailing_newline, knowledge_tree_is_empty, persist_delivery, retrieve_stage_pack,
};

// Shared markdown section formatters.
pub(super) use super::section_formatters::{
    format_conflicting_files_section, format_execution_rules_section,
    format_knowledge_target_section, format_stage_context_section, format_target_section,
};

/// Handoff trigger: the PostToolUse hook reports the ceiling, so the agent never
/// estimates it. The second line states the handoff's cost so a session does not
/// use it as a routine checkpoint.
pub(super) const CONTEXT_CEILING_HANDOFF: &str = "- When the PostToolUse hook reports the context ceiling, finish the current unit and run `loom handoff --stage <id> --session <id> --trigger ceiling`, then stop. A handoff discards a loaded session, so only the hook triggers it.\n";

/// Write a signal file to the signals directory, creating it if needed.
pub(super) fn write_signal_file(
    session_id: &str,
    content: &str,
    work_dir: &Path,
) -> Result<PathBuf> {
    let signals_dir = work_dir.join("signals");

    if !signals_dir.exists() {
        fs::create_dir_all(&signals_dir).context("Failed to create signals directory")?;
    }

    let signal_path = signals_dir.join(format!("{session_id}.md"));

    fs::write(&signal_path, content)
        .with_context(|| format!("Failed to write signal file: {}", signal_path.display()))?;

    Ok(signal_path)
}

/// Publish a signal, then record its optional context delivery receipt: a failed
/// write leaves no receipt, and a failed receipt leaves the signal usable. The
/// receipt proves publication, never that the recipient read the signal.
pub(super) fn publish_signal(
    session_id: &str,
    content: &str,
    work_dir: &Path,
    stage: &crate::models::stage::Stage,
    embedded_context: &super::types::EmbeddedContext,
) -> Result<PathBuf> {
    let path = write_signal_file(session_id, content, work_dir)?;
    persist_delivery(work_dir, stage, session_id, embedded_context);
    Ok(path)
}

/// Lines paired with whether they are outside a fenced code block.
pub(super) fn signal_lines(content: &str) -> impl Iterator<Item = (&str, bool)> {
    let mut fence: Option<(char, usize)> = None;
    content.lines().map(move |line| {
        let trimmed = line.trim_start();
        let marker = trimmed.chars().next().filter(|c| *c == '`' || *c == '~');
        if let Some(marker) = marker {
            let count = trimmed.chars().take_while(|c| *c == marker).count();
            // A backtick run followed by more backticks is inline code: it must
            // not swallow the signal structure after it as an unclosed fence.
            let inline_code = marker == '`' && trimmed[count..].contains('`');
            if count >= 3 && !inline_code {
                match fence {
                    None => fence = Some((marker, count)),
                    Some((open, len))
                        if marker == open && count >= len && trimmed[count..].trim().is_empty() =>
                    {
                        fence = None
                    }
                    _ => {}
                }
                return (line, false);
            }
        }
        (line, fence.is_none())
    })
}

/// Iterate lines with their structural heading, excluding headings inside fences.
pub(super) fn signal_sections(
    content: &str,
) -> impl Iterator<Item = (&str, bool, Option<(usize, &str)>)> {
    signal_lines(content).map(|(line, outside)| {
        let trimmed = line.trim();
        let depth = trimmed.bytes().take_while(|b| *b == b'#').count();
        let heading = (outside && depth > 0 && trimmed.as_bytes().get(depth) == Some(&b' '))
            .then(|| (depth, trimmed[depth + 1..].trim()));
        (line, outside, heading)
    })
}

pub(super) fn numbered_item(line: &str) -> Option<&str> {
    let digits = line.bytes().take_while(u8::is_ascii_digit).count();
    if digits == 0 {
        return None;
    }
    line[digits..]
        .strip_prefix(". ")
        .or_else(|| line[digits..].strip_prefix(") "))
}

pub(super) fn field_value<'a>(line: &'a str, field: &str) -> Option<&'a str> {
    let line = line.trim().strip_prefix("- ").unwrap_or(line.trim());
    let bold = format!("**{field}**:");
    let plain = format!("{field}:");
    line.strip_prefix(&bold)
        .or_else(|| line.strip_prefix(&plain))
        .map(str::trim)
}

/// Parse markdown content into sections keyed by structural `## ` headers.
///
/// Returns a map from section name to the non-empty trimmed lines in that section,
/// fenced lines excluded. Lines before the first `## ` header are stored under the
/// empty string key.
pub(super) fn parse_signal_sections(content: &str) -> HashMap<String, Vec<String>> {
    let mut sections: HashMap<String, Vec<String>> = HashMap::new();
    let mut current_section = String::new();

    for (line, outside_fence, heading) in signal_sections(content) {
        if !outside_fence {
            continue;
        }
        let trimmed = line.trim();

        if let Some((2, header)) = heading {
            current_section = header.to_string();
            sections.entry(current_section.clone()).or_default();
            continue;
        }

        if !trimmed.is_empty() {
            sections
                .entry(current_section.clone())
                .or_default()
                .push(trimmed.to_string());
        }
    }

    sections
}

/// Extract a markdown bold field value from a list of lines.
///
/// Looks for lines matching `- **{field}**: value` and returns the value.
/// Useful in combination with `parse_signal_sections` for extracting
/// specific fields from a section.
pub(super) fn extract_field_from_lines<'a>(lines: &'a [String], field: &str) -> Option<&'a str> {
    for line in lines {
        if let Some(value) = field_value(line, field) {
            return Some(value);
        }
    }
    None
}

/// Extract backtick-wrapped items from a list of markdown bullet lines.
///
/// Parses lines like `- \`path/to/file\`` and returns the unwrapped values.
/// Useful for extracting file lists from "Conflicting Files" or "Source Branches" sections.
pub(super) fn extract_backtick_items(lines: &[String]) -> Vec<String> {
    lines
        .iter()
        .filter_map(|line| {
            line.strip_prefix("- `")
                .and_then(|rest| rest.strip_suffix('`'))
                .map(|s| s.to_string())
        })
        .collect()
}

/// Append the commit-timing doctrine: commits are the ORCHESTRATOR's, made ONLY
/// at completion after every subagent has returned and verification is green,
/// or before filing a block or dispute when the stage cannot finish.
///
/// `gate` names the verification gate for the stage type (code stages: build,
/// tests, lint, format, acceptance; documentation stages: acceptance only) and
/// `review` states the review condition (code stages: the mini adversarial
/// review returned and its findings fixed; documentation stages: the knowledge
/// files re-read). Both are interpolated into otherwise identical text so the
/// doctrine lives in exactly one place; `tests_commit_timing.rs` pins the
/// sentinel phrases across every stable prefix and `CLAUDE.md.template`.
///
/// `add_target` is what the doctrine sentence stages (`commit_command_rule`).
pub(super) fn append_commit_timing_rules(
    content: &mut String,
    gate: &str,
    review: &str,
    add_target: &str,
) {
    content.push_str("**When to Commit (ORCHESTRATOR ONLY — AT EXIT):**\n\n");
    content.push_str("Commits are yours alone and ONLY as the final step of the stage — never mid-stage. Exception: commit before filing a block or dispute when the stage cannot finish. For completion, commit only once ALL of these hold:\n\n");
    content.push_str("1. Every subagent, coordinator, team, and Workflow you spawned has RETURNED, result absorbed.\n");
    content.push_str(&format!(
        "2. The full verification gate is GREEN on the complete tree: {gate}.\n"
    ));
    content.push_str(&format!("3. {review}\n\n"));
    content.push_str(&format!(
        "Then {} One logical commit per concern. Then run `loom stage complete <stage-id>`.\n\n",
        commit_command_rule(add_target)
    ));
}

/// The commit doctrine sentence, in lower case so it follows "Then" or a
/// label; a surface that opens a sentence with it capitalises the first
/// letter. Every surface that tells a session how to commit uses this
/// wording (`add_target` replaces `<specific-files>`).
pub(super) fn commit_command_rule(add_target: &str) -> String {
    format!(
        "commit with `git add {add_target}` then `loom stage commit <stage-id> -m \"type(scope): description\"`, and wait for it with `loom request status <id> --wait 90`; never run `git commit`."
    )
}

/// The knowledge-stage header bullets that state where commits go.
pub(super) fn append_knowledge_commit_header(content: &mut String) {
    content.push_str("- **COMMITS REQUIRED** - commit through `loom stage commit`, never `git commit`; your commits go directly to main\n");
    content.push_str("- **NO MERGING** - Your commits go directly to main (no branch to merge)\n");
}

/// The knowledge-stage completion bullet that commits the knowledge files.
pub(super) fn append_knowledge_commit_bullet(content: &mut String) {
    content.push_str(&format!(
        "- **Commit knowledge changes**: {} Message: `docs(knowledge): populate codebase knowledge`\n",
        commit_command_rule("doc/loom/knowledge/")
    ));
}

/// The knowledge stage's mission block.
pub(super) fn append_knowledge_mission(content: &mut String) {
    content.push_str("**Your Mission:**\n\n");
    content.push_str(
        "Build a **briefing document** for future implementation agents. Every entry you\n",
    );
    content.push_str(
        "write should help an agent who has never seen this codebase avoid mistakes and\n",
    );
    content.push_str("find their way quickly. Implementation stages build on this foundation.\n\n");
    content.push_str("1. **Exhaustively map** the codebase (hierarchically) — entry points, every module, data flow, patterns, conventions; leave no major area unmapped.\n");
    content.push_str(
        "2. **Document** findings using `loom knowledge update <file> <content>` commands\n",
    );
    content.push_str("3. **Backfill** any knowledge gaps — if existing knowledge files are sparse, enrich them\n");
    content.push_str("4. **Contextualize the plan** — understand what the plan intends to change and document the current state of those areas\n");
    content.push_str("5. **Review existing mistakes** — pull them with `loom knowledge context --stage <stage-id> --query \"mistakes\" --budget-tokens <n>` and check if any entries are now obsolete or fixed. Remove stale entries to keep the briefing accurate\n");
    content.push_str("6. **Verify** acceptance criteria before completing\n\n");
    content.push_str("**Do NOT modify the project's CLAUDE.md** — it is the user's file. All knowledge goes to `loom knowledge update`; your own insights go to `loom memory`.\n\n");
}

/// Append the shared "settled stage" completion doctrine: `loom stage
/// complete` is the session's LAST act, run only once the stage is SETTLED,
/// plus the "verify acceptance criteria" line, for the standard,
/// integration-verify and knowledge prefixes.
pub(super) fn append_settled_completion_rules(content: &mut String) {
    content.push_str(
        "- **`loom stage complete` is the LAST act of your session.** Run it ONLY when SETTLED: every subagent returned, every defect fixed and re-verified, all work committed (`git status` clean).\n",
    );
    content.push_str(
        "- **After `completed`: STOP and end the session** — post-completion work is LOST WORK.\n",
    );
    content.push_str("- **Verify acceptance criteria** before completing\n");
}

/// BLOCK-F: when a stage cannot finish, the main agent disputes a wrong check
/// or blocks the stage on a need only a person can meet. `tests_doctrine_v2.rs`
/// pins every prefix copy.
const STAGE_EXIT_RULES: &str = "**When the stage cannot finish.** Fix what the stage can fix. A wrong criterion, wiring check, contract, review finding or test-integrity event gets a dispute: `loom stage dispute-criteria` (`--field` for wiring, wiring-tests, impact-tests entries), `dispute-contract`, `dispute-findings` or `dispute-integrity`. Filing ends this session; if the verdict changes nothing, loom reruns completion on your last-completed tree (an impact-tests-only failure too) and completes the stage on a pass, else a fresh session starts. Never revert, weaken or postpone correct work to avoid a dispute. A criterion failing only because of where completion runs (no network, unreadable git metadata, timeout, sandbox denial) is disputed, never blocked: one `loom stage dispute-criteria <stage-id> --criterion-index <n> [--criterion-index <n> ...]` filing names them all. `loom stage block <stage-id> \"<what is needed and why>\"` is for a credential, a host install or a decision only a person can make; loom refuses it while the latest `loom stage complete` fails a criterion. Before a dispute, run `loom stage complete` on that commit. Never end a turn asking the operator to act mid-stage.";

/// Append completion rules shared between standard and integration-verify prefixes
pub(super) fn append_completion_rules(content: &mut String) {
    append_settled_completion_rules(content);
    content.push_str("- No `loom stage retry` mid-session.\n");
    content.push_str(CONTEXT_CEILING_HANDOFF);
    content.push_str("- Run `loom stage complete <stage-id>` in the worktree ROOT directory. `Host job:` requested/pending/stopped: rerun; failed/rejected/refused: fix, commit, rerun. Run it alone; at most `2>&1 | tail -N` may follow it.\n\n");
    content.push_str(STAGE_EXIT_RULES);
    content.push_str("\n\n");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_write_signal_file() {
        let tmp = tempfile::TempDir::new().unwrap();
        let work_dir = tmp.path();

        let path = write_signal_file("session-123", "test content", work_dir).unwrap();
        assert!(path.exists());
        assert_eq!(fs::read_to_string(&path).unwrap(), "test content");
        assert_eq!(path, work_dir.join("signals").join("session-123.md"));
    }

    #[test]
    fn test_write_signal_file_creates_dir() {
        let tmp = tempfile::TempDir::new().unwrap();
        let work_dir = tmp.path();
        let signals_dir = work_dir.join("signals");
        assert!(!signals_dir.exists());

        write_signal_file("session-456", "content", work_dir).unwrap();
        assert!(signals_dir.exists());
    }

    #[test]
    fn test_parse_signal_sections() {
        let content = r#"# Header

Some preamble

## Target

- **Session**: session-123
- **Stage**: my-stage

## Conflicting Files

- `src/main.rs`
- `src/lib.rs`

## Empty Section
"#;
        let sections = parse_signal_sections(content);

        let target = sections.get("Target").unwrap();
        assert_eq!(target.len(), 2);
        assert!(target[0].contains("Session"));
        assert!(target[1].contains("Stage"));

        let files = sections.get("Conflicting Files").unwrap();
        assert_eq!(files.len(), 2);

        let empty = sections.get("Empty Section").unwrap();
        assert!(empty.is_empty());
    }

    #[test]
    fn inline_triple_backticks_do_not_open_a_fence() {
        let sections = parse_signal_sections(
            "## Assignment\nuse ```x``` here\n```y```\n## Target\n- Stage: a\n",
        );
        assert_eq!(
            extract_field_from_lines(&sections["Target"], "Stage"),
            Some("a")
        );
    }

    #[test]
    fn test_extract_field_from_lines() {
        let lines = vec![
            "- **Session**: session-123".to_string(),
            "- **Stage**: my-stage".to_string(),
            "- **Target Branch**: main".to_string(),
        ];

        assert_eq!(extract_field_from_lines(&lines, "Stage"), Some("my-stage"));
        assert_eq!(
            extract_field_from_lines(&lines, "Target Branch"),
            Some("main")
        );
        assert_eq!(extract_field_from_lines(&lines, "Missing"), None);
    }

    #[test]
    fn test_extract_backtick_items() {
        let lines = vec![
            "- `src/main.rs`".to_string(),
            "- `src/lib.rs`".to_string(),
            "Some other line".to_string(),
        ];

        let items = extract_backtick_items(&lines);
        assert_eq!(items, vec!["src/main.rs", "src/lib.rs"]);
    }
}
