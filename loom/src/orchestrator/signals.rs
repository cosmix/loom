use anyhow::{bail, Context, Result};
use std::fs;
use std::path::{Path, PathBuf};

use crate::models::session::Session;
use crate::models::stage::Stage;
use crate::models::worktree::Worktree;

#[derive(Debug, Clone)]
pub struct DependencyStatus {
    pub stage_id: String,
    pub name: String,
    pub status: String,
}

#[derive(Debug, Clone)]
pub struct SignalContent {
    pub session_id: String,
    pub stage_id: String,
    pub plan_id: Option<String>,
    pub stage_name: String,
    pub description: String,
    pub tasks: Vec<String>,
    pub acceptance_criteria: Vec<String>,
    pub context_files: Vec<String>,
    pub files_to_modify: Vec<String>,
}

#[derive(Debug, Default)]
pub struct SignalUpdates {
    pub add_tasks: Option<Vec<String>>,
    pub update_dependencies: Option<Vec<DependencyStatus>>,
    pub add_context_files: Option<Vec<String>>,
}

pub fn generate_signal(
    session: &Session,
    stage: &Stage,
    worktree: &Worktree,
    dependencies_status: &[DependencyStatus],
    handoff_file: Option<&str>,
    work_dir: &Path,
) -> Result<PathBuf> {
    let signals_dir = work_dir.join("signals");

    if !signals_dir.exists() {
        fs::create_dir_all(&signals_dir).context("Failed to create signals directory")?;
    }

    let signal_path = signals_dir.join(format!("{}.md", session.id));
    let content =
        format_signal_content(session, stage, worktree, dependencies_status, handoff_file);

    fs::write(&signal_path, content)
        .with_context(|| format!("Failed to write signal file: {}", signal_path.display()))?;

    Ok(signal_path)
}

pub fn update_signal(session_id: &str, updates: SignalUpdates, work_dir: &Path) -> Result<()> {
    let signal_path = work_dir.join("signals").join(format!("{session_id}.md"));

    if !signal_path.exists() {
        bail!("Signal file does not exist: {}", signal_path.display());
    }

    let content = fs::read_to_string(&signal_path).context("Failed to read signal file")?;

    let mut updated_content = content;

    if let Some(tasks) = updates.add_tasks {
        if !tasks.is_empty() {
            let task_section = tasks
                .iter()
                .enumerate()
                .map(|(i, task)| format!("{}. {}", i + 1, task))
                .collect::<Vec<_>>()
                .join("\n");

            if let Some(pos) = updated_content.find("## Immediate Tasks") {
                if let Some(next_section) = updated_content[pos..].find("\n\n## ") {
                    let insert_pos = pos + next_section;
                    updated_content.insert_str(insert_pos, &format!("\n{task_section}"));
                }
            }
        }
    }

    if let Some(deps) = updates.update_dependencies {
        if !deps.is_empty() {
            let dep_table = format_dependency_table(&deps);
            if let Some(start) = updated_content.find("## Dependencies Status") {
                if let Some(table_start) = updated_content[start..].find("| Dependency") {
                    let abs_table_start = start + table_start;
                    if let Some(next_section) = updated_content[abs_table_start..].find("\n\n## ") {
                        let end_pos = abs_table_start + next_section;
                        updated_content.replace_range(abs_table_start..end_pos, &dep_table);
                    }
                }
            }
        }
    }

    if let Some(files) = updates.add_context_files {
        if !files.is_empty() {
            let file_list = files
                .iter()
                .map(|f| format!("- {f}"))
                .collect::<Vec<_>>()
                .join("\n");

            if let Some(pos) = updated_content.find("## Context Restoration") {
                if let Some(next_section) = updated_content[pos..].find("\n\n## ") {
                    let insert_pos = pos + next_section;
                    updated_content.insert_str(insert_pos, &format!("\n{file_list}"));
                }
            }
        }
    }

    fs::write(&signal_path, updated_content).context("Failed to update signal file")?;

    Ok(())
}

pub fn remove_signal(session_id: &str, work_dir: &Path) -> Result<()> {
    let signal_path = work_dir.join("signals").join(format!("{session_id}.md"));

    if signal_path.exists() {
        fs::remove_file(&signal_path)
            .with_context(|| format!("Failed to remove signal file: {}", signal_path.display()))?;
    }

    Ok(())
}

