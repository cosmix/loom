//! Cross-stage regression contracts for token-optimization invariants.

#[path = "integration/helpers.rs"]
// only loom_cmd() is used; the shared module serves the integration target
#[allow(dead_code)]
mod helpers;

use std::fs;
use std::path::Path;
use std::time::Duration;

use loom::fs::work_dir::ContextConfig;
use loom::models::stage::{AcceptanceCriterion, Stage, TruthCheck};
use loom::orchestrator::monitor::ContextHealth;
use loom::parser::frontmatter::extract_yaml_frontmatter;
use loom::plan::parser::parse_plan_content;
use loom::quota::quota_health;
use loom::verify::{run_acceptance_with_config, CachePolicy, CriteriaConfig};
use serde_json::{json, Value};
use tempfile::{Builder, TempDir};

fn scratch() -> TempDir {
    Builder::new()
        .prefix("loom-token-optimization-contracts-")
        .tempdir_in(std::env::temp_dir())
        .expect("create unique scratch directory")
}

/// Builds a stage whose truth check can require an empty stderr stream.
fn stage_with_truth(command: &str, contains: &[&str], require_empty_stderr: bool) -> Stage {
    let mut stage = Stage::new("token-contract".to_string(), None);
    stage.add_acceptance_criterion(AcceptanceCriterion::Extended(TruthCheck {
        command: command.to_string(),
        stdout_contains: contains.iter().map(|value| (*value).to_string()).collect(),
        stdout_not_contains: vec!["forbidden".to_string()],
        stderr_empty: require_empty_stderr.then_some(true),
        exit_code: Some(0),
        description: None,
    }));
    stage
}

fn cached_config(cache: &Path) -> CriteriaConfig {
    CriteriaConfig::with_timeout(Duration::from_secs(2))
        .with_cache_dir(cache)
        .with_cache_policy(CachePolicy::Use)
}

fn provider<'a>(report: &'a Value, name: &str) -> &'a Value {
    report["provider_ledger"]["providers"]
        .as_array()
        .expect("provider reports")
        .iter()
        .find(|provider| provider["provider"] == name)
        .expect("selected provider report")
}

fn receipt(provider: &str, input: u64, cached: Option<u64>, output: u64) -> Value {
    json!({
        "schema_version": 1,
        "provider": provider,
        "observed_at": "2026-09-12T20:00:00Z",
        "request_id": format!("{provider}-request"),
        "usage": {
            "input_tokens": input,
            "cache_read_input_tokens": cached,
            "output_tokens": output,
        },
    })
}

fn usage_report_with_provider_receipts() -> Value {
    let scratch = scratch();
    let receipts = scratch.path().join("receipts");
    let project = scratch.path().join("project");
    let claude_root = scratch.path().join("claude");
    let codex_root = scratch.path().join("codex");
    for directory in [&receipts, &project, &claude_root, &codex_root] {
        fs::create_dir_all(directory).expect("create usage fixture directory");
    }
    fs::write(
        receipts.join("claude.json"),
        receipt("claude", 11, None, 3).to_string(),
    )
    .expect("write Claude receipt");
    fs::write(
        receipts.join("codex.json"),
        receipt("codex", 30, Some(5), 7).to_string(),
    )
    .expect("write Codex receipt");

    let output = helpers::loom_cmd()
        .args([
            "usage",
            "--provider",
            "all",
            "--json",
            "--since",
            "2026-09-12",
        ])
        .args(["--project"])
        .arg(&project)
        .args(["--claude-root"])
        .arg(&claude_root)
        .args(["--codex-root"])
        .arg(&codex_root)
        .args(["--receipts-root"])
        .arg(&receipts)
        .output()
        .expect("run loom usage");
    assert!(
        output.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).expect("usage JSON")
}

#[test]
fn context_defaults_keep_800k_ceilings_and_80_100_feedback() {
    let config = ContextConfig::default();

    assert_eq!(
        (config.ceiling_tokens, config.subagent_ceiling_tokens),
        (800_000, 800_000)
    );
    assert_eq!(config.ceiling_for(Some(700_000)), 700_000);
    assert_eq!(quota_health(80.0), ContextHealth::Yellow);
    assert_eq!(quota_health(100.0), ContextHealth::Red);
    assert_eq!(quota_health(59.0), ContextHealth::Green);
}

fn assert_max_turns_at_or_above_floor(
    mapping: &serde_yaml::Mapping,
    path: &Path,
    floor: u64,
) -> bool {
    let max_turns = serde_yaml::Value::String("maxTurns".to_string());
    let Some(value) = mapping.get(&max_turns) else {
        return false;
    };
    let value = value
        .as_u64()
        .unwrap_or_else(|| panic!("{} maxTurns must be an integer", path.display()));
    assert!(
        value >= floor,
        "{} maxTurns must be >= {floor}",
        path.display()
    );
    true
}

