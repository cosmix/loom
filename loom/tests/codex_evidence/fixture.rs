use anyhow::{ensure, Context, Result};
use loom::codex_lifecycle::CodexAuthorization;
use loom::orchestrator::monitor::{Monitor, MonitorConfig};
use loom::subagent_lifecycle::{replay, WorkerOutcome};
use serde_json::{json, Value};
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::PathBuf;
use std::process::Output;
use tempfile::{Builder, TempDir};

use crate::fixture_support::{
    apply_status, install_fake_companion, install_hooks, output_text, updated_command,
    workspace_state_dir, write_session, write_stage, ProcessEnvironment,
};

pub const STAGE: &str = "codex-stage";
pub const LOOM_SESSION: &str = "loom-session-a";
pub const MODEL: &str = "gpt-5.6-sol";
pub const EFFORT: &str = "xhigh";

#[derive(Clone)]
pub struct Forwarder {
    pub parent: String,
    pub agent: String,
    pub transcript: PathBuf,
}

pub struct Launch {
    pub authorization: CodexAuthorization,
    pub forwarder: Forwarder,
    pub job_id: String,
    pub job_path: PathBuf,
    pub wrapper: Output,
    pub calls_path: PathBuf,
}

pub struct Fixture {
    _temp: TempDir,
    pub root: PathBuf,
    pub home: PathBuf,
    pub project: PathBuf,
    pub work: PathBuf,
    pub hooks: PathBuf,
    pub tmp: PathBuf,
}

impl Fixture {
    pub fn new(label: &str) -> Result<Self> {
        let temp = Builder::new()
            .prefix(&format!("codex-evidence-{label}-"))
            .tempdir()?;
        let root = fs::canonicalize(temp.path())?;
        let home = root.join("home");
        let project = root.join("project");
        let work = project.join(".loom/work");
        let hooks = home.join(".claude/hooks/loom");
        let tmp = root.join("tmp");
        for directory in [&home, &project, &work, &hooks, &tmp] {
            fs::create_dir_all(directory)?;
        }
        fs::create_dir(project.join(".git"))?;
        install_hooks(&hooks)?;
        install_fake_companion(&home)?;
        fs::create_dir_all(home.join(".codex/plugin-data/state"))?;
        write_stage(&work, &project)?;
        write_session(&work, &project)?;
        Ok(Self {
            _temp: temp,
            root,
            home,
            project,
            work,
            hooks,
            tmp,
        })
    }

    pub fn add_forwarder(&self, parent: &str, agent: &str) -> Result<Forwarder> {
        let project_dir = self.root.join("claude-project");
        let transcript = project_dir
            .join(parent)
            .join("subagents")
            .join(format!("agent-{agent}.jsonl"));
        fs::create_dir_all(transcript.parent().context("forwarder transcript parent")?)?;
        fs::write(
            project_dir.join(format!("{parent}.jsonl")),
            "{\"type\":\"parent\"}\n",
        )?;
        fs::write(&transcript, format!("{}\n", transcript_row()))?;
        self.append_start(parent, agent)?;
        Ok(Forwarder {
            parent: parent.into(),
            agent: agent.into(),
            transcript,
        })
    }

    pub fn launch(
        &self,
        forwarder: &Forwarder,
        unit: Option<&str>,
        job_id: &str,
        status: &str,
    ) -> Result<Launch> {
        self.launch_with_ids(
            forwarder,
            unit,
            job_id,
            status,
            "thread-exact",
            "turn-exact",
        )
    }

    pub fn launch_with_ids(
        &self,
        forwarder: &Forwarder,
        unit: Option<&str>,
        job_id: &str,
        status: &str,
        thread_id: &str,
        turn_id: &str,
    ) -> Result<Launch> {
        let original = self.forward_command(unit);
        let guard = self.run_guard(forwarder, &original, true, None)?;
        ensure!(
            guard.status.success(),
            "guard failed: {}",
            output_text(&guard)
        );
        let updated = updated_command(&guard)?;
        let invocation = updated
            .split_ascii_whitespace()
            .last()
            .context("injected command omitted invocation")?;
        let authorization = self.authorization_for(invocation)?;
        self.verify_authorization(&authorization, forwarder, unit)?;
        let calls_path = self.root.join(format!("calls-{job_id}.jsonl"));
        let wrapper =
            self.run_wrapper(&updated, job_id, status, thread_id, turn_id, &calls_path)?;
        let job_path = self.job_path(job_id);
        ensure!(
            job_path.is_file(),
            "fake companion did not write exact job file"
        );
        Ok(Launch {
            authorization,
            forwarder: forwarder.clone(),
            job_id: job_id.into(),
            job_path,
            wrapper,
            calls_path,
        })
    }

