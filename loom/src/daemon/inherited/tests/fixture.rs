use super::*;

pub(super) struct Fixture {
    _temp: tempfile::TempDir,
    pub repo: PathBuf,
    pub root: PathBuf,
    pub cwd: PathBuf,
    pub session: Session,
}

impl Fixture {
    pub(super) fn new() -> Self {
        // Short /tmp root keeps AF_UNIX below Darwin's 104-byte limit.
        let temp = tempfile::Builder::new()
            .prefix("inh-")
            .tempdir_in("/tmp")
            .unwrap();
        let repo = temp.path().canonicalize().unwrap();
        let root = repo.join(".loom/work");
        let cwd = repo.join(".worktrees/stage");
        for path in [
            &cwd,
            &root.join("sessions"),
            &root.join("stages"),
            &root.join("capsules"),
        ] {
            std::fs::create_dir_all(path).unwrap();
        }
        let session = active_pair(&root);
        std::fs::write(
            root.join("orchestrator.lock"),
            crate::fs::daemon_state::format_identity(crate::fs::daemon_state::current_identity()),
        )
        .unwrap();
        std::fs::write(
            root.join(format!("capsules/{}.settings.json", session.id)),
            r#"{"sandbox":{"enabled":true}}"#,
        )
        .unwrap();
        std::fs::write(repo.join("secret"), "untouched").unwrap();
        Self {
            _temp: temp,
            repo,
            root,
            cwd,
            session,
        }
    }

    pub(super) fn change(&self, mode: &str) {
        let capsule = self
            .root
            .join(format!("capsules/{}.settings.json", self.session.id));
        match mode {
            "missing-capsule" => std::fs::remove_file(capsule).unwrap(),
            "disabled-capsule" => {
                std::fs::write(capsule, r#"{"sandbox":{"enabled":false}}"#).unwrap()
            }
            "stale-daemon" => std::fs::write(
                self.root.join("orchestrator.lock"),
                format!("{} 0\n", std::process::id()),
            )
            .unwrap(),
            "fake-socket" => {
                // SAFETY: getppid reads the current process's parent identity without pointers.
                let parent = unsafe { libc::getppid() } as u32;
                std::fs::write(
                    self.root.join("orchestrator.lock"),
                    crate::fs::daemon_state::format_identity(crate::process::ProcessIdentity {
                        pid: parent,
                        start_time: crate::process::process_start_time(parent),
                    }),
                )
                .unwrap();
            }
            "stale-session" => {
                use crate::orchestrator::terminal::native::NativeBackend;
                let (_, key) = NativeBackend::window_title_and_pid_key(&self.session).unwrap();
                std::fs::write(
                    self.root.join(format!("pids/{key}.pid")),
                    format!("{}\n0\n", std::process::id()),
                )
                .unwrap();
            }
            "wrong-session" => {
                crate::verify::transitions::update_stage("stage", &self.root, |stage| {
                    stage.session = Some("someone-else".into());
                    Ok(())
                })
                .unwrap();
            }
            "writable-state" => {}
            _ => panic!("unknown negative fixture {mode}"),
        }
    }

    pub(super) fn run(&self, mode: &str, sandbox: bool) -> String {
        let listener = UnixListener::bind(socket_path(&self.root)).unwrap();
        let stop = Arc::new(AtomicBool::new(false));
        let server = {
            let (root, stop) = (self.root.clone(), stop.clone());
            std::thread::spawn(move || serve(listener, root, stop))
        };
        let output = bounded_output(&mut self.command(mode, sandbox));
        stop.store(true, Ordering::Release);
        server.join().unwrap();
        let output = output.unwrap();
        assert!(
            output.status.success(),
            "mode={mode} stdout={} stderr={}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        String::from_utf8(output.stdout).unwrap()
    }

    fn command(&self, mode: &str, sandbox: bool) -> Command {
        let executable = std::env::current_exe().unwrap();
        let mut command = if sandbox {
            let mut command = Command::new("/usr/bin/sandbox-exec");
            command.arg("-p").arg(self.profile(mode)).arg(executable);
            command
        } else {
            Command::new(executable)
        };
        command
            .current_dir(&self.cwd)
            .args([
                "--exact",
                "daemon::inherited::tests::inherited_seatbelt_child",
                "--nocapture",
            ])
            .env("LOOM_INHERITANCE_TEST", mode)
            .env("LOOM_WORK_DIR", &self.root)
            .env("LOOM_STAGE_ID", "stage")
            .env("LOOM_SESSION_ID", &self.session.id);
        command
    }

    fn profile(&self, mode: &str) -> String {
        let quote = |path: &Path| serde_json::to_string(path.to_str().unwrap()).unwrap();
        let protection = if mode == "writable-state" {
            String::new()
        } else {
            format!("(deny file-write* (subpath {}))", quote(&self.root))
        };
        format!(
            "(version 1)(allow default){protection}\
            (deny file-read-data file-write* (literal {}))\
            (deny network*)(allow network-outbound (literal {}))",
            quote(&self.repo.join("secret")),
            quote(&socket_path(&self.root))
        )
    }
}

fn active_pair(root: &Path) -> Session {
    let mut session = Session::new();
    session.assign_to_stage("stage".into());
    session.status = SessionStatus::Running;
    session.pid = Some(std::process::id());
    let mut stage = Stage::new("stage".into(), None);
    stage.id = "stage".into();
    stage.status = StageStatus::Executing;
    stage.session = Some(session.id.clone());
    stage.worktree = Some("stage".into());
    save_stage(&stage, root).unwrap();
    save_session(&session, root).unwrap();
    write_test_pid_identity(root, &session, std::process::id()).unwrap();
    session
}

fn bounded_output(command: &mut Command) -> std::io::Result<std::process::Output> {
    use std::io::{Read, Seek};
    use std::os::unix::process::CommandExt;
    let mut stdout = tempfile::tempfile()?;
    let mut stderr = tempfile::tempfile()?;
    command
        .process_group(0)
        .stdin(std::process::Stdio::null())
        .stdout(stdout.try_clone()?)
        .stderr(stderr.try_clone()?);
    let mut process = crate::process::OwnedProcess::new(command.spawn()?);
    let status = process.wait(Duration::from_secs(20))?;
    process.retire();
    let status = status.ok_or(std::io::ErrorKind::TimedOut)?;
    stdout.rewind()?;
    stderr.rewind()?;
    let (mut out, mut err) = (Vec::new(), Vec::new());
    stdout.read_to_end(&mut out)?;
    stderr.read_to_end(&mut err)?;
    Ok(std::process::Output {
        status,
        stdout: out,
        stderr: err,
    })
}
