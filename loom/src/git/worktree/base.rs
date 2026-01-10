//! Base branch resolution for worktree creation
//!
//! Determines the correct base branch for a new worktree based on stage dependencies.
//! Supports single-dependency inheritance, multi-dependency merging, and fallback to main.

use anyhow::{bail, Context, Result};
use std::path::Path;
use std::process::Command;

use crate::git::branch::{branch_exists, branch_name_for_stage, default_branch};
use crate::plan::graph::{ExecutionGraph, NodeStatus};

/// Result of resolving the base branch for a stage
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ResolvedBase {
    /// Use the main/master branch as base
    Main(String),
    /// Use a specific loom branch as base
    Branch(String),
    /// Use a temporary merge branch created from multiple dependencies
    TempMerge(String),
}

impl ResolvedBase {
    /// Get the branch name to use
    pub fn branch_name(&self) -> &str {
        match self {
            ResolvedBase::Main(name) => name,
            ResolvedBase::Branch(name) => name,
            ResolvedBase::TempMerge(name) => name,
        }
    }
}

/// Error information for merge conflicts
#[derive(Debug, Clone)]
pub struct MergeConflictInfo {
    /// The temporary branch that was being created
    pub temp_branch: String,
    /// The branch that caused the conflict
    pub conflicting_branch: String,
    /// List of conflicting files
    pub conflicting_files: Vec<String>,
}

/// Resolve the base branch for a stage based on its dependencies
///
/// # Logic
///
/// - Empty deps → `ResolvedBase::Main` (uses init_base_branch if provided, else default_branch)
/// - Single dep with existing branch → `ResolvedBase::Branch("loom/{dep}")`
/// - Single dep, branch missing + dep Completed → `ResolvedBase::Main` (fallback)
/// - Single dep, branch missing + dep not Completed → Error (scheduling bug)
/// - Multiple deps → Create temp merge branch `loom/_base/{stage_id}`
///
/// # Arguments
///
/// * `stage_id` - The ID of the stage needing a worktree
/// * `dependencies` - The stage's dependency list
/// * `graph` - The execution graph (for checking dependency status)
/// * `repo_root` - Path to the git repository root
/// * `init_base_branch` - Optional base branch from config.toml (for stages with no deps)
///
/// # Returns
///
/// * `Ok(ResolvedBase)` - The resolved base branch to use
/// * `Err` - If there's a scheduling bug or merge conflict
pub fn resolve_base_branch(
    stage_id: &str,
    dependencies: &[String],
    graph: &ExecutionGraph,
    repo_root: &Path,
    init_base_branch: Option<&str>,
) -> Result<ResolvedBase> {
    // Empty deps → use init_base_branch if provided, otherwise fall back to default
    if dependencies.is_empty() {
        let base = init_base_branch
            .map(String::from)
            .unwrap_or_else(|| default_branch(repo_root).unwrap_or_else(|_| "main".to_string()));
        return Ok(ResolvedBase::Main(base));
    }

    // Single dependency case
    if dependencies.len() == 1 {
        let dep = &dependencies[0];
        let dep_branch = branch_name_for_stage(dep);

        if branch_exists(&dep_branch, repo_root)? {
            return Ok(ResolvedBase::Branch(dep_branch));
        }

        // Branch doesn't exist - check dependency status
        let dep_node = graph
            .get_node(dep)
            .ok_or_else(|| anyhow::anyhow!("Dependency '{dep}' not found in graph"))?;

        if dep_node.status == NodeStatus::Completed {
            // Dependency completed but branch missing (likely already merged)
            // Fall back to main
            let main = default_branch(repo_root)?;
            return Ok(ResolvedBase::Main(main));
        }

        // Dependency not completed but branch missing - scheduling bug
        bail!(
            "Scheduling error: dependency '{}' is {:?} but branch '{}' does not exist. \
             This indicates a bug in the orchestrator - stages should only be scheduled \
             after their dependencies complete.",
            dep,
            dep_node.status,
            dep_branch
        );
    }

    // Multiple dependencies - need to create a temporary merge branch
    create_temp_merge_branch(stage_id, dependencies, graph, repo_root)
}

