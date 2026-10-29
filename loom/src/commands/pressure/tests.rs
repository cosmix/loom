use super::*;
use crate::pressure::codex_run::{spawn_codex_with_fallback, CodexOutcome};
use crate::pressure::{completion_instruction, MarkerHome};
use std::{fs, path::PathBuf};
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
fn test_claude_marker_path_is_inside_repo_not_temp_dir() {
    let repo = Path::new("/repo");
    let marker = claude_marker_path(repo);
    // The agent's sandbox mounts /tmp read-only, so a temp-dir marker can never
    // be created and the session would never auto-close.
    assert!(
        !marker.starts_with(std::env::temp_dir()),
        "marker in temp dir: {}",
        marker.display()
    );
    assert!(
        marker.starts_with(repo.join(".loom").join("work").join("pressure")),
        "got: {}",
        marker.display()
    );
    assert_eq!(marker.extension().unwrap(), "done");
}

fn default_models() -> PressureModels {
    PressureModels::resolve(
        crate::cli::types_pressure::PressureModelFlags::default(),
        &crate::fs::work_dir::PressureConfig::default(),
        &crate::user_config::UserConfig::default(),
    )
}

#[test]
fn test_render_dry_run_shows_real_argv() {
    let report = PathBuf::from("/repo/doc/plans/codex-PLAN-foo.md");
    let repo = Path::new("/repo");
    let marker = PathBuf::from("/repo/.loom/work/pressure/claude-1.done");
    let codex_log = PathBuf::from("/tmp/loom-pressure-codex-1.log");
    let out = render_dry_run(
        1,
        "doc/plans/PLAN-foo.md",
        &report,
        repo,
        &marker,
        &codex_log,
        &default_models(),
    );
    assert!(out.contains("Dry run: 1 round"));
    assert!(out.ends_with("  5. write the ## Pressure tests record in doc/plans/PLAN-foo.md\n"));
    assert!(out.contains("--permission-mode auto --model opus --effort xhigh"));
    assert!(out.contains("--append-system-prompt"));
    assert!(out.contains("/pressure doc/plans/PLAN-foo.md"));
    assert!(out.contains(
        "codex exec --sandbox workspace-write -m gpt-6.1-sol -c model_reasoning_effort=xhigh \
         -C /repo $pressure doc/plans/PLAN-foo.md"
    ));
    assert!(out.contains("/address doc/plans/PLAN-foo.md"));
    assert!(out.contains("CLAUDE_CODE_EXPERIMENTAL_AGENT_TEAMS=1"));
    assert!(out.contains("codex-PLAN-foo.md"));
    // The parallel pressure step is labelled and the codex log path shown.
    assert!(out.contains("[parallel]"));
    assert!(out.contains("loom-pressure-codex-1.log"));
    assert!(out.contains(".loom/work/pressure/claude-1.done"));
    assert!(out.contains(
        "Models:                  claude=opus/xhigh  codex=gpt-6.1-sol (fallback: gpt-6-sol)/xhigh  address=opus/high"
    ));
}

#[test]
fn test_render_dry_run_shows_a_non_default_model_selection_per_step() {
    // A non-default, all-distinct selection must reach the right step's argv -
    // this is the whole point of three independently-resolved slots.
    let report = PathBuf::from("/repo/doc/plans/codex-PLAN-foo.md");
    let repo = Path::new("/repo");
    let marker = PathBuf::from("/repo/.loom/work/pressure/claude-1.done");
    let codex_log = PathBuf::from("/tmp/loom-pressure-codex-1.log");
    let models = PressureModels {
        claude: "fable".to_string(),
        claude_effort: "low".to_string(),
        codex: "gpt-6-astra".to_string(),
        codex_effort: "medium".to_string(),
        address: "sonnet".to_string(),
        address_effort: "max".to_string(),
    };
    let out = render_dry_run(
        1,
        "doc/plans/PLAN-foo.md",
        &report,
        repo,
        &marker,
        &codex_log,
        &models,
    );
    // The header shows model AND effort for all three slots.
    assert!(out.contains(
        "Models:                  claude=fable/low  codex=gpt-6-astra/medium  address=sonnet/max"
    ));
    assert!(out.contains("--permission-mode auto --model fable --effort low"));
    assert!(out.contains("-m gpt-6-astra"));
    assert!(out.contains("-c model_reasoning_effort=medium"));
    assert!(out.contains("--model sonnet --effort max --append-system-prompt"));
}

