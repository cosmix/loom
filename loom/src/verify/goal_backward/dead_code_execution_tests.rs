use super::*;
use crate::plan::schema::StageDefinition;
use crate::verify::goal_backward::run_goal_backward_verification;
use tempfile::TempDir;

fn check(command: &str) -> DeadCodeCheck {
    DeadCodeCheck {
        command: command.into(),
        fail_patterns: vec!["unused".into()],
        ignore_patterns: vec![],
    }
}

#[test]
fn failed_analyzers_fail_helper_and_goal_gate() {
    let dir = TempDir::new().unwrap();
    for (command, passed) in [
        ("printf clean", true),
        ("printf unused", false),
        ("exit 1", false),
        ("exit 127", false),
        ("kill -TERM $$", false),
    ] {
        let check = check(command);
        let result =
            run_dead_code_check(&check, dir.path(), None, CommandConfinement::default()).unwrap();
        assert_eq!(result.is_empty(), passed, "helper: {command}");
        let stage = StageDefinition {
            id: "analyzer".into(),
            name: "Analyzer".into(),
            working_dir: ".".into(),
            dead_code_check: Some(check),
            ..Default::default()
        };
        let goal = run_goal_backward_verification(
            &stage,
            dir.path(),
            None,
            CommandConfinement::default(),
            1,
        )
        .unwrap();
        assert_eq!(goal.is_passed(), passed, "goal gate: {command}");
    }
}

#[test]
fn timed_out_analyzer_is_unavailable_analysis() {
    let dir = TempDir::new().unwrap();
    // exec keeps the sleep in the managed child process group.
    let gaps = run_with_timeout(
        &check("exec sleep 10"),
        dir.path(),
        None,
        CommandConfinement::default(),
        Duration::from_millis(25),
    )
    .unwrap();
    assert!(gaps.iter().any(|gap| gap.description.contains("timed out")));
}

#[test]
fn incomplete_analyzer_output_cannot_certify_absence() {
    let dir = TempDir::new().unwrap();
    let gaps = run_dead_code_check(
        &check("head -c 10485761 /dev/zero; printf unused"),
        dir.path(),
        None,
        CommandConfinement::default(),
    )
    .unwrap();
    assert!(gaps
        .iter()
        .any(|gap| gap.description.contains("capture incomplete")));
}