/// Create a temporary merge branch from multiple dependency branches
///
/// Steps:
/// 1. Start from main
/// 2. Create temp branch `loom/_base/{stage_id}`
/// 3. Merge each dep branch with `--no-ff`
/// 4. On conflict: atomic rollback, return error with conflict info
fn create_temp_merge_branch(
    stage_id: &str,
    dependencies: &[String],
    graph: &ExecutionGraph,
    repo_root: &Path,
) -> Result<ResolvedBase> {
    let temp_branch = format!("loom/_base/{stage_id}");
    let main = default_branch(repo_root)?;

    // Clean up any existing temp branch
    cleanup_temp_branch(&temp_branch, repo_root);

    // Get current branch to restore on error
    let original_branch = get_current_branch(repo_root)?;

    // Collect valid dependency branches (skipping completed deps without branches)
    let mut dep_branches = Vec::new();
    for dep in dependencies {
        let dep_branch = branch_name_for_stage(dep);

        if branch_exists(&dep_branch, repo_root)? {
            dep_branches.push((dep.clone(), dep_branch));
        } else {
            // Check if dependency is completed (branch was merged away)
            let dep_node = graph
                .get_node(dep)
                .ok_or_else(|| anyhow::anyhow!("Dependency '{dep}' not found in graph"))?;

            if dep_node.status != NodeStatus::Completed {
                // Restore original branch before failing
                checkout(&original_branch, repo_root).ok();
                bail!(
                    "Scheduling error: dependency '{}' is {:?} but branch '{}' does not exist",
                    dep,
                    dep_node.status,
                    dep_branch
                );
            }
            // Completed dependency without branch - skip it (changes are in main)
        }
    }

    // If no branches to merge, just use main
    if dep_branches.is_empty() {
        return Ok(ResolvedBase::Main(main));
    }

    // If only one branch after filtering, use it directly
    if dep_branches.len() == 1 {
        return Ok(ResolvedBase::Branch(dep_branches[0].1.clone()));
    }

    // Create temp branch from main
    checkout(&main, repo_root)?;

    let output = Command::new("git")
        .args(["checkout", "-b", &temp_branch])
        .current_dir(repo_root)
        .output()
        .with_context(|| format!("Failed to create temp branch {temp_branch}"))?;

    if !output.status.success() {
        checkout(&original_branch, repo_root).ok();
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!("Failed to create temp branch '{temp_branch}': {stderr}");
    }

    // Merge each dependency branch
    for (dep_id, dep_branch) in &dep_branches {
        let merge_result = merge_branch(dep_branch, repo_root);

        if let Err(e) = merge_result {
            // Get conflict info before cleanup
            let conflicts = get_conflicting_files(repo_root).unwrap_or_default();

            // Atomic rollback
            abort_merge(repo_root).ok();
            checkout(&original_branch, repo_root).ok();
            delete_branch(&temp_branch, repo_root).ok();

            if !conflicts.is_empty() {
                bail!(
                    "Merge conflict creating base branch for '{stage_id}': \
                     cannot merge '{dep_branch}' (from dep '{dep_id}'). Conflicting files: {conflicts:?}. \
                     Consider reordering dependencies or resolving conflicts manually."
                );
            }

            bail!(
                "Failed to merge dependency '{dep_id}' branch '{dep_branch}' into temp base: {e}"
            );
        }
    }

    // Restore original branch (temp branch stays for worktree creation)
    checkout(&original_branch, repo_root)?;

    Ok(ResolvedBase::TempMerge(temp_branch))
}

/// Clean up a temporary base branch if it exists
pub fn cleanup_temp_branch(branch_name: &str, repo_root: &Path) {
    if branch_name.starts_with("loom/_base/") {
        delete_branch(branch_name, repo_root).ok();
    }
}

/// Clean up all temporary base branches
pub fn cleanup_all_temp_branches(repo_root: &Path) -> Result<Vec<String>> {
    let output = Command::new("git")
        .args(["branch", "--list", "loom/_base/*"])
        .current_dir(repo_root)
        .output()
        .with_context(|| "Failed to list temp base branches")?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut cleaned = Vec::new();

    for line in stdout.lines() {
        let branch = line.trim().trim_start_matches('*').trim();
        if !branch.is_empty() && delete_branch(branch, repo_root).is_ok() {
            cleaned.push(branch.to_string());
        }
    }

    Ok(cleaned)
}

// Helper functions

