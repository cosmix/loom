//! Fixture builders for the usage-evidence contracts: comparison artifacts,
//! producer projects (stage records, a review round, Claude and Codex rows)
//! and quota history, all synthetic.

use chrono::{DateTime, Utc};
use loom::models::forward_receipt::{
    receipts_path, ForwardBackend, ForwardIdentity, ForwardObservation, ForwardState,
    FORWARD_RECEIPT_SCHEMA,
};
use loom::models::stage::{Stage, StageStatus};
use loom::verify::review::report::Finding;
use loom::verify::review::store::{write_round, RecordedFinding, ReviewRound};
use loom::verify::transitions::create_stage;
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Output;
use tempfile::TempDir;

use crate::helpers;

pub const SINCE: &str = "2026-10-14T00:00:00Z";
pub const UNTIL: &str = "2026-10-15T00:00:00Z";
const VALID_IMPROVEMENT: &str =
    "tests/fixtures/token_optimization/comparison/valid_improvement.json";

/// A scratch root holding HOME/LOOM_HOME config, used as the current dir.
pub struct Scratch {
    pub dir: TempDir,
}

impl Scratch {
    pub fn new() -> Self {
        let dir = TempDir::new().expect("create scratch root");
        fs::write(dir.path().join("config.toml"), "[update]\ncheck = false\n").unwrap();
        Self { dir }
    }

    pub fn path(&self, name: &str) -> PathBuf {
        let path = self.dir.path().join(name);
        fs::create_dir_all(&path).unwrap();
        path
    }

    /// A project directory holding `.loom/work/`.
    pub fn project(&self, name: &str) -> PathBuf {
        let project = self.path(name);
        fs::create_dir_all(project.join(".loom/work")).unwrap();
        project
    }

    /// Runs `loom <args>` with the scratch root as HOME, LOOM_HOME and cwd.
    pub fn run(&self, args: &[&str]) -> Output {
        let mut command = helpers::loom_cmd();
        helpers::clear_relay_env(&mut command);
        command.args(args);
        command
            .env("HOME", self.dir.path())
            .env("LOOM_HOME", self.dir.path())
            .current_dir(self.dir.path());
        command.output().expect("execute loom")
    }

    /// Writes `artifact` and runs `usage --compare <file> --json`.
    pub fn compare(&self, artifact: &Value) -> Output {
        let path = self.dir.path().join("artifact.json");
        fs::write(&path, serde_json::to_vec_pretty(artifact).unwrap()).unwrap();
        self.run(&["usage", "--compare", path_str(&path), "--json"])
    }

    /// Writes `declaration` and runs the RunEvidence producer over `project`.
    pub fn run_evidence(
        &self,
        declaration: &Value,
        project: &Path,
        claude: &Path,
        codex: &Path,
    ) -> Output {
        let name = format!("decl-{}.json", declaration["run_id"].as_str().unwrap());
        let path = self.dir.path().join(name);
        fs::write(&path, serde_json::to_vec_pretty(declaration).unwrap()).unwrap();
        self.run(&[
            "usage",
            "--run-evidence",
            path_str(&path),
            "--project",
            path_str(project),
            "--since",
            SINCE,
            "--until",
            UNTIL,
            "--claude-root",
            path_str(claude),
            "--codex-root",
            path_str(codex),
        ])
    }
}

pub fn path_str(path: &Path) -> &str {
    path.to_str().expect("utf-8 path")
}

/// Decoded stdout; panics with stderr when stdout is not JSON.
pub fn stdout_json(output: &Output) -> Value {
    serde_json::from_slice(&output.stdout).unwrap_or_else(|error| {
        panic!(
            "stdout must be JSON: {error}; status={:?} stderr={}",
            output.status.code(),
            String::from_utf8_lossy(&output.stderr)
        )
    })
}

pub fn has_code(codes: &Value, code: &str) -> bool {
    codes
        .as_array()
        .is_some_and(|codes| codes.iter().any(|value| value == code))
}

/// The Claude project slug: the canonical path with '/' and '.' as '-'.
pub fn slug(project: &Path) -> String {
    fs::canonicalize(project)
        .unwrap()
        .to_string_lossy()
        .replace(['/', '.'], "-")
}

/// Writes `lines`, each followed by a newline, creating parent directories.
pub fn write_lines(path: &Path, lines: &[String]) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    let body: String = lines.iter().map(|line| format!("{line}\n")).collect();
    fs::write(path, body).unwrap();
}

