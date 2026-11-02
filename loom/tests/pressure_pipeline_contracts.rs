//! Contracts for the shared pressure pipeline (`loom::pressure`), its
//! `## Pressure tests` record writer, and the built-in tmux backend default.

#[path = "pressure_pipeline_contracts/support.rs"]
mod support;

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;

use loom::models::session::SessionBackendKind;
use loom::pressure::record::{append_record, append_to_text, RecordWrite};
use loom::pressure::{
    completion_instruction, run_pipeline, MarkerHome, PipelineInput, PipelineOutcome,
};
use support::{
    date, git_repo_with_plan, record_line, run_loom, Call, FakeDriver, END_MARKER, MINIMAL_PLAN,
};

const NEW_LINE: &str = "- 2026-10-26: Claude opus, Codex gpt-6.1-sol; 1 round, review addressed";

fn section_at_end(line: &str) -> String {
    format!("\n## Pressure tests\n\n{line}\n")
}

#[test]
fn record_appends_after_existing_section_items() {
    let tail = "\n## Pressure tests\n\n- 2026-10-01: old\n\n## Notes\n\ntext\n";
    let text = format!("{MINIMAL_PLAN}{tail}");
    let expected = format!(
        "{MINIMAL_PLAN}\n## Pressure tests\n\n- 2026-10-01: old\n{NEW_LINE}\n\n## Notes\n\ntext\n"
    );
    assert_eq!(append_to_text(&text, NEW_LINE), expected);
}

#[test]
fn record_creates_section_at_end_when_absent() {
    assert!(MINIMAL_PLAN.ends_with(&format!("{END_MARKER}\n")));
    let with_marker = append_to_text(MINIMAL_PLAN, NEW_LINE);
    assert_eq!(
        with_marker,
        format!("{MINIMAL_PLAN}{}", section_at_end(NEW_LINE))
    );

    let no_marker = "# PLAN: prose only\n\nNo metadata block here.\n";
    assert_eq!(
        append_to_text(no_marker, NEW_LINE),
        format!("{no_marker}{}", section_at_end(NEW_LINE))
    );
}

#[test]
fn record_ignores_a_heading_before_the_marker() {
    let prose = "# PLAN: heading in prose\n\n## Pressure tests\n\n- 2026-01-01: prose item\n\n";
    let metadata_start = MINIMAL_PLAN
        .find("<!-- loom METADATA -->")
        .expect("fixture has a metadata block");
    let text = format!("{prose}{}", &MINIMAL_PLAN[metadata_start..]);

    let result = append_to_text(&text, NEW_LINE);
    assert_eq!(result, format!("{text}{}", section_at_end(NEW_LINE)));
    assert!(result.starts_with(prose), "prose before the marker changed");
}

#[test]
fn record_line_renders_rounds_models_and_outcome() {
    assert_eq!(
        record_line(&["gpt-6.1-sol", "gpt-6.1-sol"], 2, &[]).render(),
        "- 2026-10-26: Claude opus, Codex gpt-6.1-sol; 2 rounds, review addressed"
    );
    assert_eq!(
        record_line(&["a", "a", "b"], 3, &[2]).render(),
        "- 2026-10-26: Claude opus, Codex a / a / b; 3 rounds, Codex review missing in round 2"
    );
    assert_eq!(record_line(&["gpt-6.1-sol"], 1, &[]).render(), NEW_LINE);
}

#[test]
fn record_keeps_the_plan_file_mode() {
    let dir = tempfile::tempdir().unwrap();
    let plan = dir.path().join("PLAN-x.md");
    fs::write(&plan, MINIMAL_PLAN).unwrap();
    fs::set_permissions(&plan, fs::Permissions::from_mode(0o640)).unwrap();

    let line = record_line(&["gpt-6.1-sol"], 1, &[]);
    let written = append_record(&plan, &line).expect("append_record");
    assert!(matches!(written, RecordWrite::Written), "expected Written");

    let mode = fs::metadata(&plan).unwrap().permissions().mode() & 0o7777;
    assert_eq!(mode, 0o640, "plan mode changed to {mode:o}");
    let text = fs::read_to_string(&plan).unwrap();
    assert_eq!(text, format!("{MINIMAL_PLAN}{}", section_at_end(NEW_LINE)));
}

fn entries(dir: &Path) -> Vec<String> {
    let mut names: Vec<String> = fs::read_dir(dir)
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    names
}

#[test]
fn moved_or_symlinked_plan_is_not_written() {
    let line = record_line(&["gpt-6.1-sol"], 1, &[]);
    let gone_dir = tempfile::tempdir().unwrap();
    let gone = gone_dir.path().join("PLAN-gone.md");
    let result = append_record(&gone, &line).expect("append_record on a removed path");
    assert!(
        matches!(result, RecordWrite::PlanMoved),
        "expected PlanMoved"
    );
    assert!(!gone.exists(), "the moved plan was recreated");
    assert!(entries(gone_dir.path()).is_empty(), "files were created");

    let target_dir = tempfile::tempdir().unwrap();
    let target = target_dir.path().join("PLAN-real.md");
    fs::write(&target, MINIMAL_PLAN).unwrap();
    let link_dir = tempfile::tempdir().unwrap();
    let link = link_dir.path().join("PLAN-x.md");
    std::os::unix::fs::symlink(&target, &link).unwrap();

    let result = append_record(&link, &line).expect("append_record on a symlink");
    assert!(
        matches!(result, RecordWrite::PlanMoved),
        "expected PlanMoved"
    );
    assert_eq!(fs::read(&target).unwrap(), MINIMAL_PLAN.as_bytes());
    assert!(fs::symlink_metadata(&link)
        .unwrap()
        .file_type()
        .is_symlink());
    assert_eq!(entries(link_dir.path()), vec!["PLAN-x.md".to_string()]);
    assert_eq!(entries(target_dir.path()), vec!["PLAN-real.md".to_string()]);
}

