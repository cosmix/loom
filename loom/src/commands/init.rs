use crate::fs::permissions::{add_worktrees_to_global_trust, ensure_loom_permissions};
use crate::fs::stage_files::{compute_stage_depths, stage_file_path, StageDependencies};
use crate::fs::work_dir::WorkDir;
use crate::models::stage::{Stage, StageStatus};
use crate::plan::parser::parse_plan;
use anyhow::{Context, Result};
use chrono::Utc;
use colored::Colorize;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

/// Initialize the .work/ directory structure
///
/// # Arguments
/// * `plan_path` - Optional path to a plan file to initialize with
/// * `clean` - If true, clean up stale resources before initialization
pub fn execute(plan_path: Option<PathBuf>, clean: bool) -> Result<()> {
    let repo_root = std::env::current_dir()?;

    // Print header
    print_header();

    // Cleanup section
    println!("\n{}", "Cleanup".bold());
    println!("{}", "─".repeat(40).dimmed());

    // Always prune stale git worktrees (non-destructive)
    prune_stale_worktrees(&repo_root)?;

    // Always clean up orphaned loom tmux sessions (non-destructive)
    cleanup_orphaned_tmux_sessions()?;

    // If --clean flag is provided, remove existing .work/ and .worktrees/ directories
    if clean {
        cleanup_work_directory(&repo_root)?;
        cleanup_worktrees_directory(&repo_root)?;
    }

    // Initialize section
    println!("\n{}", "Initialize".bold());
    println!("{}", "─".repeat(40).dimmed());

    let work_dir = WorkDir::new(".")?;
    work_dir.initialize()?;
    println!(
        "  {} Directory structure created {}",
        "✓".green().bold(),
        ".work/".dimmed()
    );

    // Ensure Claude Code permissions are configured for loom directories
    ensure_loom_permissions(&repo_root)?;
    println!(
        "  {} Permissions configured",
        "✓".green().bold()
    );

    // Add .worktrees/ to Claude Code's global trusted directories
    // This prevents the "trust this folder?" prompt when spawning worktree sessions
    add_worktrees_to_global_trust(&repo_root)?;
    println!(
        "  {} Worktrees directory trusted",
        "✓".green().bold()
    );

    if let Some(path) = plan_path {
        let stage_count = initialize_with_plan(&work_dir, &path)?;
        print_summary(Some(&path), stage_count);
    } else {
        print_summary(None, 0);
    }

    Ok(())
}

/// Print the loom init header
fn print_header() {
    println!();
    println!(
        "{}",
        "╭──────────────────────────────────────╮".cyan()
    );
    println!(
        "{}",
        "│       Initializing Loom...           │".cyan().bold()
    );
    println!(
        "{}",
        "╰──────────────────────────────────────╯".cyan()
    );
}

/// Print the final summary
fn print_summary(plan_path: Option<&Path>, stage_count: usize) {
    println!();
    println!("{}", "═".repeat(40).dimmed());

    if let Some(path) = plan_path {
        println!(
            "{} Initialized from {}",
            "✓".green().bold(),
            path.display().to_string().cyan()
        );
        println!(
            "  {} stage{} ready for execution",
            stage_count.to_string().bold(),
            if stage_count == 1 { "" } else { "s" }
        );
    } else {
        println!(
            "{} Empty workspace initialized",
            "✓".green().bold()
        );
    }

    println!();
    println!("{}", "Next steps:".bold());
    println!(
        "  {}  Start execution",
        "loom run".cyan()
    );
    println!(
        "  {}  View dashboard",
        "loom status".cyan()
    );
    println!();
}