fn get_current_branch(repo_root: &Path) -> Result<String> {
    let output = Command::new("git")
        .args(["rev-parse", "--abbrev-ref", "HEAD"])
        .current_dir(repo_root)
        .output()
        .with_context(|| "Failed to get current branch")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!("git rev-parse failed: {stderr}");
    }

    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

fn checkout(branch: &str, repo_root: &Path) -> Result<()> {
    let output = Command::new("git")
        .args(["checkout", branch])
        .current_dir(repo_root)
        .output()
        .with_context(|| format!("Failed to checkout branch {branch}"))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!("git checkout failed: {stderr}");
    }

    Ok(())
}

fn merge_branch(branch: &str, repo_root: &Path) -> Result<()> {
    let output = Command::new("git")
        .args(["merge", "--no-ff", "-m"])
        .arg(format!("Merge {branch} into temp base"))
        .arg(branch)
        .current_dir(repo_root)
        .output()
        .with_context(|| format!("Failed to merge branch {branch}"))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!("git merge failed: {stderr}");
    }

    Ok(())
}

fn abort_merge(repo_root: &Path) -> Result<()> {
    let output = Command::new("git")
        .args(["merge", "--abort"])
        .current_dir(repo_root)
        .output()
        .with_context(|| "Failed to abort merge")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!("git merge --abort failed: {stderr}");
    }

    Ok(())
}

fn delete_branch(branch: &str, repo_root: &Path) -> Result<()> {
    let output = Command::new("git")
        .args(["branch", "-D", branch])
        .current_dir(repo_root)
        .output()
        .with_context(|| format!("Failed to delete branch {branch}"))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!("git branch -D failed: {stderr}");
    }

    Ok(())
}

