use super::*;
use chrono::Utc;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;

fn make_stage(id: &str, deps: Vec<&str>, status: StageStatus) -> Stage {
    Stage {
        id: id.to_string(),
        name: id.to_string(),
        description: None,
        status,
        dependencies: deps.into_iter().map(String::from).collect(),
        parallel_group: None,
        acceptance: vec![],
        setup: vec![],
        files: vec![],
        stage_type: Default::default(),
        plan_id: None,
        worktree: None,
        session: None,
        held: false,
        parent_stage: None,
        child_stages: vec![],
        created_at: Utc::now(),
        updated_at: Utc::now(),
        completed_at: None,
        started_at: None,
        duration_secs: None,
        execution_secs: None,
        attempt_started_at: None,
        close_reason: None,
        auto_merge: None,
        working_dir: None,
        retry_count: 0,
        max_retries: None,
        last_failure_at: None,
        failure_info: None,
        resolved_base: None,
        base_branch: None,
        base_merged_from: vec![],
        outputs: vec![],
        completed_commit: None,
        merged: false,
        merge_conflict: false,
        verification_status: Default::default(),
        context_budget: None,
        truths: Vec::new(),
        artifacts: Vec::new(),
        wiring: Vec::new(),
        sandbox: Default::default(),
        execution_mode: None,
        fix_attempts: 0,
        max_fix_attempts: None,
        review_reason: None,
    }
}

#[test]
fn test_empty_stages() {
    let widget = TreeWidget::new(&[]);
    let lines = widget.build_lines();
    assert_eq!(lines.len(), 1);
}

#[test]
fn test_single_stage() {
    let stages = vec![make_stage("bootstrap", vec![], StageStatus::Completed)];
    let widget = TreeWidget::new(&stages);
    let lines = widget.build_lines();
    assert_eq!(lines.len(), 1);
}

#[test]
fn test_linear_dependency() {
    let stages = vec![
        make_stage("a", vec![], StageStatus::Completed),
        make_stage("b", vec!["a"], StageStatus::Executing),
        make_stage("c", vec!["b"], StageStatus::WaitingForDeps),
    ];
    let widget = TreeWidget::new(&stages);
    let lines = widget.build_lines();
    assert_eq!(lines.len(), 3);
}

#[test]
fn test_diamond_dependency() {
    let stages = vec![
        make_stage("a", vec![], StageStatus::Completed),
        make_stage("b", vec!["a"], StageStatus::Completed),
        make_stage("c", vec!["a"], StageStatus::Completed),
        make_stage("d", vec!["b", "c"], StageStatus::Executing),
    ];
    let widget = TreeWidget::new(&stages);
    let lines = widget.build_lines();
    assert_eq!(lines.len(), 4);
}

#[test]
fn test_widget_render() {
    let stages = vec![make_stage("test", vec![], StageStatus::Completed)];
    let widget = execution_tree(&stages);

    let area = Rect::new(0, 0, 40, 10);
    let mut buf = Buffer::empty(area);
    widget.render(area, &mut buf);

    // The border should be rendered
    assert_ne!(buf[(0, 0)].symbol(), " ");
}

#[test]
fn test_stage_levels_computed_correctly() {
    let stages = vec![
        make_stage("a", vec![], StageStatus::Completed),
        make_stage("b", vec!["a"], StageStatus::Completed),
        make_stage("c", vec!["a"], StageStatus::Completed),
        make_stage("d", vec!["b", "c"], StageStatus::Completed),
    ];
    let levels = compute_stage_levels(&stages);
    assert_eq!(levels.get("a"), Some(&0));
    assert_eq!(levels.get("b"), Some(&1));
    assert_eq!(levels.get("c"), Some(&1));
    assert_eq!(levels.get("d"), Some(&2));
}

#[test]
fn test_status_indicators() {
    assert_eq!(StageStatus::Completed.icon(), "✓");
    assert_eq!(StageStatus::Executing.icon(), "●");
    assert_eq!(StageStatus::Blocked.icon(), "✗");
}

#[test]
fn test_with_context_and_elapsed() {
    let stages = vec![make_stage("exec", vec![], StageStatus::Executing)];

    let mut ctx = HashMap::new();
    ctx.insert("exec".to_string(), 0.45);

    let mut elapsed = HashMap::new();
    elapsed.insert("exec".to_string(), 120_i64);

    let widget = TreeWidget::new(&stages)
        .context_percentages(ctx)
        .elapsed_times(elapsed);

    let lines = widget.build_lines();
    assert_eq!(lines.len(), 1);
}

#[test]
fn test_with_base_branch() {
    let mut stage = make_stage("exec", vec!["root"], StageStatus::Executing);
    stage.base_branch = Some("loom/exec".to_string());
    let stages = vec![make_stage("root", vec![], StageStatus::Completed), stage];

    let widget = TreeWidget::new(&stages);
    let lines = widget.build_lines();
    // Should have 2 lines for root and exec, plus 1 for base branch info
    assert_eq!(lines.len(), 3);
}