#[test]
fn agent_frontmatter_keeps_max_turns_at_or_above_current_floor() {
    // Agent definitions carry this value today.
    const MAX_TURNS_FLOOR: u64 = 150;
    let required_agents = [
        "loom-code-reviewer.md",
        "loom-software-engineer.md",
        "loom-senior-software-engineer.md",
    ];
    let mut required_seen = [false; 3];
    let mut declarations = 0;
    let agents_dir = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("agents");

    for entry in fs::read_dir(&agents_dir).expect("read repository agent definitions") {
        let path = entry.expect("read agent definition entry").path();
        if path.extension().and_then(|extension| extension.to_str()) != Some("md") {
            continue;
        }
        let content = fs::read_to_string(&path).expect("read agent definition");
        let frontmatter = extract_yaml_frontmatter(&content).expect("parse agent YAML frontmatter");
        let mapping = frontmatter.as_mapping().expect("agent frontmatter mapping");
        if assert_max_turns_at_or_above_floor(mapping, &path, MAX_TURNS_FLOOR) {
            declarations += 1;
            if let Some(index) = path
                .file_name()
                .and_then(|name| name.to_str())
                .and_then(|name| {
                    required_agents
                        .iter()
                        .position(|required| *required == name)
                })
            {
                required_seen[index] = true;
            }
        }
    }

    assert!(declarations > 0, "at least one agent must declare maxTurns");
    for (name, seen) in required_agents.iter().zip(required_seen) {
        assert!(
            seen,
            "{} must declare maxTurns",
            agents_dir.join(name).display()
        );
    }
}

#[test]
fn plan_construction_preserves_briefs_review_wait_and_overview_opt_out() {
    let plan = parse_plan_content(
        r#"# PLAN: Token contracts

<!-- loom METADATA - Do not edit manually -->

```yaml
loom:
  version: 1
  stages:
    - id: integration-verify
      name: Integration verify
      description: Verify the complete feature independently.
      files: ["loom/src/verify/**", "loom/tests/token_optimization_contracts.rs"]
      working_dir: "."
      acceptance: ["cargo test --offline --locked --manifest-path loom/Cargo.toml --test token_optimization_contracts"]
      stage_type: integration-verify
      plan_overview: false
      subagent_timeout_secs: 3600
      code_review:
        dimensions: [security, architecture, functional]
        require_all: true
```

<!-- END loom METADATA -->
"#,
        Path::new("token-contracts.md"),
    )
    .expect("parse explicit integration-verify contract");
    let definition = &plan.stages[0];
    let runtime = Stage::from_definition(definition, &plan.id);

    assert_eq!(runtime.files, definition.files);
    assert_eq!(runtime.plan_overview, Some(false));
    assert_eq!(runtime.effective_subagent_timeout_secs(), 3600);
    let review = runtime.code_review.expect("independent review");
    assert_eq!(
        review.dimensions,
        vec!["security", "architecture", "functional"]
    );
    assert!(review.require_all);
}

#[test]
fn acceptance_rejects_forbidden_stdout_cold_and_warm() {
    let repo = helpers::init_test_repo();
    let cache = scratch();
    let stage = stage_with_truth("printf forbidden", &[], false);
    let config = cached_config(cache.path());

    let cold = run_acceptance_with_config(&stage, Some(repo.path()), &config).unwrap();
    let warm = run_acceptance_with_config(&stage, Some(repo.path()), &config).unwrap();

    assert!(!cold.all_passed() && !warm.all_passed());
    assert!(!cold.results()[0].cached && !warm.results()[0].cached);
    assert!(cold.results()[0].stdout.contains("forbidden"));
    assert!(warm.failures()[0].contains("forbidden pattern"));
}

#[test]
fn acceptance_keeps_positive_stdout_and_empty_stderr_contracts_truthful() {
    let repo = helpers::init_test_repo();
    let cache = scratch();
    let config = cached_config(cache.path());
    let positive = stage_with_truth("printf expected", &["expected"], true);
    let stderr = stage_with_truth(
        "printf expected; printf diagnostic >&2",
        &["expected"],
        true,
    );

    let success = run_acceptance_with_config(&positive, Some(repo.path()), &config).unwrap();
    let failure = run_acceptance_with_config(&stderr, Some(repo.path()), &config).unwrap();

    assert!(success.all_passed());
    assert_eq!(success.results()[0].stdout, "expected");
    assert!(!failure.all_passed());
    assert!(failure.failures()[0].contains("stderr was not empty"));
}

#[test]
fn acceptance_cache_misses_when_context_changes_after_a_pass() {
    let repo = helpers::init_test_repo();
    let cache = scratch();
    let context = repo.path().join("context.txt");
    fs::write(&context, "alpha\n").unwrap();
    let stage = stage_with_truth("test -f context.txt", &[], false);
    let config = cached_config(cache.path());

    let first = run_acceptance_with_config(&stage, Some(repo.path()), &config).unwrap();
    let hit = run_acceptance_with_config(&stage, Some(repo.path()), &config).unwrap();
    fs::write(&context, "bravo\n").unwrap();
    let changed = run_acceptance_with_config(&stage, Some(repo.path()), &config).unwrap();

    assert!(first.all_passed() && hit.all_passed() && changed.all_passed());
    assert!(!first.results()[0].cached && hit.results()[0].cached);
    assert!(
        !changed.results()[0].cached,
        "changed context must invalidate a pass"
    );
}

#[test]
fn usage_report_keeps_provider_token_counters_separate() {
    let report = usage_report_with_provider_receipts();
    let claude = provider(&report, "claude");
    let codex = provider(&report, "codex");

    assert_eq!(claude["totals"]["fresh_input"]["value"], 11);
    assert_eq!(codex["totals"]["fresh_input"]["value"], 25);
    assert_eq!(codex["totals"]["cache_read"]["value"], 5);
}

#[test]
fn usage_report_marks_unavailable_quota_history_without_points() {
    let report = usage_report_with_provider_receipts();

    for quota in report["provider_ledger"]["quota_history"]["providers"]
        .as_array()
        .expect("quota providers")
    {
        assert_eq!(quota["source"], "unavailable");
        assert!(quota["points"].as_array().expect("quota points").is_empty());
    }
}
