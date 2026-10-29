use super::*;
use std::path::PathBuf;
use tempfile::TempDir;

#[derive(Debug, PartialEq, Eq)]
enum Call {
    Delete,
    Pressure(u32),
    Address(u32),
}

#[derive(Default)]
struct RecordingDriver {
    calls: Vec<Call>,
}

fn input<'a>(rounds: u32, plan: &'a Path, report: &'a Path) -> PipelineInput<'a> {
    PipelineInput {
        rounds,
        invocation: "PLAN-test.md",
        report,
        plan_path: plan,
        claude_model: "opus",
        today: chrono::NaiveDate::from_ymd_opt(2026, 10, 26).unwrap(),
    }
}

impl PressureDriver for RecordingDriver {
    fn delete_report(&mut self, _report: &Path) -> Result<()> {
        self.calls.push(Call::Delete);
        Ok(())
    }

    fn run_pressure(&mut self, round: u32, claude: &str, codex: &str) -> Result<PressureResult> {
        assert_eq!(claude, "/pressure PLAN-test.md");
        assert_eq!(codex, "$pressure PLAN-test.md");
        self.calls.push(Call::Pressure(round));
        Ok(PressureResult {
            verdict: StepVerdict::Continue,
            codex_model_used: Some("gpt-6.1-sol".into()),
        })
    }

    fn run_address(&mut self, round: u32, slash: &str) -> Result<StepVerdict> {
        assert_eq!(slash, "/address PLAN-test.md");
        self.calls.push(Call::Address(round));
        Ok(StepVerdict::Continue)
    }
}

#[test]
fn test_pipeline_two_rounds_driver_call_order() {
    let dir = TempDir::new().unwrap();
    let plan = dir.path().join("PLAN-test.md");
    let report = dir.path().join("codex-PLAN-test.md");
    std::fs::write(&plan, "# Test plan\n\n<!-- END loom METADATA -->\n").unwrap();
    let mut driver = RecordingDriver::default();
    let input = PipelineInput {
        rounds: 2,
        invocation: "PLAN-test.md",
        report: &report,
        plan_path: &plan,
        claude_model: "opus",
        today: chrono::NaiveDate::from_ymd_opt(2026, 10, 26).unwrap(),
    };

    run_pipeline(&mut driver, &input).unwrap();

    assert_eq!(
        driver.calls,
        vec![
            Call::Delete,
            Call::Pressure(1),
            Call::Address(1),
            Call::Delete,
            Call::Pressure(2),
            Call::Address(2),
            Call::Delete,
        ]
    );
}

#[test]
fn test_record_failure_still_deletes_report_and_returns_error() {
    let dir = TempDir::new().unwrap();
    let plan = dir.path().join("PLAN-test.md");
    let report = dir.path().join("codex-PLAN-test.md");
    std::fs::write(&plan, b"# Plan \xff\xfe not utf-8\n").unwrap();
    let mut driver = RecordingDriver::default();

    let result = run_pipeline(&mut driver, &input(1, &plan, &report));

    assert!(result.is_err());
    assert_eq!(
        driver.calls,
        vec![
            Call::Delete,
            Call::Pressure(1),
            Call::Address(1),
            Call::Delete,
        ],
        "the final delete_report ran after the failed record"
    );
}

#[test]
fn test_zero_rounds_is_rejected_before_any_step() {
    let dir = TempDir::new().unwrap();
    let plan = dir.path().join("PLAN-test.md");
    let report = dir.path().join("codex-PLAN-test.md");
    std::fs::write(&plan, "<!-- END loom METADATA -->\n").unwrap();
    let mut driver = RecordingDriver::default();

    assert!(run_pipeline(&mut driver, &input(0, &plan, &report)).is_err());

    assert!(driver.calls.is_empty());
    assert_eq!(
        std::fs::read_to_string(&plan).unwrap(),
        "<!-- END loom METADATA -->\n"
    );
}

#[test]
fn test_plan_steps_order_and_count() {
    let report = PathBuf::from("/repo/doc/plans/codex-PLAN-foo.md");
    let steps = plan_steps(2, "doc/plans/PLAN-foo.md", &report);
    // 3 steps per round (delete, parallel pressure, address) + 1 final delete.
    assert_eq!(steps.len(), 2 * 3 + 1);
    assert_eq!(steps[0], Step::DeleteReport(report.clone()));
    assert_eq!(
        steps[1],
        Step::Pressure {
            claude: "/pressure doc/plans/PLAN-foo.md".into(),
            codex: "$pressure doc/plans/PLAN-foo.md".into(),
        }
    );
    assert_eq!(
        steps[2],
        Step::Address("/address doc/plans/PLAN-foo.md".into())
    );
    // Per-round delete must precede that round's parallel pressure step.
    assert_eq!(steps[3], Step::DeleteReport(report.clone()));
    assert_eq!(*steps.last().unwrap(), Step::DeleteReport(report));
}

#[test]
fn test_plan_steps_single_round() {
    let report = PathBuf::from("/r/codex-P.md");
    let steps = plan_steps(1, "P.md", &report);
    assert_eq!(steps.len(), 4);
}

#[test]
fn test_completion_instruction_workspace_preserves_original_text() {
    let marker = Path::new("/repo/.loom/work/pressure.done");
    let expected = format!(
        "AUTONOMOUS RUN: this Claude session was launched by `loom pressure`; no human will end it for you. \
         When the task is FULLY complete and the plan file is fully updated (after every subagent has finished), \
         your FINAL action MUST be to run exactly this shell command and nothing after it: touch {}. \
         Do not run it earlier. That path is inside the repo's gitignored `.loom/work/` because the agent sandbox \
         mounts /tmp read-only; creating this one marker is the sanctioned exception to the rule against \
         writing under `.loom/work/` directly. Once that file exists the driver closes this session.",
        marker.display()
    );

    let instruction = completion_instruction(marker, MarkerHome::Workspace);

    assert_eq!(instruction, expected);
    assert!(instruction.contains(".loom/work"));
}

#[test]
fn test_completion_instruction_plan_session_uses_session_marker_home() {
    let marker = Path::new("/sessions/test/markers/pressure.done");

    let instruction = completion_instruction(marker, MarkerHome::PlanSession);

    assert!(instruction.contains(&marker.display().to_string()));
    assert!(!instruction.contains(".loom/work"));
    assert!(instruction.contains(
        "That path is this session's completion-marker directory, the one place outside the plan this session may write; \
         creating this one marker is the only write it is for."
    ));
}
