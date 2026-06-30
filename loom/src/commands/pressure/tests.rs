use super::*;
use std::fs;
use tempfile::TempDir;

fn canonical(dir: &TempDir) -> PathBuf {
    dir.path().canonicalize().unwrap()
}

#[test]
fn test_resolve_plan_path_direct_relative() {
    let temp = TempDir::new().unwrap();
    let root = canonical(&temp);
    let plans = root.join("doc/plans");
    fs::create_dir_all(&plans).unwrap();
    fs::write(plans.join("PLAN-foo.md"), "# plan").unwrap();

    let resolved = resolve_plan_path("doc/plans/PLAN-foo.md", &root).unwrap();
    assert_eq!(resolved.invocation, "doc/plans/PLAN-foo.md");
    assert_eq!(resolved.fs_path, plans.join("PLAN-foo.md"));
}

#[test]
fn test_resolve_plan_path_falls_back_to_doc_plans() {
    let temp = TempDir::new().unwrap();
    let root = canonical(&temp);
    let plans = root.join("doc/plans");
    fs::create_dir_all(&plans).unwrap();
    fs::write(plans.join("PLAN-bar.md"), "# plan").unwrap();

    // Bare filename: not present at root, present under doc/plans.
    let resolved = resolve_plan_path("PLAN-bar.md", &root).unwrap();
    assert_eq!(resolved.invocation, "doc/plans/PLAN-bar.md");
}

#[test]
fn test_resolve_plan_path_no_double_prefix() {
    let temp = TempDir::new().unwrap();
    let root = canonical(&temp);
    fs::create_dir_all(root.join("doc/plans")).unwrap();
    // arg starts with doc/plans/ but the file is absent → must NOT try
    // doc/plans/doc/plans/..., it should bail outright.
    let err = resolve_plan_path("doc/plans/missing.md", &root).unwrap_err();
    let msg = err.to_string();
    assert!(msg.contains("plan file not found"), "got: {msg}");
    assert!(
        !msg.contains("doc/plans/doc/plans"),
        "double-prefixed: {msg}"
    );
}

#[test]
fn test_resolve_plan_path_absolute_outside_repo() {
    let repo = TempDir::new().unwrap();
    let other = TempDir::new().unwrap();
    let root = canonical(&repo);
    let plan = canonical(&other).join("PLAN-ext.md");
    fs::write(&plan, "# plan").unwrap();

    let resolved = resolve_plan_path(plan.to_str().unwrap(), &root).unwrap();
    // Outside the repo → invocation is the absolute path, never cwd-relative.
    assert_eq!(resolved.invocation, plan.to_string_lossy());
    assert!(Path::new(&resolved.invocation).is_absolute());
}

#[test]
fn test_resolve_plan_path_rejects_directory() {
    let temp = TempDir::new().unwrap();
    let root = canonical(&temp);
    // A directory that happens to be named like a plan must not be accepted.
    fs::create_dir_all(root.join("doc/plans/PLAN-dir.md")).unwrap();
    let err = resolve_plan_path("doc/plans/PLAN-dir.md", &root).unwrap_err();
    assert!(
        err.to_string().contains("plan file not found"),
        "got: {err}"
    );
}

#[test]
fn test_codex_report_path_is_sibling() {
    let report = codex_report_path(Path::new("/repo/doc/plans/PLAN-foo.md"));
    assert_eq!(report, PathBuf::from("/repo/doc/plans/codex-PLAN-foo.md"));
}

#[test]
fn test_plan_steps_order_and_count() {
    let report = PathBuf::from("/repo/doc/plans/codex-PLAN-foo.md");
    let steps = plan_steps(2, "doc/plans/PLAN-foo.md", &report);
    // 4 steps per round + 1 final cleanup delete.
    assert_eq!(steps.len(), 2 * 4 + 1);
    assert_eq!(steps[0], Step::DeleteReport(report.clone()));
    assert_eq!(
        steps[1],
        Step::Claude("/pressure doc/plans/PLAN-foo.md".into())
    );
    assert_eq!(
        steps[2],
        Step::Codex("$pressure doc/plans/PLAN-foo.md".into())
    );
    assert_eq!(
        steps[3],
        Step::Claude("/address doc/plans/PLAN-foo.md".into())
    );
    // Per-round delete must precede that round's /pressure.
    assert_eq!(steps[4], Step::DeleteReport(report.clone()));
    assert_eq!(*steps.last().unwrap(), Step::DeleteReport(report));
}

#[test]
fn test_plan_steps_single_round() {
    let report = PathBuf::from("/r/codex-P.md");
    let steps = plan_steps(1, "P.md", &report);
    assert_eq!(steps.len(), 5);
}

#[test]
fn test_render_dry_run_shows_real_argv() {
    let report = PathBuf::from("/repo/doc/plans/codex-PLAN-foo.md");
    let repo = Path::new("/repo");
    let out = render_dry_run(1, "doc/plans/PLAN-foo.md", &report, repo);
    assert!(out.contains("Dry run: 1 round"));
    // The preview must show the REAL argv so it matches what spawns.
    assert!(
        out.contains("--permission-mode acceptEdits --model opus /pressure doc/plans/PLAN-foo.md")
    );
    assert!(out
        .contains("codex exec --sandbox workspace-write -C /repo $pressure doc/plans/PLAN-foo.md"));
    assert!(out.contains("/address doc/plans/PLAN-foo.md"));
    assert!(out.contains("CLAUDE_CODE_EXPERIMENTAL_AGENT_TEAMS=1"));
    assert!(out.contains("codex-PLAN-foo.md"));
}

#[test]
fn test_classify_code_all_arms() {
    assert_eq!(classify_code(Some(0)), ExitAction::Continue);
    assert_eq!(classify_code(Some(130)), ExitAction::Abort);
    assert_eq!(classify_code(Some(2)), ExitAction::Abort);
    assert_eq!(classify_code(None), ExitAction::Abort); // signal-killed
    assert_eq!(classify_code(Some(1)), ExitAction::Warn);
    assert_eq!(classify_code(Some(42)), ExitAction::Warn);
}
