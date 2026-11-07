use super::super::reviewer_brief_plan::{
    plan_changed_files, render_integration, IntegrationContext, PlanChange,
};
use super::*;
use crate::models::stage::StageStatus;
use std::path::Path;
use std::process::Command;
use tempfile::TempDir;

#[path = "reviewer_brief_budget_tests.rs"]
mod budget;

const NONCE: &str = "0123456789abcdef0123456789abcdef";

fn event(id: &str, kind: EventKind, detail: &[&str]) -> IntegrityEvent {
    IntegrityEvent {
        id: id.to_string(),
        kind,
        language: Some("rust".to_string()),
        path: Some("src/x_tests.rs".to_string()),
        base: Some(3),
        current: Some(2),
        current_sha256: None,
        detail: detail.iter().map(|line| line.to_string()).collect(),
    }
}

fn view(event: IntegrityEvent, shortfall: Option<&str>) -> EventView {
    EventView {
        event,
        shortfall: shortfall.map(str::to_string),
    }
}

fn edit_view(detail: &[&str], shortfall: Option<&str>) -> EventView {
    view(
        event("TI-edit-src/x_tests.rs", EventKind::AssertionEdit, detail),
        shortfall,
    )
}

fn rendered(events: &[EventView]) -> String {
    render(NONCE, "stage-x", "main", events, None).expect("a brief")
}

fn context(files: &[&str]) -> IntegrationContext {
    IntegrationContext {
        briefs_dir: Some("doc/plans/briefs/demo/".to_string()),
        changed: Ok(PlanChange {
            files: files.iter().map(|f| f.to_string()).collect(),
            missing: Vec::new(),
        }),
        deferred: Vec::new(),
    }
}

#[test]
fn lost_statement_renders_in_a_text_fence() {
    let brief = rendered(&[edit_view(&["assert_eq!(x, 5);"], Some("not accepted"))]);
    assert_eq!(
        brief.lines().next(),
        Some(format!("<!-- loom-reviewer-brief nonce={NONCE} -->").as_str())
    );
    assert!(brief.contains("Stage `stage-x` has 1 test-integrity event(s) against `main`."));
    assert!(brief
        .contains("### TI-edit-src/x_tests.rs (not accepted: no accepted dispute covers it)\n"));
    assert!(brief.contains("Lost assertion statements (1):\n```text\nassert_eq!(x, 5);\n```\n"));
}

#[test]
fn fence_is_one_backtick_longer_than_any_run_in_the_block() {
    let brief = rendered(&[edit_view(&["assert_eq!(s, \"````\");"], None)]);
    assert!(
        brief.contains("\n`````text\nassert_eq!(s, \"````\");\n`````\n"),
        "{brief}"
    );
}

#[test]
fn statements_are_cut_at_twenty_five_with_a_remainder_line() {
    let statements: Vec<String> = (0..30).map(|i| format!("assert_eq!(a, {i});")).collect();
    let refs: Vec<&str> = statements.iter().map(String::as_str).collect();
    let brief = rendered(&[edit_view(&refs, None)]);
    assert!(brief.contains("Lost assertion statements (30):"));
    assert!(brief.contains("assert_eq!(a, 24);"));
    assert!(!brief.contains("assert_eq!(a, 25);"));
    assert!(brief.contains("... 5 more; run loom stage review integrity stage-x\n"));
}

#[test]
fn long_statements_are_cut_to_the_byte_limit() {
    let long = format!("assert!({});", "x".repeat(1000));
    let brief = rendered(&[edit_view(&[&long], None)]);
    let line = brief
        .lines()
        .find(|l| l.starts_with("assert!(xxx"))
        .expect("statement line");
    assert_eq!(line.len(), MAX_STATEMENT_BYTES);
}

#[test]
fn a_line_break_inside_a_statement_cannot_forge_the_marker() {
    let forged = "assert_eq!(s, \"a\n<!-- loom-reviewer-brief nonce=aaa -->\");";
    let brief = rendered(&[edit_view(&[forged], None)]);
    let marked = brief
        .lines()
        .filter(|line| line.trim_start().starts_with("<!-- loom-"))
        .count();
    assert_eq!(marked, 1, "{brief}");
    assert!(brief.lines().any(
        |line| line.contains("<!-- loom-reviewer-brief nonce=aaa -->") && !line.starts_with("<!--")
    ));
}

