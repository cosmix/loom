//! One scheduling pass over several stages: what one stage's failure or
//! uncertainty leaves for the rest, and for the operators watching.

use super::*;

/// The plan's graph over stages with these ids, so a snapshot accepts their files.
fn graph_of(stage_ids: &[&str]) -> ExecutionGraph {
    let definitions = stage_ids
        .iter()
        .map(|id| crate::plan::schema::StageDefinition {
            id: (*id).to_string(),
            name: (*id).to_string(),
            working_dir: ".".to_string(),
            ..Default::default()
        })
        .collect();
    ExecutionGraph::build(definitions).unwrap()
}

/// One stage's admission lock cannot be opened: that stage waits with a reason
/// operators can read, the next stage still starts, and the tick goes on.
#[test]
fn a_failed_start_is_reported_per_stage_and_the_pass_goes_on() {
    let temp = work_dir();
    let work = temp.path().join(".loom").join("work");
    stage_at(&work, "a-locked", StageStatus::Queued);
    stage_at(&work, "b-live", StageStatus::Queued);
    let live = session_for("b-live", SessionStatus::Running);
    save_session(&live, &work).unwrap();
    spawn_a_live_agent(&work, &live);
    std::fs::create_dir_all(work.join("stage-admission/a-locked.lock")).unwrap();

    let mut orchestrator = orchestrator_for(&work, temp.path());
    orchestrator.graph = graph_of(&["a-locked", "b-live"]);
    let started = orchestrator.start_ready_stages().unwrap();

    assert_eq!(started, 1);
    assert_eq!(
        load_stage("b-live", &work).unwrap().status,
        StageStatus::Executing
    );
    let reason = orchestrator.spawn_blocks.get("a-locked").expect("a reason");
    assert!(reason.describe().contains("admission"), "{reason:?}");
    assert!(
        matches!(reason, BlockReason::OwnershipUncertain { .. }),
        "{reason:?}"
    );
    assert!(std::fs::read_to_string(work.join("scheduling.json"))
        .unwrap()
        .contains("a-locked"));
}

/// Unknown session inventory defers the spawn, and says so where operators
/// look: without it the stage just sits Queued with no explanation.
#[test]
fn uncertain_inventory_defers_with_a_visible_scheduling_reason() {
    let temp = work_dir();
    let work = temp.path().join(".loom").join("work");
    stage_at(&work, "alpha", StageStatus::Queued);
    std::fs::create_dir_all(work.join("sessions")).unwrap();
    std::fs::write(work.join("sessions").join("broken.md"), "malformed").unwrap();

    let mut orchestrator = orchestrator_for(&work, temp.path());
    orchestrator.graph = graph_of(&["alpha"]);
    assert_eq!(orchestrator.start_ready_stages().unwrap(), 0);

    let reason = orchestrator.spawn_blocks.get("alpha").expect("a reason");
    assert!(
        matches!(reason, BlockReason::OwnershipUncertain { .. }),
        "{reason:?}"
    );
    assert!(!reason.self_resolving(), "{reason:?}");
    assert!(reason.describe().contains("uncertain"), "{reason:?}");
    assert!(reason.describe().contains("broken.md"), "{reason:?}");
    let published = std::fs::read_to_string(work.join("scheduling.json")).unwrap();
    assert!(published.contains("alpha"), "{published}");
    assert!(published.contains("ownership_uncertain"), "{published}");
    assert_eq!(
        load_stage("alpha", &work).unwrap().status,
        StageStatus::Queued
    );
}

/// A dependency completed and merged by an out-of-process write after this
/// tick's graph sync (a knowledge stage's `loom stage complete`) is ready by
/// its file. The graph must agree before base-branch resolution reads it,
/// or every child is refused as `SchedulingNotReady`.
#[test]
fn a_dependency_merged_on_disk_after_graph_sync_does_not_refuse_its_child() {
    let temp = work_dir();
    let work = temp.path().join(".loom").join("work");
    let mut dep = Stage::new("dep".to_string(), None);
    dep.id = "dep".to_string();
    dep.status = StageStatus::Completed;
    dep.merged = true;
    save_stage(&dep, &work).unwrap();
    let mut child = Stage::new("child".to_string(), None);
    child.id = "child".to_string();
    child.status = StageStatus::Queued;
    child.dependencies = vec!["dep".to_string()];
    save_stage(&child, &work).unwrap();

    let mut graph = ExecutionGraph::build(vec![
        crate::plan::schema::StageDefinition {
            id: "dep".to_string(),
            name: "dep".to_string(),
            working_dir: ".".to_string(),
            ..Default::default()
        },
        crate::plan::schema::StageDefinition {
            id: "child".to_string(),
            name: "child".to_string(),
            working_dir: ".".to_string(),
            dependencies: vec!["dep".to_string()],
            ..Default::default()
        },
    ])
    .unwrap();
    graph.mark_status("dep", StageStatus::Executing).unwrap();

    let mut orchestrator = orchestrator_for(&work, temp.path());
    orchestrator.graph = graph;
    orchestrator.start_ready_stages().unwrap();

    let node = orchestrator.graph.get_node("dep").unwrap();
    assert_eq!(node.status, StageStatus::Completed);
    assert!(node.merged);
    let reason = orchestrator.spawn_blocks.get("child");
    assert!(
        !matches!(reason, Some(BlockReason::SchedulingNotReady { .. })),
        "{reason:?}"
    );
}