pub fn read_signal(session_id: &str, work_dir: &Path) -> Result<Option<SignalContent>> {
    let signal_path = work_dir.join("signals").join(format!("{session_id}.md"));

    if !signal_path.exists() {
        return Ok(None);
    }

    let content = fs::read_to_string(&signal_path).context("Failed to read signal file")?;

    let parsed = parse_signal_content(session_id, &content)?;
    Ok(Some(parsed))
}

pub fn list_signals(work_dir: &Path) -> Result<Vec<String>> {
    let signals_dir = work_dir.join("signals");

    if !signals_dir.exists() {
        return Ok(Vec::new());
    }

    let mut signals = Vec::new();

    for entry in fs::read_dir(signals_dir).context("Failed to read signals directory")? {
        let entry = entry.context("Failed to read directory entry")?;
        let path = entry.path();

        if path.extension().and_then(|s| s.to_str()) == Some("md") {
            if let Some(name) = path.file_stem().and_then(|s| s.to_str()) {
                signals.push(name.to_string());
            }
        }
    }

    signals.sort();
    Ok(signals)
}

fn format_signal_content(
    session: &Session,
    stage: &Stage,
    worktree: &Worktree,
    dependencies_status: &[DependencyStatus],
    handoff_file: Option<&str>,
) -> String {
    let mut content = String::new();

    content.push_str(&format!("# Signal: {}\n\n", session.id));

    // Worktree context - self-contained signal
    content.push_str("## Worktree Context\n\n");
    content.push_str(
        "You are in an **isolated git worktree**. This signal contains everything you need:\n\n",
    );
    content.push_str("- **Your stage assignment and acceptance criteria are below** - this file is self-contained\n");
    content.push_str("- **Access `.work/` via symlink** for handoffs and structure map\n");
    content.push_str(
        "- **Commit to your worktree branch** - it will be merged after verification\n\n",
    );

    // Add reminder to follow CLAUDE.md rules
    content.push_str("## Execution Rules\n\n");
    content.push_str("Follow your `~/.claude/CLAUDE.md` rules. Key reminders:\n");
    content.push_str(
        "- **Delegate work to subagents** - use Task tool with appropriate agent types\n",
    );
    content.push_str("- **Use TodoWrite** to plan and track progress\n");
    content.push_str("- **Verify acceptance criteria** before marking stage complete\n");
    content.push_str("- **Create handoff** if context exceeds 75%\n\n");

    content.push_str("## Target\n\n");
    content.push_str(&format!("- **Session**: {}\n", session.id));
    content.push_str(&format!("- **Stage**: {}\n", stage.id));
    if let Some(plan_id) = &stage.plan_id {
        content.push_str(&format!(
            "- **Plan**: {plan_id} (reference only - content embedded below)\n"
        ));
    }
    content.push_str(&format!("- **Worktree**: {}\n", worktree.path.display()));
    content.push_str(&format!("- **Branch**: {}\n", worktree.branch));
    content.push('\n');

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

    content.push_str("## Context Restoration\n\n");
    content.push_str("The `.work/` directory is accessible via symlink. Available resources:\n\n");
    if let Some(handoff) = handoff_file {
        content.push_str(&format!(
            "- `.work/handoffs/{handoff}.md` - **READ THIS FIRST** - Previous session handoff\n"
        ));
    }
    content.push_str("- `.work/structure.md` - Codebase structure map (if exists)\n");
    for file in &stage.files {
        content.push_str(&format!("- `{file}` - Relevant code to modify\n"));
    }
    content.push('\n');

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

fn format_dependency_table(deps: &[DependencyStatus]) -> String {
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

fn extract_tasks_from_description(description: &str) -> Vec<String> {
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

fn parse_signal_content(session_id: &str, content: &str) -> Result<SignalContent> {
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
                    // Strip the "(reference only - content embedded below)" suffix if present
                    let clean_pid = pid
                        .strip_suffix(" (reference only - content embedded below)")
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
                        .or_else(|| file.strip_suffix("` - Codebase structure map (if exists)"))
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
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::session::Session;
    use crate::models::stage::{Stage, StageStatus};
    use crate::models::worktree::Worktree;
    use tempfile::TempDir;

    fn create_test_session() -> Session {
        let mut session = Session::new();
        session.id = "session-test-123".to_string();
        session.assign_to_stage("stage-1".to_string());
        session
    }

    fn create_test_stage() -> Stage {
        let mut stage = Stage::new(
            "Implement signals module".to_string(),
            Some("Create signal file generation logic".to_string()),
        );
        stage.id = "stage-1".to_string();
        stage.status = StageStatus::Executing;
        stage.add_acceptance_criterion("Signal files are generated correctly".to_string());
        stage.add_acceptance_criterion("All tests pass".to_string());
        stage.add_file_pattern("src/orchestrator/signals.rs".to_string());
        stage
    }

    fn create_test_worktree() -> Worktree {
        Worktree::new(
            "stage-1".to_string(),
            PathBuf::from("/repo/.worktrees/stage-1"),
            "loom/stage-1".to_string(),
        )
    }

    #[test]
    fn test_generate_signal_basic() {
        let temp_dir = TempDir::new().unwrap();
        let work_dir = temp_dir.path().join(".work");
        fs::create_dir_all(&work_dir).unwrap();

        let session = create_test_session();
        let stage = create_test_stage();
        let worktree = create_test_worktree();

        let result = generate_signal(&session, &stage, &worktree, &[], None, &work_dir);

        assert!(result.is_ok());
        let signal_path = result.unwrap();
        assert!(signal_path.exists());

        let content = fs::read_to_string(&signal_path).unwrap();
        assert!(content.contains("# Signal: session-test-123"));
        assert!(content.contains("- **Session**: session-test-123"));
        assert!(content.contains("- **Stage**: stage-1"));
        assert!(content.contains("## Assignment"));
        assert!(content.contains("Implement signals module"));
    }

    #[test]
    fn test_generate_signal_with_dependencies() {
        let temp_dir = TempDir::new().unwrap();
        let work_dir = temp_dir.path().join(".work");
        fs::create_dir_all(&work_dir).unwrap();

        let session = create_test_session();
        let stage = create_test_stage();
        let worktree = create_test_worktree();

        let deps = vec![DependencyStatus {
            stage_id: "stage-0".to_string(),
            name: "Setup models".to_string(),
            status: "completed".to_string(),
        }];

        let result = generate_signal(&session, &stage, &worktree, &deps, None, &work_dir);

        assert!(result.is_ok());
        let signal_path = result.unwrap();
        let content = fs::read_to_string(&signal_path).unwrap();

        assert!(content.contains("## Dependencies Status"));
        assert!(content.contains("Setup models"));
        assert!(content.contains("completed"));
    }

    #[test]
    fn test_generate_signal_with_handoff() {
        let temp_dir = TempDir::new().unwrap();
        let work_dir = temp_dir.path().join(".work");
        fs::create_dir_all(&work_dir).unwrap();

        let session = create_test_session();
        let stage = create_test_stage();
        let worktree = create_test_worktree();

        let result = generate_signal(
            &session,
            &stage,
            &worktree,
            &[],
            Some("2026-01-06-previous-work"),
            &work_dir,
        );

        assert!(result.is_ok());
        let signal_path = result.unwrap();
        let content = fs::read_to_string(&signal_path).unwrap();

        assert!(content.contains("## Context Restoration"));
        assert!(content.contains("2026-01-06-previous-work.md"));
    }

    #[test]
    fn test_format_signal_content() {
        let session = create_test_session();
        let stage = create_test_stage();
        let worktree = create_test_worktree();

        let content = format_signal_content(&session, &stage, &worktree, &[], None);

        assert!(content.contains("# Signal: session-test-123"));
        assert!(content.contains("## Worktree Context"));
        assert!(content.contains("This signal contains everything you need"));
        assert!(content.contains("## Target"));
        assert!(content.contains("## Assignment"));
        assert!(content.contains("## Immediate Tasks"));
        assert!(content.contains("## Context Restoration"));
        assert!(content.contains("## Acceptance Criteria"));
        assert!(content.contains("## Files to Modify"));
        assert!(content.contains("src/orchestrator/signals.rs"));
    }

    #[test]
    fn test_extract_tasks_from_description() {
        let desc1 = "- First task\n- Second task\n- Third task";
        let tasks1 = extract_tasks_from_description(desc1);
        assert_eq!(tasks1.len(), 3);
        assert_eq!(tasks1[0], "First task");

        let desc2 = "1. First task\n2. Second task\n3. Third task";
        let tasks2 = extract_tasks_from_description(desc2);
        assert_eq!(tasks2.len(), 3);
        assert_eq!(tasks2[1], "Second task");

        let desc3 = "* Task one\n* Task two";
        let tasks3 = extract_tasks_from_description(desc3);
        assert_eq!(tasks3.len(), 2);
        assert_eq!(tasks3[0], "Task one");

        let desc4 = "No tasks here";
        let tasks4 = extract_tasks_from_description(desc4);
        assert_eq!(tasks4.len(), 0);
    }

    #[test]
    fn test_remove_signal() {
        let temp_dir = TempDir::new().unwrap();
        let work_dir = temp_dir.path().join(".work");
        fs::create_dir_all(work_dir.join("signals")).unwrap();

        let signal_path = work_dir.join("signals").join("session-test-123.md");
        fs::write(&signal_path, "test content").unwrap();
        assert!(signal_path.exists());

        let result = remove_signal("session-test-123", &work_dir);
        assert!(result.is_ok());
        assert!(!signal_path.exists());

        let result2 = remove_signal("nonexistent", &work_dir);
        assert!(result2.is_ok());
    }

    #[test]
    fn test_list_signals() {
        let temp_dir = TempDir::new().unwrap();
        let work_dir = temp_dir.path().join(".work");
        let signals_dir = work_dir.join("signals");
        fs::create_dir_all(&signals_dir).unwrap();

        fs::write(signals_dir.join("session-1.md"), "").unwrap();
        fs::write(signals_dir.join("session-2.md"), "").unwrap();
        fs::write(signals_dir.join("session-3.md"), "").unwrap();
        fs::write(signals_dir.join("not-a-signal.txt"), "").unwrap();

        let signals = list_signals(&work_dir).unwrap();
        assert_eq!(signals.len(), 3);
        assert!(signals.contains(&"session-1".to_string()));
        assert!(signals.contains(&"session-2".to_string()));
        assert!(signals.contains(&"session-3".to_string()));
        assert!(!signals.contains(&"not-a-signal".to_string()));
    }

    #[test]
    fn test_read_signal() {
        let temp_dir = TempDir::new().unwrap();
        let work_dir = temp_dir.path().join(".work");
        fs::create_dir_all(&work_dir).unwrap();

        let session = create_test_session();
        let stage = create_test_stage();
        let worktree = create_test_worktree();

        generate_signal(&session, &stage, &worktree, &[], None, &work_dir).unwrap();

        let result = read_signal("session-test-123", &work_dir);
        assert!(result.is_ok());

        let signal_content = result.unwrap();
        assert!(signal_content.is_some());

        let content = signal_content.unwrap();
        assert_eq!(content.session_id, "session-test-123");
        assert_eq!(content.stage_id, "stage-1");
        assert_eq!(content.stage_name, "Implement signals module");
        assert!(!content.acceptance_criteria.is_empty());
    }

    #[test]
    fn test_update_signal_add_tasks() {
        let temp_dir = TempDir::new().unwrap();
        let work_dir = temp_dir.path().join(".work");
        fs::create_dir_all(&work_dir).unwrap();

        let session = create_test_session();
        let stage = create_test_stage();
        let worktree = create_test_worktree();

        generate_signal(&session, &stage, &worktree, &[], None, &work_dir).unwrap();

        let updates = SignalUpdates {
            add_tasks: Some(vec!["New task 1".to_string(), "New task 2".to_string()]),
            ..Default::default()
        };

        let result = update_signal("session-test-123", updates, &work_dir);
        assert!(result.is_ok());

        let signal_path = work_dir.join("signals").join("session-test-123.md");
        let content = fs::read_to_string(signal_path).unwrap();
        assert!(content.contains("New task 1"));
        assert!(content.contains("New task 2"));
    }
}
