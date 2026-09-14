use super::support_more::WatchChild;
use chrono::{DateTime, Utc};
use loom::subagent_lifecycle::{
    validate_subagent_stop, ActiveStageSession, ClaudeEnvironment, ClaudeStartEvidence,
    ClaudeStarts,
};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::os::unix::ffi::OsStrExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use tempfile::{Builder, TempDir};

pub const STAGE: &str = "owned-wait-stage";
pub const LOOM_SESSION: &str = "loom-session-owned-wait";
pub const PARENT_UUID: &str = "11111111-2222-4333-8444-555555555555";
pub const AGENT_ID: &str = "worker-a";
pub const AGENT_TYPE: &str = "loom-software-engineer";
pub const UNIT_ID: &str = "unit-a";
const INVOCATION: &str = "inv-0123456789abcdef0123456789abcdef";
const MODEL: &str = "gpt-5.6-sol";
const EFFORT: &str = "xhigh";
const JOB_ID: &str = "job-owned-wait";

pub struct Fixture {
    _temp: TempDir,
    pub(crate) home: PathBuf,
    pub(crate) tmp: PathBuf,
    pub(crate) repo: PathBuf,
    pub(crate) work: PathBuf,
    pub(crate) parent: PathBuf,
    pub(crate) worker: PathBuf,
    companion: PathBuf,
    state_root: PathBuf,
}

impl Fixture {
    pub fn new(label: &str) -> Self {
        let temp = Builder::new()
            .prefix(&format!("owned-wait-{label}-"))
            .tempdir_in(std::env::temp_dir())
            .expect("create fixture beneath TMPDIR");
        let root = fs::canonicalize(temp.path()).expect("canonical fixture root");
        let home = root.join("home");
        let tmp = root.join("tmp");
        let repo = root.join("repo");
        let work = root.join("work");
        let companion =
            home.join(".claude/plugins/cache/openai-codex/codex/1.0.6/scripts/codex-companion.mjs");
        let state_root = home.join(".codex/plugin-data/state");
        for directory in [&home, &tmp, &repo, &work, &state_root] {
            fs::create_dir_all(directory).expect("create fixture directory");
        }
        fs::create_dir_all(companion.parent().expect("companion parent"))
            .expect("create companion directory");
        fs::write(&companion, "// fixture companion\n").expect("write companion");
        init_git(&repo);
        write_stage_and_session(&work, &repo);
        let (parent, worker) = write_claude_layout(&home, &repo);
        write_start(&work);
        Self {
            _temp: temp,
            home,
            tmp,
            repo,
            work,
            parent,
            worker,
            companion,
            state_root,
        }
    }

    pub fn write_claude_stop(&self) {
        let observed_at = "2026-09-14T10:01:00Z"
            .parse::<DateTime<Utc>>()
            .expect("valid stop timestamp");
        let payload = json!({
            "session_id": PARENT_UUID,
            "agent_id": AGENT_ID,
            "agent_type": AGENT_TYPE,
            "transcript_path": self.parent,
            "agent_transcript_path": self.worker,
        });
        let record = validate_subagent_stop(
            &payload,
            &ClaudeEnvironment {
                work_dir: &self.work,
                stage_id: STAGE,
                loom_session_id: LOOM_SESSION,
                observed_at,
            },
            &StartEvidence,
            &ActiveStageSession {
                stage_id: STAGE,
                loom_session_id: LOOM_SESSION,
            },
        )
        .expect("build correlated stop");
        write_json_line(&self.stage_dir().join("lifecycle.jsonl"), &record);
    }

    pub fn write_codex_job(&self, status: &str) {
        let authorization = self.authorization();
        write_json_line(&self.stage_dir().join("codex.jsonl"), &authorization);
        let terminal = matches!(status, "failed" | "cancelled" | "completed");
        let phase = match status {
            "failed" => "failed",
            "cancelled" => "cancelled",
            "completed" => "done",
            _ => "starting",
        };
        let job = json!({
            "id": JOB_ID,
            "workspaceRoot": self.repo,
            "jobClass": "task",
            "write": true,
            "sessionId": format!("loom.v1:{STAGE}:{LOOM_SESSION}:{UNIT_ID}:{INVOCATION}"),
            "status": status,
            "phase": phase,
            "threadId": terminal.then_some("thread-owned-wait"),
            "turnId": terminal.then_some("turn-owned-wait"),
            "completedAt": terminal.then_some("2026-09-14T10:02:00.000Z"),
            "errorMessage": match status {
                "failed" => Some("companion failed"),
                "cancelled" => Some("companion cancelled"),
                _ => None,
            },
            "result": terminal.then_some(json!({"text": "terminal"})),
            "request": {
                "cwd": self.repo,
                "model": MODEL,
                "effort": EFFORT,
                "prompt": "fixture",
                "write": true,
                "resumeLast": false,
                "jobId": JOB_ID,
            }
        });
        let jobs = workspace_state_dir(&self.state_root, &self.repo).join("jobs");
        fs::create_dir_all(&jobs).expect("create companion jobs");
        fs::write(jobs.join(format!("{JOB_ID}.json")), job.to_string())
            .expect("write companion job");
    }

