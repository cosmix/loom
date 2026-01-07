//! Test helper functions for E2E tests

use anyhow::{Context, Result};
use loom::models::session::{Session, SessionStatus};
use loom::models::stage::{Stage, StageStatus};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::thread;
use std::time::{Duration, Instant};
use tempfile::TempDir;

/// Creates a temporary git repository with initial commit
///
/// Returns a TempDir that must be kept in scope for the lifetime of the test
pub fn create_temp_git_repo() -> Result<TempDir> {
    let temp = TempDir::new().context("Failed to create temp directory")?;

    Command::new("git")
        .args(["init"])
        .current_dir(temp.path())
        .output()
        .context("Failed to run git init")?;

    Command::new("git")
        .args(["config", "user.email", "test@test.com"])
        .current_dir(temp.path())
        .output()
        .context("Failed to set git user.email")?;

    Command::new("git")
        .args(["config", "user.name", "Test User"])
        .current_dir(temp.path())
        .output()
        .context("Failed to set git user.name")?;

    std::fs::write(temp.path().join("README.md"), "# Test Repository\n")
        .context("Failed to write README.md")?;

    Command::new("git")
        .args(["add", "."])
        .current_dir(temp.path())
        .output()
        .context("Failed to git add")?;

    Command::new("git")
        .args(["commit", "-m", "Initial commit"])
        .current_dir(temp.path())
        .output()
        .context("Failed to git commit")?;

    Ok(temp)
}

/// Initializes loom with a plan document
///
/// Creates the necessary directory structure and writes the plan file
pub fn init_loom_with_plan(work_dir: &Path, plan_content: &str) -> Result<PathBuf> {
    let doc_plans_dir = work_dir.join("doc").join("plans");
    std::fs::create_dir_all(&doc_plans_dir).context("Failed to create doc/plans directory")?;

    let plan_path = doc_plans_dir.join("PLAN-0001-test.md");
    std::fs::write(&plan_path, plan_content).context("Failed to write plan file")?;

    let loom_work_dir = work_dir.join(".work");
    let subdirs = [
        "runners", "tracks", "signals", "handoffs", "archive", "stages", "sessions",
    ];

    for subdir in &subdirs {
        let path = loom_work_dir.join(subdir);
        std::fs::create_dir_all(&path)
            .with_context(|| format!("Failed to create {subdir} directory"))?;
    }

    Ok(plan_path)
}

/// Writes a stage to .work/stages/{stage.id}.md
pub fn create_stage_file(work_dir: &Path, stage: &Stage) -> Result<()> {
    let stages_dir = work_dir.join(".work").join("stages");
    std::fs::create_dir_all(&stages_dir).context("Failed to create stages directory")?;

    let stage_path = stages_dir.join(format!("{}.md", stage.id));

    let yaml =
        serde_yaml::to_string(stage).context("Failed to serialize stage to YAML")?;

    let mut content = String::new();
    content.push_str("---\n");
    content.push_str(&yaml);
    content.push_str("---\n\n");

    content.push_str(&format!("# Stage: {}\n\n", stage.name));

    if let Some(desc) = &stage.description {
        content.push_str(&format!("{desc}\n\n"));
    }

    content.push_str(&format!("**Status**: {:?}\n\n", stage.status));

    if !stage.dependencies.is_empty() {
        content.push_str("## Dependencies\n\n");
        for dep in &stage.dependencies {
            content.push_str(&format!("- {dep}\n"));
        }
        content.push('\n');
    }

    if !stage.acceptance.is_empty() {
        content.push_str("## Acceptance Criteria\n\n");
        for criterion in &stage.acceptance {
            content.push_str(&format!("- [ ] {criterion}\n"));
        }
        content.push('\n');
    }

    if !stage.files.is_empty() {
        content.push_str("## Files\n\n");
        for file in &stage.files {
            content.push_str(&format!("- `{file}`\n"));
        }
        content.push('\n');
    }

    std::fs::write(&stage_path, content)
        .with_context(|| format!("Failed to write stage file: {}", stage_path.display()))?;

    Ok(())
}

/// Writes a session to .work/sessions/{session.id}.md
pub fn create_session_file(work_dir: &Path, session: &Session) -> Result<()> {
    let sessions_dir = work_dir.join(".work").join("sessions");
    std::fs::create_dir_all(&sessions_dir).context("Failed to create sessions directory")?;

    let session_path = sessions_dir.join(format!("{}.md", session.id));

    let yaml = serde_yaml::to_string(session)
        .context("Failed to serialize session to YAML")?;

    let content = format!(
        "---\n{yaml}---\n\n# Session: {}\n\n## Details\n\n- **Status**: {:?}\n- **Stage**: {}\n- **Tmux**: {}\n- **Context**: {:.1}%\n",
        session.id,
        session.status,
        session.stage_id.as_ref().unwrap_or(&"None".to_string()),
        session.tmux_session.as_ref().unwrap_or(&"None".to_string()),
        session.context_health()
    );

    std::fs::write(&session_path, content).with_context(|| {
        format!(
            "Failed to write session file: {}",
            session_path.display()
        )
    })?;

    Ok(())
}

