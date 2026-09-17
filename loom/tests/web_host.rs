//! Binary-level proof that `loom status --web 0 --host 0.0.0.0` binds a
//! wildcard listener, prints a working bootstrap URL, and enforces the
//! remote-mode cookie on every other route - all against the real compiled
//! binary, not the library directly.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpStream;
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdout, Command, Stdio};
use std::sync::mpsc::{self, Receiver};
use std::thread;
use std::time::{Duration, Instant};

use loom::fs::work_dir::WorkDir;

/// Path to the `loom` binary this test run built.
const LOOM: &str = env!("CARGO_BIN_EXE_loom");

/// Every `LOOM_*` variable a spawned loom session may export, cleared so this
/// test process's own environment never leaks into the child under test.
const RELAY_ENV_VARS: &[&str] = &[
    "LOOM_SESSION_ID",
    "LOOM_STAGE_ID",
    "LOOM_WORK_DIR",
    "LOOM_WORKTREE_PATH",
    "LOOM_MAIN_AGENT_PID",
    "LOOM_SESSION_TYPE",
    "LOOM_MERGE_SESSION",
    "LOOM_SCRATCH_DIR",
    "LOOM_BIN",
    "LOOM_HOOK_PATH",
    "LOOM_HOOK_CONTEXT",
    "LOOM_CONTROL_BROKER",
];

const STARTUP_DEADLINE: Duration = Duration::from_secs(10);

/// Kills and waits on the child on every exit path, including a panic.
struct ChildGuard(Child);

impl Drop for ChildGuard {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

/// A fresh `.loom/work` tree and the scratch `LOOM_HOME` beside it - never
/// the operator's real config or session state.
fn workspace() -> (tempfile::TempDir, PathBuf, tempfile::TempDir) {
    let temp = tempfile::tempdir().expect("create temp workspace");
    let base = temp.path().to_path_buf();
    WorkDir::new(&base)
        .expect("build work dir")
        .initialize()
        .expect("initialize work dir");
    let home = tempfile::tempdir().expect("create scratch LOOM_HOME");
    std::fs::write(home.path().join("config.toml"), "[update]\ncheck = false\n")
        .expect("write scratch user config");
    (temp, base, home)
}

fn loom_cmd(base: &Path, home: &Path, args: &[&str]) -> Command {
    let mut command = Command::new(LOOM);
    for var in RELAY_ENV_VARS {
        command.env_remove(var);
    }
    command.env("LOOM_HOME", home);
    command.current_dir(base);
    command.args(args);
    command.stdout(Stdio::piped());
    command.stderr(Stdio::piped());
    command
}

/// Stream a child's stdout into a channel on a background thread, so the
/// caller can wait for a specific line with a bounded deadline instead of
/// blocking forever on a process that never prints it.
fn spawn_line_reader(stdout: ChildStdout) -> Receiver<String> {
    let (sender, receiver) = mpsc::channel();
    thread::spawn(move || {
        let mut reader = BufReader::new(stdout);
        loop {
            let mut line = String::new();
            match reader.read_line(&mut line) {
                Ok(0) | Err(_) => break,
                Ok(_) => {
                    if sender.send(line).is_err() {
                        break;
                    }
                }
            }
        }
    });
    receiver
}

/// Read lines from `receiver` until one contains `needle`, or `deadline`
/// passes.
fn wait_for_line(receiver: &Receiver<String>, needle: &str, deadline: Instant) -> Option<String> {
    loop {
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            return None;
        }
        match receiver.recv_timeout(remaining) {
            Ok(line) if line.contains(needle) => return Some(line),
            Ok(_) => continue,
            Err(_) => return None,
        }
    }
}

