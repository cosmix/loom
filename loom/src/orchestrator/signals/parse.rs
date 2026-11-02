//! Signal file parsing with format validation.
//!
//! This module parses signal files and validates their format,
//! logging warnings when sections don't match expected patterns.

use anyhow::{bail, Result};

use super::types::SignalContent;

/// Structural `## ` section headers emitted by the signal kinds written to
/// `signals/<session-id>.md`: stage, contract, recovery and merge signals. This
/// list only suppresses the "unexpected section" warning below (see the
/// `Context`/`Handoff`/prefix exemptions there for sections not listed here), so
/// a missing entry is a false warning on that kind of signal, never a failure.
pub(super) const EXPECTED_SECTIONS: &[&str] = &[
    "Worktree Context",
    "Target",
    "Assignment",
    "Acceptance Criteria",
    "Files to Modify",
    "Immediate Tasks",
    "Execution Rules",
    "Your Task",
    "Important",
    "Inherited Responsibilities",
    "Situation",
    "Stage Context",
    "Conflicting Files",
    "Files to Explore",
    "Knowledge Management",
    "Stage Memory",
    "Ultracode Mode",
    "Worktree Isolation",
    "Dependency Outputs",
    "Recommended Skills",
    "Codex Implementers",
    "Sandbox Status",
    "Sandbox Restrictions",
    "Subagent Response Budget",
    "Knowledge Brief",
    "Review Dimensions",
    "Protected Paths",
    "Why the Last Merge Attempt Failed",
    "Cross-Stage Changes",
    super::generate::DEPENDENCY_FINDINGS_HEADING,
    "Goal-Backward Verification",
    "Adjudicator Feedback (from your prior dispute)",
    "Merge Resolver Handback",
    "Contracts",
    "Harness",
    "Scaffold",
    "Language Skills",
    "Your Job",
    "Adjudicator Feedback (from the refused freeze)",
    "Frozen Contracts",
    "Review Gate",
    "Reviewer Suggestions",
    "Unimplemented Suggestions",
];

/// Required sections that must be present
const REQUIRED_SECTIONS: &[&str] = &["Target"];

/// Validate signal format and log warnings for issues
fn validate_signal_format(content: &str, session_id: &str) {
    let mut found_sections: Vec<&str> = Vec::new();

    for (_, outside_fence, heading) in super::helpers::signal_sections(content) {
        if !outside_fence {
            continue;
        }
        if let Some((2, section)) = heading {
            found_sections.push(section);
        }
    }

    // Check for required sections
    for required in REQUIRED_SECTIONS {
        if !found_sections.iter().any(|s| s == required) {
            eprintln!(
                "Warning: Signal file for session '{session_id}' missing required section '{required}'"
            );
        }
    }

    // Check for unexpected sections (not in our expected list)
    for section in &found_sections {
        if !EXPECTED_SECTIONS.contains(section)
            && !section.starts_with("Recovery")
            && !section.starts_with("Previous")
            && !section.starts_with("Plan")
            && !section.starts_with("Dependencies")
            && !section.starts_with("Session")
            && *section != "Knowledge Summary"
            && *section != "Context Restoration"
        {
            // Only warn for truly unexpected sections, not variant sections
            if !section.contains("Context") && !section.contains("Handoff") {
                eprintln!(
                    "Warning: Signal file for session '{session_id}' contains unexpected section '{section}'"
                );
            }
        }
    }
}

/// Validate a field line matches expected format
fn validate_field_format(line: &str, section: &str, session_id: &str) {
    // Check Target section field format
    if section == "Target" && line.starts_with("- ") && !line.contains(':') {
        eprintln!(
            "Warning: Signal file for session '{session_id}' has invalid Target field format: '{line}'. Expected '- **Field**: value'"
        );
    }

    // Check Acceptance Criteria format
    if section == "Acceptance Criteria"
        && line.starts_with("- ")
        && !line.starts_with("- [ ] ")
        && !line.starts_with("- [x] ")
    {
        eprintln!(
            "Warning: Signal file for session '{session_id}' has invalid Acceptance Criteria format: '{line}'. Expected '- [ ] criterion'"
        );
    }
}

