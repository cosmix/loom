use super::support::{write_json_line, Fixture, AGENT_TYPE, LOOM_SESSION, STAGE};
use chrono::{DateTime, Utc};
use loom::subagent_lifecycle::{
    validate_subagent_stop, validate_teammate_idle, ActiveStageSession, ClaudeEnvironment,
    ClaudeStartEvidence, ClaudeStarts, LifecycleRecord,
};
use serde_json::json;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::PathBuf;
use std::process::Output;
use std::time::{Duration, SystemTime};

pub const PARENT_B: &str = "aaaaaaaa-bbbb-4ccc-8ddd-eeeeeeeeeeee";
pub const OTHER_PARENT: &str = "99999999-8888-4777-8666-555555555555";
pub const WORKER_B: &str = "worker-b";

impl Fixture {
    pub fn watch_session(&self, workers: &[&str], timeout: u64, session: &str) -> Output {
        let mut args = vec!["subagents", "watch"];
        for worker in workers {
            args.extend(["--worker", worker]);
        }
        let timeout = timeout.to_string();
        args.extend(["--timeout", &timeout, "--session", session, "--json"]);
        self.run(&args)
    }

    pub fn add_claude_worker(&self, parent: &str, agent: &str) {
        let (parent_path, worker_path) = self.claude_paths(parent, agent);
        fs::create_dir_all(worker_path.parent().expect("worker parent"))
            .expect("create extra transcript directory");
        if !parent_path.exists() {
            fs::write(&parent_path, "{\"type\":\"parent\"}\n")
                .expect("write extra parent transcript");
        }
        fs::write(
            &worker_path,
            "{\"type\":\"assistant\",\"message\":\"done\"}\n",
        )
        .expect("write extra worker transcript");
        self.write_start_for(parent, agent);
    }

    pub fn write_stop_for(&self, parent: &str, agent: &str) {
        let record = self.stop_record(parent, agent);
        write_json_line(&self.stage_dir().join("lifecycle.jsonl"), &record);
    }

    pub fn write_idle_for(&self, parent: &str, teammate: &str) {
        let (parent_path, _) = self.claude_paths(parent, teammate);
        let payload = json!({
            "session_id": parent,
            "team_name": "owned-wait-team",
            "teammate_name": teammate,
            "transcript_path": parent_path,
        });
        let record =
            validate_teammate_idle(&payload, &self.claude_environment(), &self.active_session())
                .expect("build teammate idle evidence");
        write_json_line(&self.stage_dir().join("lifecycle.jsonl"), &record);
    }

    pub fn append_worker_turn(&self, parent: &str, agent: &str) {
        let (_, worker) = self.claude_paths(parent, agent);
        let mut file = OpenOptions::new()
            .append(true)
            .open(worker)
            .expect("open worker transcript");
        writeln!(file, "{{\"type\":\"assistant\",\"message\":\"later\"}}")
            .expect("append worker turn");
    }

    pub fn add_newer_unrelated_transcript(&self) {
        let path = self
            .parent
            .parent()
            .expect("project directory")
            .join(format!("{OTHER_PARENT}.jsonl"));
        fs::write(
            &path,
            "{\"type\":\"assistant\",\"message\":\"unrelated\"}\n",
        )
        .expect("write unrelated transcript");
        let file = OpenOptions::new()
            .write(true)
            .open(path)
            .expect("open unrelated transcript");
        let newer = SystemTime::now() + Duration::from_secs(60);
        file.set_modified(newer)
            .expect("make unrelated transcript newer");
    }

    pub fn replay_lifecycle_journal(&self) {
        let path = self.stage_dir().join("lifecycle.jsonl");
        let bytes = fs::read(&path).expect("read lifecycle journal for replay");
        let mut file = OpenOptions::new()
            .append(true)
            .open(path)
            .expect("open lifecycle journal");
        file.write_all(&bytes).expect("replay lifecycle records");
    }

    pub fn write_wrong_loom_session_stop(&self, parent: &str, agent: &str) {
        let mut value =
            serde_json::to_value(self.stop_record(parent, agent)).expect("serialize stop record");
        value["identity"]["loom_session_id"] = json!("another-loom-session");
        write_json_line(&self.stage_dir().join("lifecycle.jsonl"), &value);
    }

    pub fn prepare_malformed_lease(&self, bytes: &[u8]) -> PathBuf {
        let dir = self.lease_dir();
        fs::create_dir_all(dir.join("results")).expect("create lease directories");
        let path = dir.join("lease.json");
        fs::write(&path, bytes).expect("write malformed lease");
        path
    }

    pub fn scratch_wait_root(&self) -> PathBuf {
        self.tmp.join("loom-subagent-waits")
    }

    fn stop_record(&self, parent: &str, agent: &str) -> LifecycleRecord {
        let (parent_path, worker_path) = self.claude_paths(parent, agent);
        let payload = json!({
            "session_id": parent,
            "agent_id": agent,
            "agent_type": AGENT_TYPE,
            "transcript_path": parent_path,
            "agent_transcript_path": worker_path,
        });
        validate_subagent_stop(
            &payload,
            &self.claude_environment(),
            &ExactStart { parent, agent },
            &self.active_session(),
        )
        .expect("build correlated stop")
    }

    fn write_start_for(&self, parent: &str, agent: &str) {
        let row = json!({
            "agent_id": agent,
            "agent_type": AGENT_TYPE,
            "stage_id": STAGE,
            "parent_session_id": parent,
            "loom_session_id": LOOM_SESSION,
            "ts": "2026-09-14T10:00:00.000Z",
        });
        write_json_line(&self.stage_dir().join("starts.jsonl"), &row);
    }

    fn claude_paths(&self, parent: &str, agent: &str) -> (PathBuf, PathBuf) {
        let project = self.parent.parent().expect("project directory");
        (
            project.join(format!("{parent}.jsonl")),
            project
                .join(parent)
                .join("subagents")
                .join(format!("agent-{agent}.jsonl")),
        )
    }

    fn claude_environment(&self) -> ClaudeEnvironment<'_> {
        ClaudeEnvironment {
            work_dir: &self.work,
            stage_id: STAGE,
            loom_session_id: LOOM_SESSION,
            observed_at: stop_time(),
        }
    }

    fn active_session(&self) -> ActiveStageSession<'_> {
        ActiveStageSession {
            stage_id: STAGE,
            loom_session_id: LOOM_SESSION,
        }
    }
}

struct ExactStart<'a> {
    parent: &'a str,
    agent: &'a str,
}

impl ClaudeStarts for ExactStart<'_> {
    fn resolve_exact(
        &self,
        stage: &str,
        parent: &str,
        session: &str,
        agent: &str,
    ) -> Option<ClaudeStartEvidence> {
        (stage == STAGE && parent == self.parent && session == LOOM_SESSION && agent == self.agent)
            .then(|| ClaudeStartEvidence {
                agent_type: AGENT_TYPE.into(),
                started_at: Some("2026-09-14T10:00:00.000Z".into()),
            })
    }
}

fn stop_time() -> DateTime<Utc> {
    "2026-09-14T10:01:00Z"
        .parse()
        .expect("valid lifecycle timestamp")
}