/// Writes a signal file to .work/signals/{session_id}.md
pub fn create_signal_file(work_dir: &Path, session_id: &str, content: &str) -> Result<()> {
    let signals_dir = work_dir.join(".work").join("signals");
    std::fs::create_dir_all(&signals_dir).context("Failed to create signals directory")?;

    let signal_path = signals_dir.join(format!("{session_id}.md"));

    std::fs::write(&signal_path, content).with_context(|| {
        format!("Failed to write signal file: {}", signal_path.display())
    })?;

    Ok(())
}

/// Reads a stage from .work/stages/{stage_id}.md
pub fn read_stage_file(work_dir: &Path, stage_id: &str) -> Result<Stage> {
    let stage_path = work_dir
        .join(".work")
        .join("stages")
        .join(format!("{stage_id}.md"));

    if !stage_path.exists() {
        anyhow::bail!("Stage file not found: {}", stage_path.display());
    }

    let content = std::fs::read_to_string(&stage_path)
        .with_context(|| format!("Failed to read stage file: {}", stage_path.display()))?;

    parse_stage_from_markdown(&content)
        .with_context(|| format!("Failed to parse stage from: {}", stage_path.display()))
}

/// Reads a session from .work/sessions/{session_id}.md
pub fn read_session_file(work_dir: &Path, session_id: &str) -> Result<Session> {
    let session_path = work_dir
        .join(".work")
        .join("sessions")
        .join(format!("{session_id}.md"));

    if !session_path.exists() {
        anyhow::bail!("Session file not found: {}", session_path.display());
    }

    let content = std::fs::read_to_string(&session_path)
        .with_context(|| format!("Failed to read session file: {}", session_path.display()))?;

    parse_session_from_markdown(&content)
        .with_context(|| format!("Failed to parse session from: {}", session_path.display()))
}

/// Parse a Stage from markdown with YAML frontmatter
fn parse_stage_from_markdown(content: &str) -> Result<Stage> {
    let frontmatter = extract_yaml_frontmatter(content)?;

    let stage: Stage = serde_yaml::from_value(frontmatter)
        .context("Failed to deserialize Stage from YAML frontmatter")?;

    Ok(stage)
}

/// Parse a Session from markdown with YAML frontmatter
fn parse_session_from_markdown(content: &str) -> Result<Session> {
    let frontmatter = extract_yaml_frontmatter(content)?;

    let session: Session = serde_yaml::from_value(frontmatter)
        .context("Failed to deserialize Session from YAML frontmatter")?;

    Ok(session)
}

/// Extract YAML frontmatter from markdown content
fn extract_yaml_frontmatter(content: &str) -> Result<serde_yaml::Value> {
    let lines: Vec<&str> = content.lines().collect();

    if lines.is_empty() || !lines[0].trim().starts_with("---") {
        anyhow::bail!("No frontmatter delimiter found at start of content");
    }

    let mut end_idx = None;
    for (idx, line) in lines.iter().enumerate().skip(1) {
        if line.trim().starts_with("---") {
            end_idx = Some(idx);
            break;
        }
    }

    let end_idx =
        end_idx.ok_or_else(|| anyhow::anyhow!("Frontmatter not properly closed with ---"))?;

    let yaml_content = lines[1..end_idx].join("\n");

    serde_yaml::from_str(&yaml_content).context("Failed to parse YAML frontmatter")
}

/// Checks if tmux is installed and available
pub fn is_tmux_available() -> bool {
    Command::new("tmux")
        .arg("-V")
        .output()
        .map(|output| output.status.success())
        .unwrap_or(false)
}

/// Cleans up tmux sessions with a given prefix
///
/// This is useful for cleaning up test sessions that may have been left running
pub fn cleanup_tmux_sessions(prefix: &str) -> Result<()> {
    if !is_tmux_available() {
        return Ok(());
    }

    let output = Command::new("tmux")
        .args(["list-sessions", "-F", "#{session_name}"])
        .output();

    if let Ok(output) = output {
        if output.status.success() {
            let sessions = String::from_utf8_lossy(&output.stdout);
            for session_name in sessions.lines() {
                if session_name.starts_with(prefix) {
                    let _ = Command::new("tmux")
                        .args(["kill-session", "-t", session_name])
                        .output();
                }
            }
        }
    }

    Ok(())
}