/// Parse signal file content with format validation.
///
/// This function parses a signal file and extracts structured content.
/// It also validates the format and logs warnings for any issues found.
pub fn parse_signal_content(session_id: &str, content: &str) -> Result<SignalContent> {
    // Validate format and log any warnings
    validate_signal_format(content, session_id);

    let mut stage_id = String::new();
    let mut plan_id = None;
    let mut stage_name = String::new();
    let mut description = String::new();
    let mut tasks = Vec::new();
    let mut acceptance_criteria = Vec::new();
    let mut context_files = Vec::new();
    let mut files_to_modify = Vec::new();
    let mut current_section = "";

    for (line, outside_fence, heading) in super::helpers::signal_sections(content) {
        let trimmed = line.trim();

        if let Some((2, section)) = heading {
            current_section = section;
            continue;
        }

        // Validate field format within known sections
        if outside_fence && !trimmed.is_empty() && !current_section.is_empty() {
            validate_field_format(trimmed, current_section, session_id);
        }

        if !outside_fence && current_section != "Assignment" {
            continue;
        }
        match current_section {
            "Target" => {
                if let Some(id) = super::helpers::field_value(trimmed, "Stage") {
                    stage_id = id.to_string();
                } else if let Some(pid) = super::helpers::field_value(trimmed, "Plan") {
                    // Strip the plan ID suffix if present (handles both old and new formats)
                    let clean_pid = pid
                        .strip_suffix(" (reference only - content embedded below)")
                        .or_else(|| pid.strip_suffix(" (overview embedded below)"))
                        .unwrap_or(pid);
                    plan_id = Some(clean_pid.to_string());
                }
            }
            "Assignment" => {
                if !trimmed.is_empty() && !description.is_empty() {
                    description.push('\n');
                }
                if let Some((name, desc)) = trimmed.split_once(": ") {
                    if stage_name.is_empty() {
                        stage_name = name.to_string();
                        description = desc.to_string();
                    } else {
                        description.push_str(trimmed);
                    }
                } else if !trimmed.is_empty() {
                    description.push_str(trimmed);
                }
            }
            "Immediate Tasks" => {
                if let Some(task) = super::helpers::numbered_item(trimmed) {
                    tasks.push(task.to_string());
                }
            }
            "Acceptance Criteria" => {
                if let Some(criterion) = trimmed.strip_prefix("- [ ] ") {
                    acceptance_criteria.push(criterion.to_string());
                }
            }
            "Context Restoration" => {
                if let Some(file) = trimmed.strip_prefix("- `") {
                    if let Some(f) = file
                        .strip_suffix("` - Stage definition")
                        .or_else(|| {
                            file.strip_suffix("` - **READ THIS FIRST** - Previous session handoff")
                        })
                        .or_else(|| file.strip_suffix("` - Previous handoff"))
                        .or_else(|| file.strip_suffix("` - Relevant code to modify"))
                        .or_else(|| file.strip_suffix("` - Relevant code"))
                        .or_else(|| file.strip_suffix('`'))
                    {
                        context_files.push(f.to_string());
                    }
                }
            }
            "Files to Modify" => {
                if let Some(file) = trimmed.strip_prefix("- ") {
                    files_to_modify.push(file.to_string());
                }
            }
            _ => {}
        }
    }

    if stage_id.is_empty() {
        bail!(
            "Signal file is missing required 'stage_id' field in Target section. \
             Expected format: '- **Stage**: stage-id'"
        );
    }

    Ok(SignalContent {
        session_id: session_id.to_string(),
        stage_id,
        plan_id,
        stage_name,
        description,
        tasks,
        acceptance_criteria,
        context_files,
        files_to_modify,
        git_history: None, // Git history is informational, not parsed back
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_long_task_list_and_ignores_fenced_structure() {
        let tasks = (1..=12)
            .map(|i| format!("{i}. task {i}\n"))
            .collect::<String>();
        let content = format!("# Signal\n## Target\n- Stage: mine\n## Assignment\nExample:\n```md\n## Target\n- **Stage**: wrong\n## Acceptance Criteria\n- [ ] wrong\n```\n## Immediate Tasks\n{tasks}");
        let parsed = parse_signal_content("s", &content).unwrap();
        assert_eq!(parsed.stage_id, "mine");
        assert_eq!(parsed.tasks.len(), 12);
        assert!(parsed.acceptance_criteria.is_empty());
    }

    #[test]
    fn test_parse_valid_signal() {
        let content = r#"## Target

- **Session**: session-123
- **Stage**: test-stage
- **Plan**: test-plan

## Assignment

Test Stage: Implement testing

## Acceptance Criteria

- [ ] cargo test
- [ ] cargo clippy

## Files to Modify

- src/lib.rs
"#;
        let result = parse_signal_content("session-123", content).unwrap();
        assert_eq!(result.stage_id, "test-stage");
        assert_eq!(result.plan_id, Some("test-plan".to_string()));
        assert_eq!(result.acceptance_criteria.len(), 2);
        assert_eq!(result.files_to_modify.len(), 1);
    }

    #[test]
    fn test_parse_missing_stage_id() {
        let content = r#"## Target

- **Session**: session-123
- **Plan**: test-plan

## Assignment

Test Stage: Testing
"#;
        let result = parse_signal_content("session-123", content);
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("missing required 'stage_id'"));
    }

    #[test]
    fn test_parse_with_plan_suffix() {
        let content = r#"## Target

- **Stage**: my-stage
- **Plan**: PLAN-feature (overview embedded below)
"#;
        let result = parse_signal_content("session-123", content).unwrap();
        assert_eq!(result.plan_id, Some("PLAN-feature".to_string()));
    }
}