#[test]
fn test_claude_args_shape() {
    let marker = PathBuf::from("/repo/.loom/work/pressure/claude-1.done");
    let args = claude_args("/pressure doc/plans/PLAN-foo.md", &marker, "opus", "xhigh");
    assert!(!args.iter().any(|a| a == "-p" || a == "--print"));
    assert_eq!(args[0], "--permission-mode");
    assert_eq!(args[1], "auto");
    assert_eq!(args[2], "--model");
    assert_eq!(args[3], "opus");
    assert_eq!(args[4], "--effort");
    assert_eq!(args[5], "xhigh");
    // The slash invocation is the final positional argument.
    assert_eq!(args.last().unwrap(), "/pressure doc/plans/PLAN-foo.md");
    // The appended system prompt names the marker so the driver can auto-close.
    let joined = args.join(" ");
    assert!(joined.contains("--append-system-prompt"));
    assert!(joined.contains("/repo/.loom/work/pressure/claude-1.done"));
}

/// Fake codex for the runner tests: accepts only `gpt-9-good`, fails
/// `gpt-9-broken` with an unrelated error, and reports every other `-m`
/// model as not supported, the way the ChatGPT-account API does.
const FAKE_CODEX: &str = r#"for arg in "$@"; do
  [ "$prev" = "-m" ] && model="$arg"
  prev="$arg"
done
case "$model" in
  gpt-9-good) exit 0 ;;
  gpt-9-broken) echo "stream error talking to $model"; exit 3 ;;
  gpt-9-quoted) echo "the '$model' model is not supported (quoted output)"; exit 1 ;;
esac
echo "ERROR: The '$model' model is not supported when using Codex with a ChatGPT account."
exit 1
"#;

/// `/bin/sh` reads `exec` in the repo dir, avoiding ETXTBSY; each attempt exits
/// immediately, and `join` waits for the runner before its temp dir is removed.
fn run_fake_codex(candidates: &[&str]) -> CodexOutcome {
    let repo = TempDir::new().unwrap();
    fs::write(repo.path().join("exec"), FAKE_CODEX).unwrap();
    let log = repo.path().join("codex.log");
    let run = spawn_codex_with_fallback(
        Path::new("/bin/sh"),
        repo.path(),
        "$pressure doc/plans/PLAN-foo.md",
        &log,
        candidates.iter().map(|model| model.to_string()).collect(),
        "xhigh",
    )
    .unwrap();
    run.join().unwrap()
}

#[test]
fn test_codex_fallback_moves_on_when_the_model_is_unsupported() {
    let outcome = run_fake_codex(&["gpt-9-gone", "gpt-9-good"]);
    assert!(outcome.status.success());
    assert_eq!(outcome.model_used, "gpt-9-good");
    assert_eq!(outcome.tried, ["gpt-9-gone", "gpt-9-good"]);
    assert!(!outcome.exhausted);
    assert_eq!(outcome.notes.len(), 1);
    // The note carries the failed attempt's error: its log was re-created.
    assert!(outcome.notes[0].starts_with(
        "codex model gpt-9-gone is unavailable to this account — fell back to gpt-9-good\n"
    ));
    assert!(outcome.notes[0].contains("The 'gpt-9-gone' model is not supported"));
}

#[test]
fn test_codex_fallback_exhausted_keeps_the_last_failure() {
    let outcome = run_fake_codex(&["gpt-9-gone", "gpt-9-also-gone"]);
    assert_eq!(outcome.status.code(), Some(1));
    assert_eq!(outcome.model_used, "gpt-9-also-gone");
    assert_eq!(outcome.tried, ["gpt-9-gone", "gpt-9-also-gone"]);
    assert!(outcome.exhausted);
    assert_eq!(outcome.notes.len(), 1);
}

