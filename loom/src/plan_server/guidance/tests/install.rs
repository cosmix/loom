//! The production install: chat installs its guidance slot, then `super::super::install`
//! replaces the provider in that slot and in the extensions the brief route reads.

use std::fs;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::os::unix::fs::DirBuilderExt;
use std::sync::{mpsc, Arc};
use std::time::Duration;

use super::{brief_response, project_files, HomeGuard};
use crate::plan_server::chat::guidance::GuidanceSlot;
use crate::plan_server::chat::install_with;
use crate::plan_server::context::{BindInfo, PlanContext};
use crate::plan_server::guidance::SessionBriefView;
use crate::plan_server::prereq::ReadOnly;
use crate::plan_session::bridge::host::{LaunchSpec, SessionHost, Spawned};
use crate::plan_session::bridge::Bridge;
use crate::plan_session::kind::SessionDir;
use crate::planning::runtime::create_runtime_dir;
use crate::planning::watch::OwnWrites;
use crate::process::{IdentityStatus, ProcessIdentity};

/// Starts nothing: this test renders guidance and never launches a session.
struct IdleHost;

impl SessionHost for IdleHost {
    fn spawn(&self, _spec: &LaunchSpec) -> anyhow::Result<Spawned> {
        anyhow::bail!("this test starts no sessions")
    }

    fn deliver(&self, _socket: &str, _target: &str, _text: &str) -> anyhow::Result<()> {
        Ok(())
    }

    fn interrupt(&self, _socket: &str, _target: &str) -> anyhow::Result<()> {
        Ok(())
    }

    fn teardown(&self, _socket: &str) -> anyhow::Result<()> {
        Ok(())
    }

    fn identity(&self, _identity: ProcessIdentity) -> IdentityStatus {
        IdentityStatus::Dead
    }
}

/// A context with chat installed over [`IdleHost`] and recovery done, guidance not installed.
fn chat_installed(temp: &tempfile::TempDir) -> Arc<PlanContext> {
    let root = temp.path().join("project");
    project_files(&root);
    let local = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 7272);
    let bind = BindInfo::new(local, &"ab".repeat(32));
    let ctx = PlanContext::new(
        root,
        ReadOnly::from_problems(Vec::new()),
        bind,
        OwnWrites::default(),
    );
    let ctx = Arc::new(ctx.unwrap());
    create_runtime_dir(ctx.runtime()).unwrap();
    let home = temp.path().join("claude-home");
    install_with(&ctx, Arc::new(IdleHost), move |config| {
        config.home = home;
        config.tick = Duration::from_millis(20);
    });
    let bridge = ctx.extensions().get::<Bridge>().unwrap();
    let (tx, rx) = mpsc::channel();
    bridge.on_recovered(Box::new(move |_| tx.send(()).unwrap_or_default()));
    rx.recv_timeout(Duration::from_secs(5)).expect("recovery");
    ctx
}

fn session_dir(temp: &tempfile::TempDir, name: &str) -> SessionDir {
    let path = temp.path().join(name);
    fs::DirBuilder::new().mode(0o700).create(&path).unwrap();
    SessionDir::new(path)
}

#[test]
#[serial_test::serial]
fn install_replaces_the_chat_provider_and_serves_the_same_one_to_the_brief_route() {
    let temp = tempfile::tempdir().unwrap();
    let _home = HomeGuard::set(&temp.path().join("home"));
    let ctx = chat_installed(&temp);
    let slot = ctx.extensions().get::<GuidanceSlot>().unwrap();
    let before = session_dir(&temp, "before-install");
    let chat_guidance = slot
        .get()
        .authoring(&ctx, &before, None, None, None)
        .unwrap();
    let chat_brief = fs::read_to_string(chat_guidance.brief_path).unwrap();
    assert!(!chat_brief.contains("## Starting point"), "{chat_brief}");

    super::super::install(&ctx);

    let session = session_dir(&temp, "after-install");
    let offer = Some("refine:plans/demo/index.md");
    let guidance = slot
        .get()
        .authoring(&ctx, &session, offer, None, None)
        .unwrap();
    let brief = fs::read_to_string(&guidance.brief_path).unwrap();
    assert!(brief.contains("## Starting point"), "{brief}");
    let response = brief_response(&ctx, "after-install");
    assert_eq!(
        response.status,
        200,
        "{}",
        String::from_utf8_lossy(&response.body)
    );
    let view: SessionBriefView = serde_json::from_slice(&response.body).unwrap();
    assert_eq!(view.brief, brief);
    assert_eq!(view.model, guidance.model);
}
