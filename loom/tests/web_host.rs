//! Binary-level proof that `loom status --web 0 --host 0.0.0.0` binds a
//! wildcard listener, prints a working bootstrap URL, serves viewing routes
//! without the cookie, and refuses a cookieless settings write - all against
//! the real compiled binary, not the library directly.

use std::io::{Read, Write};
use std::net::{Ipv4Addr, TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::mpsc::{self, Receiver};
use std::sync::{Arc, Mutex};
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

/// Drain both pipes even after the diagnostic capture fills.
fn capture_output(mut output: impl Read + Send + 'static) -> (Arc<Mutex<Vec<u8>>>, Receiver<()>) {
    const CAP: usize = 16 * 1024;
    let captured = Arc::new(Mutex::new(Vec::new()));
    let sink = Arc::clone(&captured);
    let (done, finished) = mpsc::channel();
    thread::spawn(move || {
        let mut chunk = [0; 1024];
        loop {
            let Ok(count) = output.read(&mut chunk) else {
                break;
            };
            if count == 0 {
                break;
            }
            let mut bytes = sink.lock().expect("capture lock");
            let take = count.min(CAP.saturating_sub(bytes.len()));
            bytes.extend_from_slice(&chunk[..take]);
        }
        let _ = done.send(());
    });
    (captured, finished)
}

fn wait_for_start(
    mut guard: ChildGuard,
    needle: &str,
    timeout: Duration,
) -> Result<(ChildGuard, String), String> {
    let (stdout, stdout_done) = capture_output(guard.0.stdout.take().ok_or("missing stdout")?);
    let (stderr, stderr_done) = capture_output(guard.0.stderr.take().ok_or("missing stderr")?);
    let deadline = Instant::now() + timeout;
    loop {
        let text = String::from_utf8_lossy(&stdout.lock().expect("stdout lock")).into_owned();
        if let Some(line) = text
            .split_inclusive('\n')
            .find(|line| line.contains(needle) && line.ends_with('\n'))
        {
            return Ok((guard, line.to_owned()));
        }
        let status = guard
            .0
            .try_wait()
            .map_err(|error| format!("checking child: {error}"))?;
        if status.is_some() || Instant::now() >= deadline {
            if status.is_some() {
                let _ = stdout_done.recv_timeout(Duration::from_millis(100));
                let _ = stderr_done.recv_timeout(Duration::from_millis(100));
            }
            let text = String::from_utf8_lossy(&stdout.lock().expect("stdout lock")).into_owned();
            let errors = String::from_utf8_lossy(&stderr.lock().expect("stderr lock")).into_owned();
            return Err(format!(
                "dashboard startup failed ({status:?}); stdout: {text:?}; stderr: {errors:?}"
            ));
        }
        thread::sleep(Duration::from_millis(10));
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

/// Spawn the dashboard and require its bootstrap URL within the deadline.
fn spawn_remote_dashboard(base: &Path, home: &Path) -> Result<(ChildGuard, u16, String), String> {
    let child = loom_cmd(base, home, &["status", "--web", "0", "--host", "0.0.0.0"])
        .spawn()
        .map_err(|error| format!("spawning dashboard: {error}"))?;
    let (guard, line) = wait_for_start(
        ChildGuard(child),
        "bootstrap from this machine at: http://127.0.0.1:",
        STARTUP_DEADLINE,
    )?;
    let (port, token) = parse_bootstrap_url(&line);
    Ok((guard, port, token))
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

#[cfg(unix)]
#[test]
fn startup_failure_and_timeout_are_errors_with_diagnostics() {
    for (script, expected) in [
        (
            "printf 'early failure\\n' >&2; sleep 0.1; exit 7",
            "early failure",
        ),
        (
            "printf 'waiting for startup\\n'; sleep 2",
            "waiting for startup",
        ),
    ] {
        let child = Command::new("sh")
            .args(["-c", script])
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("spawn fixture");
        let result = wait_for_start(ChildGuard(child), "bootstrap:", Duration::from_millis(200));
        let error = match result {
            Ok(_) => panic!("fixture unexpectedly started"),
            Err(error) => error,
        };
        assert!(error.contains(expected), "{error}");
    }
}

/// `loom status --web 0 --host 0.0.0.0`, in a disposable workspace, proves the
/// remote-mode contract against the real binary: viewing needs no cookie, the
/// bootstrap redirect sets one, and a settings write without it is refused.
#[test]
fn remote_wildcard_dashboard_serves_viewing_without_the_cookie() {
    let (_temp, base, home) = workspace();
    const NAME: &str = "remote_wildcard_dashboard_serves_viewing_without_the_cookie";
    match TcpListener::bind((Ipv4Addr::UNSPECIFIED, 0)) {
        Ok(listener) => drop(listener),
        Err(error)
            if matches!(
                error.kind(),
                std::io::ErrorKind::PermissionDenied
                    | std::io::ErrorKind::AddrNotAvailable
                    | std::io::ErrorKind::Unsupported
            ) =>
        {
            eprintln!("SKIP {NAME}: wildcard bind unavailable: {error}");
            return;
        }
        Err(error) => panic!("wildcard bind probe failed: {error}"),
    }
    let (_guard, port, token) =
        spawn_remote_dashboard(&base, home.path()).expect("dashboard startup");
    assert_eq!(token.len(), 64, "process token must be 64 hex characters");
    let host = format!("Host: 127.0.0.1:{port}\r\n");

    for path in ["/", "/api/status"] {
        let viewed = request(port, &format!("GET {path} HTTP/1.1\r\n{host}\r\n"));
        assert!(viewed.starts_with("HTTP/1.1 200"), "{path}: {viewed}");
    }

    let bootstrap = request(port, &format!("GET /?token={token} HTTP/1.1\r\n{host}\r\n"));
    assert!(bootstrap.starts_with("HTTP/1.1 302"), "{bootstrap}");
    assert!(bootstrap.contains("Location: /"), "{bootstrap}");
    assert!(!cookie_value(&bootstrap).is_empty());

    let body = "{}";
    let write = request(
        port,
        &format!(
            "POST /api/config HTTP/1.1\r\n{host}Origin: http://127.0.0.1:{port}\r\n\
             Content-Type: application/json\r\nContent-Length: {}\r\n\r\n{body}",
            body.len()
        ),
    );
    assert!(write.starts_with("HTTP/1.1 401"), "{write}");
}