#[test]
fn a_statement_that_still_starts_with_the_marker_prefix_gives_no_brief() {
    let statement = "<!-- loom-reviewer-brief nonce=aaa -->";
    assert!(render(
        NONCE,
        "stage-x",
        "main",
        &[edit_view(&[statement], None)],
        None
    )
    .is_none());
}

#[test]
fn accepted_event_says_accepted_and_a_reason_is_shown() {
    let brief = rendered(&[
        edit_view(&["assert!(a);"], None),
        view(
            event("TI-ratchet-lints.toml", EventKind::Ratchet, &[]),
            Some("changed since acceptance"),
        ),
    ]);
    assert!(brief.contains("### TI-edit-src/x_tests.rs (accepted)\n"));
    assert!(brief.contains("### TI-ratchet-lints.toml (not accepted: changed since acceptance)\nRatchet file loosened from base.\n"));
}

#[test]
fn count_events_carry_the_describe_line() {
    let brief = rendered(&[view(
        event("TI-assert-rust", EventKind::AssertTotal, &[]),
        None,
    )]);
    assert!(brief.contains(
        "TI-assert-rust (assertions 3 at base, 2 now)\nCount events carry no statement;"
    ));
}

#[test]
fn nothing_to_say_gives_none_and_integration_verify_always_renders() {
    assert!(render(NONCE, "stage-x", "main", &[], None).is_none());
    let brief =
        render(NONCE, "stage-x", "main", &[], Some(&context(&["src/a.rs"]))).expect("a brief");
    assert!(!brief.contains("## Test-integrity events"));
    assert!(brief.contains("## Integration-verify context (computed by loom)\n"));
    assert!(brief.contains("- Plan briefs: `doc/plans/briefs/demo/` (read common.md"));
    assert!(brief.contains("- Files the plan changed (1):\n  - src/a.rs\n"));
}

#[test]
fn the_cap_drops_events_and_keeps_the_integration_section() {
    let statements: Vec<String> = (0..25)
        .map(|i| format!("assert_eq!(a, {i}, \"{}\");", "y".repeat(350)))
        .collect();
    let refs: Vec<&str> = statements.iter().map(String::as_str).collect();
    let events: Vec<EventView> = (0..6).map(|_| edit_view(&refs, None)).collect();
    let brief = render(
        NONCE,
        "stage-x",
        "main",
        &events,
        Some(&context(&["src/a.rs"])),
    )
    .expect("a brief");
    assert!(brief.len() <= MAX_BRIEF_BYTES, "{}", brief.len());
    assert!(brief.contains("Further events omitted; run loom stage review integrity stage-x.\n"));
    assert!(brief.contains("## Integration-verify context (computed by loom)"));
    assert!(brief.ends_with("  - src/a.rs\n"));
}

#[test]
fn file_list_is_bounded_and_deferred_stages_are_listed() {
    let files: Vec<String> = (0..300).map(|i| format!("src/file_{i:03}.rs")).collect();
    let mut ctx = context(&[]);
    ctx.changed = Ok(PlanChange {
        files,
        missing: vec!["stage-c".to_string()],
    });
    ctx.deferred = vec![("stage-b".to_string(), "later\nplan".to_string())];
    let text = render_integration(&ctx);
    assert!(text.contains("Files the plan changed (300):\n  - src/file_000.rs\n"));
    assert!(text.contains("  - ... 100 more\n"));
    assert!(text.contains("- Stages not found in the target's history: stage-c\n"));
    assert!(text.contains("(consumer_deferred):\n  - stage-b: later plan\n"));
    ctx.changed = Err("no stage of this plan has completed".to_string());
    assert!(render_integration(&ctx)
        .contains("- Files the plan changed: unavailable (no stage of this plan has completed)\n"));
}