/// Pull `(port, token)` out of a loopback bootstrap URL printed for the
/// wildcard listener: `... http://127.0.0.1:<port>/?token=<token>`.
fn parse_bootstrap_url(line: &str) -> (u16, String) {
    let marker = "http://127.0.0.1:";
    let start = line.find(marker).expect("loopback bootstrap URL present") + marker.len();
    let rest = &line[start..];
    let (port_str, rest) = rest.split_once('/').expect("path follows the port");
    let port: u16 = port_str.parse().expect("numeric port");
    let token_key = "token=";
    let token_start = rest.find(token_key).expect("token query present") + token_key.len();
    let token: String = rest[token_start..]
        .trim_end()
        .chars()
        .take_while(|c| c.is_ascii_hexdigit())
        .collect();
    (port, token)
}

/// Send one raw request and read the whole response back.
fn request(port: u16, request: &str) -> String {
    let mut stream = TcpStream::connect(("127.0.0.1", port)).expect("connect to dashboard");
    stream
        .set_read_timeout(Some(Duration::from_secs(5)))
        .expect("set read timeout");
    stream.write_all(request.as_bytes()).expect("write request");
    let mut response = String::new();
    let _ = stream.read_to_string(&mut response);
    response
}

/// Spawn `loom status --web 0 --host 0.0.0.0` and capture the port/token off
/// its printed loopback bootstrap line, or `None` when the process could not
/// be spawned or never printed one (no bindable wildcard address).
fn spawn_remote_dashboard(base: &Path, home: &Path) -> Option<(ChildGuard, u16, String)> {
    let mut child = loom_cmd(base, home, &["status", "--web", "0", "--host", "0.0.0.0"])
        .spawn()
        .ok()?;
    let stdout = child.stdout.take().expect("child stdout");
    let guard = ChildGuard(child);
    let lines = spawn_line_reader(stdout);
    let deadline = Instant::now() + STARTUP_DEADLINE;
    let line = wait_for_line(
        &lines,
        "bootstrap from this machine at: http://127.0.0.1:",
        deadline,
    )?;
    let (port, token) = parse_bootstrap_url(&line);
    Some((guard, port, token))
}

/// The value half of a response's `Set-Cookie` header.
fn cookie_value(response: &str) -> String {
    response
        .lines()
        .find(|line| line.starts_with("Set-Cookie:"))
        .unwrap_or_else(|| panic!("bootstrap must set a cookie: {response}"))
        .trim_start_matches("Set-Cookie:")
        .trim()
        .split(';')
        .next()
        .expect("cookie carries a value")
        .to_owned()
}

/// `loom status --web 0 --host 0.0.0.0`, in a disposable workspace, proves the
/// full remote-mode contract against the real binary: no-cookie refusal on
/// every route, a working bootstrap redirect, and an authenticated read.
#[test]
fn remote_wildcard_dashboard_requires_the_bootstrap_cookie() {
    let (_temp, base, home) = workspace();
    const NAME: &str = "remote_wildcard_dashboard_requires_the_bootstrap_cookie";
    let Some((_guard, port, token)) = spawn_remote_dashboard(&base, home.path()) else {
        eprintln!("SKIP {NAME}: dashboard did not start (no bindable wildcard address?)");
        return;
    };
    assert_eq!(token.len(), 64, "process token must be 64 hex characters");
    let host = format!("Host: 127.0.0.1:{port}\r\n");

    let unauthenticated = request(port, &format!("GET / HTTP/1.1\r\n{host}\r\n"));
    assert!(
        unauthenticated.starts_with("HTTP/1.1 401"),
        "{unauthenticated}"
    );

    let bootstrap = request(port, &format!("GET /?token={token} HTTP/1.1\r\n{host}\r\n"));
    assert!(bootstrap.starts_with("HTTP/1.1 302"), "{bootstrap}");
    assert!(bootstrap.contains("Location: /"), "{bootstrap}");

    let authenticated = request(
        port,
        &format!(
            "GET /api/status HTTP/1.1\r\n{host}Cookie: {}\r\n\r\n",
            cookie_value(&bootstrap)
        ),
    );
    assert!(authenticated.starts_with("HTTP/1.1 200"), "{authenticated}");
}
