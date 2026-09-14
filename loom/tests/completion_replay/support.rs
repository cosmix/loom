use std::ffi::OsStr;
use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

use anyhow::{bail, Context, Result};
use fs2::FileExt;
use loom::models::session::{Session, SessionStatus};
use loom::models::stage::{AcceptanceCriterion, Stage, StageStatus};
use tempfile::TempDir;

pub struct ReplayFixture {
    _daemon_lock: File,
    _root: TempDir,
    pub stage_id: String,
    pub session_id: String,
    pub worktree: PathBuf,
    stage_file: PathBuf,
}

impl ReplayFixture {
    pub fn new(acceptance: &[&str]) -> Result<Self> {
        let root = tempfile::Builder::new()
            .prefix("loom-completion-replay-")
            .tempdir()
            .context("creating completion replay root")?;
        init_repository(root.path())?;

        let stage_id = "completion-replay".to_string();
        let session_id = "session-completion-replay".to_string();
        let worktree = root.path().join(".worktrees").join(&stage_id);
        create_worktree(root.path(), &worktree, &stage_id)?;
        let stage_file = create_state(root.path(), &worktree, &stage_id, &session_id, acceptance)?;
        let daemon_lock = hold_daemon_lock(&root.path().join(".loom/work"))?;

        Ok(Self {
            _daemon_lock: daemon_lock,
            _root: root,
            stage_id,
            session_id,
            worktree,
            stage_file,
        })
    }

    pub fn run_completion(&self) -> Output {
        let mut command = Command::new(env!("CARGO_BIN_EXE_loom"));
        self.configure_child(&mut command, false);
        command
            .args(["stage", "complete", &self.stage_id])
            .output()
            .expect("completion replay binary must start and wait successfully")
    }

    pub fn run_hook_post(&self, command: &str, stdout: &str, is_error: bool) -> Output {
        let hook =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../loom-hooks/loom-control-complete.sh");
        let payload = serde_json::json!({
            "tool_name": "Bash",
            "tool_input": { "command": command },
            "tool_response": { "stdout": stdout, "is_error": is_error },
        });
        let mut command = Command::new("bash");
        self.configure_child(&mut command, true);
        let mut child = command
            .arg(hook)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("completion hook must start");
        child
            .stdin
            .take()
            .expect("completion hook stdin must be piped")
            .write_all(payload.to_string().as_bytes())
            .expect("completion hook payload must be written");
        child.wait_with_output().expect("completion hook must exit")
    }

    fn configure_child(&self, command: &mut Command, hook_harness: bool) {
        for (key, _) in std::env::vars_os() {
            if key.to_string_lossy().starts_with("LOOM_") {
                command.env_remove(key);
            }
        }
        command
            .current_dir(&self.worktree)
            .env("LOOM_STAGE_ID", &self.stage_id)
            .env("LOOM_SESSION_ID", &self.session_id)
            .env("LOOM_WORKTREE_PATH", &self.worktree);
        if hook_harness {
            command
                .env("LOOM_CONTROL_TESTING", "1")
                .env("LOOM_CONTROL_TEST_BIN", env!("CARGO_BIN_EXE_loom"));
        }
    }

    pub fn pinned_command(&self) -> String {
        format!(
            "{} stage complete {}",
            env!("CARGO_BIN_EXE_loom"),
            self.stage_id
        )
    }

    pub fn rewrite_stage_session(&self, session_id: &str) -> Result<()> {
        let mut stage = self.reload_stage()?;
        stage.session = Some(session_id.to_string());
        write_record(&self.stage_file, &stage, "Stage")
    }

    pub fn reload_stage(&self) -> Result<Stage> {
        let content = fs::read_to_string(&self.stage_file)
            .with_context(|| format!("reading {}", self.stage_file.display()))?;
        let frontmatter = content
            .strip_prefix("---\n")
            .and_then(|body| body.split_once("\n---").map(|(yaml, _)| yaml))
            .context("stage file must contain YAML frontmatter")?;
        serde_yaml::from_str(frontmatter).context("deserializing replay stage")
    }
}

fn hold_daemon_lock(work_dir: &Path) -> Result<File> {
    let lock_path = work_dir.join("orchestrator.lock");
    let lock = OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(&lock_path)
        .with_context(|| format!("opening {}", lock_path.display()))?;
    lock.try_lock_exclusive()
        .context("holding isolated daemon-liveness lock")?;
    Ok(lock)
}

fn init_repository(root: &Path) -> Result<()> {
    run_git(root, &["init", "-b", "main"])?;
    run_git(root, &["config", "user.email", "completion-replay@test"])?;
    run_git(root, &["config", "user.name", "Completion Replay"])?;
    fs::write(root.join("README.md"), "# Completion replay fixture\n")?;
    run_git(root, &["add", "README.md"])?;
    run_git(root, &["commit", "-m", "fixture"])
}

fn create_worktree(root: &Path, worktree: &Path, stage_id: &str) -> Result<()> {
    fs::create_dir_all(root.join(".worktrees"))?;
    let branch = format!("loom/{stage_id}");
    let args = [
        OsStr::new("worktree"),
        OsStr::new("add"),
        OsStr::new("-b"),
        OsStr::new(branch.as_str()),
        worktree.as_os_str(),
    ];
    run_git(root, &args)
}

fn create_state(
    root: &Path,
    worktree: &Path,
    stage_id: &str,
    session_id: &str,
    acceptance: &[&str],
) -> Result<PathBuf> {
    let work_dir = root.join(".loom").join("work");
    fs::create_dir_all(work_dir.join("stages"))?;
    fs::create_dir_all(work_dir.join("sessions"))?;
    fs::create_dir_all(work_dir.join("handoffs"))?;
    fs::create_dir_all(worktree.join(".loom"))?;
    std::os::unix::fs::symlink("../../../.loom/work", worktree.join(".loom/work"))?;

    let mut stage = Stage::new("Completion replay".to_string(), None);
    stage.id = stage_id.to_string();
    stage.status = StageStatus::Executing;
    stage.acceptance = acceptance
        .iter()
        .map(|command| AcceptanceCriterion::Simple((*command).to_string()))
        .collect();
    stage.worktree = Some(stage_id.to_string());
    stage.session = Some(session_id.to_string());
    stage.base_branch = Some("main".to_string());
    stage.resolved_base = Some("main".to_string());
    stage.working_dir = Some(".".to_string());

    let mut session = Session::new();
    session.id = session_id.to_string();
    session.assign_to_stage(stage_id.to_string());
    session.worktree_path = Some(worktree.to_path_buf());
    session.status = SessionStatus::Running;

    let stage_file = work_dir.join("stages").join(format!("{stage_id}.md"));
    write_record(&stage_file, &stage, "Stage")?;
    let session_file = work_dir.join("sessions").join(format!("{session_id}.md"));
    write_record(&session_file, &session, "Session")?;
    Ok(stage_file)
}

fn write_record<T: serde::Serialize>(path: &Path, value: &T, title: &str) -> Result<()> {
    let yaml = serde_yaml::to_string(value).context("serializing replay state")?;
    fs::write(path, format!("---\n{yaml}---\n\n# {title}\n"))
        .with_context(|| format!("writing {}", path.display()))
}

fn run_git<S: AsRef<OsStr>>(root: &Path, args: &[S]) -> Result<()> {
    let output = Command::new("git")
        .args(args)
        .current_dir(root)
        .output()
        .context("starting fixture git command")?;
    if !output.status.success() {
        bail!(
            "fixture git command failed ({:?}):\nstdout:\n{}\nstderr:\n{}",
            output.status,
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }
    Ok(())
}
