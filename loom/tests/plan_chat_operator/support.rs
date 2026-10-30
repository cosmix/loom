//! Fixture repository, server start-up and cookie/HTTP/WebSocket helpers for the operator proof.

use std::ffi::OsString;
use std::fs;
use std::net::{IpAddr, Ipv4Addr, TcpStream};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant};

use anyhow::{bail, ensure, Context, Result};
use loom::claude::auth::{operator_auth_status, AuthProbe};
use loom::fs::permissions::untrust_worktree;
use loom::plan_server::ids::PublicId;
use loom::plan_server::prereq::ReadOnly;
use loom::plan_server::wire::chat::ChatCommand;
use loom::plan_server::wire::message::{ClientMessage, PlanMessage};
use loom::plan_server::{start, RunningServer, ServerOptions};
use loom::plan_session::attachments::{AttachmentInput, AttachmentKind};
use loom::plan_session::bridge::registry::read_record;
use loom::plan_session::bridge::Bridge;
use loom::plan_session::kind::SessionDir;
use loom::planning::runtime::server_runtime_paths;
use loom::process::{run_bounded, BoundedOutput};
use reqwest::blocking::Client;
use tempfile::TempDir;
use tungstenite::client::IntoClientRequest;
use tungstenite::{Message, WebSocket};

pub const TARGET: &str = "doc/product/demo/section_01.md";
pub const SOURCE: &str = "# Notes\n\nCreate, edit and delete plain-text notes stored locally.\n";
pub type Socket = WebSocket<TcpStream>;

pub fn claude_version() -> String {
    let result = (|| -> Result<String> {
        let claude = loom::claude::find_claude_path().context("claude is missing")?;
        let output = run_bounded(
            Command::new(claude).arg("--version"),
            Duration::from_secs(20),
        )?;
        match output {
            BoundedOutput::Completed(output) => Ok(format!(
                "{} ({}): {}",
                String::from_utf8_lossy(&output.stdout).trim(),
                output.status,
                String::from_utf8_lossy(&output.stderr).trim()
            )),
            BoundedOutput::TimedOut => bail!("claude --version timed out"),
        }
    })();
    result.unwrap_or_else(|error| format!("unavailable: {error:#}"))
}

pub fn prerequisites() -> Result<()> {
    which::which("tmux").context("tmux is missing; install tmux for the operator test")?;
    let claude =
        loom::claude::find_claude_path().context("claude is missing; install Claude Code")?;
    match operator_auth_status(&claude) {
        AuthProbe::LoggedIn { .. } => Ok(()),
        AuthProbe::NotLoggedIn => bail!("a logged-in claude is missing; run claude auth login"),
        AuthProbe::Unknown(reason) => bail!("cannot establish a logged-in claude: {reason}"),
    }
}

// Preserve HOME so Claude sees the operator's login, as in planning_guidance_operator.
pub struct LoomHome {
    _dir: TempDir,
    previous: Option<OsString>,
}

impl LoomHome {
    pub fn new() -> Result<Self> {
        let dir = tempfile::tempdir()?;
        let previous = std::env::var_os("LOOM_HOME");
        std::env::set_var("LOOM_HOME", dir.path());
        Ok(Self {
            _dir: dir,
            previous,
        })
    }
}

impl Drop for LoomHome {
    fn drop(&mut self) {
        match &self.previous {
            Some(value) => std::env::set_var("LOOM_HOME", value),
            None => std::env::remove_var("LOOM_HOME"),
        }
    }
}

struct SessionServers(PathBuf);

impl Drop for SessionServers {
    fn drop(&mut self) {
        let Ok(sessions) = fs::read_dir(&self.0) else {
            return;
        };
        for entry in sessions.flatten() {
            if let Ok(record) = read_record(&SessionDir::new(entry.path())) {
                let _ = Command::new("tmux")
                    .args(["-L", record.socket.as_str(), "kill-server"])
                    .output();
            }
        }
    }
}

pub struct ServerGuard {
    pub server: Option<RunningServer>,
    _sessions: SessionServers,
}

impl ServerGuard {
    /// Starts the planning server in-process on port 0 over `root`.
    pub fn start(root: &Path) -> Result<Self> {
        let (runtime, _) = server_runtime_paths(root)?;
        let sessions = SessionServers(runtime.sessions);
        let server = start(ServerOptions {
            root: root.to_path_buf(),
            host: IpAddr::V4(Ipv4Addr::LOCALHOST),
            port: Some(0),
            read_only: ReadOnly::from_problems(vec![]),
        })?;
        Ok(Self {
            server: Some(server),
            _sessions: sessions,
        })
    }
}

impl Drop for ServerGuard {
    fn drop(&mut self) {
        if let Some(server) = self.server.take() {
            if let Some(bridge) = server.ctx.extensions().get::<Bridge>() {
                bridge.abort();
            }
            let root = server.ctx.root().to_path_buf();
            let result = server.stop().and_then(|()| untrust_worktree(&root));
            if let Err(error) = result {
                eprintln!(
                    "claude --version: {}\noperator cleanup failed: {error:#}",
                    claude_version()
                );
            }
        }
    }
}

