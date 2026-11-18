use super::*;

fn summary(id: &str, status: StageStatus, deps: &[&str]) -> StageSummary {
    StageSummary {
        id: id.to_string(),
        name: id.to_string(),
        status,
        dependencies: deps.iter().map(|dep| (*dep).to_string()).collect(),
        model: String::new(),
        ..Default::default()
    }
}

fn blocker(state: CompletionBlockerState, fingerprint: &str) -> CompletionBlockerSummary {
    CompletionBlockerSummary {
        state,
        fingerprint: fingerprint.to_owned(),
        failure_code: "acceptance_failed".to_owned(),
        summary: Some("verification failed".to_owned()),
        commit: "abc1234".to_owned(),
        repeat_count: 1,
        first_observed_at: None,
        last_observed_at: None,
        next_action: "inspect output".to_owned(),
    }
}

#[test]
fn test_graph_state_default() {
    let state = GraphState::default();
    assert_eq!(state.scroll_y, 0);
    assert_eq!(state.total_lines, 0);
    assert_eq!(state.viewport_height, 0);
}

#[test]
fn test_graph_state_scroll_by() {
    let mut state = GraphState {
        scroll_y: 5,
        total_lines: 20,
        viewport_height: 10,
    };

    state.scroll_by(3);
    assert_eq!(state.scroll_y, 8);

    state.scroll_by(-3);
    assert_eq!(state.scroll_y, 5);

    state.scroll_by(100);
    assert_eq!(state.scroll_y, 10);

    state.scroll_by(-100);
    assert_eq!(state.scroll_y, 0);
}

#[test]
fn test_graph_state_scroll_to_start_end() {
    let mut state = GraphState {
        scroll_y: 5,
        total_lines: 20,
        viewport_height: 10,
    };

    state.scroll_to_end();
    assert_eq!(state.scroll_y, 10);
    state.scroll_to_start();
    assert_eq!(state.scroll_y, 0);
}

#[test]
fn test_live_status_compute_levels() {
    let status = LiveStatus {
        data: StatusData {
            stages: vec![
                summary("a", StageStatus::WaitingForDeps, &[]),
                summary("b", StageStatus::WaitingForDeps, &["a"]),
                summary("c", StageStatus::WaitingForDeps, &["a", "b"]),
            ],
            ..Default::default()
        },
    };

    let levels = status.compute_levels();

    assert_eq!(levels.get("a"), Some(&0));
    assert_eq!(levels.get("b"), Some(&1));
    assert_eq!(levels.get("c"), Some(&2));
}

#[test]
fn blocker_transitions_deduplicate_and_track_fingerprint() {
    let mut log = TuiActivityLog::new();
    let mut stage = summary("stage", StageStatus::Executing, &[]);
    log.update(&[&stage]);
    let baseline = log.len();
    stage.completion_blocker = Some(blocker(CompletionBlockerState::Pending, "first"));
    log.update(&[&stage]);
    let after_pending = log.len();
    assert_eq!(after_pending, baseline + 1);
    log.update(&[&stage]);
    assert_eq!(log.len(), after_pending);
    stage.completion_blocker.as_mut().unwrap().state = CompletionBlockerState::Blocked;
    log.update(&[&stage]);
    assert_eq!(log.len(), baseline + 2);
    stage.completion_blocker.as_mut().unwrap().fingerprint = "second".to_owned();
    log.update(&[&stage]);
    assert_eq!(log.len(), baseline + 3);
}
