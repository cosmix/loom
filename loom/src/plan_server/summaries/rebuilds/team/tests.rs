//! A draft team's run: when it settles, what it presses, and that a settled run presses once.

use std::fs;
use std::path::PathBuf;
use std::sync::Arc;

use super::super::tests::{owned, Probe};
use super::super::tests_bridge::{ended_with, turn_ended, Rig};
use super::super::tests_ordering::{seal_feature, served};
use super::*;
use crate::plan_server::context::PlanContext;
use crate::plan_session::bridge::{BridgeEvent, EndOutcome};
use crate::plan_session::kind::SessionKind;
use crate::plan_session::model::TokenCounts;
use crate::planning::summary::engine::EngineUpdate;
use crate::planning::summary::evaluate::evaluate;
use crate::planning::summary::evaluate::fixtures::{self, Fixture, ID1, PRODUCT, S1, VISION};
use crate::planning::watch::Batch;

const CHAT: &str = "chat";

fn mate(state: TeammateState) -> TeammateStatus {
    TeammateStatus {
        agent_id: "agent-1".into(),
        name: "writer".into(),
        documents: Vec::new(),
        current: None,
        state,
        tokens: TokenCounts::default(),
    }
}

fn team(id: &str, state: TeammateState) -> BridgeEvent {
    BridgeEvent::Team {
        id: id.into(),
        teammates: vec![mate(state)],
    }
}

fn section_path(n: u32) -> String {
    format!("doc/product/demo/section_{n:02}.md")
}

fn section_id(n: u32) -> String {
    format!("demo/product/section_{n:02}")
}

/// A feature whose vision lists sections 1 to `count`, each drafted, none summarized.
fn drafted(count: u32) -> Fixture {
    let numbers: Vec<u32> = (1..=count).collect();
    let vision = fixtures::vision(&numbers);
    let sections: Vec<(String, String)> = numbers
        .iter()
        .map(|n| (section_path(*n), fixtures::section(*n, "Alpha.", "Beta.")))
        .collect();
    let mut files = vec![(VISION, vision.as_str())];
    files.extend(
        sections
            .iter()
            .map(|(path, text)| (path.as_str(), text.as_str())),
    );
    fixtures::fixture(&files)
}

/// Record a draft team scope over sections 1 to `count` for the session `CHAT`.
fn write_team_scope(f: &Fixture, count: u32) {
    let documents = (1..=count)
        .map(|n| PathBuf::from(section_path(n)))
        .collect();
    write_scope(
        f,
        &SessionScope::team(documents, Vec::new()).expect("team scope"),
    );
}

fn write_scope(f: &Fixture, scope: &SessionScope) {
    let dir = f.paths.sessions.join(CHAT);
    fs::create_dir_all(&dir).expect("session dir");
    scope::write(&SessionDir::new(dir), scope).expect("write the scope");
}

/// Refresh the served project with the files the watcher saw `changed`, and hand the facade the
/// evaluation with every summary to rebuild as invalidated.
fn update(ctx: &PlanContext, rebuilds: &Rebuilds, changed: &[&str]) {
    if !changed.is_empty() {
        let batch = Batch {
            changed: changed.iter().map(PathBuf::from).collect(),
            ..Batch::default()
        };
        ctx.project_mut().refresh(&batch).expect("refresh");
    }
    let evaluation = evaluate(&ctx.project(), &[]);
    let invalidated = super::super::evaluation::to_rebuild(&evaluation);
    rebuilds.on_update(&EngineUpdate {
        evaluation,
        rewritten: Vec::new(),
        deleted: Vec::new(),
        invalidated,
    });
}

/// The rebuilds of `f` with the lead's turn `t1` open and the evaluation that lists its
/// summaries as out of date.
fn serving(f: &Fixture) -> (Arc<PlanContext>, Arc<Probe>, Arc<Rebuilds>) {
    let (ctx, probe, rebuilds) = served(f);
    rebuilds.turn_started(Some(SessionKind::Authoring), CHAT, "t1");
    update(&ctx, &rebuilds, &[]);
    (ctx, probe, rebuilds)
}