/// Initialize with a plan file
/// Returns the number of stages created
fn initialize_with_plan(work_dir: &WorkDir, plan_path: &Path) -> Result<usize> {
    // Validate plan file exists
    if !plan_path.exists() {
        anyhow::bail!("Plan file does not exist: {}", plan_path.display());
    }

    // Parse the plan file to extract stages
    let parsed_plan = parse_plan(plan_path)
        .with_context(|| format!("Failed to parse plan file: {}", plan_path.display()))?;

    println!(
        "  {} Plan parsed: {}",
        "✓".green().bold(),
        parsed_plan.name.bold()
    );

    // Create config.toml to track the active plan
    let config_content = format!(
        "# loom Configuration\n# Generated from plan: {}\n\n[plan]\nsource_path = \"{}\"\nplan_id = \"{}\"\nplan_name = \"{}\"\n",
        plan_path.display(),
        plan_path.display(),
        parsed_plan.id,
        parsed_plan.name
    );

    let config_path = work_dir.root().join("config.toml");
    fs::write(&config_path, config_content).context("Failed to write config.toml")?;
    println!(
        "  {} Config saved {}",
        "✓".green().bold(),
        "config.toml".dimmed()
    );

    // Compute topological depths for all stages
    let stage_deps: Vec<StageDependencies> = parsed_plan
        .stages
        .iter()
        .map(|s| StageDependencies {
            id: s.id.clone(),
            dependencies: s.dependencies.clone(),
        })
        .collect();

    let depths = compute_stage_depths(&stage_deps).context("Failed to compute stage depths")?;

    // Create stage files
    let stages_dir = work_dir.root().join("stages");
    if !stages_dir.exists() {
        fs::create_dir_all(&stages_dir).context("Failed to create stages directory")?;
    }

    // Stages section
    let stage_count = parsed_plan.stages.len();
    println!(
        "\n{} {}",
        "Stages".bold(),
        format!("({stage_count})").dimmed()
    );
    println!("{}", "─".repeat(40).dimmed());

    // Find the longest stage ID for alignment
    let max_id_len = parsed_plan
        .stages
        .iter()
        .map(|s| s.id.len())
        .max()
        .unwrap_or(0);

    for stage_def in &parsed_plan.stages {
        let stage = create_stage_from_definition(stage_def, &parsed_plan.id);
        let depth = depths.get(&stage.id).copied().unwrap_or(0);
        let stage_path = stage_file_path(&stages_dir, depth, &stage.id);

        let content = serialize_stage_to_markdown(&stage)
            .with_context(|| format!("Failed to serialize stage: {}", stage.id))?;

        fs::write(&stage_path, content)
            .with_context(|| format!("Failed to write stage file: {}", stage_path.display()))?;

        // Status indicator based on dependencies
        let status_indicator = if stage_def.dependencies.is_empty() {
            "●".green() // Ready to run
        } else {
            "○".yellow() // Waiting for deps
        };

        println!(
            "  {}  {:width$}  {}",
            status_indicator,
            stage.id.dimmed(),
            stage.name,
            width = max_id_len
        );
    }

    Ok(stage_count)
}

/// Create a Stage from a StageDefinition
fn create_stage_from_definition(
    stage_def: &crate::plan::schema::StageDefinition,
    plan_id: &str,
) -> Stage {
    let now = Utc::now();

    // Determine initial status: Ready if no dependencies, Pending otherwise
    let status = if stage_def.dependencies.is_empty() {
        StageStatus::Queued
    } else {
        StageStatus::WaitingForDeps
    };

    Stage {
        id: stage_def.id.clone(),
        name: stage_def.name.clone(),
        description: stage_def.description.clone(),
        status,
        dependencies: stage_def.dependencies.clone(),
        parallel_group: stage_def.parallel_group.clone(),
        acceptance: stage_def.acceptance.clone(),
        setup: stage_def.setup.clone(),
        files: stage_def.files.clone(),
        plan_id: Some(plan_id.to_string()),
        worktree: None,
        session: None,
        held: false,
        parent_stage: None,
        child_stages: Vec::new(),
        created_at: now,
        updated_at: now,
        completed_at: None,
        close_reason: None,
    }
}

/// Serialize a Stage to markdown with YAML frontmatter
fn serialize_stage_to_markdown(stage: &Stage) -> Result<String> {
    let yaml = serde_yaml::to_string(stage).context("Failed to serialize Stage to YAML")?;

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

    Ok(content)
}

/// Prune stale git worktrees that have been deleted but are still registered
fn prune_stale_worktrees(repo_root: &Path) -> Result<()> {
    let output = Command::new("git")
        .args(["worktree", "prune"])
        .current_dir(repo_root)
        .output();

    match output {
        Ok(result) if result.status.success() => {
            println!(
                "  {} Stale worktrees pruned",
                "✓".green().bold()
            );
        }
        Ok(result) => {
            let stderr = String::from_utf8_lossy(&result.stderr);
            println!(
                "  {} Worktree prune: {}",
                "⚠".yellow().bold(),
                stderr.trim().dimmed()
            );
        }
        Err(e) => {
            println!(
                "  {} Worktree prune: {}",
                "⚠".yellow().bold(),
                e.to_string().dimmed()
            );
        }
    }

    Ok(())
}

