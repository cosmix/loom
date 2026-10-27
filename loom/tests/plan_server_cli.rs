//! Binary-level planning server startup and shutdown contracts.

use std::io::{BufRead, BufReader};
use std::net::{SocketAddr, TcpListener};
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::process::{Child, Command, Output, Stdio};
use std::sync::mpsc;
use std::time::{Duration, Instant};

use loom::planning::runtime::project_key;
use nix::sys::signal::{kill, Signal};
use nix::unistd::Pid;
use tempfile::TempDir;

fn git(repo: &Path, args: &[&str]) -> Output {
    Command::new("git")
        .args(["-c", "user.name=t", "-c", "user.email=t@example.com"])
        .args(args)
        .current_dir(repo)
        .env_remove("GIT_DIR")
        .env_remove("GIT_WORK_TREE")
        .output()
        .expect("run git")
}

fn repo() -> TempDir {
    let repo = tempfile::tempdir().unwrap();
    for args in [
        &["init", "-q", "-b", "main"][..],
        &["commit", "-q", "--allow-empty", "-m", "init"],
    ] {
        let output = git(repo.path(), args);
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    repo
}

/// A `loom plan --no-open` command in `repo`, with a scratch `LOOM_HOME` and no other inherited
/// `LOOM_*` variable (a real session's identity must not reach the child), nor `GIT_DIR` or
/// `GIT_WORK_TREE`. Callers add a `LOOM_*` variable after this returns when a test needs one.
fn command(repo: &Path, home: &Path) -> Command {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_loom"));
    for (key, _) in std::env::vars_os() {
        if key.to_string_lossy().starts_with("LOOM_") {
            cmd.env_remove(key);
        }
    }
    cmd.arg("plan")
        .arg("--no-open")
        .current_dir(repo)
        .env("LOOM_HOME", home)
        .env_remove("GIT_DIR")
        .env_remove("GIT_WORK_TREE")
        .stdin(Stdio::null());
    cmd
}

struct Server(Child);

impl Drop for Server {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

fn start(cmd: &mut Command) -> (Server, String) {
    let mut child = cmd
        .arg("--port")
        .arg("0")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let stdout = child.stdout.take().unwrap();
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        for line in BufReader::new(stdout).lines().map_while(Result::ok) {
            if tx.send(line).is_err() {
                break;
            }
        }
    });
    let server = Server(child);
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        let line = rx
            .recv_timeout(deadline.saturating_duration_since(Instant::now()))
            .expect("server did not print a URL");
        if line.starts_with("http://") {
            return (server, line);
        }
    }
}

fn loopback_available() -> bool {
    TcpListener::bind("127.0.0.1:0").is_ok()
}

#[test]
fn second_process_reports_running_pid_and_url() {
    if !loopback_available() {
        return;
    }
    let repo = repo();
    let home = tempfile::tempdir().unwrap();
    let (server, url) = start(&mut command(repo.path(), home.path()));
    let output = command(repo.path(), home.path())
        .args(["--port", "0"])
        .output()
        .unwrap();
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert_eq!(output.status.code(), Some(1), "{stderr}");
    assert!(
        stderr.contains(&format!(
            "loom plan is already running for this project (pid {}).",
            server.0.id()
        )),
        "{stderr}"
    );
    assert!(stderr.contains(&format!("Open {url}")), "{stderr}");
}

#[test]
fn outside_repository_reports_directory() {
    let dir = tempfile::tempdir().unwrap();
    let home = tempfile::tempdir().unwrap();
    let output = command(dir.path(), home.path())
        .args(["--port", "0"])
        .output()
        .unwrap();
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(!output.status.success(), "{stderr}");
    assert!(
        stderr.contains(&dir.path().display().to_string()),
        "{stderr}"
    );
    assert!(stderr.contains("git repository"), "{stderr}");
}

#[test]
fn occupied_port_reports_bind_error() {
    let Ok(listener) = TcpListener::bind("127.0.0.1:0") else {
        return;
    };
    let repo = repo();
    let home = tempfile::tempdir().unwrap();
    let port = listener.local_addr().unwrap().port().to_string();
    let output = command(repo.path(), home.path())
        .args(["--port", &port])
        .output()
        .unwrap();
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(!output.status.success(), "{stderr}");
    assert!(
        stderr.to_lowercase().contains("address already in use"),
        "{stderr}"
    );
}

fn assert_private_runtime(runtime: &Path) {
    assert_eq!(
        std::fs::metadata(runtime).unwrap().permissions().mode() & 0o777,
        0o700
    );
    assert_eq!(
        std::fs::metadata(runtime.join("lock"))
            .unwrap()
            .permissions()
            .mode()
            & 0o777,
        0o600
    );
}

fn wait_for_exit(server: &mut Server) -> std::process::ExitStatus {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        if let Some(status) = server.0.try_wait().unwrap() {
            return status;
        }
        assert!(
            Instant::now() < deadline,
            "server did not stop within five seconds"
        );
        std::thread::sleep(Duration::from_millis(10));
    }
}

fn read_stderr(server: &mut Server) -> String {
    use std::io::Read;
    let mut stderr = String::new();
    server
        .0
        .stderr
        .take()
        .unwrap()
        .read_to_string(&mut stderr)
        .unwrap();
    stderr
}

#[test]
fn runtime_is_private_ignores_session_state_and_sigint_stops_cleanly() {
    if !loopback_available() {
        return;
    }
    let repo = repo();
    let home = tempfile::tempdir().unwrap();
    let other = tempfile::tempdir().unwrap();
    let key = project_key(repo.path()).unwrap();
    let ignored = other.path().join(&key);
    std::fs::create_dir(&ignored).unwrap();
    let mut cmd = command(repo.path(), home.path());
    cmd.env("LOOM_PLAN_STATE", &ignored);
    let (mut server, url) = start(&mut cmd);
    let (authority, path) = url
        .strip_prefix("http://")
        .unwrap()
        .split_once('/')
        .unwrap();
    let addr: SocketAddr = authority.parse().unwrap();
    assert_ne!(addr.port(), 0, "{url}");
    assert!(
        path.starts_with("?token=") && path.len() > "?token=".len(),
        "{url}"
    );
    assert_private_runtime(&home.path().join("plan").join(key));
    assert_eq!(std::fs::read_dir(&ignored).unwrap().count(), 0);
    kill(Pid::from_raw(server.0.id() as i32), Signal::SIGINT).unwrap();
    let status = wait_for_exit(&mut server);
    assert!(status.success(), "{status}");
    let stderr = read_stderr(&mut server);
    assert!(
        stderr.contains("LOOM_PLAN_STATE") && stderr.contains("ignored"),
        "{stderr}"
    );
    let status = git(repo.path(), &["status", "--porcelain"]);
    assert!(status.status.success());
    assert!(
        status.stdout.is_empty(),
        "{}",
        String::from_utf8_lossy(&status.stdout)
    );
}