    pub fn watch(&self, workers: &[&str], timeout: u64) -> Output {
        let mut args = vec!["subagents", "watch"];
        for worker in workers {
            args.extend(["--worker", worker]);
        }
        let timeout = timeout.to_string();
        args.extend(["--timeout", &timeout, "--json"]);
        self.run(&args)
    }

    pub fn run(&self, args: &[&str]) -> Output {
        let mut command = self.cli_command();
        command.args(args);
        command.output().expect("run built loom CLI")
    }

    /// Spawn `loom subagents watch`; see `WatchChild` for cleanup guarantees.
    pub fn watch_spawn(&self, workers: &[&str], timeout: u64) -> WatchChild {
        let mut args = vec!["subagents", "watch"];
        for worker in workers {
            args.extend(["--worker", worker]);
        }
        let timeout = timeout.to_string();
        args.extend(["--timeout", &timeout, "--json"]);
        let stderr_path = self.tmp.join("watch-owner-stderr.log");
        let stderr = fs::File::create(&stderr_path).expect("create owner stderr file");
        let mut command = self.cli_command();
        command
            .args(&args)
            .stdout(Stdio::piped())
            .stderr(Stdio::from(stderr));
        WatchChild::new(command.spawn().expect("spawn loom CLI"), stderr_path)
    }

    fn cli_command(&self) -> Command {
        let mut command = Command::new(env!("CARGO_BIN_EXE_loom"));
        for (key, _) in std::env::vars_os() {
            if key.to_string_lossy().starts_with("LOOM_") {
                command.env_remove(key);
            }
        }
        command
            .current_dir(&self.repo)
            .env("HOME", &self.home)
            .env("TMPDIR", &self.tmp)
            .env("LOOM_STAGE_ID", STAGE)
            .env("LOOM_SESSION_ID", LOOM_SESSION)
            .env("LOOM_WORK_DIR", &self.work)
            .env("LOOM_WORKTREE_PATH", &self.repo)
            .env("CLAUDE_PLUGIN_DATA", self.home.join(".codex/plugin-data"));
        command
    }

    pub fn events(&self, output: &Output) -> Vec<Value> {
        String::from_utf8_lossy(&output.stdout)
            .lines()
            .map(|line| serde_json::from_str(line).expect("parse wait JSON line"))
            .collect()
    }

    pub fn transcript_dir_str(&self) -> &str {
        self.worker
            .parent()
            .and_then(Path::to_str)
            .expect("UTF-8 transcript directory")
    }

    pub fn seed_live_lease(&self, events: &[Value]) -> PathBuf {
        let wait_id = events[0]["wait_id"].as_str().expect("first wait id");
        let dir = self.lease_dir();
        let result = dir.join("results").join(format!("{wait_id}.json"));
        let mut lease: Value =
            serde_json::from_slice(&fs::read(result).expect("read result lease"))
                .expect("parse result lease");
        lease["wait_id"] = json!("live-owned-wait");
        lease["owner"] = json!({
            "pid": std::process::id(),
            "start_time": loom::process::process_start_time(std::process::id()),
        });
        lease["terminal_result"] = Value::Null;
        lease["finished_unix_secs"] = Value::Null;
        let path = dir.join("lease.json");
        fs::write(
            &path,
            serde_json::to_vec_pretty(&lease).expect("encode live lease"),
        )
        .expect("write live lease directly");
        path
    }

    pub(crate) fn stage_dir(&self) -> PathBuf {
        let path = self.work.join("subagents").join(STAGE);
        fs::create_dir_all(&path).expect("create stage evidence directory");
        path
    }

    fn authorization(&self) -> Value {
        json!({
            "v": 2,
            "ts": "2026-09-14T10:00:00.000Z",
            "stage_id": STAGE,
            "session_id": LOOM_SESSION,
            "parent_session_id": PARENT_UUID,
            "forwarder_agent_id": "forwarder-a",
            "tool_use_id": "tool-owned-wait",
            "unit_id": UNIT_ID,
            "invocation_id": INVOCATION,
            "model": MODEL,
            "effort": EFFORT,
            "workspace_root": self.repo,
            "companion_version": "1.0.6",
            "companion_path": self.companion,
            "state_root": self.state_root,
        })
    }

