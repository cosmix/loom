use anyhow::{ensure, Context, Result};
use loom::codex_lifecycle::CodexAuthorization;
use serde_json::Value;
use std::fs::{self, File};
use std::io::Write;
use std::path::Path;
use std::process::{Command, Output, Stdio};
use tempfile::NamedTempFile;

use crate::fixture::{Fixture, Forwarder, Launch, EFFORT, LOOM_SESSION, MODEL, STAGE};
use crate::fixture_support::{assert_authorization, guard_payload};

impl Fixture {
    pub(crate) fn run_guard(
        &self,
        forwarder: &Forwarder,
        original: &str,
        active: bool,
        agent_override: Option<&str>,
    ) -> Result<Output> {
        let payload = guard_payload(forwarder, original, agent_override, &self.project);
        let mut input = NamedTempFile::new_in(&self.tmp)?;
        input.write_all(payload.to_string().as_bytes())?;
        input.flush()?;
        let stdin = File::open(input.path())?;
        let mut command = Command::new(self.hooks.join("codex-forward-guard.sh"));
        command.stdin(Stdio::from(stdin));
        self.configure_child(&mut command, active);
        command.output().context("running real Codex guard")
    }

    pub(crate) fn forward_command(&self, unit: Option<&str>) -> String {
        let mut command = format!(
            "{} task 'fixture prompt' --model {MODEL} --effort {EFFORT} --write",
            self.hooks.join("codex-forward.sh").display()
        );
        if let Some(unit) = unit {
            command.push_str(&format!(" --unit-id {unit}"));
        }
        command
    }

    pub(crate) fn authorization_for(&self, invocation: &str) -> Result<CodexAuthorization> {
        let path = self.work.join("subagents").join(STAGE).join("codex.jsonl");
        let text = fs::read_to_string(path)?;
        let values: Vec<Value> = text
            .lines()
            .map(serde_json::from_str)
            .collect::<serde_json::Result<_>>()?;
        let value = values
            .iter()
            .find(|value| value["invocation_id"] == invocation)
            .context("missing authorization for injected invocation")?;
        CodexAuthorization::from_v2_value(value)
    }

    pub(crate) fn verify_authorization(
        &self,
        authorization: &CodexAuthorization,
        forwarder: &Forwarder,
        unit: Option<&str>,
    ) -> Result<()> {
        let unit = unit
            .map(str::to_owned)
            .unwrap_or_else(|| format!("fwd-{}", forwarder.agent));
        assert_authorization(authorization, forwarder, &unit, &self.home, &self.project)
    }

    pub(crate) fn run_wrapper(
        &self,
        updated: &str,
        job_id: &str,
        status: &str,
        thread_id: &str,
        turn_id: &str,
        calls: &Path,
    ) -> Result<Output> {
        let mut command = Command::new("bash");
        command.args(["-c", updated]);
        self.configure_child(&mut command, true);
        command
            .env("FAKE_CODEX_JOB_ID", job_id)
            .env("FAKE_CODEX_STATUS", status)
            .env("FAKE_CODEX_THREAD_ID", thread_id)
            .env("FAKE_CODEX_TURN_ID", turn_id)
            .env("FAKE_CODEX_CALLS", calls);
        command
            .output()
            .context("running injected real Codex wrapper")
    }

    fn configure_child(&self, command: &mut Command, active: bool) {
        for (key, _) in std::env::vars_os() {
            if key.to_string_lossy().starts_with("LOOM_") {
                command.env_remove(key);
            }
        }
        command
            .current_dir(&self.project)
            .env("HOME", &self.home)
            .env("TMPDIR", &self.tmp)
            .env("PATH", std::env::var_os("PATH").unwrap_or_default())
            .env("LOOM_WORK_DIR", &self.work)
            .env("LOOM_STAGE_ID", if active { STAGE } else { "" })
            .env("LOOM_SESSION_ID", if active { LOOM_SESSION } else { "" })
            .env("LOOM_WORKTREE_PATH", &self.project);
    }

    pub(crate) fn cli_command(&self) -> Command {
        let mut command = Command::new(env!("CARGO_BIN_EXE_loom"));
        self.configure_child(&mut command, true);
        command
    }

    pub fn list(&self, forwarder: &Forwarder) -> Result<Value> {
        let mut command = self.cli_command();
        command
            .args(["subagents", "list", "--dir"])
            .arg(transcript_directory(forwarder)?)
            .arg("--json");
        let output = command.output()?;
        ensure!(
            output.status.success(),
            "list failed: {}",
            crate::fixture_support::output_text(&output)
        );
        serde_json::from_slice(&output.stdout).context("parsing subagents list JSON")
    }

    pub fn watch(&self, launch: &Launch) -> Result<Output> {
        let mut command = self.cli_command();
        command
            .args(["subagents", "watch", "--worker"])
            .arg(format!("codex:{}", launch.authorization.unit_id))
            .args(["--timeout", "3", "--json"]);
        command.output().context("running subagents watch")
    }

    pub fn harvest(&self, forwarder: &Forwarder) -> Result<Output> {
        let mut command = self.cli_command();
        command
            .args(["subagents", "harvest", "--dir"])
            .arg(transcript_directory(forwarder)?)
            .args(["--debounce", "100000"]);
        command.output().context("running subagents harvest")
    }
}

fn transcript_directory(forwarder: &Forwarder) -> Result<&Path> {
    forwarder
        .transcript
        .parent()
        .context("transcript directory")
}
