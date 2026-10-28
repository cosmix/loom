//! Contract tests for stage plan-server-core: the planning server's access gate, authoring
//! model, ids, display names, lock, project root, browser opener and the bare `loom plan`.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::{IpAddr, Ipv4Addr, SocketAddr, TcpStream};
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::sync::mpsc;
use std::time::{Duration, SystemTime};

use loom::plan_server::authoring::{Authoring, Refusal, Take};
use loom::plan_server::browser::{decide, EnvView, OpenDecision, SkipReason};
use loom::plan_server::ids::{BrowserId, PublicId};
use loom::plan_server::lock::{acquire, Acquired};
use loom::plan_server::prereq::ReadOnly;
use loom::plan_server::presence::sanitize_name;
use loom::plan_server::root::resolve_project;
use loom::plan_server::{start, RunningServer, ServerOptions};
use reqwest::blocking::{Client, Response};
use tempfile::TempDir;

fn git(dir: &Path, args: &[&str]) {
    let status = Command::new("git")
        .args(["-c", "user.name=t", "-c", "user.email=t@example.com"])
        .args(args)
        .current_dir(dir)
        .env_remove("GIT_DIR")
        .env_remove("GIT_WORK_TREE")
        .status()
        .expect("run git");
    assert!(status.success(), "git {args:?} failed");
}

/// A temp git repository on branch `main` with one commit and `doc/product/index.md`.
fn project_repo() -> TempDir {
    let dir = tempfile::tempdir().expect("tempdir");
    git(dir.path(), &["init", "-q", "-b", "main"]);
    std::fs::create_dir_all(dir.path().join("doc/product")).unwrap();
    std::fs::write(dir.path().join("doc/product/index.md"), "# Demo\n").unwrap();
    git(dir.path(), &["add", "."]);
    git(dir.path(), &["commit", "-q", "-m", "init"]);
    dir
}

fn serve(repo: &TempDir) -> RunningServer {
    start(ServerOptions {
        root: repo.path().to_path_buf(),
        host: IpAddr::V4(Ipv4Addr::LOCALHOST),
        port: Some(0),
        read_only: ReadOnly::from_problems(Vec::new()),
    })
    .expect("start the planning server")
}

fn client() -> Client {
    Client::builder()
        .no_proxy()
        .redirect(reqwest::redirect::Policy::none())
        .timeout(Duration::from_secs(20))
        .build()
        .unwrap()
}

fn get(addr: SocketAddr, path: &str, cookie: &str) -> Response {
    let mut req = client().get(format!("http://{addr}{path}"));
    if !cookie.is_empty() {
        req = req.header("Cookie", cookie);
    }
    req.send().expect("GET")
}

/// The `name=value` pairs of every Set-Cookie header, joined for a Cookie header.
fn cookies_of(resp: &Response) -> String {
    resp.headers()
        .get_all("set-cookie")
        .iter()
        .filter_map(|v| v.to_str().ok())
        .filter_map(|v| v.split(';').next())
        .map(str::trim)
        .collect::<Vec<_>>()
        .join("; ")
}

#[test]
fn api_without_cookie_is_401_on_loopback() {
    let repo = project_repo();
    let server = serve(&repo);
    let resp = get(server.local, "/api/server", "");
    assert_eq!(
        resp.status().as_u16(),
        401,
        "viewing routes need the access cookie on loopback"
    );
    let _ = server.stop();
}

#[test]
fn token_on_other_path_sets_no_cookie() {
    let repo = project_repo();
    let server = serve(&repo);
    let port = server.local.port();
    let first = get(
        server.local,
        &format!("/files/x.md?token={}", server.token),
        "",
    );
    let cookies = cookies_of(&first);
    assert!(
        !cookies.contains(&format!("loom_plan_{port}=")),
        "?token= off the root path must set no access cookie: {cookies}"
    );
    let second = get(server.local, "/api/server", &cookies);
    assert_eq!(second.status().as_u16(), 401);
    // Positive control: the same token on `/` does bootstrap.
    let boot = get(server.local, &format!("/?token={}", server.token), "");
    assert!(cookies_of(&boot).contains(&format!("loom_plan_{port}=")));
    let _ = server.stop();
}

