//! Operator-only proof that the rendered plugin loads and authors each document family.

#[path = "planning_guidance_operator/support.rs"]
mod support;

use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{mpsc, Arc};
use std::thread;
use std::time::{Duration, Instant};

use anyhow::{bail, ensure, Context, Result};
use loom::fs::permissions::untrust_worktree;
use loom::plan_server::prereq::ReadOnly;
use loom::plan_server::{start, RunningServer, ServerOptions};
use loom::plan_session::bridge::{Bridge, EndCause, EndMode, StartRequest};
use loom::plan_session::kind::{SessionDir, SessionKind};
use loom::plan_session::model::{Origin, SessionState};
use loom::plan_session::spool::SpoolReader;
use loom::planning::checklist::Offer;
use loom::planning::guidance::{
    render_session_guidance, AuthoringStart, GuidanceModels, SessionGuidanceKind,
};
use loom::planning::runtime::create_runtime_dir;
use reqwest::blocking::Client;
use serde_json::Value;
use support::fixture;
use tempfile::TempDir;

#[test]
#[ignore = "needs a logged-in Claude Code, tmux and network"]
#[serial_test::serial]
fn operator_vision_session_loads_its_skill() -> Result<()> {
    operator(
        Offer::StartVision {
            feature: "demo".into(),
        },
        "vision",
        "doc/product/demo/index.md",
    )
}

#[test]
#[ignore = "needs a logged-in Claude Code, tmux and network"]
#[serial_test::serial]
fn operator_section_session_loads_its_skill() -> Result<()> {
    operator(
        Offer::WriteSection {
            feature: "demo".into(),
            number: 2,
        },
        "section",
        "doc/product/demo/section_02.md",
    )
}

#[test]
#[ignore = "needs a logged-in Claude Code, tmux and network"]
#[serial_test::serial]
fn operator_metaplan_session_loads_its_skill() -> Result<()> {
    operator(
        Offer::StartMetaplan {
            feature: "demo".into(),
        },
        "metaplan",
        "doc/plans/demo/index.md",
    )
}

#[test]
#[ignore = "needs a logged-in Claude Code, tmux and network"]
#[serial_test::serial]
fn operator_plan_session_loads_its_skill() -> Result<()> {
    operator(
        Offer::WritePlan {
            feature: "demo".into(),
            metaplan: "doc/plans/demo/index.md".into(),
            slug: "notes".into(),
        },
        "plan",
        "doc/plans/demo/PLAN-notes.md",
    )
}

fn missing_prerequisites(name: &str) -> bool {
    loom::process::sandbox_probe::skip_unless(
        which::which("tmux").is_ok() && which::which("claude").is_ok(),
        name,
        "tmux or claude missing",
    )
}

/// Keep the real HOME for Claude authentication, isolating only loom's runtime/config.
struct LoomHome {
    _dir: TempDir,
    previous: Option<OsString>,
}