    pub(crate) fn lease_dir(&self) -> PathBuf {
        let digest = hex::encode(Sha256::digest(self.repo.as_os_str().as_bytes()));
        self.tmp
            .join("loom-subagent-waits")
            .join(&digest[..16])
            .join(STAGE)
            .join(LOOM_SESSION)
            .join(PARENT_UUID)
    }
}

struct StartEvidence;

impl ClaudeStarts for StartEvidence {
    fn resolve_exact(
        &self,
        stage: &str,
        parent: &str,
        session: &str,
        agent: &str,
    ) -> Option<ClaudeStartEvidence> {
        (stage == STAGE && parent == PARENT_UUID && session == LOOM_SESSION && agent == AGENT_ID)
            .then(|| ClaudeStartEvidence {
                agent_type: AGENT_TYPE.into(),
                started_at: Some("2026-09-14T10:00:00.000Z".into()),
            })
    }
}

fn init_git(repo: &Path) {
    let output = Command::new("git")
        .args(["init", "--quiet"])
        .current_dir(repo)
        .output()
        .expect("git init fixture repository");
    assert!(output.status.success(), "git init failed");
}

fn write_stage_and_session(work: &Path, repo: &Path) {
    fs::create_dir_all(work.join("stages")).expect("create stages");
    fs::create_dir_all(work.join("sessions")).expect("create sessions");
    let stage = format!(
        "---\nid: {STAGE}\nname: Owned wait\ndescription: Fixture\nstatus: executing\ndependencies: []\nparallel_group: null\nacceptance: []\nfiles: []\nplan_id: null\nworktree: {}\nsession: {LOOM_SESSION}\nparent_stage: null\nchild_stages: []\ncreated_at: \"2026-09-14T10:00:00Z\"\nupdated_at: \"2026-09-14T10:00:00Z\"\ncompleted_at: null\nclose_reason: null\n---\n",
        repo.display()
    );
    fs::write(work.join(format!("stages/01-{STAGE}.md")), stage).expect("write stage");
    let session = format!(
        "---\nid: {LOOM_SESSION}\nstage_id: {STAGE}\nworktree_path: {}\npid: null\nstatus: running\ncontext_tokens: 0\ncreated_at: \"2026-09-14T10:00:00Z\"\nlast_active: \"2026-09-14T10:00:00Z\"\n---\n",
        repo.display()
    );
    fs::write(work.join(format!("sessions/{LOOM_SESSION}.md")), session).expect("write session");
}

fn write_claude_layout(home: &Path, repo: &Path) -> (PathBuf, PathBuf) {
    let slug: String = repo
        .to_string_lossy()
        .chars()
        .map(|c| if c == '/' || c == '.' { '-' } else { c })
        .collect();
    let project = home.join(".claude/projects").join(slug);
    let parent = project.join(format!("{PARENT_UUID}.jsonl"));
    let worker = project
        .join(PARENT_UUID)
        .join("subagents")
        .join(format!("agent-{AGENT_ID}.jsonl"));
    fs::create_dir_all(worker.parent().expect("worker parent")).expect("create transcripts");
    fs::write(&parent, "{\"type\":\"parent\"}\n").expect("write parent transcript");
    fs::write(&worker, "{\"type\":\"assistant\",\"message\":\"done\"}\n")
        .expect("write worker transcript");
    (parent, worker)
}

fn write_start(work: &Path) {
    let row = json!({
        "agent_id": AGENT_ID,
        "agent_type": AGENT_TYPE,
        "stage_id": STAGE,
        "parent_session_id": PARENT_UUID,
        "loom_session_id": LOOM_SESSION,
        "ts": "2026-09-14T10:00:00.000Z",
    });
    write_json_line(
        &work.join("subagents").join(STAGE).join("starts.jsonl"),
        &row,
    );
}

pub(crate) fn write_json_line(path: &Path, value: &impl serde::Serialize) {
    fs::create_dir_all(path.parent().expect("JSONL parent")).expect("create JSONL parent");
    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .expect("open JSONL fixture");
    writeln!(
        file,
        "{}",
        serde_json::to_string(value).expect("serialize JSONL")
    )
    .expect("write JSONL fixture");
}

fn workspace_state_dir(state_root: &Path, workspace: &Path) -> PathBuf {
    let canonical = workspace.to_str().expect("UTF-8 workspace");
    let basename = workspace
        .file_name()
        .and_then(|v| v.to_str())
        .unwrap_or("workspace");
    let slug: String = basename
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '-'
            }
        })
        .collect();
    let hash = hex::encode(Sha256::digest(canonical.as_bytes()));
    state_root.join(format!("{slug}-{}", &hash[..16]))
}

pub fn assert_exit(output: &Output, expected: i32) {
    assert_eq!(
        output.status.code(),
        Some(expected),
        "stdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );
}
