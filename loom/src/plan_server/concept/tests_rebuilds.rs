//! Concept projections of the rebuild scheduler's reasons, progress and estimates.

use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use super::fixtures::{self as fx, Fixture};
use super::page::{load_page, PageContext, PageKey, Scope};
use super::routes::page_context;
use super::state::out_of_date_reason;
use crate::plan_server::context::{BindInfo, PlanContext};
use crate::plan_server::prereq::ReadOnly;
use crate::plan_server::summaries::rebuilds::Rebuilds;
use crate::plan_server::summaries::scheduler::SessionSpawner;
use crate::plan_server::wire::concept::{EmptyKind, OutOfDate, OutOfDateReason, Progress};
use crate::plan_session::bridge::{Bridge, BridgeConfig, BridgeEvent, EndOutcome};
use crate::plan_session::kind::SessionKind;
use crate::plan_session::tmux_host::TmuxHost;
use crate::planning::summary::cost::Trigger;
use crate::planning::summary::engine::EngineUpdate;
use crate::planning::summary::evaluate::{evaluate, Evaluation, SummaryState};
use crate::planning::watch::OwnWrites;

const SESSION: &str = "concept-rebuild";

struct StubSpawner;

impl SessionSpawner for StubSpawner {
    fn start_summary(&self, _ids: &[String]) -> anyhow::Result<String> {
        Ok(SESSION.to_owned())
    }

    fn stop(&self, _session: &str, _release_claims: bool) {}
}

fn context() -> PageContext {
    PageContext {
        rebuilds: Some(Rebuilds::new_for_test(
            Arc::new(StubSpawner),
            None,
            Arc::new(|_| {}),
        )),
        ..Default::default()
    }
}

fn update(evaluation: &Evaluation, invalidated: Vec<String>) -> EngineUpdate {
    EngineUpdate {
        evaluation: evaluation.clone(),
        rewritten: Vec::new(),
        deleted: Vec::new(),
        invalidated,
    }
}

fn awaiting_context() -> PageContext {
    let ctx = context();
    let ids = (1..=6)
        .map(|i| format!("demo/product/section_{i:02}"))
        .collect();
    ctx.rebuilds
        .as_ref()
        .unwrap()
        .enqueue(ids, Trigger::Background);
    ctx
}

// Only the empty bridge's transcript lookup is used: no sessions or driver are started.
fn bridge(fixture: &Fixture) -> Arc<Bridge> {
    let config = BridgeConfig {
        project_root: fixture.root.clone(),
        runtime: fixture.paths.clone(),
        loom_bin: fixture.root.join("loom"),
        claude_bin: fixture.root.join("claude"),
        home: fixture.root.join("home"),
        closing_message: Arc::new(String::new),
        delivery_timeout: Duration::from_secs(10),
        final_write_bound: Duration::from_secs(600),
        permission_deny_after: Duration::from_secs(3590),
        identity_interval: Duration::from_secs(2),
        tick: Duration::from_millis(200),
        fetch_policy: Default::default(),
    };
    Bridge::new(config, Arc::new(TmuxHost), None)
}

#[test]
fn an_id_in_the_open_question_awaits_approval() {
    let ctx = awaiting_context();

    let reason = out_of_date_reason(fx::ID1, SummaryState::Stale, &ctx);

    assert_eq!(
        reason,
        Some(OutOfDate {
            reason: OutOfDateReason::AwaitingApproval,
            detail: None,
        })
    );
    assert_eq!(
        out_of_date_reason("demo/product/unasked", SummaryState::Stale, &ctx),
        Some(OutOfDate {
            reason: OutOfDateReason::SourceChanged,
            detail: None,
        }),
    );
}

#[test]
fn claim_state_keeps_its_rebuilding_reason() {
    let ctx = awaiting_context();

    assert_eq!(
        out_of_date_reason(fx::ID1, SummaryState::Rebuilding, &ctx),
        Some(OutOfDate {
            reason: OutOfDateReason::Rebuilding,
            detail: None,
        }),
    );
}

