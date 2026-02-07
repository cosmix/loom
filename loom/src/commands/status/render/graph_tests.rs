use super::*;
use crate::commands::status::data::{ActivityStatus, MergeSummary, ProgressSummary};

fn make_stage_summary(id: &str, deps: Vec<&str>, status: StageStatus) -> StageSummary {
    StageSummary {
        id: id.to_string(),
        name: id.to_string(),
        status,
        dependencies: deps.into_iter().map(String::from).collect(),
        context_pct: None,
        elapsed_secs: None,
        execution_secs: None,
        base_branch: None,
        base_merged_from: vec![],
        failure_info: None,
        activity_status: ActivityStatus::default(),
        last_tool: None,
        last_activity: None,
        staleness_secs: None,
        context_budget_pct: None,
        review_reason: None,
    }
}

fn make_status_data(stages: Vec<StageSummary>) -> StatusData {
    StatusData {
        stages,
        merge: MergeSummary {
            merged: vec![],
            pending: vec![],
            conflicts: vec![],
        },
        progress: ProgressSummary {
            total: 0,
            completed: 0,
            executing: 0,
            pending: 0,
            blocked: 0,
        },
    }
}

#[test]
fn test_compute_stage_levels_empty() {
    let stages: Vec<StageSummary> = vec![];
    let levels = compute_stage_levels(&stages);
    assert!(levels.is_empty());
}

#[test]
fn test_compute_stage_levels_linear() {
    let stages = vec![
        make_stage_summary("a", vec![], StageStatus::Completed),
        make_stage_summary("b", vec!["a"], StageStatus::Completed),
        make_stage_summary("c", vec!["b"], StageStatus::Completed),
    ];
    let levels = compute_stage_levels(&stages);
    assert_eq!(levels.get("a"), Some(&0));
    assert_eq!(levels.get("b"), Some(&1));
    assert_eq!(levels.get("c"), Some(&2));
}

#[test]
fn test_compute_stage_levels_diamond() {
    let stages = vec![
        make_stage_summary("a", vec![], StageStatus::Completed),
        make_stage_summary("b", vec!["a"], StageStatus::Completed),
        make_stage_summary("c", vec!["a"], StageStatus::Completed),
        make_stage_summary("d", vec!["b", "c"], StageStatus::Completed),
    ];
    let levels = compute_stage_levels(&stages);
    assert_eq!(levels.get("a"), Some(&0));
    assert_eq!(levels.get("b"), Some(&1));
    assert_eq!(levels.get("c"), Some(&1));
    assert_eq!(levels.get("d"), Some(&2));
}

#[test]
fn test_render_graph_empty() {
    let data = make_status_data(vec![]);
    let mut output = Vec::new();
    render_graph(&mut output, &data).unwrap();
    let output_str = String::from_utf8(output).unwrap();
    assert!(output_str.contains("no stages found"));
}

#[test]
fn test_render_graph_linear() {
    let data = make_status_data(vec![
        make_stage_summary("bootstrap", vec![], StageStatus::Completed),
        make_stage_summary("implement", vec!["bootstrap"], StageStatus::Executing),
        make_stage_summary("verify", vec!["implement"], StageStatus::WaitingForDeps),
    ]);
    let mut output = Vec::new();
    render_graph(&mut output, &data).unwrap();
    let output_str = String::from_utf8(output).unwrap();
    assert!(output_str.contains("bootstrap"));
    assert!(output_str.contains("implement"));
    assert!(output_str.contains("verify"));
    // Tree connectors should be present
    assert!(output_str.contains("├──") || output_str.contains("└──"));
}

#[test]
fn test_render_graph_with_context() {
    let mut stage = make_stage_summary("executing", vec![], StageStatus::Executing);
    stage.context_pct = Some(0.45);
    stage.elapsed_secs = Some(120);

    let data = make_status_data(vec![stage]);
    let mut output = Vec::new();
    render_graph(&mut output, &data).unwrap();
    let output_str = String::from_utf8(output).unwrap();
    assert!(output_str.contains("45%"));
    assert!(output_str.contains("2m0s"));
}

#[test]
fn test_status_indicators() {
    // Just verify they don't panic
    let _ = status_indicator(&StageStatus::Completed);
    let _ = status_indicator(&StageStatus::Executing);
    let _ = status_indicator(&StageStatus::Blocked);
    let _ = status_indicator(&StageStatus::NeedsHandoff);
}
