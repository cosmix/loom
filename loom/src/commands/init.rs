use crate::fs::permissions::ensure_loom_permissions;
use crate::fs::stage_files::{compute_stage_depths, stage_file_path, StageDependencies};
use crate::fs::work_dir::WorkDir;
use crate::models::stage::{Stage, StageStatus};
use crate::plan::parser::parse_plan;
use anyhow::{Context, Result};
use chrono::Utc;
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

    // Always prune stale git worktrees (non-destructive)
    prune_stale_worktrees(&repo_root)?;

    // Always clean up orphaned loom tmux sessions (non-destructive)
    cleanup_orphaned_tmux_sessions()?;

    // If --clean flag is provided, remove existing .work/ and .worktrees/ directories
    if clean {
        cleanup_work_directory(&repo_root)?;
        cleanup_worktrees_directory(&repo_root)?;
    }

    let work_dir = WorkDir::new(".")?;
    work_dir.initialize()?;

    // Ensure Claude Code permissions are configured for loom directories
    ensure_loom_permissions(&repo_root)?;

    if let Some(path) = plan_path {
        initialize_with_plan(&work_dir, &path)?;
        println!(
            "Initialized .work/ directory structure with plan from {}",
            path.display()
        );
    } else {
        println!("Initialized .work/ directory structure");
    }

    Ok(())
}

/// Initialize with a plan file
fn initialize_with_plan(work_dir: &WorkDir, plan_path: &Path) -> Result<()> {
    // Validate plan file exists
    if !plan_path.exists() {
        anyhow::bail!("Plan file does not exist: {}", plan_path.display());
    }

    // Parse the plan file to extract stages
    let parsed_plan = parse_plan(plan_path)
        .with_context(|| format!("Failed to parse plan file: {}", plan_path.display()))?;

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

    // Compute topological depths for all stages
    let stage_deps: Vec<StageDependencies> = parsed_plan
        .stages
        .iter()
        .map(|s| StageDependencies {
            id: s.id.clone(),
            dependencies: s.dependencies.clone(),
        })
        .collect();

    let depths = compute_stage_depths(&stage_deps)
        .context("Failed to compute stage depths")?;

    // Create stage files
    let stages_dir = work_dir.root().join("stages");
    if !stages_dir.exists() {
        fs::create_dir_all(&stages_dir)
            .context("Failed to create stages directory")?;
    }

    for stage_def in &parsed_plan.stages {
        let stage = create_stage_from_definition(stage_def, &parsed_plan.id);
        let depth = depths.get(&stage.id).copied().unwrap_or(0);
        let stage_path = stage_file_path(&stages_dir, depth, &stage.id);

        let content = serialize_stage_to_markdown(&stage)
            .with_context(|| format!("Failed to serialize stage: {}", stage.id))?;

        fs::write(&stage_path, content)
            .with_context(|| format!("Failed to write stage file: {}", stage_path.display()))?;

        println!("  Created stage: {} ({})", stage.name, stage_path.display());
    }

    println!("  Total stages created: {}", parsed_plan.stages.len());

    Ok(())
}

/// Create a Stage from a StageDefinition
fn create_stage_from_definition(
    stage_def: &crate::plan::schema::StageDefinition,
    plan_id: &str,
) -> Stage {
    let now = Utc::now();

    // Determine initial status: Ready if no dependencies, Pending otherwise
    let status = if stage_def.dependencies.is_empty() {
        StageStatus::Ready
    } else {
        StageStatus::Pending
    };

    Stage {
        id: stage_def.id.clone(),
        name: stage_def.name.clone(),
        description: stage_def.description.clone(),
        status,
        dependencies: stage_def.dependencies.clone(),
        parallel_group: stage_def.parallel_group.clone(),
        acceptance: stage_def.acceptance.clone(),
        files: stage_def.files.clone(),
        plan_id: Some(plan_id.to_string()),
        worktree: None,
        session: None,
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
    println!("Pruning stale git worktrees...");

    let output = Command::new("git")
        .args(["worktree", "prune"])
        .current_dir(repo_root)
        .output();

    match output {
        Ok(result) if result.status.success() => {
            println!("  Stale worktrees pruned");
        }
        Ok(result) => {
            let stderr = String::from_utf8_lossy(&result.stderr);
            eprintln!("  Warning: Failed to prune worktrees: {}", stderr.trim());
        }
        Err(e) => {
            eprintln!("  Warning: Failed to prune worktrees: {e}");
        }
    }

    Ok(())
}

/// Kill any orphaned loom-* tmux sessions from previous runs
fn cleanup_orphaned_tmux_sessions() -> Result<()> {
    println!("Cleaning up orphaned loom tmux sessions...");

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
            println!("  No tmux sessions to clean up");
            return Ok(());
        }
        Err(_) => {
            // tmux might not be installed, which is fine
            println!("  No tmux sessions to clean up");
            return Ok(());
        }
    };

    if sessions.is_empty() {
        println!("  No orphaned sessions found");
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
                eprintln!(
                    "  Warning: Failed to kill session '{}': {}",
                    session_name,
                    stderr.trim()
                );
            }
            Err(e) => {
                eprintln!("  Warning: Failed to kill session '{session_name}': {e}");
            }
        }
    }

    if killed_count > 0 {
        println!("  Killed {killed_count} orphaned tmux session(s)");
    }

    Ok(())
}

/// Remove the existing .work/ directory
fn cleanup_work_directory(repo_root: &Path) -> Result<()> {
    let work_dir = repo_root.join(".work");

    if !work_dir.exists() {
        return Ok(());
    }

    println!("Removing old .work/ directory...");
    fs::remove_dir_all(&work_dir).with_context(|| {
        format!(
            "Failed to remove .work/ directory at {}",
            work_dir.display()
        )
    })?;
    println!("  Old .work/ directory removed");

    Ok(())
}

/// Remove existing loom worktrees and the .worktrees/ directory
fn cleanup_worktrees_directory(repo_root: &Path) -> Result<()> {
    let worktrees_dir = repo_root.join(".worktrees");

    if !worktrees_dir.exists() {
        return Ok(());
    }

    println!("Removing old .worktrees/ directory...");

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

    println!("  Old .worktrees/ directory removed");

    Ok(())
}