/// `install_all` must run `chat::install` before `guidance::install`: guidance looks up the
/// chat module's slot and, finding none, installs nothing, which makes the brief route answer
/// 500 `internal` for every id. An unknown id answering 404 `unknown_session` proves the
/// provider reached the context extensions.
#[test]
fn brief_route_has_its_provider_after_the_real_install_order() {
    let repo = project_repo();
    let server = serve(&repo);
    let boot = get(server.local, &format!("/?token={}", server.token), "");
    let cookies = cookies_of(&boot);
    let resp = get(
        server.local,
        "/api/session/brief?session=no-such-session",
        &cookies,
    );
    let status = resp.status().as_u16();
    let body = resp.text().expect("brief body");
    assert_eq!(status, 404, "unknown session id: {body}");
    assert!(body.contains("unknown_session"), "{body}");
    let _ = server.stop();
}

#[test]
fn ws_upgrade_with_loopback_alias_origin_is_refused() {
    let repo = project_repo();
    let server = serve(&repo);
    let port = server.local.port();
    let boot = get(server.local, &format!("/?token={}", server.token), "");
    let cookies = cookies_of(&boot);
    assert!(
        cookies.contains(&format!("loom_plan_browser_{port}=")),
        "{cookies}"
    );
    let mut stream = TcpStream::connect(server.local).unwrap();
    stream
        .set_read_timeout(Some(Duration::from_secs(20)))
        .unwrap();
    let request = format!(
        "GET /ws HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nOrigin: http://localhost:{port}\r\n\
         Cookie: {cookies}\r\nUpgrade: websocket\r\nConnection: Upgrade\r\n\
         Sec-WebSocket-Version: 13\r\nSec-WebSocket-Key: dGhlIHNhbXBsZSBub25jZQ==\r\n\r\n"
    );
    stream.write_all(request.as_bytes()).unwrap();
    let mut status_line = String::new();
    BufReader::new(&mut stream)
        .read_line(&mut status_line)
        .unwrap();
    let code = status_line.split_whitespace().nth(1).unwrap_or("");
    assert_ne!(
        code, "101",
        "a loopback alias Origin must not complete the handshake"
    );
    assert!(
        code.starts_with('4'),
        "expected a 4xx refusal, got {status_line:?}"
    );
    let _ = server.stop();
}

#[test]
fn second_browser_cannot_take_authoring_while_holder_away() {
    let auth = Authoring::new(false);
    let a = BrowserId::mint().unwrap();
    let pa = PublicId::mint();
    let b = BrowserId::mint().unwrap();
    let pb = PublicId::mint();
    let t0 = SystemTime::UNIX_EPOCH + Duration::from_secs(1_000_000);
    assert!(auth.require(&a, &pa, Take::TakeIfFree, false).is_ok());
    auth.connection_opened(&a);
    auth.connection_closed(&a, t0);
    auth.tick(t0 + Duration::from_secs(599));
    match auth.require(&b, &pb, Take::TakeIfFree, false) {
        Err(Refusal::NotHolder { holder }) => assert_eq!(holder, pa),
        other => panic!("B took authoring while A was away (ok: {})", other.is_ok()),
    }
    auth.tick(t0 + Duration::from_secs(600));
    assert!(auth.require(&b, &pb, Take::TakeIfFree, false).is_ok());
    assert!(auth.is_holder(&b));
}

#[test]
fn browser_ids_are_random_128_bit_and_never_printed() {
    let one = BrowserId::mint().unwrap();
    let two = BrowserId::mint().unwrap();
    assert_ne!(one, two);
    for id in [&one, &two] {
        let s = id.as_str();
        assert_eq!(s.len(), 32, "{} chars", s.len());
        assert!(s
            .bytes()
            .all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c)));
        assert!(
            !format!("{id:?}").contains(s),
            "Debug prints the cookie value"
        );
    }
    let hex = "0123456789abcdef0123456789abcdef";
    assert_eq!(
        BrowserId::parse(hex).map(|id| id.as_str().to_owned()),
        Some(hex.to_owned())
    );
    assert!(BrowserId::parse(&hex[..31]).is_none());
    assert!(BrowserId::parse(&format!("{hex}0")).is_none());
    assert!(BrowserId::parse(&hex.to_uppercase()).is_none());
    assert!(BrowserId::parse("0123456789abcdef0123456789abcdeg").is_none());
    assert_ne!(PublicId::mint(), PublicId::mint());
}

#[test]
fn display_names_drop_bidi_and_keep_32_chars() {
    assert_eq!(sanitize_name("\u{202E}\u{2066}Dana\u{0007}"), "Dana");
    let long = "é".repeat(40);
    assert_eq!(sanitize_name(&long), "é".repeat(32));
}

