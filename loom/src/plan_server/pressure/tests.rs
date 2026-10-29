//! Pressure runs over a fake session bridge (`fake`), a fake `codex` on `PATH` and a fake
//! `tmux`, against a planning context in a scratch directory. Every test is serial: the
//! fixture sets `LOOM_HOME`, `CODEX_HOME` and `PATH`.

mod authoring;
mod duties;
mod fake;
mod routes;
mod runs;

use std::ffi::{OsStr, OsString};
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use serde_json::{json, Value};
use tempfile::TempDir;

use self::fake::FakeSessions;
use super::install::{install_with, PressureRuns, Tools};
use crate::plan_server::chat::lease::AuthoringLeases;
use crate::plan_server::context::{BindInfo, PlanContext};
use crate::plan_server::ids::{BrowserId, ConnCtx};
use crate::plan_server::prereq::ReadOnly;
use crate::plan_server::request::{ApiRequest, QueryParams};
use crate::plan_server::summaries::rebuilds::Rebuilds;
use crate::plan_server::summaries::scheduler::SessionSpawner;
use crate::plan_server::wire::pressure::{PressureEnd, PressureRunView};
use crate::plan_server::wire::server::{MissingPrereq, PrereqKind};
use crate::plan_session::bridge::RecoveryReport;
use crate::planning::runtime::{create_runtime_dir, server_runtime_paths};
use crate::planning::summary::cost::Trigger;
use crate::planning::watch::OwnWrites;
use crate::web_server::http::parse_head;

pub(super) const PLAN: &str = "doc/plans/PLAN-a.md";
pub(super) const REPORT: &str = "doc/plans/codex-PLAN-a.md";
const PLAN_TEXT: &str = "# Plan A\n\nBody.\n\n<!-- END loom METADATA -->\n";