fn input<'a>(rounds: u32, report: &'a Path, plan: &'a Path) -> PipelineInput<'a> {
    PipelineInput {
        rounds,
        invocation: "doc/plans/PLAN-x.md",
        report,
        plan_path: plan,
        claude_model: "opus",
        today: date(2026, 10, 26),
    }
}

#[test]
fn stopped_pipeline_writes_no_record() {
    let dir = tempfile::tempdir().unwrap();
    let plan = dir.path().join("PLAN-x.md");
    let report = dir.path().join("codex-PLAN-x.md");
    fs::write(&plan, MINIMAL_PLAN).unwrap();

    let mut driver = FakeDriver::new(&report, &[1], Some(Call::Address(1)));
    let outcome = run_pipeline(&mut driver, &input(2, &report, &plan)).expect("run_pipeline");

    assert!(
        matches!(outcome, PipelineOutcome::Stopped { .. }),
        "expected Stopped"
    );
    assert!(
        driver.calls.contains(&Call::Address(1)),
        "{:?}",
        driver.calls
    );
    assert!(
        !driver.calls.contains(&Call::Pressure(2)),
        "{:?}",
        driver.calls
    );
    assert_eq!(fs::read(&plan).unwrap(), MINIMAL_PLAN.as_bytes());
    assert!(!report.exists(), "the Codex report survived the stop");
}

#[test]
fn missing_report_round_is_recorded() {
    let dir = tempfile::tempdir().unwrap();
    let plan = dir.path().join("PLAN-x.md");
    let report = dir.path().join("codex-PLAN-x.md");
    fs::write(&plan, MINIMAL_PLAN).unwrap();

    let mut driver = FakeDriver::new(&report, &[1], None);
    let outcome = run_pipeline(&mut driver, &input(2, &report, &plan)).expect("run_pipeline");

    let expected = "- 2026-10-26: Claude opus, Codex gpt-6.1-sol; 2 rounds, \
                    Codex review missing in round 2";
    match outcome {
        PipelineOutcome::Recorded { line } => assert_eq!(line, expected),
        _ => panic!("expected Recorded"),
    }
    let text = fs::read_to_string(&plan).unwrap();
    let last = text.lines().last().expect("plan has lines");
    assert!(last.ends_with("Codex review missing in round 2"), "{last}");
    assert_eq!(text, format!("{MINIMAL_PLAN}{}", section_at_end(expected)));
}

#[test]
fn recorded_plan_passes_verify_and_preflight() {
    let (_temp, root, plan) = git_repo_with_plan(MINIMAL_PLAN);
    let line = record_line(&["gpt-6.1-sol", "gpt-6.1-sol"], 2, &[]);
    let written = append_record(&plan, &line).expect("append_record");
    assert!(matches!(written, RecordWrite::Written), "expected Written");
    assert!(fs::read_to_string(&plan)
        .unwrap()
        .contains("## Pressure tests"));

    let out = run_loom(&root, &["plan", "verify", plan.to_str().unwrap()]);
    assert!(
        out.status.success(),
        "plan verify failed: {}\n{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    if let Err(err) = loom::commands::init::preflight_plan(&plan) {
        panic!("preflight_plan refused the recorded plan: {err:#}");
    }
}

#[test]
fn completion_instruction_follows_the_marker_home() {
    let marker = Path::new("/run/user/1000/loom/plan-session/claude-42.done");
    let planning = completion_instruction(marker, MarkerHome::PlanSession);
    assert!(
        planning.contains(&marker.display().to_string()),
        "{planning}"
    );
    assert!(!planning.contains(".loom/work"), "{planning}");

    let workspace = completion_instruction(marker, MarkerHome::Workspace);
    assert!(
        workspace.contains(&marker.display().to_string()),
        "{workspace}"
    );
    assert!(workspace.contains(".loom/work"), "{workspace}");
}

#[test]
fn dry_run_lists_the_record_step() {
    let (_temp, root, _plan) = git_repo_with_plan(MINIMAL_PLAN);
    let out = run_loom(&root, &["pressure", "--dry-run", "doc/plans/PLAN-x.md"]);
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        out.status.success(),
        "dry run failed: {stdout}\n{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let lines: Vec<&str> = stdout.lines().collect();
    let first_delete = lines
        .iter()
        .position(|l| l.contains("delete report"))
        .unwrap_or_else(|| panic!("no delete-report step: {stdout}"));
    let last_delete = lines
        .iter()
        .rposition(|l| l.contains("delete report"))
        .unwrap_or_else(|| panic!("no delete-report step: {stdout}"));
    let record = lines
        .iter()
        .position(|l| l.contains("Pressure tests record"))
        .unwrap_or_else(|| panic!("no record step: {stdout}"));
    assert!(
        first_delete < record && record < last_delete,
        "record step is not between initial and final report cleanup: {stdout}"
    );
}

#[test]
fn builtin_backend_default_is_tmux() {
    assert_eq!(SessionBackendKind::default(), SessionBackendKind::Tmux);
}