/// `valid_improvement.json`: one pair, baseline and candidate on one reset.
pub fn base_artifact() -> Value {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join(VALID_IMPROVEMENT);
    serde_json::from_slice(&fs::read(path).unwrap()).unwrap()
}

/// The fixture's pair with distinct work_unit_id and run_id values.
pub fn numbered_pair(artifact: &Value, index: usize) -> Value {
    let mut pair = artifact["pairs"][0].clone();
    pair["work_unit_id"] = json!(format!("unit-{index}"));
    pair["baseline"]["run_id"] = json!(format!("base-{index}"));
    pair["candidate"]["run_id"] = json!(format!("cand-{index}"));
    pair
}

/// `artifact` with `pairs` and `fields` merged into its intervention.
pub fn with_pairs(artifact: &Value, pairs: Vec<Value>, fields: Value) -> Value {
    let mut artifact = artifact.clone();
    artifact["pairs"] = Value::Array(pairs);
    for (key, value) in fields.as_object().unwrap() {
        artifact["intervention"][key] = value.clone();
    }
    artifact
}

/// A Claude-only run declaration in the shape of the Producer runs bullet.
pub fn declaration(run_id: &str, revision: &str) -> Value {
    json!({
        "run_id": run_id,
        "workload_id": "workload-a",
        "fixture_id": "fixture-a",
        "source_input_revision": "source-v1",
        "source_input_digest": "sha256:input-a",
        "environment_id": "linux-x86-test",
        "acceptance_contract_digest": "sha256:contract-a",
        "implementation_revision": revision,
        "assignments": [{"provider": "claude", "model": "claude-sonnet-5-5", "effort": "medium"}],
        "accepted": true,
        "required_checks": [{"command_id": "gate", "contract_id": "c1", "verdict": "passed"}],
        "reviewed_dimensions": [{"dimension": "correctness", "independent": true, "verdict": "passed"}],
        "omitted_requirements": [],
        "semantic_completion": "passed",
        "quota_observations": null
    })
}

fn at(text: &str) -> DateTime<Utc> {
    text.parse().unwrap()
}

fn stage(id: &str, deps: &[&str], start: &str, end: &str, retries: u32, fixes: u32) -> Stage {
    Stage {
        id: id.to_string(),
        name: id.to_string(),
        status: StageStatus::Completed,
        dependencies: deps.iter().map(|dep| dep.to_string()).collect(),
        started_at: Some(at(start)),
        completed_at: Some(at(end)),
        retry_count: retries,
        fix_attempts: fixes,
        ..Stage::default()
    }
}

/// stage-a, stage-b (depends on stage-a) and stage-z outside the window.
fn write_stages(work_dir: &Path) {
    let stages = [
        stage(
            "stage-z",
            &[],
            "2026-10-13T06:00:00Z",
            "2026-10-13T06:10:00Z",
            5,
            4,
        ),
        stage(
            "stage-a",
            &[],
            "2026-10-14T06:00:00Z",
            "2026-10-14T06:10:00Z",
            1,
            2,
        ),
        stage(
            "stage-b",
            &["stage-a"],
            "2026-10-14T06:12:00Z",
            "2026-10-14T06:20:00Z",
            0,
            1,
        ),
    ];
    for stage in &stages {
        create_stage(stage, work_dir).expect("create stage record");
    }
}

/// One review round on stage-a with one unresolved `major` finding.
fn write_review(work_dir: &Path) {
    let finding = Finding {
        severity: "major".to_string(),
        file: "src/lib.rs".to_string(),
        line: 3,
        claim: "the parser drops the last field".to_string(),
        scenario: Some("a three-field row parses as two".to_string()),
        rule: None,
    };
    let round = ReviewRound {
        version: 1,
        round: 1,
        agent_id: "agent-r1".to_string(),
        harvested_at: at("2026-10-14T06:09:00Z"),
        fingerprint: format!("sha256:{}", "a".repeat(64)),
        files: BTreeMap::new(),
        malformed: None,
        findings: vec![RecordedFinding {
            id: "F-1-1".to_string(),
            finding,
        }],
        resolved: Vec::new(),
        unresolved: Vec::new(),
        suggestion_memory_ids: Vec::new(),
    };
    write_round(work_dir, "stage-a", &round).expect("write review round");
}