/// The fake `codex exec`: records `<pid> <model>` per attempt, then acts on the files beside
/// it: `unavailable-<model>` (an account error, exit 1), `report` (write the review), `config`
/// (edit `.claude/settings.local.json`), `sleep` (seconds, TERM-aware), `exit` (the code).
const FAKE_CODEX: &str = r##"#!/bin/sh
d=$(dirname "$0")
model=$5
root=$9
plan=${10#\$pressure }
echo "$$ $model" >> "$d/pids"
echo "codex fake run with $model"
if [ -f "$d/unavailable-$model" ]; then
  sleep 0.3
  echo "ERROR: model not found: $model"
  exit 1
fi
if [ -f "$d/report" ]; then
  echo "# Review" > "$root/$(dirname "$plan")/codex-$(basename "$plan")"
fi
if [ -f "$d/config" ]; then
  mkdir -p "$root/.claude"
  echo '{}' > "$root/.claude/settings.local.json"
fi
if [ -f "$d/sleep" ]; then
  trap 'kill $child 2>/dev/null; exit 143' TERM
  sleep "$(cat "$d/sleep")" &
  child=$!
  wait $child
fi
echo "codex fake done"
exit "$(cat "$d/exit" 2>/dev/null || echo 0)"
"##;

/// The fake `tmux list-panes`: as many panes as `panes` says (one by default).
const FAKE_TMUX: &str = r#"#!/bin/sh
d=$(dirname "$0")
n=$(cat "$d/panes" 2>/dev/null || echo 1)
i=0
while [ "$i" -lt "$n" ]; do
  echo "plan-pressure:0.$i: [80x24] %$i"
  i=$((i + 1))
done
"#;

/// Sets environment variables until dropped, then restores them.
pub(super) struct EnvGuard(Vec<(&'static str, Option<OsString>)>);

impl EnvGuard {
    pub(super) fn set(&mut self, key: &'static str, value: impl AsRef<OsStr>) {
        if !self.0.iter().any(|(saved, _)| *saved == key) {
            self.0.push((key, std::env::var_os(key)));
        }
        std::env::set_var(key, value);
    }
}

impl Drop for EnvGuard {
    fn drop(&mut self) {
        for (key, old) in self.0.drain(..).rev() {
            match old {
                Some(value) => std::env::set_var(key, value),
                None => std::env::remove_var(key),
            }
        }
    }
}

/// Counts summary sessions started: plan 10's test seam for the suspension.
#[derive(Default)]
pub(super) struct CountingSpawner(AtomicUsize);

impl SessionSpawner for CountingSpawner {
    fn start_summary(&self, _ids: &[String]) -> anyhow::Result<String> {
        let n = self.0.fetch_add(1, Ordering::SeqCst);
        Ok(format!("summary-{n}"))
    }

    fn stop(&self, _session: &str, _release_claims: bool) {}
}

/// One open tab; a thread keeps its frames.
pub(super) struct Tab {
    pub(super) opened: Vec<Value>,
    log: Arc<Mutex<Vec<Value>>>,
}

impl Tab {
    pub(super) fn frames(&self) -> Vec<Value> {
        self.log.lock().unwrap().clone()
    }
}

pub(super) struct Fixture {
    pub(super) ctx: Arc<PlanContext>,
    pub(super) fake: Arc<FakeSessions>,
    pub(super) root: PathBuf,
    /// The fake `codex` and its behaviour files.
    pub(super) bin: PathBuf,
    /// The fake `tmux` and its `panes` file.
    pub(super) tools: PathBuf,
    pub(super) holder: BrowserId,
    pub(super) summaries: Arc<CountingSpawner>,
    pub(super) rebuilds: Arc<Rebuilds>,
    pub(super) env: EnvGuard,
    _tmp: TempDir,
}

impl Fixture {
    pub(super) fn new() -> Fixture {
        Fixture::build(false)
    }

    pub(super) fn read_only() -> Fixture {
        Fixture::build(true)
    }

    fn build(read_only: bool) -> Fixture {
        let tmp = tempfile::tempdir().unwrap();
        let base = tmp.path().canonicalize().unwrap();
        let (bin, tools, root) = (base.join("bin"), base.join("tools"), base.join("project"));
        script(&bin.join("codex"), FAKE_CODEX);
        script(&tools.join("tmux"), FAKE_TMUX);
        project(&root);
        let mut env = EnvGuard(Vec::new());
        env.set("LOOM_HOME", base.join("loom-home"));
        let skill = base.join("codex-home/skills/pressure");
        fs::create_dir_all(&skill).unwrap();
        fs::write(skill.join("SKILL.md"), "# Pressure\n").unwrap();
        env.set("CODEX_HOME", base.join("codex-home"));
        let path = std::env::var_os("PATH").unwrap_or_default();
        let mut dirs = vec![bin.clone()];
        dirs.extend(std::env::split_paths(&path));
        env.set("PATH", std::env::join_paths(dirs).unwrap());
        let ctx = context(&root, read_only);
        let summaries = Arc::new(CountingSpawner::default());
        let broadcast = Arc::new(|_: &crate::plan_server::wire::message::PlanMessage| {});
        let rebuilds = Rebuilds::new_for_test(summaries.clone(), None, broadcast);
        ctx.extensions().insert(Arc::clone(&rebuilds));
        ctx.extensions()
            .insert(Arc::new(AuthoringLeases::default()));
        let fake = FakeSessions::new(ctx.runtime().sessions.clone());
        install_with(&ctx, fake.clone(), tools_at(&tools));
        let holder = BrowserId::mint().unwrap();
        Fixture {
            ctx,
            fake,
            root,
            bin,
            tools,
            holder,
            summaries,
            rebuilds,
            env,
            _tmp: tmp,
        }
    }

    /// A second server over the same project and runtime directory, its own fake bridge
    /// reporting `recovered` as its start-up recovery.
    pub(super) fn restart(&self, recovered: RecoveryReport) -> Arc<PlanContext> {
        let ctx = context(&self.root, false);
        let fake = FakeSessions::new(ctx.runtime().sessions.clone());
        *fake.recovery.lock().unwrap() = recovered;
        install_with(&ctx, fake, tools_at(&self.tools));
        ctx
    }

    pub(super) fn runs(&self) -> Arc<PressureRuns> {
        self.ctx.extensions().get::<PressureRuns>().unwrap()
    }

    /// Give the fake `codex` a behaviour file.
    pub(super) fn codex(&self, name: &str, content: &str) {
        fs::write(self.bin.join(name), content).unwrap();
    }

    /// `(pid, model)` of every fake Codex attempt so far.
    pub(super) fn codex_attempts(&self) -> Vec<(u32, String)> {
        let text = fs::read_to_string(self.bin.join("pids")).unwrap_or_default();
        let parse = |line: &str| {
            let (pid, model) = line.split_once(' ')?;
            Some((pid.parse::<u32>().ok()?, model.to_owned()))
        };
        text.lines().filter_map(parse).collect()
    }

    pub(super) fn plan_text(&self) -> String {
        fs::read_to_string(self.root.join(PLAN)).unwrap()
    }

    /// Call a pressure route's handler as `browser`.
    pub(super) fn call(
        &self,
        path: &str,
        browser: &BrowserId,
        body: Option<Value>,
    ) -> (u16, Value) {
        call_on(&self.ctx, path, browser, body)
    }

    /// Start a run of `rounds` rounds on [`PLAN`] as the holder; the response must be 202.
    pub(super) fn start(&self, rounds: u32) -> Value {
        let (status, body) = self.call(
            "/api/pressure/start",
            &self.holder,
            Some(start_body(PLAN, rounds)),
        );
        assert_eq!(status, 202, "{body}");
        body
    }

    /// Wait for the current run to end and clean up; its last view.
    pub(super) fn finished(&self) -> PressureRunView {
        let runs = self.runs();
        wait_for("the run's end", || {
            let view = runs.last_view().filter(|view| view.ended.is_some());
            view.filter(|_| !runs.running())
        })
    }

    pub(super) fn ended(&self) -> PressureEnd {
        self.finished().ended.unwrap()
    }

    /// Open a tab of `browser`, counted by authoring as an open connection.
    pub(super) fn connect(&self, browser: &BrowserId) -> Tab {
        let conn: ConnCtx = self.ctx.conn_ctx(browser.clone());
        let rx = self.ctx.hub().register(&conn);
        self.ctx.authoring().connection_opened(browser);
        let parse = |frame: &str| serde_json::from_str::<Value>(frame).unwrap();
        let opened = self.ctx.hub().open_frames(&conn);
        let opened = opened.iter().map(|frame| parse(frame)).collect();
        let log: Arc<Mutex<Vec<Value>>> = Arc::default();
        let keep = Arc::clone(&log);
        thread::spawn(move || rx.iter().for_each(|f| keep.lock().unwrap().push(parse(&f))));
        Tab { opened, log }
    }

    /// Press the summary rebuild button; the number of summary sessions started so far.
    pub(super) fn press_rebuild(&self) -> usize {
        self.rebuilds
            .enqueue(vec!["doc/product/index.md".into()], Trigger::Button);
        self.summaries.0.load(Ordering::SeqCst)
    }
}

fn context(root: &Path, read_only: bool) -> Arc<PlanContext> {
    let (runtime, _) = server_runtime_paths(root).unwrap();
    create_runtime_dir(&runtime).unwrap();
    fs::create_dir_all(&runtime.sessions).unwrap();
    let problem = MissingPrereq {
        what: PrereqKind::Tmux,
        message: "tmux is missing".into(),
        fix: "install tmux".into(),
    };
    let problems = read_only.then_some(problem).into_iter().collect();
    let bind = BindInfo::new("127.0.0.1:7272".parse().unwrap(), &"ab".repeat(32));
    let ctx = PlanContext::new(
        root.to_path_buf(),
        ReadOnly::from_problems(problems),
        bind,
        OwnWrites::default(),
    );
    Arc::new(ctx.unwrap())
}

fn tools_at(dir: &Path) -> Tools {
    Tools {
        tmux: dir.join("tmux"),
        pane_check: Duration::from_millis(150),
        chat_close: Duration::from_secs(5),
    }
}

fn project(root: &Path) {
    for (path, text) in [
        ("doc/product/index.md", "# Vision\n"),
        (PLAN, PLAN_TEXT),
        ("doc/plans/IN_PROGRESS-PLAN-b.md", "# Plan B\n"),
    ] {
        let path = root.join(path);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, text).unwrap();
    }
}

fn script(path: &Path, text: &str) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, text).unwrap();
    fs::set_permissions(path, fs::Permissions::from_mode(0o755)).unwrap();
}