#[test]
fn test_codex_fallback_walks_three_candidates() {
    let outcome = run_fake_codex(&["gpt-9-gone", "gpt-9-also-gone", "gpt-9-good"]);
    assert!(outcome.status.success());
    assert_eq!(outcome.model_used, "gpt-9-good");
    assert_eq!(
        outcome.tried,
        ["gpt-9-gone", "gpt-9-also-gone", "gpt-9-good"]
    );
    assert!(!outcome.exhausted);
    assert_eq!(outcome.notes.len(), 2);
}

#[test]
fn test_codex_fallback_ignores_unprefixed_unavailable_text() {
    let outcome = run_fake_codex(&["gpt-9-quoted", "gpt-9-good"]);
    assert_eq!(outcome.status.code(), Some(1));
    assert_eq!(outcome.tried, ["gpt-9-quoted"]);
    assert!(!outcome.exhausted);
    assert!(outcome.notes.is_empty());
}

#[test]
fn test_codex_fallback_ignores_unrelated_failures() {
    let outcome = run_fake_codex(&["gpt-9-broken", "gpt-9-good"]);
    assert_eq!(outcome.status.code(), Some(3));
    assert_eq!(outcome.tried, ["gpt-9-broken"]);
    assert!(!outcome.exhausted);
    assert!(outcome.notes.is_empty());
}

#[test]
fn test_codex_args_shape() {
    let repo = Path::new("/repo");
    let args = codex_args(repo, "$pressure doc/plans/PLAN-foo.md", "gpt-6-sol", "high");
    assert_eq!(args[0], "exec");
    assert!(args.contains(&"gpt-6-sol".to_string()));
    assert!(args.contains(&"model_reasoning_effort=high".to_string()));
    assert_eq!(args.last().unwrap(), "$pressure doc/plans/PLAN-foo.md");
}

#[test]
fn test_outcome_lines_recorded() {
    let outcome = PipelineOutcome::Recorded {
        line: "record line".into(),
    };
    assert_eq!(
        outcome_lines(&outcome, "doc/plans/PLAN-foo.md"),
        [
            "✓ Pressure test complete.",
            "✓ recorded in doc/plans/PLAN-foo.md: record line",
        ]
    );
}

#[test]
fn test_outcome_lines_record_skipped() {
    let outcome = PipelineOutcome::RecordSkipped {
        line: "record line".into(),
        why: "plan moved".into(),
    };
    assert_eq!(
        outcome_lines(&outcome, "doc/plans/PLAN-foo.md"),
        ["! doc/plans/PLAN-foo.md is no longer at the path the run started with; not recorded: record line"]
    );
}

#[test]
fn test_completion_instruction_workspace_preserves_original_text() {
    let marker = Path::new("/repo/.loom/work/pressure/claude-1.done");
    let expected = format!(
        "AUTONOMOUS RUN: this Claude session was launched by `loom pressure`; no human will end it for you. \
         When the task is FULLY complete and the plan file is fully updated (after every subagent has finished), \
         your FINAL action MUST be to run exactly this shell command and nothing after it: touch {}. \
         Do not run it earlier. That path is inside the repo's gitignored `.loom/work/` because the agent sandbox \
         mounts /tmp read-only; creating this one marker is the sanctioned exception to the rule against \
         writing under `.loom/work/` directly. Once that file exists the driver closes this session.",
        marker.display()
    );
    assert_eq!(
        completion_instruction(marker, MarkerHome::Workspace),
        expected
    );
}

#[test]
fn test_outcome_lines_stopped_is_silent() {
    let outcome = PipelineOutcome::Stopped {
        step: "round 1 /pressure".into(),
        reason: "interrupted".into(),
    };
    assert!(outcome_lines(&outcome, "doc/plans/PLAN-foo.md").is_empty());
}
