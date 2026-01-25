//! Base branch resolution for worktree creation
//!
//! Determines the correct base branch for a new worktree based on stage dependencies.
//! With progressive merge, all dependency work is merged to main before dependent stages
//! can be scheduled, so the base is always main (or a single unmerged dependency branch).

use anyhow::{bail, Result};
use std::path::Path;

use crate::git::branch::{branch_exists, branch_name_for_stage, default_branch};
use crate::plan::graph::{ExecutionGraph, NodeStatus};

/// Result of resolving the base branch for a stage
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ResolvedBase {
    /// Use the main/master branch as base (merge point containing all dep work)
    Main(String),
    /// Use a specific loom branch as base (single unmerged dependency)
    Branch(String),
}

impl ResolvedBase {
    /// Get the branch name to use
    pub fn branch_name(&self) -> &str {
        match self {
            ResolvedBase::Main(name) => name,
            ResolvedBase::Branch(name) => name,
        }
    }
}

/// Resolve the base branch for a stage based on its dependencies.
///
/// # Logic
///
/// With progressive merge, stages are only scheduled when all dependencies are
/// `Completed` AND `merged: true`. This means all dependency work is already
/// in main, so we can use main as the base.
///
/// - No deps → Use init_base_branch if provided, else default_branch (main)
/// - All deps merged → Use main (contains all dependency work)
/// - Single dep not merged → Use that dep's branch (legacy/fallback path)
/// - Multiple deps, any not merged → Scheduling error (should not happen)
///
/// # Arguments
///
/// * `stage_id` - The ID of the stage needing a worktree
/// * `dependencies` - The stage's dependency list
/// * `graph` - The execution graph (for checking dependency status and merged state)
/// * `repo_root` - Path to the git repository root
/// * `init_base_branch` - Optional base branch from config.toml (for stages with no deps)
///
/// # Returns
///
/// * `Ok(ResolvedBase)` - The resolved base branch to use
/// * `Err` - If dependencies are not ready (not completed or not merged)
pub fn resolve_base_branch(
    stage_id: &str,
    dependencies: &[String],
    graph: &ExecutionGraph,
    repo_root: &Path,
    init_base_branch: Option<&str>,
) -> Result<ResolvedBase> {
    eprintln!(
        "[resolve_base_branch] stage={stage_id}, deps={dependencies:?}, init_base_branch={init_base_branch:?}"
    );

    // No deps → use init_base_branch if provided, otherwise fall back to default
    if dependencies.is_empty() {
        let base = init_base_branch
            .map(String::from)
            .unwrap_or_else(|| default_branch(repo_root).unwrap_or_else(|_| "main".to_string()));
        eprintln!("[resolve_base_branch] No deps, using base: {base}");
        return Ok(ResolvedBase::Main(base));
    }

    // Check all dependencies are completed and merged
    let mut unmerged_deps: Vec<(&str, bool, bool)> = Vec::new(); // (id, completed, merged)

    for dep in dependencies {
        let dep_node = graph
            .get_node(dep)
            .ok_or_else(|| anyhow::anyhow!("Dependency '{dep}' not found in graph"))?;

        let is_completed = dep_node.status == NodeStatus::Completed;
        let is_merged = dep_node.merged;

        eprintln!(
            "[resolve_base_branch] Dep '{}': status={:?}, completed={}, merged={}",
            dep, dep_node.status, is_completed, is_merged
        );

        if !is_completed || !is_merged {
            unmerged_deps.push((dep.as_str(), is_completed, is_merged));
        }
    }

    // All deps completed and merged - use merge point (init_base_branch or default)
    if unmerged_deps.is_empty() {
        let base = init_base_branch
            .map(String::from)
            .unwrap_or_else(|| default_branch(repo_root).unwrap_or_else(|_| "main".to_string()));
        eprintln!("[resolve_base_branch] All deps merged, using base: {base}");
        return Ok(ResolvedBase::Main(base));
    }

    // Single dependency that's completed but not merged - use its branch directly
    // (This handles the edge case during initial stages or when progressive merge is disabled)
    if dependencies.len() == 1 {
        let (dep, is_completed, is_merged) = unmerged_deps[0];

        if is_completed && !is_merged {
            // Dep is done but not merged yet - check if branch exists
            let dep_branch = branch_name_for_stage(dep);
            if branch_exists(&dep_branch, repo_root)? {
                return Ok(ResolvedBase::Branch(dep_branch));
            }
            // Branch doesn't exist but dep is completed - assume merged, use main
            let main = default_branch(repo_root)?;
            return Ok(ResolvedBase::Main(main));
        }

        // Dependency not completed
        bail!(
            "Scheduling error: dependency '{}' is {:?} (completed={}, merged={}). \
             Stages should only be scheduled after their dependencies complete and merge.",
            dep,
            graph.get_node(dep).map(|n| &n.status),
            is_completed,
            is_merged
        );
    }

    // Multiple dependencies with some not ready - scheduling error
    let not_ready: Vec<_> = unmerged_deps
        .iter()
        .map(|(id, completed, merged)| format!("{id}(completed={completed}, merged={merged})"))
        .collect();

    bail!(
        "Scheduling error: stage '{}' has dependencies not ready: [{}]. \
         All dependencies must be completed AND merged before scheduling.",
        stage_id,
        not_ready.join(", ")
    );
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
                working_dir: ".".to_string(),
                stage_type: crate::plan::schema::StageType::default(),
                truths: vec![],
                artifacts: vec![],
                wiring: vec![],
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
    fn test_single_dep_completed_and_merged_uses_main() {
        let temp_dir = init_test_repo();
        let repo_root = temp_dir.path();

        // With progressive merge, completed+merged deps mean we use main
        let mut graph = build_test_graph(vec![("dep-1", vec![]), ("stage-1", vec!["dep-1"])]);
        graph.mark_executing("dep-1").unwrap();
        graph.mark_completed("dep-1").unwrap();
        graph.mark_merged("dep-1").unwrap();

        let result =
            resolve_base_branch("stage-1", &["dep-1".to_string()], &graph, repo_root, None)
                .unwrap();

        // All deps merged - use main as base
        assert_eq!(result, ResolvedBase::Main("main".to_string()));
    }

    #[test]
    fn test_single_dep_completed_not_merged_uses_branch() {
        let temp_dir = init_test_repo();
        let repo_root = temp_dir.path();

        create_branch_with_commit("loom/dep-1", "dep1.txt", "content", repo_root);

        // Dep is completed but not merged (legacy/fallback path)
        let mut graph = build_test_graph(vec![("dep-1", vec![]), ("stage-1", vec!["dep-1"])]);
        graph.mark_executing("dep-1").unwrap();
        graph.mark_completed("dep-1").unwrap();
        // Note: NOT calling mark_merged()

        let result =
            resolve_base_branch("stage-1", &["dep-1".to_string()], &graph, repo_root, None)
                .unwrap();

        // Single dep not merged but completed - use its branch
        assert_eq!(result, ResolvedBase::Branch("loom/dep-1".to_string()));
    }

    #[test]
    fn test_single_dep_completed_not_merged_branch_missing_uses_main() {
        let temp_dir = init_test_repo();
        let repo_root = temp_dir.path();

        // No branch created - assume it was merged already
        let mut graph = build_test_graph(vec![("dep-1", vec![]), ("stage-1", vec!["dep-1"])]);
        graph.mark_executing("dep-1").unwrap();
        graph.mark_completed("dep-1").unwrap();
        // Note: NOT calling mark_merged()

        let result =
            resolve_base_branch("stage-1", &["dep-1".to_string()], &graph, repo_root, None)
                .unwrap();

        // Branch missing but completed - fall back to main
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
    fn test_multiple_deps_all_merged_uses_main() {
        let temp_dir = init_test_repo();
        let repo_root = temp_dir.path();

        // With progressive merge, all deps merged means we use main
        let mut graph = build_test_graph(vec![
            ("dep-1", vec![]),
            ("dep-2", vec![]),
            ("stage-1", vec!["dep-1", "dep-2"]),
        ]);
        graph.mark_executing("dep-1").unwrap();
        graph.mark_completed("dep-1").unwrap();
        graph.mark_merged("dep-1").unwrap();
        graph.mark_executing("dep-2").unwrap();
        graph.mark_completed("dep-2").unwrap();
        graph.mark_merged("dep-2").unwrap();

        let result = resolve_base_branch(
            "stage-1",
            &["dep-1".to_string(), "dep-2".to_string()],
            &graph,
            repo_root,
            None,
        )
        .unwrap();

        // All deps merged - use main as base
        assert_eq!(result, ResolvedBase::Main("main".to_string()));
    }

    #[test]
    fn test_multiple_deps_some_not_merged_returns_error() {
        let temp_dir = init_test_repo();
        let repo_root = temp_dir.path();

        // Multiple deps, but not all merged - scheduling error
        let mut graph = build_test_graph(vec![
            ("dep-1", vec![]),
            ("dep-2", vec![]),
            ("stage-1", vec!["dep-1", "dep-2"]),
        ]);
        graph.mark_executing("dep-1").unwrap();
        graph.mark_completed("dep-1").unwrap();
        graph.mark_merged("dep-1").unwrap();
        graph.mark_executing("dep-2").unwrap();
        graph.mark_completed("dep-2").unwrap();
        // Note: dep-2 NOT marked as merged

        let result = resolve_base_branch(
            "stage-1",
            &["dep-1".to_string(), "dep-2".to_string()],
            &graph,
            repo_root,
            None,
        );

        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("Scheduling error"));
        assert!(err.contains("dep-2"));
        assert!(err.contains("merged=false"));
    }

    #[test]
    fn test_multiple_deps_none_completed_returns_error() {
        let temp_dir = init_test_repo();
        let repo_root = temp_dir.path();

        // Dependencies not completed at all
        let graph = build_test_graph(vec![
            ("dep-1", vec![]),
            ("dep-2", vec![]),
            ("stage-1", vec!["dep-1", "dep-2"]),
        ]);

        let result = resolve_base_branch(
            "stage-1",
            &["dep-1".to_string(), "dep-2".to_string()],
            &graph,
            repo_root,
            None,
        );

        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("Scheduling error"));
        assert!(err.contains("completed=false"));
    }

    #[test]
    fn test_resolved_base_branch_name() {
        let main = ResolvedBase::Main("main".to_string());
        assert_eq!(main.branch_name(), "main");

        let branch = ResolvedBase::Branch("loom/stage-1".to_string());
        assert_eq!(branch.branch_name(), "loom/stage-1");
    }

    #[test]
    fn test_empty_deps_uses_init_base_branch_when_provided() {
        let temp_dir = init_test_repo();
        let repo_root = temp_dir.path();

        // Create the feature branch so it exists
        Command::new("git")
            .args(["branch", "feat-my-feature"])
            .current_dir(repo_root)
            .output()
            .unwrap();

        let graph = build_test_graph(vec![("stage-1", vec![])]);

        // When init_base_branch is provided, use it instead of default_branch
        let result =
            resolve_base_branch("stage-1", &[], &graph, repo_root, Some("feat-my-feature"))
                .unwrap();

        assert_eq!(result, ResolvedBase::Main("feat-my-feature".to_string()));
    }

    #[test]
    fn test_merged_deps_uses_init_base_branch_when_provided() {
        let temp_dir = init_test_repo();
        let repo_root = temp_dir.path();

        // Create the feature branch so it exists
        Command::new("git")
            .args(["branch", "feat-my-feature"])
            .current_dir(repo_root)
            .output()
            .unwrap();

        // With progressive merge, completed+merged deps should use init_base_branch
        let mut graph = build_test_graph(vec![("dep-1", vec![]), ("stage-1", vec!["dep-1"])]);
        graph.mark_executing("dep-1").unwrap();
        graph.mark_completed("dep-1").unwrap();
        graph.mark_merged("dep-1").unwrap();

        // When init_base_branch is provided, use it instead of default_branch
        let result = resolve_base_branch(
            "stage-1",
            &["dep-1".to_string()],
            &graph,
            repo_root,
            Some("feat-my-feature"),
        )
        .unwrap();

        // All deps merged AND init_base_branch provided - use init_base_branch
        assert_eq!(result, ResolvedBase::Main("feat-my-feature".to_string()));
    }
}