/// Kill any orphaned loom sessions from previous runs
fn cleanup_orphaned_tmux_sessions() -> Result<()> {
    // List all tmux sessions with loom- prefix
    let output = Command::new("tmux")
        .args(["list-sessions", "-F", "#{session_name}"])
        .output();

    let sessions: Vec<String> = match output {
        Ok(result) if result.status.success() => {
            let stdout = String::from_utf8_lossy(&result.stdout);
            stdout
                .lines()
                .filter(|line| line.starts_with("loom-"))
                .map(|s| s.to_string())
                .collect()
        }
        Ok(_) => {
            // tmux returns non-zero when no sessions exist
            println!(
                "  {} No orphaned sessions",
                "✓".green().bold()
            );
            return Ok(());
        }
        Err(_) => {
            // tmux might not be installed, which is fine
            println!(
                "  {} Sessions check skipped {}",
                "─".dimmed(),
                "(tmux not available)".dimmed()
            );
            return Ok(());
        }
    };

    if sessions.is_empty() {
        println!(
            "  {} No orphaned sessions",
            "✓".green().bold()
        );
        return Ok(());
    }

    let mut killed_count = 0;
    for session_name in &sessions {
        match Command::new("tmux")
            .args(["kill-session", "-t", session_name])
            .output()
        {
            Ok(result) if result.status.success() => {
                killed_count += 1;
            }
            Ok(result) => {
                let stderr = String::from_utf8_lossy(&result.stderr);
                println!(
                    "  {} Failed to kill '{}': {}",
                    "⚠".yellow().bold(),
                    session_name,
                    stderr.trim().dimmed()
                );
            }
            Err(e) => {
                println!(
                    "  {} Failed to kill '{}': {}",
                    "⚠".yellow().bold(),
                    session_name,
                    e.to_string().dimmed()
                );
            }
        }
    }

    if killed_count > 0 {
        println!(
            "  {} Cleaned {} orphaned session{}",
            "✓".green().bold(),
            killed_count.to_string().bold(),
            if killed_count == 1 { "" } else { "s" }
        );
    }

    Ok(())
}

/// Remove the existing .work/ directory
fn cleanup_work_directory(repo_root: &Path) -> Result<()> {
    let work_dir = repo_root.join(".work");

    if !work_dir.exists() {
        return Ok(());
    }

    fs::remove_dir_all(&work_dir).with_context(|| {
        format!(
            "Failed to remove .work/ directory at {}",
            work_dir.display()
        )
    })?;
    println!(
        "  {} Removed old {}",
        "✓".green().bold(),
        ".work/".dimmed()
    );

    Ok(())
}

