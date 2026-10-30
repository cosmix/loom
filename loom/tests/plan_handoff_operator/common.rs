//! Helpers of the operator runs: the scratch repository, a browser session on the planning
//! server, the run routes' start calls and the `loom plan` child.

use std::ffi::OsString;
use std::io::{BufRead, BufReader};
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::mpsc;
use std::time::Duration;

use loom::plan_server::prereq::ReadOnly;
use loom::plan_server::{start, RunningServer, ServerOptions};
use loom::plan_session::bridge::registry::read_record;
use loom::plan_session::bridge::Bridge;
use loom::plan_session::kind::{SessionDir, SessionKind};
use loom::planning::runtime::{create_runtime_dir, server_runtime_paths};
use loom::pressure::codex_run::codex_skill_installed;
use reqwest::blocking::Client;
use serde_json::{json, Value};
use tempfile::TempDir;

pub const PLAN: &str = "doc/plans/PLAN-operator.md";
pub const REPORT: &str = "doc/plans/codex-PLAN-operator.md";
pub const MISSING: &str = "claude, codex with its pressure skill, or tmux is not on PATH";

/// One standard stage, an integration-verify and a knowledge-distill stage: a plan `loom init`
/// accepts.
const OPERATOR_PLAN: &str = r#"# PLAN: Operator fixture

Greet a user by name.

---

<!-- loom METADATA -->

```yaml
loom:
  version: 2
  sandbox:
    enabled: true
  stages:
    - id: add-greeting
      name: "Add a greeting"
      stage_type: standard
      working_dir: "."
      dependencies: []
      description: "Greet a user by name in greeting.txt."
      acceptance:
        - "test -f greeting.txt"
    - id: integration-verify
      name: "Integration verification"
      stage_type: integration-verify
      working_dir: "."
      dependencies: ["add-greeting"]
      description: "Verify the merged tree."
      acceptance:
        - "test -f greeting.txt"
    - id: knowledge-distill
      name: "Knowledge distillation"
      stage_type: knowledge-distill
      working_dir: "."
      dependencies: ["integration-verify"]
      description: "Curate the stage memories into knowledge."
      acceptance:
        - "true"
```

<!-- END loom METADATA -->
"#;

pub fn tools_ready() -> bool {
    ["claude", "codex", "tmux"]
        .iter()
        .all(|tool| which::which(tool).is_ok())
        && codex_skill_installed()
}

/// A scratch git repository with one written plan; `LOOM_HOME` points at a directory of its
/// own until dropped, so the runtime directory is the test's.
pub struct Scratch {
    pub root: PathBuf,
    previous_home: Option<OsString>,
    // Drop session servers before either temporary directory is removed.
    _sessions: SessionServers,
    _repo: TempDir,
    home: TempDir,
}

impl Scratch {
    pub fn new() -> Scratch {
        let (repo, home) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
        let root = repo.path().canonicalize().unwrap();
        git(&root, &["init", "-q", "-b", "main"]);
        git(&root, &["config", "user.name", "Operator"]);
        git(&root, &["config", "user.email", "operator@example.invalid"]);
        put(
            &root,
            "doc/product/index.md",
            "# Greeting\n\nGreet users by name.\n",
        );
        put(&root, PLAN, OPERATOR_PLAN);
        git(&root, &["add", "-A"]);
        git(&root, &["commit", "-q", "-m", "plan"]);
        let previous_home = std::env::var_os("LOOM_HOME");
        std::env::set_var("LOOM_HOME", home.path());
        let (runtime, _) = server_runtime_paths(&root).unwrap();
        Scratch {
            root,
            previous_home,
            _sessions: SessionServers(runtime.sessions),
            _repo: repo,
            home,
        }
    }

    /// A `loom` child in the repository. The sanctioned-spawner scrub: every inherited `LOOM_*`
    /// variable is removed (a real session's identity must not reach the child), then
    /// `LOOM_HOME` is set to this scratch's, so the child never reads the operator's `~/.loom`.
    /// `GIT_DIR` and `GIT_WORK_TREE` are removed too, as they would redirect its git calls.
    pub fn loom(&self) -> Command {
        let mut command = Command::new(env!("CARGO_BIN_EXE_loom"));
        for (key, _) in std::env::vars_os() {
            if key.to_string_lossy().starts_with("LOOM_") {
                command.env_remove(key);
            }
        }
        command
            .current_dir(&self.root)
            .env("LOOM_HOME", self.home.path())
            .env_remove("GIT_DIR")
            .env_remove("GIT_WORK_TREE");
        command
    }

    pub fn records(&self) -> usize {
        let text = std::fs::read_to_string(self.root.join(PLAN)).unwrap();
        let after = text.split("## Pressure tests").nth(1).unwrap_or("");
        after.lines().filter(|line| line.starts_with("- ")).count()
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        match self.previous_home.take() {
            Some(home) => std::env::set_var("LOOM_HOME", home),
            None => std::env::remove_var("LOOM_HOME"),
        }
    }
}

struct SessionServers(PathBuf);

