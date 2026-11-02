use super::*;
use crate::git::run_git;
use crate::plan::graph::ExecutionGraph;
use crate::plan::schema::StageDefinition;
use tempfile::TempDir;

fn init_test_repo() -> TempDir {
    let temp_dir = TempDir::new().unwrap();
    let repo_root = temp_dir.path();

    run_git(&["init"], repo_root).unwrap();
    run_git(&["config", "user.email", "test@test.com"], repo_root).unwrap();
    run_git(&["config", "user.name", "Test"], repo_root).unwrap();

    // Create initial commit on main
    std::fs::write(repo_root.join("README.md"), "# Test").unwrap();
    run_git(&["add", "."], repo_root).unwrap();
    run_git(&["commit", "-m", "Initial commit"], repo_root).unwrap();

    // Rename to main if needed (some git versions default to master)
    run_git(&["branch", "-M", "main"], repo_root).unwrap();

    temp_dir
}

fn create_branch_with_commit(name: &str, file: &str, content: &str, repo_root: &Path) {
    run_git(&["checkout", "-b", name], repo_root).unwrap();

    std::fs::write(repo_root.join(file), content).unwrap();

    run_git(&["add", file], repo_root).unwrap();
    run_git(&["commit", "-m", &format!("Add {file}")], repo_root).unwrap();
    run_git(&["checkout", "main"], repo_root).unwrap();
}

fn build_test_graph(stages: Vec<(&str, Vec<&str>)>) -> ExecutionGraph {
    let stage_defs: Vec<StageDefinition> = stages
        .into_iter()
        .map(|(id, deps)| StageDefinition {
            id: id.to_string(),
            name: id.to_string(),
            dependencies: deps.into_iter().map(String::from).collect(),
            description: Some(format!("Test stage {id}")),
            working_dir: ".".to_string(),
            ..Default::default()
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
        resolve_base_branch("stage-1", &["dep-1".to_string()], &graph, repo_root, None).unwrap();

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
        resolve_base_branch("stage-1", &["dep-1".to_string()], &graph, repo_root, None).unwrap();

    // Single dep not merged but completed - use its branch
    assert_eq!(result, ResolvedBase::Branch("loom/dep-1".to_string()));
}

#[test]
fn test_single_dep_completed_not_merged_branch_missing_uses_main() {
    let temp_dir = init_test_repo();
    let repo_root = temp_dir.path();

    create_branch_with_commit("feature", "feature.txt", "target work", repo_root);
    // No dependency branch created; exercise the recovery assumption.
    let mut graph = build_test_graph(vec![("dep-1", vec![]), ("stage-1", vec!["dep-1"])]);
    graph.mark_executing("dep-1").unwrap();
    graph.mark_completed("dep-1").unwrap();
    // Note: NOT calling mark_merged()

    let result = resolve_base_branch(
        "stage-1",
        &["dep-1".to_string()],
        &graph,
        repo_root,
        Some("feature"),
    )
    .unwrap();

    assert_eq!(result, ResolvedBase::Main("feature".to_string()));
    let wt = repo_root.join("worktree");
    let output = run_git(
        &[
            "worktree",
            "add",
            "-b",
            "loom/stage-1",
            wt.to_str().unwrap(),
            result.branch_name(),
        ],
        repo_root,
    )
    .unwrap();
    assert!(output.status.success());
    assert!(wt.join("feature.txt").exists());
}

#[test]
fn test_single_dep_branch_missing_dep_not_completed_error() {
    let temp_dir = init_test_repo();
    let repo_root = temp_dir.path();

    // No branch, dep not completed
    let graph = build_test_graph(vec![("dep-1", vec![]), ("stage-1", vec!["dep-1"])]);

    let result = resolve_base_branch("stage-1", &["dep-1".to_string()], &graph, repo_root, None);

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
    run_git(&["branch", "feat-my-feature"], repo_root).unwrap();

    let graph = build_test_graph(vec![("stage-1", vec![])]);

    // When init_base_branch is provided, use it instead of default_branch
    let result =
        resolve_base_branch("stage-1", &[], &graph, repo_root, Some("feat-my-feature")).unwrap();

    assert_eq!(result, ResolvedBase::Main("feat-my-feature".to_string()));
}

#[test]
fn test_merged_deps_uses_init_base_branch_when_provided() {
    let temp_dir = init_test_repo();
    let repo_root = temp_dir.path();

    // Create the feature branch so it exists
    run_git(&["branch", "feat-my-feature"], repo_root).unwrap();

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