#[test]
fn read_only_keeps_its_reason_when_approval_is_pending() {
    let mut ctx = awaiting_context();
    ctx.read_only = Some("locked".into());

    assert_eq!(
        out_of_date_reason(fx::ID1, SummaryState::Stale, &ctx),
        Some(OutOfDate {
            reason: OutOfDateReason::CannotRebuild,
            detail: Some("locked".into()),
        }),
    );
}

#[test]
fn a_failed_id_shows_the_session_error() {
    let fixture = fx::two_sections();
    let ctx = context();
    let rebuilds = ctx.rebuilds.as_ref().unwrap();
    rebuilds.enqueue(vec![fx::ID1.into()], Trigger::Button);
    rebuilds.on_bridge_event(
        &bridge(&fixture),
        &BridgeEvent::Ended {
            id: SESSION.into(),
            kind: SessionKind::Summary,
            outcome: EndOutcome::Failed("the summary session failed".into()),
        },
    );

    let reason = out_of_date_reason(fx::ID1, SummaryState::Incomplete, &ctx);

    assert_eq!(
        reason,
        Some(OutOfDate {
            reason: OutOfDateReason::Failed,
            detail: Some("the summary session failed".into()),
        })
    );
    assert_eq!(
        out_of_date_reason(fx::ID2, SummaryState::Incomplete, &ctx),
        Some(OutOfDate {
            reason: OutOfDateReason::SourceChanged,
            detail: None,
        }),
    );
}

#[test]
fn concept_pages_show_progress_and_estimate_during_a_rebuild() {
    let fixture = fx::two_sections();
    let ctx = context();
    let rebuilds = ctx.rebuilds.as_ref().unwrap();
    let initial = evaluate(&fx::load(&fixture.root), &[]);
    rebuilds.on_update(&update(
        &initial,
        initial.summaries.keys().cloned().collect(),
    ));
    fx::seal_document(&fixture.root, fx::ID1, &[("Alpha holds.", "a")]);
    let project = fx::load(&fixture.root);
    let evaluation = evaluate(&project, &[]);
    rebuilds.on_update(&update(&evaluation, Vec::new()));

    for key in [
        PageKey::Overview {
            feature: "demo".into(),
            scope: Scope::Product,
        },
        PageKey::Summary {
            feature: Some("demo".into()),
            scope: Scope::Product,
            doc: "section_01".into(),
        },
    ] {
        let page = load_page(&project, &evaluation, &key, &ctx).expect("page");
        assert_eq!(page.building, Some(Progress { done: 1, total: 4 }));
        assert_eq!(
            page.estimate.as_deref(),
            Some("about 230000 tokens to build, an estimate")
        );
    }
}

#[test]
fn a_never_built_project_shows_an_estimate_without_running_progress() {
    let fixture = fx::two_sections();
    let project = fx::load(&fixture.root);
    let evaluation = evaluate(&project, &[]);
    let ctx = context();
    ctx.rebuilds
        .as_ref()
        .unwrap()
        .on_update(&update(&evaluation, Vec::new()));

    let key = PageKey::Overview {
        feature: "demo".into(),
        scope: Scope::Product,
    };
    let page = load_page(&project, &evaluation, &key, &ctx).expect("page");

    assert_eq!(page.empty.expect("empty state").kind, EmptyKind::NeverBuilt);
    assert_eq!(page.building, None);
    assert_eq!(
        page.estimate.as_deref(),
        Some("about 270000 tokens to build, an estimate")
    );
}

#[test]
fn route_page_context_uses_the_installed_rebuilds() {
    let fixture = fx::two_sections();
    let local = SocketAddr::from(([127, 0, 0, 1], 0));
    let server = PlanContext::new(
        fixture.root.clone(),
        ReadOnly::from_problems(Vec::new()),
        BindInfo::new(local, "t"),
        OwnWrites::default(),
    )
    .expect("server context");
    let rebuilds = context().rebuilds.unwrap();
    server.extensions().insert(rebuilds.clone());

    let page = page_context(&server);

    assert!(Arc::ptr_eq(&page.rebuilds.expect("rebuilds"), &rebuilds));
}
