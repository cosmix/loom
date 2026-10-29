//! Fixtures for the pressure pipeline contracts: a recording fake driver, a
//! minimal valid version 2 plan, and scratch git repositories.
#![allow(dead_code)]

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use loom::pressure::record::RecordLine;
use loom::pressure::{PressureDriver, PressureResult, StepVerdict};
use tempfile::TempDir;

/// The end-of-metadata marker line every plan's metadata block closes with.
pub const END_MARKER: &str = "<!-- END loom METADATA -->";

/// A minimal plan `loom plan verify` and `preflight_plan` accept: one standard
/// stage with one contract, an integration-verify and a knowledge-distill stage,
/// sandbox enabled. It ends with the END marker line and its newline.
pub const MINIMAL_PLAN: &str = r#"# PLAN: Pressure record fixture

One standard stage with a behavioural contract, an integration-verify stage and a
knowledge distillation.

---

<!-- loom METADATA -->

```yaml
loom:
  version: 2
  sandbox:
    enabled: true
  stages:
    - id: add-greeting
      name: "Add a greeting"
      stage_type: standard
      working_dir: "."
      dependencies: []
      description: "Greet a user by name."
      contracts:
        - id: greets-by-name
          file: "loom/tests/greeting.rs"
          test: "greets_by_name"
          runner: cargo-test
          scenario: "a greeting is built for the user named Ada"
          rejects: "a greeting that ignores the name and always says hello world"
      acceptance:
        - "cargo test --manifest-path loom/Cargo.toml --test greeting"

    - id: integration-verify
      name: "Integration verification"
      stage_type: integration-verify
      working_dir: "."
      dependencies: ["add-greeting"]
      description: "Verify the merged tree."
      acceptance:
        - "cargo test --manifest-path loom/Cargo.toml --all-targets"

    - id: knowledge-distill
      name: "Knowledge distillation"
      stage_type: knowledge-distill
      working_dir: "."
      dependencies: ["integration-verify"]
      description: "Curate the stage memories into knowledge."
      acceptance:
        - "loom knowledge check --strict --baseline doc/loom/knowledge/check-baseline.txt"
```

<!-- END loom METADATA -->
"#;

/// One driver call, as the fake records it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Call {
    Delete,
    Pressure(u32),
    Address(u32),
}

/// A `PressureDriver` that records each call, writes the Codex report in the
/// chosen rounds, and returns `StepVerdict::Stop` at one chosen call.
pub struct FakeDriver {
    pub calls: Vec<Call>,
    report: PathBuf,
    write_report_in: Vec<u32>,
    stop_at: Option<Call>,
    codex_model: String,
}

impl FakeDriver {
    pub fn new(report: &Path, write_report_in: &[u32], stop_at: Option<Call>) -> Self {
        Self {
            calls: Vec::new(),
            report: report.to_path_buf(),
            write_report_in: write_report_in.to_vec(),
            stop_at,
            codex_model: "gpt-6.1-sol".to_string(),
        }
    }

    fn verdict(&self, call: &Call) -> StepVerdict {
        if self.stop_at.as_ref() == Some(call) {
            StepVerdict::Stop {
                step: format!("{call:?}"),
                reason: "fake driver told to stop".to_string(),
            }
        } else {
            StepVerdict::Continue
        }
    }
}

impl PressureDriver for FakeDriver {
    fn delete_report(&mut self, report: &Path) -> anyhow::Result<()> {
        self.calls.push(Call::Delete);
        if report.exists() {
            fs::remove_file(report)?;
        }
        Ok(())
    }

    fn run_pressure(
        &mut self,
        round: u32,
        _claude: &str,
        _codex: &str,
    ) -> anyhow::Result<PressureResult> {
        let call = Call::Pressure(round);
        self.calls.push(call.clone());
        if self.write_report_in.contains(&round) {
            fs::write(&self.report, format!("# Codex review, round {round}\n"))?;
        }
        Ok(PressureResult {
            verdict: self.verdict(&call),
            codex_model_used: Some(self.codex_model.clone()),
        })
    }

    fn run_address(&mut self, round: u32, _slash: &str) -> anyhow::Result<StepVerdict> {
        let call = Call::Address(round);
        self.calls.push(call.clone());
        Ok(self.verdict(&call))
    }
}

pub fn date(year: i32, month: u32, day: u32) -> chrono::NaiveDate {
    chrono::NaiveDate::from_ymd_opt(year, month, day).expect("valid date")
}

pub fn record_line(codex_models: &[&str], rounds: u32, missing_rounds: &[u32]) -> RecordLine {
    RecordLine {
        date: date(2026, 10, 26),
        claude_model: "opus".to_string(),
        codex_models: codex_models.iter().map(|m| m.to_string()).collect(),
        rounds,
        missing_rounds: missing_rounds.to_vec(),
    }
}

/// A git-initialised scratch repository holding `doc/plans/PLAN-x.md` with
/// `plan`. Returns the TempDir, its canonical root and the plan's path.
pub fn git_repo_with_plan(plan: &str) -> (TempDir, PathBuf, PathBuf) {
    let temp = tempfile::tempdir().expect("create scratch directory");
    let root = temp.path().canonicalize().expect("canonical scratch root");
    let status = Command::new("git")
        .args(["init", "-q", "."])
        .current_dir(&root)
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .status()
        .expect("spawn git init");
    assert!(status.success(), "git init failed: {status}");
    let plan_path = root.join("doc/plans/PLAN-x.md");
    fs::create_dir_all(plan_path.parent().unwrap()).unwrap();
    fs::write(&plan_path, plan).unwrap();
    (temp, root, plan_path)
}

/// Run the loom binary in `root` with no inherited `LOOM_*` variable, so a real
/// session's identity never reaches the child.
pub fn run_loom(root: &Path, args: &[&str]) -> Output {
    let mut command = Command::new(env!("CARGO_BIN_EXE_loom"));
    for (key, _) in std::env::vars_os() {
        if key.to_string_lossy().starts_with("LOOM_") {
            command.env_remove(key);
        }
    }
    command
        .args(args)
        .current_dir(root)
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .output()
        .expect("spawn loom")
}