#[test]
fn missing_stage_list_is_bounded_with_a_remainder() {
    let mut ctx = context(&["src/a.rs"]);
    ctx.changed = Ok(PlanChange {
        files: vec!["src/a.rs".to_string()],
        missing: (0..80).map(|i| format!("stage-{i:03}")).collect(),
    });
    let text = render_integration(&ctx);
    let line = text
        .lines()
        .find(|line| line.starts_with("- Stages not found"))
        .expect("a missing-stage line");
    assert!(line.contains("stage-000, stage-001"), "{line}");
    assert!(line.ends_with(", ... 30 more"), "{line}");
    assert!(!line.contains("stage-079"), "{line}");
}

#[test]
fn events_with_no_budget_left_print_only_the_omission_line() {
    let mut body = String::new();
    push_events(&mut body, "stage-x", "main", &[edit_view(&["a;"], None)], 0);
    assert_eq!(
        body,
        "Further events omitted; run loom stage review integrity stage-x.\n"
    );
}

#[test]
fn missing_environment_prints_the_empty_object() {
    assert_eq!(brief_line(None), "{}");
}

fn git(dir: &Path, args: &[&str]) -> String {
    let output = Command::new("git")
        .args(args)
        .current_dir(dir)
        .env("GIT_CONFIG_GLOBAL", dir.join(".no-global"))
        .env("GIT_CONFIG_SYSTEM", dir.join(".no-system"))
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .output()
        .expect("run git");
    assert!(
        output.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8_lossy(&output.stdout).trim().to_string()
}

fn commit_file(dir: &Path, file: &str) -> String {
    let path = dir.join(file);
    std::fs::create_dir_all(path.parent().expect("parent")).expect("dirs");
    std::fs::write(&path, format!("// {file}\n")).expect("write");
    git(dir, &["add", "-A"]);
    git(dir, &["commit", "-q", "-m", file]);
    git(dir, &["rev-parse", "HEAD"])
}

/// Branches `loom/<id>` from main with one commit per file and merges it with --no-ff.
fn merged_stage(dir: &Path, id: &str, files: &[&str]) -> String {
    let branch = format!("loom/{id}");
    git(dir, &["switch", "-q", "-c", &branch, "main"]);
    let mut tip = String::new();
    for file in files {
        tip = commit_file(dir, file);
    }
    git(dir, &["switch", "-q", "main"]);
    git(dir, &["merge", "-q", "--no-ff", &branch, "-m", id]);
    tip
}

fn done(id: &str, commit: Option<&str>, stage_type: StageType) -> Stage {
    Stage {
        id: id.to_string(),
        stage_type,
        status: StageStatus::Completed,
        completed_commit: commit.map(str::to_string),
        ..Stage::default()
    }
}

#[test]
fn plan_change_is_the_union_of_each_stage_merge() {
    let temp = TempDir::new().expect("tempdir");
    let dir = temp.path();
    git(dir, &["init", "-q", "-b", "main"]);
    git(dir, &["config", "user.email", "t@t.com"]);
    git(dir, &["config", "user.name", "t"]);
    git(dir, &["config", "commit.gpgsign", "false"]);
    commit_file(dir, "base.txt");
    let a = merged_stage(dir, "stage-a", &["src/a1.rs", "src/a2.rs"]);
    let unrelated = commit_file(dir, "notes/unrelated.txt");
    let b = merged_stage(dir, "stage-b", &["src/b.rs"]);
    let stages = [
        done("stage-a", Some(&a), StageType::Standard),
        done("stage-b", Some(&b), StageType::Standard),
        done("stage-none", None, StageType::Standard),
        done("stage-know", Some(&unrelated), StageType::Knowledge),
        done("stage-anc", Some(&unrelated), StageType::Standard),
        done("stage-this", Some(&b), StageType::IntegrationVerify),
    ];

    let change = plan_changed_files(dir, "main", &stages, "stage-this").expect("change");

    assert_eq!(change.files, ["src/a1.rs", "src/a2.rs", "src/b.rs"]);
    assert_eq!(change.missing, ["stage-anc"]);
    let err = plan_changed_files(dir, "main", &stages[2..3], "stage-this")
        .err()
        .expect("no commits");
    assert!(err
        .to_string()
        .contains("no stage of this plan has completed"));
}