/// A main-session assistant row with usage.
fn assistant(ts: &str, output: u64) -> String {
    json!({
        "type": "assistant",
        "timestamp": ts,
        "sessionId": "s1",
        "message": {
            "id": "m1",
            "model": "claude-sonnet-5-5",
            "content": [],
            "usage": {
                "input_tokens": 10,
                "cache_creation_input_tokens": 0,
                "cache_read_input_tokens": 0,
                "output_tokens": output
            }
        }
    })
    .to_string()
}

/// Producer fixture project `name`: stages, review round, and one Claude row
/// under `<claude>/<slug>--worktrees-stage-a/s1.jsonl` with `output`.
pub fn producer_project(scratch: &Scratch, name: &str, claude: &Path, output: u64) -> PathBuf {
    let project = scratch.project(name);
    let work_dir = project.join(".loom/work");
    write_stages(&work_dir);
    write_review(&work_dir);
    let dir = claude.join(format!("{}--worktrees-stage-a", slug(&project)));
    write_lines(
        &dir.join("s1.jsonl"),
        &[assistant("2026-10-14T06:05:00Z", output)],
    );
    project
}

fn observation(
    identity: &ForwardIdentity,
    state: ForwardState,
    exit_code: Option<i32>,
) -> ForwardObservation {
    ForwardObservation {
        schema: FORWARD_RECEIPT_SCHEMA,
        receipt_id: identity.receipt_id(),
        parent_session_id: identity.parent_session_id.clone(),
        agent_id: identity.agent_id.clone(),
        tool_use_id: identity.tool_use_id.clone(),
        stage_id: identity.stage_id.clone(),
        loom_session_id: identity.loom_session_id.clone(),
        backend: ForwardBackend::Direct,
        backend_id: "thread-p".to_string(),
        state,
        observed_at: at("2026-10-14T06:06:00Z"),
        exit_code,
        codex_thread_id: Some("thread-p".to_string()),
        locator: None,
        model: Some("gpt-5.6-terra".to_string()),
        effort: Some("xhigh".to_string()),
    }
}

/// A forward receipt for thread-p on stage-a, and its rollout under `codex`
/// with one token_usage_record of output 9 inside the window.
pub fn codex_evidence(project: &Path, codex: &Path) {
    let identity =
        ForwardIdentity::new("parent-1", "fwd-0", "toolu_0", "stage-a", "session-a").unwrap();
    let lines: Vec<String> = [
        observation(&identity, ForwardState::Running, None),
        observation(&identity, ForwardState::Succeeded, Some(0)),
    ]
    .iter()
    .map(|line| line.encode_line().unwrap().trim_end().to_string())
    .collect();
    let receipts = receipts_path(&project.join(".loom/work"), "stage-a").unwrap();
    write_lines(&receipts, &lines);
    let meta = json!({
        "timestamp": "2026-10-14T06:05:30Z",
        "type": "session_meta",
        "payload": {"id": "thread-p", "cwd": "/clones/p"}
    });
    let record = json!({
        "timestamp": "2026-10-14T06:05:50Z",
        "type": "token_usage_record",
        "payload": {
            "response_id": "response-thread-p",
            "model": "gpt-5.6-terra",
            "usage": {
                "input_tokens": 100,
                "cached_input_tokens": 40,
                "output_tokens": 9,
                "reasoning_output_tokens": 0,
                "total_tokens": 109
            }
        }
    });
    let rollout = codex.join("sessions/2026/10/14/rollout-p.jsonl");
    write_lines(&rollout, &[meta.to_string(), record.to_string()]);
}

/// The six claude quota rows `.loom/work/quota/history/claude.jsonl` held at
/// 6fee4ecd: both windows' `resets_at` alternate by one second on every row.
pub fn write_jittered_quota_history(project: &Path) {
    let observed = [
        1792001698u64,
        1792001878,
        1792002059,
        1792002240,
        1792002420,
        1792002601,
    ];
    let used = [16.0, 16.0, 17.0, 17.0, 17.0, 17.0];
    let lines: Vec<String> = observed
        .iter()
        .zip(used)
        .enumerate()
        .map(|(index, (observed_at, used_percent))| {
            let jitter = (index % 2) as u64;
            json!({
                "schema_version": 1,
                "observed_at": observed_at,
                "windows": [
                    {"kind": "five-hour", "used_percent": used_percent, "resets_at": 1792013399 + jitter},
                    {"kind": "seven-day", "used_percent": 83.0, "resets_at": 1792349999 + jitter}
                ],
                "plan": null
            })
            .to_string()
        })
        .collect();
    write_lines(
        &project.join(".loom/work/quota/history/claude.jsonl"),
        &lines,
    );
}