/// Remove existing loom worktrees and the .worktrees/ directory
fn cleanup_worktrees_directory(repo_root: &Path) -> Result<()> {
    let worktrees_dir = repo_root.join(".worktrees");

    if !worktrees_dir.exists() {
        return Ok(());
    }

    // First, try to remove each worktree properly via git
    if let Ok(entries) = fs::read_dir(&worktrees_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                let stage_id = entry.file_name().to_string_lossy().to_string();

                // Try git worktree remove first
                let _ = Command::new("git")
                    .args(["worktree", "remove", "--force"])
                    .arg(&path)
                    .current_dir(repo_root)
                    .output();

                // Also try to delete the loom/ branch if it exists
                let branch_name = format!("loom/{stage_id}");
                let _ = Command::new("git")
                    .args(["branch", "-D", &branch_name])
                    .current_dir(repo_root)
                    .output();
            }
        }
    }

    // Final prune to clean up any remaining stale entries
    let _ = Command::new("git")
        .args(["worktree", "prune"])
        .current_dir(repo_root)
        .output();

    // Remove the directory itself if it still exists
    if worktrees_dir.exists() {
        fs::remove_dir_all(&worktrees_dir).with_context(|| {
            format!(
                "Failed to remove .worktrees/ directory at {}",
                worktrees_dir.display()
            )
        })?;
    }

    println!(
        "  {} Removed old {}",
        "✓".green().bold(),
        ".worktrees/".dimmed()
    );

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::plan::schema::{LoomConfig, LoomMetadata, StageDefinition};
    use std::fs;
    use tempfile::TempDir;

    /// Helper to create a minimal valid plan file
    fn create_test_plan(dir: &Path, stages: Vec<StageDefinition>) -> PathBuf {
        let metadata = LoomMetadata {
            loom: LoomConfig { version: 1, stages },
        };

        let yaml = serde_yaml::to_string(&metadata).unwrap();
        let plan_content = format!(
            "# Test Plan\n\n## Overview\n\nTest plan for unit tests\n\n<!-- loom METADATA -->\n```yaml\n{yaml}```\n<!-- END loom METADATA -->\n"
        );

        let plan_path = dir.join("test-plan.md");
        fs::write(&plan_path, plan_content).unwrap();
        plan_path
    }

    #[test]
    fn test_create_stage_from_definition_no_dependencies() {
        let stage_def = StageDefinition {
            id: "stage-1".to_string(),
            name: "Stage 1".to_string(),
            description: Some("Test stage".to_string()),
            dependencies: vec![],
            parallel_group: None,
            acceptance: vec!["cargo test".to_string()],
            setup: vec![],
            files: vec!["src/*.rs".to_string()],
        };

        let stage = create_stage_from_definition(&stage_def, "plan-001");

        assert_eq!(stage.id, "stage-1");
        assert_eq!(stage.name, "Stage 1");
        assert_eq!(stage.status, StageStatus::Queued); // No deps = Ready
        assert_eq!(stage.plan_id, Some("plan-001".to_string()));
        assert_eq!(stage.dependencies.len(), 0);
        assert_eq!(stage.acceptance.len(), 1);
    }

    #[test]
    fn test_create_stage_from_definition_with_dependencies() {
        let stage_def = StageDefinition {
            id: "stage-2".to_string(),
            name: "Stage 2".to_string(),
            description: None,
            dependencies: vec!["stage-1".to_string()],
            parallel_group: Some("core".to_string()),
            acceptance: vec![],
            setup: vec!["cargo build".to_string()],
            files: vec![],
        };

        let stage = create_stage_from_definition(&stage_def, "plan-002");

        assert_eq!(stage.id, "stage-2");
        assert_eq!(stage.status, StageStatus::WaitingForDeps); // Has deps = Pending
        assert_eq!(stage.dependencies, vec!["stage-1".to_string()]);
        assert_eq!(stage.parallel_group, Some("core".to_string()));
    }

    #[test]
    fn test_serialize_stage_to_markdown_minimal() {
        let stage = Stage {
            id: "test-stage".to_string(),
            name: "Test Stage".to_string(),
            description: None,
            status: StageStatus::Queued,
            dependencies: vec![],
            parallel_group: None,
            acceptance: vec![],
            setup: vec![],
            files: vec![],
            plan_id: None,
            worktree: None,
            session: None,
            held: false,
            parent_stage: None,
            child_stages: vec![],
            created_at: Utc::now(),
            updated_at: Utc::now(),
            completed_at: None,
            close_reason: None,
        };

        let content = serialize_stage_to_markdown(&stage).unwrap();

        assert!(content.starts_with("---\n"));
        assert!(content.contains("# Stage: Test Stage"));
        assert!(content.contains("**Status**: Queued"));
    }

    #[test]
    fn test_serialize_stage_to_markdown_with_all_fields() {
        let stage = Stage {
            id: "full-stage".to_string(),
            name: "Full Stage".to_string(),
            description: Some("Detailed description".to_string()),
            status: StageStatus::Executing,
            dependencies: vec!["dep1".to_string(), "dep2".to_string()],
            parallel_group: Some("group1".to_string()),
            acceptance: vec!["test1".to_string(), "test2".to_string()],
            setup: vec![],
            files: vec!["file1.rs".to_string(), "file2.rs".to_string()],
            plan_id: Some("plan-123".to_string()),
            worktree: None,
            session: None,
            held: false,
            parent_stage: None,
            child_stages: vec![],
            created_at: Utc::now(),
            updated_at: Utc::now(),
            completed_at: None,
            close_reason: None,
        };

        let content = serialize_stage_to_markdown(&stage).unwrap();

        assert!(content.contains("## Dependencies"));
        assert!(content.contains("- dep1"));
        assert!(content.contains("- dep2"));
        assert!(content.contains("## Acceptance Criteria"));
        assert!(content.contains("- [ ] test1"));
        assert!(content.contains("- [ ] test2"));
        assert!(content.contains("## Files"));
        assert!(content.contains("- `file1.rs`"));
        assert!(content.contains("- `file2.rs`"));
    }

    #[test]
    fn test_initialize_with_plan_nonexistent_file() {
        let temp_dir = TempDir::new().unwrap();
        let work_dir = WorkDir::new(temp_dir.path()).unwrap();
        work_dir.initialize().unwrap();

        let nonexistent_path = temp_dir.path().join("nonexistent.md");

        let result = initialize_with_plan(&work_dir, &nonexistent_path);

        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("does not exist"));
    }

    #[test]
    fn test_initialize_with_plan_creates_config() {
        let temp_dir = TempDir::new().unwrap();
        let work_dir = WorkDir::new(temp_dir.path()).unwrap();
        work_dir.initialize().unwrap();

        let stage_def = StageDefinition {
            id: "test-stage".to_string(),
            name: "Test Stage".to_string(),
            description: None,
            dependencies: vec![],
            parallel_group: None,
            acceptance: vec![],
            setup: vec![],
            files: vec![],
        };

        let plan_path = create_test_plan(temp_dir.path(), vec![stage_def]);

        let result = initialize_with_plan(&work_dir, &plan_path);

        assert!(result.is_ok());

        let config_path = work_dir.root().join("config.toml");
        assert!(config_path.exists());

        let config_content = fs::read_to_string(config_path).unwrap();
        assert!(config_content.contains("source_path"));
        assert!(config_content.contains("plan_id"));
    }

    #[test]
    fn test_initialize_with_plan_creates_stage_files() {
        let temp_dir = TempDir::new().unwrap();
        let work_dir = WorkDir::new(temp_dir.path()).unwrap();
        work_dir.initialize().unwrap();

        let stages = vec![
            StageDefinition {
                id: "stage-1".to_string(),
                name: "Stage One".to_string(),
                description: Some("First stage".to_string()),
                dependencies: vec![],
                parallel_group: None,
                acceptance: vec!["cargo test".to_string()],
                setup: vec![],
                files: vec![],
            },
            StageDefinition {
                id: "stage-2".to_string(),
                name: "Stage Two".to_string(),
                description: None,
                dependencies: vec!["stage-1".to_string()],
                parallel_group: None,
                acceptance: vec![],
                setup: vec![],
                files: vec![],
            },
        ];

        let plan_path = create_test_plan(temp_dir.path(), stages);

        let result = initialize_with_plan(&work_dir, &plan_path);

        assert!(result.is_ok());

        let stages_dir = work_dir.root().join("stages");
        assert!(stages_dir.exists());

        // Check that stage files were created
        let stage_files: Vec<_> = fs::read_dir(stages_dir)
            .unwrap()
            .filter_map(|e| e.ok())
            .filter(|e| e.path().extension().and_then(|s| s.to_str()) == Some("md"))
            .collect();

        assert_eq!(stage_files.len(), 2);
    }

    #[test]
    fn test_cleanup_work_directory_removes_existing() {
        let temp_dir = TempDir::new().unwrap();
        let work_dir = temp_dir.path().join(".work");

        fs::create_dir_all(&work_dir).unwrap();
        fs::write(work_dir.join("test.txt"), "content").unwrap();

        assert!(work_dir.exists());

        let result = cleanup_work_directory(temp_dir.path());

        assert!(result.is_ok());
        assert!(!work_dir.exists());
    }

    #[test]
    fn test_cleanup_work_directory_nonexistent_ok() {
        let temp_dir = TempDir::new().unwrap();
        let work_dir = temp_dir.path().join(".work");

        assert!(!work_dir.exists());

        let result = cleanup_work_directory(temp_dir.path());

        assert!(result.is_ok());
    }

    #[test]
    fn test_initialize_with_plan_invalid_yaml() {
        let temp_dir = TempDir::new().unwrap();
        let work_dir = WorkDir::new(temp_dir.path()).unwrap();
        work_dir.initialize().unwrap();

        let invalid_plan = temp_dir.path().join("invalid.md");
        fs::write(
            &invalid_plan,
            "# Invalid Plan\n\n<!-- loom METADATA -->\n```yaml\ninvalid: yaml: content:\n```\n",
        )
        .unwrap();

        let result = initialize_with_plan(&work_dir, &invalid_plan);

        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("parse"));
    }

    #[test]
    fn test_prune_stale_worktrees_does_not_fail() {
        let temp_dir = TempDir::new().unwrap();

        // Should not fail even if git command fails
        let result = prune_stale_worktrees(temp_dir.path());

        assert!(result.is_ok());
    }

    #[test]
    fn test_cleanup_orphaned_tmux_sessions_does_not_fail() {
        // Should not fail even if tmux is not available
        let result = cleanup_orphaned_tmux_sessions();

        assert!(result.is_ok());
    }
}