pub(super) fn call_on(
    ctx: &PlanContext,
    path: &str,
    browser: &BrowserId,
    body: Option<Value>,
) -> (u16, Value) {
    let route = super::routes::all()
        .into_iter()
        .find(|route| route.path == path);
    let route = route.unwrap_or_else(|| panic!("no route {path}"));
    let method = if path.ends_with("/options") {
        "GET"
    } else {
        "POST"
    };
    let raw = format!("{method} {path} HTTP/1.1\r\nHost: localhost\r\n\r\n");
    let head = parse_head(raw.as_bytes()).unwrap().unwrap();
    let public_id = ctx.presence().public_id(browser);
    let request = ApiRequest {
        ctx,
        head: &head,
        browser,
        public_id: &public_id,
        query: QueryParams::parse(None).unwrap(),
        body: body
            .map(|body| body.to_string().into_bytes())
            .unwrap_or_default(),
    };
    let response = (route.handler)(&request);
    let value = serde_json::from_slice(&response.body).unwrap_or(Value::Null);
    (response.status, value)
}

/// A start request body; every slot inside its vocabulary.
pub(super) fn start_body(plan: &str, rounds: u32) -> Value {
    json!({
        "plan": plan,
        "rounds": rounds,
        "claude_model": "opus",
        "claude_effort": "high",
        "codex_model": "gpt-6.1-sol",
        "codex_effort": "high",
        "address_model": "sonnet",
        "address_effort": "medium",
    })
}

pub(super) fn wait_for<T>(what: &str, mut f: impl FnMut() -> Option<T>) -> T {
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        if let Some(value) = f() {
            return value;
        }
        assert!(Instant::now() < deadline, "timed out waiting for {what}");
        thread::sleep(Duration::from_millis(20));
    }
}

pub(super) fn wait_until(what: &str, mut f: impl FnMut() -> bool) {
    wait_for(what, || f().then_some(()));
}