/// Polls a predicate function until it returns true or timeout is reached
///
/// Useful for waiting for asynchronous operations to complete in tests
pub fn wait_for_condition<F>(predicate: F, timeout_ms: u64) -> Result<()>
where
    F: Fn() -> bool,
{
    let start = Instant::now();
    let timeout = Duration::from_millis(timeout_ms);

    while start.elapsed() < timeout {
        if predicate() {
            return Ok(());
        }
        thread::sleep(Duration::from_millis(100));
    }

    anyhow::bail!("Timeout waiting for condition after {timeout_ms}ms")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_temp_git_repo() {
        let repo = create_temp_git_repo().expect("Should create git repo");

        let git_dir = repo.path().join(".git");
        assert!(git_dir.exists(), "Git directory should exist");

        let readme = repo.path().join("README.md");
        assert!(readme.exists(), "README should exist");
    }

    #[test]
    fn test_init_loom_with_plan() {
        let temp = TempDir::new().expect("Should create temp dir");
        let plan_content = r#"# Test Plan

<!-- loom METADATA - Do not edit manually -->

```yaml
loom:
  version: 1
  stages:
    - id: stage-1
      name: "Test Stage"
```

<!-- END loom METADATA -->
"#;

        let plan_path =
            init_loom_with_plan(temp.path(), plan_content).expect("Should init loom");

        assert!(plan_path.exists(), "Plan file should exist");

        let work_dir = temp.path().join(".work");
        assert!(work_dir.exists(), ".work directory should exist");
        assert!(
            work_dir.join("stages").exists(),
            "stages directory should exist"
        );
        assert!(
            work_dir.join("sessions").exists(),
            "sessions directory should exist"
        );
    }

    #[test]
    fn test_create_and_read_stage_file() {
        let temp = TempDir::new().expect("Should create temp dir");
        let work_dir = temp.path();

        let mut stage = Stage::new("Test Stage".to_string(), Some("Test description".to_string()));
        stage.id = "test-stage-1".to_string();
        stage.status = StageStatus::Ready;
        stage.add_dependency("dep-1".to_string());
        stage.add_acceptance_criterion("Tests pass".to_string());

        create_stage_file(work_dir, &stage).expect("Should create stage file");

        let loaded = read_stage_file(work_dir, "test-stage-1").expect("Should read stage file");

        assert_eq!(loaded.id, stage.id);
        assert_eq!(loaded.name, stage.name);
        assert_eq!(loaded.description, stage.description);
        assert_eq!(loaded.status, stage.status);
        assert_eq!(loaded.dependencies, stage.dependencies);
        assert_eq!(loaded.acceptance, stage.acceptance);
    }

    #[test]
    fn test_create_and_read_session_file() {
        let temp = TempDir::new().expect("Should create temp dir");
        let work_dir = temp.path();

        let mut session = Session::new();
        session.id = "test-session-1".to_string();
        session.status = SessionStatus::Running;
        session.assign_to_stage("stage-1".to_string());
        session.set_tmux_session("test-tmux".to_string());

        create_session_file(work_dir, &session).expect("Should create session file");

        let loaded =
            read_session_file(work_dir, "test-session-1").expect("Should read session file");

        assert_eq!(loaded.id, session.id);
        assert_eq!(loaded.status, session.status);
        assert_eq!(loaded.stage_id, session.stage_id);
        assert_eq!(loaded.tmux_session, session.tmux_session);
    }

    #[test]
    fn test_create_signal_file() {
        let temp = TempDir::new().expect("Should create temp dir");
        let work_dir = temp.path();

        let signal_content = "# Signal: test-signal\n\nTest signal content";

        create_signal_file(work_dir, "test-signal", signal_content)
            .expect("Should create signal file");

        let signal_path = work_dir
            .join(".work")
            .join("signals")
            .join("test-signal.md");
        assert!(signal_path.exists(), "Signal file should exist");

        let content = std::fs::read_to_string(signal_path).expect("Should read signal file");
        assert_eq!(content, signal_content);
    }

    #[test]
    fn test_is_tmux_available() {
        let result = is_tmux_available();
        println!("tmux available: {result}");
    }

    #[test]
    fn test_wait_for_condition_success() {
        use std::sync::{Arc, Mutex};

        let counter = Arc::new(Mutex::new(0));
        let counter_clone = counter.clone();

        std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(200));
            *counter_clone.lock().unwrap() = 1;
        });

        let result = wait_for_condition(
            || {
                let count = *counter.lock().unwrap();
                count == 1
            },
            1000,
        );

        assert!(result.is_ok(), "Condition should be met");
    }

    #[test]
    fn test_wait_for_condition_timeout() {
        let result = wait_for_condition(|| false, 500);

        assert!(result.is_err(), "Should timeout");
        assert!(
            result.unwrap_err().to_string().contains("Timeout"),
            "Error should mention timeout"
        );
    }

    #[test]
    fn test_extract_yaml_frontmatter() {
        let content = r#"---
id: test-1
name: Test
status: Pending
---

# Body content"#;

        let yaml = extract_yaml_frontmatter(content).expect("Should extract frontmatter");
        assert!(yaml.is_mapping());

        let map = yaml.as_mapping().unwrap();
        assert_eq!(
            map.get(serde_yaml::Value::String("id".to_string()))
                .unwrap()
                .as_str()
                .unwrap(),
            "test-1"
        );
    }

    #[test]
    fn test_extract_yaml_frontmatter_missing_delimiter() {
        let content = "id: test-1\nname: Test";

        let result = extract_yaml_frontmatter(content);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("No frontmatter"));
    }
}