pub fn fixture() -> Result<TempDir> {
    let repo = tempfile::tempdir()?;
    fs::create_dir_all(repo.path().join("doc/product/demo"))?;
    fs::create_dir_all(repo.path().join("doc/plans/demo"))?;
    for (path, text) in [
        ("doc/product/demo/index.md", "# Pocket notes\n\nA local plain-text notebook for one person.\n\n\
         ## Specification sections\n\n| # | Section |\n| --- | --- |\n| 01 | Notes |\n| 02 | Search |\n"),
        (TARGET, SOURCE),
        ("doc/product/demo/section_02.md", "# Search\n\nFind local notes by a case-insensitive substring.\n"),
        ("doc/plans/demo/index.md", "# Metaplan\n\n## 1. Notes (`PLAN-notes.md`)\n\nImplement section 01.\n"),
        ("doc/plans/demo/PLAN-notes.md", "# Notes\n\n## Goal\n\nImplement local note storage and editing from section 01.\n\n\
         ## Stages\n\n### storage\n\nStore each note in a local plain-text file.\n"),
    ] {
        fs::write(repo.path().join(path), text)?;
    }
    git(repo.path(), &["init", "-q", "-b", "main"])?;
    git(repo.path(), &["add", "--", "doc"])?;
    git(
        repo.path(),
        &["commit", "-q", "-m", "seed operator fixture"],
    )?;
    Ok(repo)
}

fn git(root: &Path, args: &[&str]) -> Result<()> {
    let output = Command::new("git")
        .args([
            "-c",
            "user.name=Operator test",
            "-c",
            "user.email=test@example.com",
            "-c",
            "commit.gpgsign=false",
            "-c",
            "core.hooksPath=/dev/null",
        ])
        .args(args)
        .current_dir(root)
        .env_remove("GIT_DIR")
        .env_remove("GIT_WORK_TREE")
        .env_remove("GIT_INDEX_FILE")
        .output()?;
    ensure!(
        output.status.success(),
        "fixture git {args:?}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    Ok(())
}

pub fn http_client() -> Result<Client> {
    Ok(Client::builder()
        .no_proxy()
        .redirect(reqwest::redirect::Policy::none())
        .timeout(Duration::from_secs(20))
        .build()?)
}

pub fn attachment() -> AttachmentInput {
    AttachmentInput {
        kind: AttachmentKind::Source,
        path: TARGET.into(),
        anchor: None,
        lines: None,
        stage: None,
        summary_sentence: None,
        rests_on: vec![],
        quote: SOURCE.into(),
        selected_hash: None,
    }
}

// Cookie bootstrap and handshake mirror plan_server_contracts and plan_server/tests/support.
pub fn bootstrap(server: &RunningServer, client: &Client) -> Result<String> {
    let response = client
        .get(format!("http://{}/?token={}", server.local, server.token))
        .send()?;
    ensure!(
        response.status().as_u16() == 302,
        "token bootstrap: {}",
        response.status()
    );
    let cookies = response
        .headers()
        .get_all("set-cookie")
        .iter()
        .map(|header| Ok(header.to_str()?.split(';').next().unwrap_or("").to_owned()))
        .collect::<Result<Vec<String>>>()?
        .join("; ");
    for name in [
        format!("loom_plan_{}=", server.local.port()),
        format!("loom_plan_browser_{}=", server.local.port()),
    ] {
        ensure!(cookies.contains(&name), "bootstrap did not issue {name}");
    }
    Ok(cookies)
}

pub fn connect(server: &RunningServer, cookies: &str) -> Result<Socket> {
    let stream = TcpStream::connect_timeout(&server.local, Duration::from_secs(20))?;
    stream.set_read_timeout(Some(Duration::from_millis(250)))?;
    stream.set_write_timeout(Some(Duration::from_secs(20)))?;
    let mut request = format!("ws://{}/ws", server.local).into_client_request()?;
    request
        .headers_mut()
        .insert("Origin", format!("http://{}", server.local).parse()?);
    request.headers_mut().insert("Cookie", cookies.parse()?);
    let (socket, _) =
        tungstenite::client(request, stream).context("planning WebSocket handshake")?;
    Ok(socket)
}

pub fn next(socket: &mut Socket) -> Result<Option<PlanMessage>> {
    match socket.read() {
        Ok(Message::Text(text)) => Ok(Some(serde_json::from_str(text.as_str())?)),
        Ok(Message::Close(frame)) => bail!("planning WebSocket closed: {frame:?}"),
        Ok(_) => {
            socket.flush()?;
            Ok(None)
        }
        Err(tungstenite::Error::Io(error))
            if matches!(
                error.kind(),
                std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
            ) =>
        {
            Ok(None)
        }
        Err(error) => Err(error.into()),
    }
}

pub fn hello(socket: &mut Socket) -> Result<PublicId> {
    let deadline = Instant::now() + Duration::from_secs(20);
    loop {
        if let Some(PlanMessage::Hello(hello)) = next(socket)? {
            return Ok(hello.you.public_id);
        }
        ensure!(Instant::now() < deadline, "WebSocket did not send hello");
    }
}

pub fn send(socket: &mut Socket, command: ChatCommand) -> Result<()> {
    socket.send(Message::Text(
        serde_json::to_string(&ClientMessage::Chat(command))?.into(),
    ))?;
    Ok(())
}