#[test]
fn second_lock_reports_url_and_token() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("lock");
    let Acquired::Held(mut held) = acquire(&path).expect("first acquire") else {
        panic!("first acquire must hold the lock");
    };
    held.publish("http://127.0.0.1:7272", "tok123").unwrap();
    match acquire(&path).expect("second acquire") {
        Acquired::Running(record) => {
            assert_eq!(record.url.as_deref(), Some("http://127.0.0.1:7272"));
            assert_eq!(record.token.as_deref(), Some("tok123"));
            assert_eq!(record.pid, std::process::id());
        }
        Acquired::Held(_) => panic!("second acquire took a lock that is already held"),
    }
    drop(held);
}

#[test]
fn symlinked_lock_path_is_refused() {
    let dir = tempfile::tempdir().unwrap();
    let target = dir.path().join("target");
    std::fs::write(&target, "keep").unwrap();
    let link = dir.path().join("lock");
    std::os::unix::fs::symlink(&target, &link).unwrap();
    let result = acquire(&link);
    assert!(result.is_err(), "a symlinked lock path must fail closed");
    assert_eq!(std::fs::read_to_string(&target).unwrap(), "keep");
}

#[test]
fn stage_branch_checkout_is_refused() {
    let repo = project_repo();
    let canonical = repo.path().canonicalize().unwrap();
    let resolved = resolve_project(&repo.path().join("doc")).expect("main checkout resolves");
    assert_eq!(resolved, canonical);
    git(repo.path(), &["checkout", "-q", "-b", "loom/demo"]);
    let refused = resolve_project(&repo.path().join("doc"));
    assert!(
        refused.is_err(),
        "a loom/ branch checkout is a stage worktree: {refused:?}"
    );
}

#[test]
fn ssh_session_skips_the_opener() {
    let env = EnvView {
        ssh_connection: false,
        ssh_tty: true,
        display: true,
        wayland: false,
        linux: true,
    };
    let decision = decide(&env, false);
    assert!(
        matches!(decision, OpenDecision::Skip(SkipReason::Ssh)),
        "an SSH session must not open a browser on the remote machine"
    );
}

/// Kills the spawned `loom plan` on every exit path.
struct ChildGuard(Child);

impl Drop for ChildGuard {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

/// Spawns `loom plan --no-open --port 0` in `repo` with a scratch `LOOM_HOME` and no other
/// inherited `LOOM_*` variable (a real session's identity must not reach the child), nor
/// `GIT_DIR` or `GIT_WORK_TREE`.
fn spawn_loom_plan(repo: &Path, home: &Path) -> (ChildGuard, mpsc::Receiver<String>) {
    let mut command = Command::new(env!("CARGO_BIN_EXE_loom"));
    for (key, _) in std::env::vars_os() {
        if key.to_string_lossy().starts_with("LOOM_") {
            command.env_remove(key);
        }
    }
    let mut child = command
        .args(["plan", "--no-open", "--port", "0"])
        .current_dir(repo)
        .env("LOOM_HOME", home)
        .env_remove("GIT_DIR")
        .env_remove("GIT_WORK_TREE")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn loom plan");
    let stdout = child.stdout.take().unwrap();
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        for line in BufReader::new(stdout).lines().map_while(Result::ok) {
            if tx.send(line).is_err() {
                break;
            }
        }
    });
    (ChildGuard(child), rx)
}

#[test]
fn bare_loom_plan_serves_the_planning_app() {
    let repo = project_repo();
    let home = tempfile::tempdir().unwrap();
    let (_child, lines) = spawn_loom_plan(repo.path(), home.path());
    let url = loop {
        let line = lines
            .recv_timeout(Duration::from_secs(60))
            .expect("loom plan printed no http://127.0.0.1: line");
        if let Some(at) = line.find("http://127.0.0.1:") {
            break line[at..].split_whitespace().next().unwrap().to_owned();
        }
    };
    assert!(url.contains("/?token="), "{url}");
    let rest = url.trim_start_matches("http://");
    let (authority, path) = rest.split_at(rest.find('/').unwrap());
    let addr: SocketAddr = authority.parse().expect("socket address in the URL");
    let boot = get(addr, path, "");
    let cookies = cookies_of(&boot);
    assert!(
        cookies.contains(&format!("loom_plan_{}=", addr.port())),
        "{cookies}"
    );
    let resp = get(addr, "/api/server", &cookies);
    assert_eq!(resp.status().as_u16(), 200);
    let mut body = String::new();
    resp.take(1 << 20).read_to_string(&mut body).unwrap();
    let info: serde_json::Value = serde_json::from_str(&body).expect("JSON body");
    let project = info["project"].as_str().unwrap_or("");
    assert!(!project.is_empty(), "server info names the project: {body}");
}
