//! Session continuation with handoff context.
//!
//! This module handles resuming work on a stage after a session hands off due to
//! context exhaustion or other reasons. It provides functionality to:
//!
//! - Prepare continuation context (stage, handoff, worktree)
//! - Create new sessions with handoff references
//! - Generate signals that include handoff file paths for context restoration
//! - Optionally spawn tmux sessions to continue work
//!
//! # Example
//!
//! ```ignore
//! use loom::orchestrator::continuation::{prepare_continuation, ContinuationConfig};
//! use std::path::Path;
//!
//! let work_dir = Path::new(".work");
//! let stage_id = "stage-1";
//!
//! // Prepare continuation context (loads stage, finds handoff, verifies worktree)
//! let context = prepare_continuation(stage_id, work_dir)?;
//!
//! println!("Stage: {}", context.stage.name);
//! println!("Worktree: {}", context.worktree_path.display());
//! if let Some(handoff) = &context.handoff_path {
//!     println!("Handoff: {}", handoff.display());
//! }
//! ```

use anyhow::{anyhow, bail, Context, Result};
use std::fs;
use std::path::{Path, PathBuf};

use crate::fs::stage_files::find_stage_file;
use crate::handoff::generator::find_latest_handoff;
use crate::models::session::Session;
use crate::models::stage::{Stage, StageStatus};
use crate::models::worktree::Worktree;
use crate::orchestrator::signals::{generate_signal, DependencyStatus};
use crate::orchestrator::terminal::{create_backend, BackendType};

/// Configuration for session continuation
#[derive(Debug, Clone)]
pub struct ContinuationConfig {
    /// Backend type for spawning sessions
    pub backend_type: BackendType,
    /// Whether to automatically spawn a terminal session
    pub auto_spawn: bool,
}

impl Default for ContinuationConfig {
    fn default() -> Self {
        Self {
            backend_type: BackendType::Native,
            auto_spawn: true,
        }
    }
}

/// Context prepared for continuing a stage after handoff
#[derive(Debug)]
pub struct ContinuationContext {
    pub stage: Stage,
    pub handoff_path: Option<PathBuf>,
    pub worktree_path: PathBuf,
    pub branch: String,
}

/// Continue work on a stage after a handoff
///
/// Creates a new session, generates signal file with handoff reference,
/// optionally spawns tmux session, and updates stage status.
///
/// # Arguments
/// * `stage` - The stage to continue work on
/// * `handoff_path` - Optional path to the handoff file for context restoration
/// * `worktree` - The worktree where work will continue
/// * `config` - Configuration for continuation (spawner settings, auto_spawn)
/// * `work_dir` - The .work directory path
///
/// # Returns
/// A new Session ready to continue the work
pub fn continue_session(
    stage: &Stage,
    handoff_path: Option<&Path>,
    worktree: &Worktree,
    config: &ContinuationConfig,
    work_dir: &Path,
) -> Result<Session> {
    // Validate stage can be continued
    if !matches!(
        stage.status,
        StageStatus::NeedsHandoff | StageStatus::Ready | StageStatus::Executing
    ) {
        bail!(
            "Stage {} is in status {:?}, which cannot be continued. Expected NeedsHandoff, Ready, or Executing.",
            stage.id,
            stage.status
        );
    }

    // Create a new session
    let mut session = Session::new();
    session.assign_to_stage(stage.id.clone());
    session.set_worktree_path(worktree.path.clone());

    // Extract handoff filename if provided
    let handoff_file = handoff_path.and_then(|p| {
        p.file_stem()
            .and_then(|s| s.to_str())
            .map(|s| s.to_string())
    });

    // Generate signal file with handoff reference
    // For continuation, we typically don't have fresh dependency status,
    // so we pass an empty array. The signal will reference the handoff
    // file which contains the full context.
    let dependencies_status: Vec<DependencyStatus> = Vec::new();

    // Store original session ID to verify consistency after spawn
    let original_session_id = session.id.clone();

    let signal_path = generate_signal(
        &session,
        stage,
        worktree,
        &dependencies_status,
        handoff_file.as_deref(),
        None, // git_history will be extracted from worktree in future enhancement
        work_dir,
    )
    .context("Failed to generate signal for continuation")?;

    // Spawn terminal session if auto_spawn is enabled
    if config.auto_spawn {
        let backend = create_backend(config.backend_type)
            .context("Failed to create terminal backend for continuation")?;
        session = backend
            .spawn_session(stage, worktree, session, &signal_path)
            .context("Failed to spawn session for continuation")?;
    }

    // Verify session ID consistency (signal file uses this ID)
    debug_assert_eq!(
        original_session_id, session.id,
        "Session ID mismatch: signal file created with '{}' but saving session with '{}'",
        original_session_id, session.id
    );

    // Save session to .work/sessions/
    save_session(&session, work_dir)?;

    Ok(session)
}