impl Drop for SessionServers {
    fn drop(&mut self) {
        let Ok(sessions) = std::fs::read_dir(&self.0) else {
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

pub fn put(root: &Path, relative: &str, text: &str) {
    let path = root.join(relative);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, text).unwrap();
}

fn git(root: &Path, args: &[&str]) {
    let status = Command::new("git")
        .args(args)
        .current_dir(root)
        .status()
        .unwrap();
    assert!(status.success(), "git {args:?}");
}

/// A browser session on a planning server: its cookies and origin.
pub struct Browser {
    client: Client,
    addr: SocketAddr,
    cookies: String,
}

impl Browser {
    /// `GET /?token=` with the server's own `Host`, keeping the two cookies of the 302.
    fn bootstrap(addr: SocketAddr, token: &str) -> Browser {
        let client = Client::builder()
            .no_proxy()
            .redirect(reqwest::redirect::Policy::none())
            .timeout(Duration::from_secs(60))
            .build()
            .unwrap();
        let boot = client
            .get(format!("http://{addr}/?token={token}"))
            .header("Host", format!("127.0.0.1:{}", addr.port()))
            .send()
            .unwrap();
        let cookies: Vec<&str> = boot
            .headers()
            .get_all("set-cookie")
            .iter()
            .filter_map(|value| value.to_str().ok()?.split(';').next())
            .collect();
        let cookies = cookies.join("; ");
        assert!(
            cookies.contains(&format!("loom_plan_browser_{}", addr.port())),
            "{cookies}"
        );
        Browser {
            client,
            addr,
            cookies,
        }
    }

    fn send(&self, request: reqwest::blocking::RequestBuilder) -> (u16, Value) {
        let request = request
            .header("Cookie", &self.cookies)
            .header("Origin", format!("http://{}", self.addr));
        let response = request.send().unwrap();
        let status = response.status().as_u16();
        (status, response.json().unwrap_or(Value::Null))
    }

    pub fn get(&self, path: &str) -> Value {
        let (status, body) = self.send(self.client.get(format!("http://{}{path}", self.addr)));
        assert_eq!(status, 200, "{path}: {body}");
        body
    }

    pub fn post(&self, path: &str, body: Value) -> (u16, Value) {
        let url = format!("http://{}{path}", self.addr);
        self.send(self.client.post(url).json(&body))
    }
}

pub fn serve(scratch: &Scratch) -> (RunningServer, Browser) {
    let (runtime, _) = server_runtime_paths(&scratch.root).unwrap();
    create_runtime_dir(&runtime).unwrap();
    let server = start(ServerOptions {
        root: scratch.root.clone(),
        host: "127.0.0.1".parse().unwrap(),
        port: Some(0),
        read_only: ReadOnly::from_problems(vec![]),
    })
    .unwrap();
    let browser = Browser::bootstrap(server.local, &server.token);
    (server, browser)
}

/// Start a one-round run on [`PLAN`] with the dialog's defaults.
pub fn start_run(browser: &Browser) {
    let options = browser.get("/api/pressure/options");
    assert_eq!(options["available"], true, "{options}");
    let mut body = options["defaults"].clone();
    body["plan"] = json!(PLAN);
    body["rounds"] = json!(1);
    let (status, answer) = browser.post("/api/pressure/start", body);
    assert_eq!(status, 202, "{answer}");
}

pub fn running(browser: &Browser) -> bool {
    browser.get("/api/pressure/options")["running"] == true
}

/// The tmux pane count of every live pressure session.
pub fn pressure_panes(server: &RunningServer, bridge: &Bridge) -> Vec<usize> {
    let sessions = bridge.sessions().into_iter();
    let pressure = sessions.filter(|(_, kind, _)| matches!(kind, SessionKind::Pressure { .. }));
    let sockets = pressure.filter_map(|(id, ..)| {
        let dir = SessionDir::new(server.ctx.runtime().sessions.join(id));
        read_record(&dir).ok().map(|record| record.socket)
    });
    let panes = sockets.filter_map(|socket| {
        let args = ["-L", socket.as_str(), "list-panes", "-a"];
        let output = Command::new("tmux").args(args).output().ok()?;
        let listing = String::from_utf8_lossy(&output.stdout).into_owned();
        output.status.success().then(|| listing.lines().count())
    });
    panes.collect()
}

/// A `loom plan` child, killed and reaped on drop so a failed assertion cannot leave it running.
pub struct PlanChild(Child);

impl PlanChild {
    /// Kill and reap the child; harmless once it has exited.
    pub fn stop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

impl Drop for PlanChild {
    fn drop(&mut self) {
        self.stop();
    }
}

/// `loom plan` as a child, so the run routes start the real `loom` (their `current_exe`).
pub fn spawn_loom_plan(scratch: &Scratch) -> (PlanChild, Browser) {
    let mut child = PlanChild(
        scratch
            .loom()
            .args(["plan", "--no-open", "--port", "0"])
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .unwrap(),
    );
    let (sender, lines) = mpsc::channel();
    let stdout = child.0.stdout.take().unwrap();
    std::thread::spawn(move || {
        for line in BufReader::new(stdout).lines().map_while(Result::ok) {
            let _ = sender.send(line);
        }
    });
    let url = loop {
        let line = lines
            .recv_timeout(Duration::from_secs(60))
            .expect("loom plan printed no URL");
        if let Some(at) = line.find("http://127.0.0.1:") {
            break line[at..].split_whitespace().next().unwrap().to_owned();
        }
    };
    let rest = url.trim_start_matches("http://");
    let (authority, query) = rest.split_once("/?token=").unwrap();
    let addr: SocketAddr = authority.parse().unwrap();
    let browser = Browser::bootstrap(
        SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), addr.port()),
        query,
    );
    (child, browser)
}