/// A sealed section 01 that the lead's open turn `t1` edits under `scope`: its summary and the
/// overview wait their quiet period once the turn ends, so only a press starts them at once.
fn edited_in_turn(scope: &SessionScope) -> (Fixture, Arc<PlanContext>, Arc<Probe>, Arc<Rebuilds>) {
    let f = fixtures::fixture(&[
        (VISION, &fixtures::vision(&[1])),
        (S1, &fixtures::section(1, "Alpha.", "Beta.")),
    ]);
    seal_feature(&f, "Alpha holds.");
    write_scope(&f, scope);
    let (ctx, probe, rebuilds) = served(&f);
    update(&ctx, &rebuilds, &[S1]);
    rebuilds.turn_started(Some(SessionKind::Authoring), CHAT, "t1");
    fixtures::put(&f.root, S1, &fixtures::section(1, "Edited.", "Beta."));
    update(&ctx, &rebuilds, &[S1]);
    (f, ctx, probe, rebuilds)
}

#[test]
fn a_run_settles_once_after_its_teammates_and_lead_are_idle() {
    let mut runs = TeamRuns::default();
    assert!(
        !runs.settle(CHAT, false, false),
        "no teammates reported yet"
    );
    runs.teammates(CHAT, &[mate(TeammateState::Working)]);
    assert!(!runs.settle(CHAT, false, false), "a teammate works");
    runs.teammates(CHAT, &[mate(TeammateState::Idle)]);
    assert!(!runs.settle(CHAT, true, false), "the lead's turn is open");
    assert!(runs.settle(CHAT, false, false), "the run settles");
    assert!(
        !runs.settle(CHAT, false, false),
        "a settled run settles once"
    );
    assert!(!runs.settle(CHAT, false, true), "nor again at its end");
}

#[test]
fn a_teammate_working_again_rearms_the_run_and_an_end_settles_a_stuck_one() {
    let mut runs = TeamRuns::default();
    runs.teammates(CHAT, &[mate(TeammateState::Idle)]);
    assert!(runs.settle(CHAT, false, false));
    runs.teammates(CHAT, &[mate(TeammateState::Working)]);
    assert!(!runs.settle(CHAT, false, false), "still working");
    assert!(
        runs.settle(CHAT, false, true),
        "the end settles a run left working"
    );
    assert!(
        runs.settle("other", false, true),
        "a session without teammates settles at its end"
    );
}

#[test]
fn a_run_presses_its_out_of_date_sections_then_their_overview() {
    let f = drafted(2);
    let project = fixtures::load(&f.root);
    let eval = evaluate(&project, &[]);
    let documents = vec![
        PathBuf::from(section_path(1)),
        PathBuf::from(section_path(2)),
    ];
    let ids = run_ids(&project, &eval, &documents);
    assert_eq!(ids, vec![section_id(1), section_id(2), PRODUCT.to_owned()]);
}

#[test]
fn a_run_whose_summaries_are_current_presses_nothing() {
    let f = fixtures::fixture(&[
        (VISION, &fixtures::vision(&[1])),
        (S1, &fixtures::section(1, "Alpha.", "Beta.")),
    ]);
    seal_feature(&f, "Alpha holds.");
    let project = fixtures::load(&f.root);
    let eval = evaluate(&project, &[]);
    assert_eq!(eval.summaries[ID1].state, SummaryState::Current);
    assert_eq!(
        run_ids(&project, &eval, &[PathBuf::from(S1)]),
        Vec::<String>::new()
    );
}