    pub fn guard_only(
        &self,
        forwarder: &Forwarder,
        agent_override: Option<&str>,
        active: bool,
    ) -> Result<Output> {
        self.run_guard(
            forwarder,
            &self.forward_command(Some("unit-guard")),
            active,
            agent_override,
        )
    }

    pub fn monitor(&self) -> Monitor {
        Monitor::new(MonitorConfig {
            work_dir: self.work.clone(),
            ..Default::default()
        })
    }

    pub fn poll(&self) -> Result<()> {
        let mut monitor = self.monitor();
        self.poll_monitor(&mut monitor)
    }

    pub fn poll_monitor(&self, monitor: &mut Monitor) -> Result<()> {
        let _environment = ProcessEnvironment::set(&self.home, &self.tmp);
        monitor.poll()?;
        Ok(())
    }

    pub fn companion_outcome(&self, launch: &Launch) -> WorkerOutcome {
        let _environment = ProcessEnvironment::set(&self.home, &self.tmp);
        loom::codex_lifecycle::companion_outcome(&self.work, &launch.authorization)
    }

    pub fn lifecycle_outcome(&self, forwarder: &Forwarder) -> Result<WorkerOutcome> {
        Ok(replay(&self.work)?.forwarded_outcome(
            STAGE,
            LOOM_SESSION,
            &forwarder.parent,
            &forwarder.agent,
        ))
    }

    pub fn set_job_status(
        &self,
        launch: &Launch,
        status: &str,
        thread_id: &str,
        turn_id: &str,
    ) -> Result<()> {
        self.edit_job(launch, |job| apply_status(job, status, thread_id, turn_id))
    }

    pub fn edit_job(&self, launch: &Launch, edit: impl FnOnce(&mut Value)) -> Result<()> {
        let mut job: Value = serde_json::from_slice(&fs::read(&launch.job_path)?)?;
        edit(&mut job);
        fs::write(
            &launch.job_path,
            format!("{}\n", serde_json::to_string_pretty(&job)?),
        )?;
        Ok(())
    }

    pub fn write_unrelated_job(&self, launch: &Launch, job_id: &str) -> Result<PathBuf> {
        let mut job: Value = serde_json::from_slice(&fs::read(&launch.job_path)?)?;
        job["id"] = json!(job_id);
        job["sessionId"] = json!(format!(
            "loom.v1:{STAGE}:{LOOM_SESSION}:other-unit:inv-aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
        ));
        job["request"]["jobId"] = json!(job_id);
        let path = launch.job_path.with_file_name(format!("{job_id}.json"));
        fs::write(&path, format!("{}\n", serde_json::to_string_pretty(&job)?))?;
        let future = std::time::SystemTime::now() + std::time::Duration::from_secs(3600);
        OpenOptions::new()
            .write(true)
            .open(&path)?
            .set_modified(future)?;
        Ok(path)
    }

    pub fn journal_values(&self) -> Result<Vec<Value>> {
        let path = self
            .work
            .join("subagents")
            .join(STAGE)
            .join("lifecycle.jsonl");
        let Ok(text) = fs::read_to_string(path) else {
            return Ok(Vec::new());
        };
        ensure!(
            text.ends_with('\n'),
            "lifecycle journal is not newline terminated"
        );
        text.lines()
            .map(|line| serde_json::from_str(line).context("parsing lifecycle line"))
            .collect()
    }

    pub fn authorization_count(&self) -> Result<usize> {
        let path = self.work.join("subagents").join(STAGE).join("codex.jsonl");
        Ok(fs::read_to_string(path).map_or(0, |text| text.lines().count()))
    }

    pub fn calls(&self, launch: &Launch) -> Result<Vec<Value>> {
        fs::read_to_string(&launch.calls_path)?
            .lines()
            .map(|line| serde_json::from_str(line).context("parsing fake companion call"))
            .collect()
    }

    pub fn job_path(&self, job_id: &str) -> PathBuf {
        workspace_state_dir(&self.home.join(".codex/plugin-data/state"), &self.project)
            .join("jobs")
            .join(format!("{job_id}.json"))
    }

    fn append_start(&self, parent: &str, agent: &str) -> Result<()> {
        let directory = self.work.join("subagents").join(STAGE);
        fs::create_dir_all(&directory)?;
        let row = json!({
            "agent_id": agent,
            "agent_type": "loom-codex-forwarder",
            "stage_id": STAGE,
            "parent_session_id": parent,
            "loom_session_id": LOOM_SESSION,
            "ts": "2026-09-14T10:00:00.000Z"
        });
        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(directory.join("starts.jsonl"))?;
        writeln!(file, "{row}")?;
        Ok(())
    }
}

fn transcript_row() -> Value {
    json!({
        "type": "assistant",
        "timestamp": "2099-01-01T00:00:00.000Z",
        "message": {"role": "assistant", "content": [{"type": "text", "text": "forwarded"}]}
    })
}