fn get_conflicting_files(repo_root: &Path) -> Result<Vec<String>> {
    let output = Command::new("git")
        .args(["diff", "--name-only", "--diff-filter=U"])
        .current_dir(repo_root)
        .output()
        .with_context(|| "Failed to get conflicting files")?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    let files: Vec<String> = stdout
        .lines()
        .filter(|s| !s.is_empty())
        .map(String::from)
        .collect();

    Ok(files)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::plan::graph::ExecutionGraph;
    use crate::plan::schema::StageDefinition;
    use std::process::Command;
    use tempfile::TempDir;

    fn init_test_repo() -> TempDir {
        let temp_dir = TempDir::new().unwrap();
        let repo_root = temp_dir.path();

        Command::new("git")
            .args(["init"])
            .current_dir(repo_root)
            .output()
            .unwrap();

        Command::new("git")
            .args(["config", "user.email", "test@test.com"])
            .current_dir(repo_root)
            .output()
            .unwrap();

        Command::new("git")
            .args(["config", "user.name", "Test"])
            .current_dir(repo_root)
            .output()
            .unwrap();

        // Create initial commit on main
        std::fs::write(repo_root.join("README.md"), "# Test").unwrap();
        Command::new("git")
            .args(["add", "."])
            .current_dir(repo_root)
            .output()
            .unwrap();
        Command::new("git")
            .args(["commit", "-m", "Initial commit"])
            .current_dir(repo_root)
            .output()
            .unwrap();

        // Rename to main if needed (some git versions default to master)
        Command::new("git")
            .args(["branch", "-M", "main"])
            .current_dir(repo_root)
            .output()
            .unwrap();

        temp_dir
    }

    fn create_branch_with_commit(name: &str, file: &str, content: &str, repo_root: &Path) {
        Command::new("git")
            .args(["checkout", "-b", name])
            .current_dir(repo_root)
            .output()
            .unwrap();

        std::fs::write(repo_root.join(file), content).unwrap();

        Command::new("git")
            .args(["add", file])
            .current_dir(repo_root)
            .output()
            .unwrap();

        Command::new("git")
            .args(["commit", "-m", &format!("Add {file}")])
            .current_dir(repo_root)
            .output()
            .unwrap();

        Command::new("git")
            .args(["checkout", "main"])
            .current_dir(repo_root)
            .output()
            .unwrap();
    }

    fn build_test_graph(stages: Vec<(&str, Vec<&str>)>) -> ExecutionGraph {
        let stage_defs: Vec<StageDefinition> = stages
            .into_iter()
            .map(|(id, deps)| StageDefinition {
                id: id.to_string(),
                name: id.to_string(),
                dependencies: deps.into_iter().map(String::from).collect(),
                description: Some(format!("Test stage {id}")),
                acceptance: vec![],
                setup: vec![],
                files: vec![],
                parallel_group: None,
                auto_merge: None,
            })
            .collect();

        ExecutionGraph::build(stage_defs).unwrap()
    }

    #[test]
    fn test_empty_deps_returns_main() {
        let temp_dir = init_test_repo();
        let repo_root = temp_dir.path();

        let graph = build_test_graph(vec![("stage-1", vec![])]);

        let result = resolve_base_branch("stage-1", &[], &graph, repo_root, None).unwrap();

        assert_eq!(result, ResolvedBase::Main("main".to_string()));
    }

    #[test]
    fn test_single_dep_with_existing_branch() {
        let temp_dir = init_test_repo();
        let repo_root = temp_dir.path();

        create_branch_with_commit("loom/dep-1", "dep1.txt", "content", repo_root);

        let mut graph = build_test_graph(vec![("dep-1", vec![]), ("stage-1", vec!["dep-1"])]);
        graph.mark_executing("dep-1").unwrap();
        graph.mark_completed("dep-1").unwrap();

        let result =
            resolve_base_branch("stage-1", &["dep-1".to_string()], &graph, repo_root, None)
                .unwrap();

        assert_eq!(result, ResolvedBase::Branch("loom/dep-1".to_string()));
    }

    #[test]
    fn test_single_dep_branch_missing_dep_completed_fallback() {
        let temp_dir = init_test_repo();
        let repo_root = temp_dir.path();

        // No branch created - simulates merged dependency

        let mut graph = build_test_graph(vec![("dep-1", vec![]), ("stage-1", vec!["dep-1"])]);
        graph.mark_executing("dep-1").unwrap();
        graph.mark_completed("dep-1").unwrap();

        let result =
            resolve_base_branch("stage-1", &["dep-1".to_string()], &graph, repo_root, None)
                .unwrap();

        // Should fall back to main
        assert_eq!(result, ResolvedBase::Main("main".to_string()));
    }

    #[test]
    fn test_single_dep_branch_missing_dep_not_completed_error() {
        let temp_dir = init_test_repo();
        let repo_root = temp_dir.path();

        // No branch, dep not completed
        let graph = build_test_graph(vec![("dep-1", vec![]), ("stage-1", vec!["dep-1"])]);

        let result =
            resolve_base_branch("stage-1", &["dep-1".to_string()], &graph, repo_root, None);

        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("Scheduling error"));
        assert!(err.contains("dep-1"));
    }

    #[test]
    fn test_multiple_deps_creates_temp_merge() {
        let temp_dir = init_test_repo();
        let repo_root = temp_dir.path();

        // Create two dependency branches with non-conflicting changes
        create_branch_with_commit("loom/dep-1", "file1.txt", "content1", repo_root);
        create_branch_with_commit("loom/dep-2", "file2.txt", "content2", repo_root);

        let mut graph = build_test_graph(vec![
            ("dep-1", vec![]),
            ("dep-2", vec![]),
            ("stage-1", vec!["dep-1", "dep-2"]),
        ]);
        graph.mark_executing("dep-1").unwrap();
        graph.mark_completed("dep-1").unwrap();
        graph.mark_executing("dep-2").unwrap();
        graph.mark_completed("dep-2").unwrap();

        let result = resolve_base_branch(
            "stage-1",
            &["dep-1".to_string(), "dep-2".to_string()],
            &graph,
            repo_root,
            None,
        )
        .unwrap();

        assert!(matches!(result, ResolvedBase::TempMerge(_)));
        if let ResolvedBase::TempMerge(branch) = result {
            assert_eq!(branch, "loom/_base/stage-1");
            // Verify the branch was created
            assert!(branch_exists(&branch, repo_root).unwrap());
        }
    }

    #[test]
    fn test_multiple_deps_with_conflict_returns_error() {
        let temp_dir = init_test_repo();
        let repo_root = temp_dir.path();

        // Create two dependency branches with conflicting changes to same file
        create_branch_with_commit("loom/dep-1", "shared.txt", "content from dep-1", repo_root);
        create_branch_with_commit("loom/dep-2", "shared.txt", "content from dep-2", repo_root);

        let mut graph = build_test_graph(vec![
            ("dep-1", vec![]),
            ("dep-2", vec![]),
            ("stage-1", vec!["dep-1", "dep-2"]),
        ]);
        graph.mark_executing("dep-1").unwrap();
        graph.mark_completed("dep-1").unwrap();
        graph.mark_executing("dep-2").unwrap();
        graph.mark_completed("dep-2").unwrap();

        let result = resolve_base_branch(
            "stage-1",
            &["dep-1".to_string(), "dep-2".to_string()],
            &graph,
            repo_root,
            None,
        );

        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("Merge conflict") || err.contains("merge failed"));
    }

    #[test]
    fn test_multiple_deps_some_merged_away() {
        let temp_dir = init_test_repo();
        let repo_root = temp_dir.path();

        // Only create one branch - other dep already merged
        create_branch_with_commit("loom/dep-2", "file2.txt", "content2", repo_root);

        let mut graph = build_test_graph(vec![
            ("dep-1", vec![]),
            ("dep-2", vec![]),
            ("stage-1", vec!["dep-1", "dep-2"]),
        ]);
        graph.mark_executing("dep-1").unwrap();
        graph.mark_completed("dep-1").unwrap();
        graph.mark_executing("dep-2").unwrap();
        graph.mark_completed("dep-2").unwrap();

        let result = resolve_base_branch(
            "stage-1",
            &["dep-1".to_string(), "dep-2".to_string()],
            &graph,
            repo_root,
            None,
        )
        .unwrap();

        // Should use the remaining branch directly
        assert_eq!(result, ResolvedBase::Branch("loom/dep-2".to_string()));
    }

    #[test]
    fn test_all_deps_merged_away_returns_main() {
        let temp_dir = init_test_repo();
        let repo_root = temp_dir.path();

        // No branches - all deps already merged
        let mut graph = build_test_graph(vec![
            ("dep-1", vec![]),
            ("dep-2", vec![]),
            ("stage-1", vec!["dep-1", "dep-2"]),
        ]);
        graph.mark_executing("dep-1").unwrap();
        graph.mark_completed("dep-1").unwrap();
        graph.mark_executing("dep-2").unwrap();
        graph.mark_completed("dep-2").unwrap();

        let result = resolve_base_branch(
            "stage-1",
            &["dep-1".to_string(), "dep-2".to_string()],
            &graph,
            repo_root,
            None,
        )
        .unwrap();

        assert_eq!(result, ResolvedBase::Main("main".to_string()));
    }

    #[test]
    fn test_resolved_base_branch_name() {
        let main = ResolvedBase::Main("main".to_string());
        assert_eq!(main.branch_name(), "main");

        let branch = ResolvedBase::Branch("loom/stage-1".to_string());
        assert_eq!(branch.branch_name(), "loom/stage-1");

        let temp = ResolvedBase::TempMerge("loom/_base/stage-2".to_string());
        assert_eq!(temp.branch_name(), "loom/_base/stage-2");
    }

    #[test]
    fn test_cleanup_temp_branch() {
        let temp_dir = init_test_repo();
        let repo_root = temp_dir.path();

        // Create a temp branch
        Command::new("git")
            .args(["branch", "loom/_base/test-stage"])
            .current_dir(repo_root)
            .output()
            .unwrap();

        assert!(branch_exists("loom/_base/test-stage", repo_root).unwrap());

        cleanup_temp_branch("loom/_base/test-stage", repo_root);

        assert!(!branch_exists("loom/_base/test-stage", repo_root).unwrap());
    }

    #[test]
    fn test_cleanup_all_temp_branches() {
        let temp_dir = init_test_repo();
        let repo_root = temp_dir.path();

        // Create multiple temp branches
        Command::new("git")
            .args(["branch", "loom/_base/stage-1"])
            .current_dir(repo_root)
            .output()
            .unwrap();
        Command::new("git")
            .args(["branch", "loom/_base/stage-2"])
            .current_dir(repo_root)
            .output()
            .unwrap();

        let cleaned = cleanup_all_temp_branches(repo_root).unwrap();

        assert_eq!(cleaned.len(), 2);
        assert!(!branch_exists("loom/_base/stage-1", repo_root).unwrap());
        assert!(!branch_exists("loom/_base/stage-2", repo_root).unwrap());
    }
}