/// Prepare context for continuing a stage
///
/// Loads the stage, finds the latest handoff if available, and verifies
/// the worktree exists. Returns all the context needed to continue work.
///
/// # Arguments
/// * `stage_id` - The ID of the stage to continue
/// * `work_dir` - The .work directory path
///
/// # Returns
/// ContinuationContext with stage, handoff path, worktree path, and branch
pub fn prepare_continuation(stage_id: &str, work_dir: &Path) -> Result<ContinuationContext> {
    // Load the stage
    let stage = load_stage(work_dir, stage_id)?;

    // Find the latest handoff for this stage (if any)
    let handoff_path = find_latest_handoff(stage_id, work_dir)?;

    // Get worktree information
    let (worktree_path, branch) = if let Some(worktree_id) = &stage.worktree {
        // Load worktree from .worktrees metadata or infer from ID
        let path = load_worktree_path(work_dir, worktree_id)?;
        let branch = Worktree::branch_name(stage_id);
        (path, branch)
    } else {
        // No worktree assigned - use project root
        let project_root = work_dir.parent().ok_or_else(|| {
            anyhow!(
                "Cannot determine project root from work_dir: {}",
                work_dir.display()
            )
        })?;
        (project_root.to_path_buf(), "main".to_string())
    };

    Ok(ContinuationContext {
        stage,
        handoff_path,
        worktree_path,
        branch,
    })
}

/// Load handoff content from a markdown file
///
/// # Arguments
/// * `handoff_path` - Path to the handoff markdown file
///
/// # Returns
/// The full markdown content of the handoff file
pub fn load_handoff_content(handoff_path: &Path) -> Result<String> {
    if !handoff_path.exists() {
        bail!("Handoff file does not exist: {}", handoff_path.display());
    }

    fs::read_to_string(handoff_path)
        .with_context(|| format!("Failed to read handoff file: {}", handoff_path.display()))
}

/// Load a stage from .work/stages/
fn load_stage(work_dir: &Path, stage_id: &str) -> Result<Stage> {
    let stages_dir = work_dir.join("stages");

    let stage_path = find_stage_file(&stages_dir, stage_id)?.ok_or_else(|| {
        anyhow!("Stage file not found for: {stage_id}. Run 'loom stage create' first.")
    })?;

    let content = fs::read_to_string(&stage_path)
        .with_context(|| format!("Failed to read stage file: {}", stage_path.display()))?;

    parse_stage_from_markdown(&content)
}

/// Parse a Stage from markdown with YAML frontmatter
fn parse_stage_from_markdown(content: &str) -> Result<Stage> {
    let frontmatter = extract_yaml_frontmatter(content)?;

    let stage: Stage = serde_yaml::from_value(frontmatter)
        .context("Failed to deserialize stage from YAML frontmatter")?;

    Ok(stage)
}

/// Extract YAML frontmatter from markdown content
fn extract_yaml_frontmatter(content: &str) -> Result<serde_yaml::Value> {
    let lines: Vec<&str> = content.lines().collect();

    if lines.is_empty() || !lines[0].starts_with("---") {
        bail!("Missing YAML frontmatter delimiter");
    }

    let end_index = lines
        .iter()
        .skip(1)
        .position(|line| line.starts_with("---"))
        .ok_or_else(|| anyhow!("Missing closing YAML frontmatter delimiter"))?
        + 1;

    let yaml_lines = &lines[1..end_index];
    let yaml_content = yaml_lines.join("\n");

    serde_yaml::from_str(&yaml_content).context("Failed to parse YAML frontmatter")
}


/// Load worktree path from worktree ID
///
/// In the future, this could load from .worktrees metadata.
/// For now, we infer the path from the convention.
fn load_worktree_path(work_dir: &Path, worktree_id: &str) -> Result<PathBuf> {
    let project_root = work_dir.parent().ok_or_else(|| {
        anyhow!(
            "Cannot determine project root from work_dir: {}",
            work_dir.display()
        )
    })?;

    let worktree_path = Worktree::worktree_path(project_root, worktree_id);

    if !worktree_path.exists() {
        bail!(
            "Worktree directory does not exist: {}. Create it with 'loom worktree create'.",
            worktree_path.display()
        );
    }

    Ok(worktree_path)
}

/// Save session to .work/sessions/{id}.md
fn save_session(session: &Session, work_dir: &Path) -> Result<()> {
    let sessions_dir = work_dir.join("sessions");
    fs::create_dir_all(&sessions_dir).context("Failed to create sessions directory")?;

    let session_file = sessions_dir.join(format!("{}.md", session.id));
    let content = session_to_markdown(session);

    fs::write(&session_file, content)
        .with_context(|| format!("Failed to write session file: {}", session_file.display()))?;

    Ok(())
}