impl LoomHome {
    fn new() -> Result<Self> {
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

struct TrustGuard(PathBuf);

impl Drop for TrustGuard {
    fn drop(&mut self) {
        if let Err(error) = untrust_worktree(&self.0) {
            eprintln!("cannot remove temporary project trust: {error:#}");
        }
    }
}

fn operator(offer: Offer, skill: &str, target: &str) -> Result<()> {
    if missing_prerequisites(&format!("operator_{skill}_session_loads_its_skill")) {
        return Ok(());
    }
    let repo = fixture(target)?;
    let _home = LoomHome::new()?;
    let _trust = TrustGuard(repo.path().to_path_buf());
    let running = start(ServerOptions {
        root: repo.path().to_path_buf(),
        host: "127.0.0.1".parse()?,
        port: Some(0),
        read_only: ReadOnly::from_problems(vec![]),
    })?;
    let result = (|| {
        create_runtime_dir(running.ctx.runtime())?;
        bootstrap(&running)?;
        exercise(&running, offer, skill, &repo.path().join(target))
    })();
    let stopped = running.stop();
    result.and(stopped)
}

fn bootstrap(running: &RunningServer) -> Result<()> {
    let client = Client::builder()
        .no_proxy()
        .redirect(reqwest::redirect::Policy::none())
        .timeout(Duration::from_secs(20))
        .build()?;
    let origin = format!("http://{}", running.local);
    let host = format!("127.0.0.1:{}", running.local.port());
    let response = client
        .get(format!("{origin}/?token={}", running.token))
        .header("Host", &host)
        .header("Origin", &origin)
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
        .map(|value| {
            Ok(value
                .to_str()?
                .split(';')
                .next()
                .unwrap_or("")
                .trim()
                .to_owned())
        })
        .collect::<Result<Vec<String>>>()?;
    check_cookies(&cookies, running.local.port())?;
    let info = client
        .get(format!("{origin}/api/server"))
        .header("Host", host)
        .header("Origin", origin)
        .header("Cookie", cookies.join("; "))
        .send()?;
    ensure!(
        info.status().as_u16() == 200,
        "authenticated API: {}",
        info.status()
    );
    Ok(())
}

fn check_cookies(cookies: &[String], port: u16) -> Result<()> {
    for name in ["loom_plan", "loom_plan_browser"] {
        let prefix = format!("{name}_{port}=");
        ensure!(
            cookies.iter().any(|cookie| cookie.starts_with(&prefix)),
            "missing {prefix}"
        );
    }
    Ok(())
}

fn exercise(running: &RunningServer, offer: Offer, skill: &str, target: &Path) -> Result<()> {
    let bridge = running
        .ctx
        .extensions()
        .get::<Bridge>()
        .context("bridge not installed")?;
    let (tx, rx) = mpsc::channel();
    bridge.on_recovered(Box::new(move |_| {
        let _ = tx.send(());
    }));
    rx.recv_timeout(Duration::from_secs(30))
        .context("bridge recovery")?;
    let ctx = Arc::clone(&running.ctx);
    let models = GuidanceModels::load(ctx.root())?;
    let offer_id = offer.id();
    let kind = SessionGuidanceKind::Authoring(AuthoringStart::Offer(offer));
    let id = bridge.start_session(StartRequest {
        kind: SessionKind::Authoring,
        guidance: Box::new(move |dir| {
            render_session_guidance(
                &kind,
                &ctx.project(),
                ctx.runtime(),
                dir,
                &models,
                Some("demo"),
            )
            .map(Into::into)
        }),
        opening_origin: Some(Origin::Offer),
        offer: Some(offer_id),
        feature: Some("demo".to_owned()),
        plan: None,
        extra_env: vec![],
    })?;
    let dir = SessionDir::new(running.ctx.runtime().sessions.join(&id));
    let result = author_and_check(&bridge, &id, &dir, skill, target);
    let ended = teardown(&bridge, &id);
    result.and(ended)
}

fn author_and_check(
    bridge: &Bridge,
    id: &str,
    dir: &SessionDir,
    skill: &str,
    target: &Path,
) -> Result<()> {
    let mut spool = SpoolReader::new(dir.events());
    wait_stop(bridge, id, &mut spool, Duration::from_secs(300))?;
    let transcript = bridge
        .transcripts(id)
        .and_then(|paths| paths.main)
        .context("no main transcript after the first Stop")?;
    println!("{skill} transcript: {}", transcript.display());
    ensure!(
        has_skill(&transcript, skill)?,
        "no Skill tool use for loom-plan:{skill} in {}",
        transcript.display()
    );
    bridge.send(
        id,
        "Write the document now with what you know; ask nothing further.".into(),
        vec![],
    )?;
    wait_stop(bridge, id, &mut spool, Duration::from_secs(600))?;
    let text = fs::read_to_string(target).with_context(|| format!("read {}", target.display()))?;
    ensure!(
        text.lines()
            .any(|line| line.starts_with("# ") && !line[2..].trim().is_empty()),
        "{} has no level-1 heading",
        target.display()
    );
    Ok(())
}

fn has_skill(transcript: &Path, skill: &str) -> Result<bool> {
    let wanted = format!("loom-plan:{skill}");
    for line in fs::read_to_string(transcript)?.lines() {
        let record: Value = serde_json::from_str(line).context("parse main transcript record")?;
        if record["type"] != "assistant" {
            continue;
        }
        let Some(blocks) = record.pointer("/message/content").and_then(Value::as_array) else {
            continue;
        };
        if blocks.iter().any(|block| {
            block["type"] == "tool_use"
                && block["name"] == "Skill"
                && block["input"]["skill"].as_str() == Some(wanted.as_str())
        }) {
            return Ok(true);
        }
    }
    Ok(false)
}

/// Require a main Stop and its consumed Idle state; subagent stops cannot satisfy a wait.
fn wait_stop(bridge: &Bridge, id: &str, spool: &mut SpoolReader, limit: Duration) -> Result<()> {
    let deadline = Instant::now() + limit;
    let mut stopped = false;
    loop {
        for event in spool.read_new() {
            if event.session_id.as_deref() != Some(id)
                || event
                    .payload
                    .get("agent_id")
                    .and_then(Value::as_str)
                    .is_some()
            {
                continue;
            }
            ensure!(
                event.event != "StopFailure",
                "main StopFailure: {:?}",
                event.payload
            );
            stopped |= event.event == "Stop";
        }
        let snapshot = bridge.chat_snapshot(true);
        let session = snapshot
            .session
            .context("chat disappeared while waiting for Stop")?;
        ensure!(session.id == id, "chat replaced while waiting for Stop");
        ensure!(
            session.state != SessionState::Failed,
            "session failed: {:?}",
            session.failure
        );
        if stopped && session.state == SessionState::Idle {
            return Ok(());
        }
        if Instant::now() >= deadline {
            bail!(
                "no main Stop within {limit:?}; session: {session:?}; transcripts: {:?}",
                bridge.transcripts(id)
            );
        }
        thread::sleep(Duration::from_millis(100));
    }
}

fn is_live(bridge: &Bridge, id: &str) -> bool {
    bridge
        .sessions()
        .iter()
        .any(|(session, _, _)| session == id)
}

fn teardown(bridge: &Bridge, id: &str) -> Result<()> {
    if !is_live(bridge, id) {
        return Ok(());
    }
    bridge.end(id, EndMode::Teardown, EndCause::Caller)?;
    let deadline = Instant::now() + Duration::from_secs(30);
    while is_live(bridge, id) {
        ensure!(Instant::now() < deadline, "teardown timed out for {id}");
        thread::sleep(Duration::from_millis(100));
    }
    Ok(())
}
