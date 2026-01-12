use anyhow::{bail, Result};

use super::types::SignalContent;

pub fn parse_signal_content(session_id: &str, content: &str) -> Result<SignalContent> {
    let mut stage_id = String::new();
    let mut plan_id = None;
    let mut stage_name = String::new();
    let mut description = String::new();
    let mut tasks = Vec::new();
    let mut acceptance_criteria = Vec::new();
    let mut context_files = Vec::new();
    let mut files_to_modify = Vec::new();

    let mut current_section = "";

    for line in content.lines() {
        let trimmed = line.trim();

        if trimmed.starts_with("## ") {
            current_section = trimmed.trim_start_matches("## ");
            continue;
        }

        match current_section {
            "Target" => {
                if let Some(id) = trimmed.strip_prefix("- **Stage**: ") {
                    stage_id = id.to_string();
                } else if let Some(pid) = trimmed.strip_prefix("- **Plan**: ") {
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
                if let Some(task) = trimmed.strip_prefix(|c: char| c.is_ascii_digit()) {
                    if let Some(t) = task.strip_prefix(". ") {
                        tasks.push(t.to_string());
                    }
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
        bail!("Signal file is missing stage_id");
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