/// Regression: a team drafts seven sections. Their summaries used to wait five minutes after the
/// lead's turn, then ask before a rebuild above the limit, while each page read "not built".
/// The run's end now starts one session for the seven sections, then the overview.
#[test]
fn a_settled_team_run_builds_its_sections_then_the_overview_without_asking() {
    let rig = Rig::new(false);
    let f = drafted(7);
    write_team_scope(&f, 7);
    let (_ctx, probe, rebuilds) = serving(&f);
    rebuilds.on_bridge_event(&rig.bridge, &team(CHAT, TeammateState::Working));
    rebuilds.on_bridge_event(&rig.bridge, &team(CHAT, TeammateState::Idle));
    assert!(
        probe.starts().is_empty(),
        "the lead's open turn holds the build"
    );

    rebuilds.on_bridge_event(&rig.bridge, &turn_ended(CHAT, "t1"));
    let mut expected: Vec<String> = (1..=7).map(section_id).collect();
    expected.push(PRODUCT.to_owned());
    assert_eq!(probe.starts(), vec![expected]);
    assert_eq!(
        rebuilds.panel().question,
        None,
        "the run's build asks nothing"
    );

    rebuilds.on_bridge_event(&rig.bridge, &team(CHAT, TeammateState::Idle));
    assert_eq!(probe.starts().len(), 1, "a settled run presses once");
}

/// Regression: the last teammate going idle settled the run when no lead turn was open, but that
/// idle wakes the lead for its consistency turn, and the teammate's transcript tail can report it
/// before the lead's turn starts. The run settled early and the lead's edits went unbuilt.
#[test]
fn a_teammate_idle_before_the_leads_next_turn_waits_for_that_turn_to_end() {
    let rig = Rig::new(false);
    let draft = SessionScope::team(vec![PathBuf::from(S1)], Vec::new()).expect("scope");
    let (_f, _ctx, probe, rebuilds) = edited_in_turn(&draft);
    rebuilds.on_bridge_event(&rig.bridge, &team(CHAT, TeammateState::Working));
    rebuilds.on_bridge_event(&rig.bridge, &turn_ended(CHAT, "t1"));
    assert!(probe.starts().is_empty(), "the turn's edit waits");
    rebuilds.on_bridge_event(&rig.bridge, &team(CHAT, TeammateState::Idle));
    assert!(
        probe.starts().is_empty(),
        "the teammate's idle settles nothing"
    );

    rebuilds.turn_started(Some(SessionKind::Authoring), CHAT, "t2");
    assert!(probe.starts().is_empty(), "the lead's turn is open");
    rebuilds.on_bridge_event(&rig.bridge, &turn_ended(CHAT, "t2"));
    assert_eq!(probe.starts(), vec![owned(&[ID1, PRODUCT])]);
}

/// An address run edits documents that exist: their summaries take the background path, so its
/// end starts nothing at once.
#[test]
fn an_address_team_run_presses_nothing() {
    let rig = Rig::new(false);
    let ledger = PathBuf::from("doc/plans/demo/findings.md");
    let documents = vec![PathBuf::from(S1), ledger.clone()];
    let address = SessionScope::address(documents, ledger).expect("scope");
    let (_f, _ctx, probe, rebuilds) = edited_in_turn(&address);
    rebuilds.on_bridge_event(&rig.bridge, &team(CHAT, TeammateState::Working));
    rebuilds.on_bridge_event(&rig.bridge, &team(CHAT, TeammateState::Idle));
    rebuilds.on_bridge_event(&rig.bridge, &turn_ended(CHAT, "t1"));
    assert!(probe.starts().is_empty(), "{:?}", probe.starts());
}

/// A teammate the lead shut down can stay "working" in its transcript: the session's end still
/// builds what the team wrote.
#[test]
fn a_team_session_ending_with_a_teammate_left_working_builds_its_sections() {
    let rig = Rig::new(false);
    let f = drafted(1);
    write_team_scope(&f, 1);
    let (_ctx, probe, rebuilds) = served(&f);
    rebuilds.on_bridge_event(&rig.bridge, &team(CHAT, TeammateState::Working));
    rebuilds.on_bridge_event(&rig.bridge, &turn_ended(CHAT, "t1"));
    assert!(probe.starts().is_empty(), "a teammate still works");

    let end = ended_with(CHAT, SessionKind::Authoring, EndOutcome::Closed);
    rebuilds.on_bridge_event(&rig.bridge, &end);
    assert_eq!(probe.starts(), vec![owned(&[ID1, PRODUCT])]);
}