/// Convert session to markdown format
fn session_to_markdown(session: &Session) -> String {
    let yaml = serde_yaml::to_string(session).unwrap_or_else(|_| String::from("{}"));

    format!(
        "---\n{yaml}---\n\n# Session: {}\n\n## Details\n\n- **Status**: {:?}\n- **Stage**: {}\n- **Tmux**: {}\n- **Context**: {:.1}%\n",
        session.id,
        session.status,
        session.stage_id.as_ref().unwrap_or(&"None".to_string()),
        session.tmux_session.as_ref().unwrap_or(&"None".to_string()),
        session.context_health()
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::stage::StageStatus;
    use crate::models::worktree::Worktree;
    use std::fs;
    use tempfile::TempDir;

    fn create_test_work_dir() -> (TempDir, PathBuf) {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let work_dir = temp_dir.path().join(".work");

        fs::create_dir_all(work_dir.join("stages")).unwrap();
        fs::create_dir_all(work_dir.join("handoffs")).unwrap();
        fs::create_dir_all(work_dir.join("sessions")).unwrap();
        fs::create_dir_all(work_dir.join("signals")).unwrap();

        (temp_dir, work_dir)
    }

    fn create_test_stage(stage_id: &str, work_dir: &Path) -> Stage {
        let mut stage = Stage::new(
            "Test Stage".to_string(),
            Some("Test description".to_string()),
        );
        stage.id = stage_id.to_string();
        stage.status = StageStatus::NeedsHandoff;
        stage.worktree = Some(stage_id.to_string());

        // Save the stage
        let stage_path = work_dir.join("stages").join(format!("{stage_id}.md"));
        let yaml = serde_yaml::to_string(&stage).unwrap();
        let content = format!("---\n{yaml}---\n\n# Stage: {stage_id}\n");
        fs::write(stage_path, content).unwrap();

        stage
    }

    fn create_test_worktree(stage_id: &str, project_root: &Path) -> Worktree {
        let worktree_path = Worktree::worktree_path(project_root, stage_id);
        fs::create_dir_all(&worktree_path).unwrap();

        let mut worktree = Worktree::new(
            stage_id.to_string(),
            worktree_path,
            Worktree::branch_name(stage_id),
        );
        worktree.mark_active();
        worktree
    }

    fn create_test_handoff(stage_id: &str, work_dir: &Path) -> PathBuf {
        let handoff_content = format!(
            r#"# Handoff: Test Handoff

## Metadata

- **Date**: 2026-01-06
- **From**: runner-1 (developer)
- **To**: runner-2 (developer)
- **Track**: {stage_id}
- **Stage**: {stage_id}
- **Context**: 75%

## Goals

Test the continuation feature.

## Completed Work

- Created test stage

## Next Steps

1. Continue work on the stage
2. Verify continuation works
"#
        );

        // Use the standard handoff naming pattern: {stage_id}-handoff-{NNN}.md
        let handoff_path = work_dir
            .join("handoffs")
            .join(format!("{stage_id}-handoff-001.md"));
        fs::write(&handoff_path, handoff_content).unwrap();
        handoff_path
    }

    #[test]
    fn test_continuation_config_default() {
        let config = ContinuationConfig::default();
        assert_eq!(config.backend_type, BackendType::Native);
        assert!(config.auto_spawn);
    }

    #[test]
    fn test_prepare_continuation_with_handoff() {
        let (_temp, work_dir) = create_test_work_dir();
        let project_root = work_dir.parent().unwrap();
        let stage_id = "stage-test-1";

        create_test_stage(stage_id, &work_dir);
        create_test_worktree(stage_id, project_root);
        let handoff_path = create_test_handoff(stage_id, &work_dir);

        let context =
            prepare_continuation(stage_id, &work_dir).expect("Should prepare continuation context");

        assert_eq!(context.stage.id, stage_id);
        assert!(context.handoff_path.is_some());
        assert_eq!(
            context.handoff_path.unwrap().canonicalize().unwrap(),
            handoff_path.canonicalize().unwrap()
        );
        assert!(context.worktree_path.exists());
        assert_eq!(context.branch, format!("loom/{stage_id}"));
    }

    #[test]
    fn test_prepare_continuation_without_handoff() {
        let (_temp, work_dir) = create_test_work_dir();
        let project_root = work_dir.parent().unwrap();
        let stage_id = "stage-test-2";

        create_test_stage(stage_id, &work_dir);
        create_test_worktree(stage_id, project_root);

        let context =
            prepare_continuation(stage_id, &work_dir).expect("Should prepare continuation context");

        assert_eq!(context.stage.id, stage_id);
        assert!(context.handoff_path.is_none());
        assert!(context.worktree_path.exists());
    }

    #[test]
    fn test_prepare_continuation_stage_not_found() {
        let (_temp, work_dir) = create_test_work_dir();

        let result = prepare_continuation("nonexistent-stage", &work_dir);
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("Stage file not found"));
    }

    #[test]
    fn test_load_handoff_content() {
        let (_temp, work_dir) = create_test_work_dir();
        let stage_id = "stage-test-3";
        let handoff_path = create_test_handoff(stage_id, &work_dir);

        let content = load_handoff_content(&handoff_path).expect("Should load handoff content");

        assert!(content.contains("# Handoff: Test Handoff"));
        assert!(content.contains(&format!("**Track**: {stage_id}")));
        assert!(content.contains("## Next Steps"));
    }

    #[test]
    fn test_load_handoff_content_not_found() {
        let (_temp, work_dir) = create_test_work_dir();
        let fake_path = work_dir.join("handoffs").join("nonexistent.md");

        let result = load_handoff_content(&fake_path);
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("Handoff file does not exist"));
    }

    #[test]
    fn test_continue_session_with_handoff() {
        let (_temp, work_dir) = create_test_work_dir();
        let project_root = work_dir.parent().unwrap();
        let stage_id = "stage-test-4";

        let stage = create_test_stage(stage_id, &work_dir);
        let worktree = create_test_worktree(stage_id, project_root);
        let handoff_path = create_test_handoff(stage_id, &work_dir);

        let config = ContinuationConfig {
            backend_type: BackendType::Native,
            auto_spawn: false,
        };

        let session = continue_session(&stage, Some(&handoff_path), &worktree, &config, &work_dir)
            .expect("Should create continuation session");

        assert!(session.stage_id.is_some());
        assert_eq!(session.stage_id.unwrap(), stage_id);
        assert!(session.worktree_path.is_some());

        // Verify signal was created
        let signal_path = work_dir.join("signals").join(format!("{}.md", session.id));
        assert!(signal_path.exists());

        let signal_content = fs::read_to_string(signal_path).unwrap();
        assert!(signal_content.contains(&format!("# Signal: {}", session.id)));
        assert!(signal_content.contains(&format!("**Stage**: {stage_id}")));
    }

    #[test]
    fn test_continue_session_without_handoff() {
        let (_temp, work_dir) = create_test_work_dir();
        let project_root = work_dir.parent().unwrap();
        let stage_id = "stage-test-5";

        let stage = create_test_stage(stage_id, &work_dir);
        let worktree = create_test_worktree(stage_id, project_root);

        let config = ContinuationConfig {
            backend_type: BackendType::Native,
            auto_spawn: false,
        };

        let session = continue_session(&stage, None, &worktree, &config, &work_dir)
            .expect("Should create continuation session without handoff");

        assert!(session.stage_id.is_some());
        assert_eq!(session.stage_id.unwrap(), stage_id);
    }

    #[test]
    fn test_continue_session_invalid_status() {
        let (_temp, work_dir) = create_test_work_dir();
        let project_root = work_dir.parent().unwrap();
        let stage_id = "stage-test-6";

        let mut stage = create_test_stage(stage_id, &work_dir);
        stage.status = StageStatus::Completed;

        let worktree = create_test_worktree(stage_id, project_root);

        let config = ContinuationConfig {
            backend_type: BackendType::Native,
            auto_spawn: false,
        };

        let result = continue_session(&stage, None, &worktree, &config, &work_dir);
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("cannot be continued"));
    }

    #[test]
    fn test_extract_yaml_frontmatter() {
        let content = r#"---
id: test-id
name: Test Name
---

# Content here
"#;

        let yaml = extract_yaml_frontmatter(content).expect("Should extract YAML");
        let id = yaml["id"].as_str().unwrap();
        assert_eq!(id, "test-id");
    }

    #[test]
    fn test_extract_yaml_frontmatter_missing() {
        let content = "# No frontmatter here";
        let result = extract_yaml_frontmatter(content);
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("Missing YAML frontmatter"));
    }

    #[test]
    fn test_session_to_markdown() {
        let mut session = Session::new();
        session.id = "test-session-123".to_string();
        session.assign_to_stage("stage-1".to_string());
        session.set_tmux_session("loom-stage-1".to_string());

        let markdown = session_to_markdown(&session);

        assert!(markdown.contains("---"));
        assert!(markdown.contains("# Session: test-session-123"));
        assert!(markdown.contains("## Details"));
        assert!(markdown.contains("**Stage**: stage-1"));
        assert!(markdown.contains("**Tmux**: loom-stage-1"));
    }
}
