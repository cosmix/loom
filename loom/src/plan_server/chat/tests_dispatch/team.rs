//! Team scopes, launch environment and teammate progress through the hub.

use std::fs;
use std::io::Write;
use std::path::PathBuf;
use std::sync::Arc;

use serde_json::{json, Value};

use super::super::guidance::{FixedGuidance, GuidanceProvider};
use super::super::tests::{browser, wait_for, wait_until, Fixture};
use super::super::{lock, ChatState};
use super::kickoff::{launch, offer};
use super::{fixture, opening, received_frames, record_guidance, refused};
use crate::plan_session::bridge::host::LaunchSpec;
use crate::plan_session::bridge::{BridgeEvent, EndCause, EndMode, StartRequest};
use crate::plan_session::kind::{SessionDir, SessionKind};
use crate::plan_session::model::SessionState;
use crate::plan_session::scope::{self, SessionScope, TeamRun};
use crate::planning::project::Project;
use crate::planning::trace::{settle, DocRole};

pub(super) fn team_fixture() -> Fixture {
    let f = fixture(false);
    let files = [
        ("doc/product/demo/index.md",
            "# Vision\n\n## Specification sections\n\n| # | Section |\n| --- | --- |\n| 01 | Written |\n| 02 | Two |\n| 03 | Three |\n"),
        ("doc/product/demo/section_01.md", "# Written\n"),
        ("doc/plans/demo/index.md",
            "# Metaplan\n\n## Written (`PLAN-written.md`)\n\n## One (`PLAN-one.md`)\n\n## Two (`PLAN-two.md`)\n"),
        ("doc/plans/demo/PLAN-written.md", "# Written\n"),
    ];
    for (path, text) in files {
        let path = f.ctx.root().join(path);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, text).unwrap();
    }
    settle_metaplan(f.ctx.root());
    *f.ctx.project_mut() = Project::load(f.ctx.root()).unwrap();
    f
}

/// Writes the demo metaplan's current `Settled:` line so plan offers can start.
fn settle_metaplan(root: &std::path::Path) {
    let project = Project::load(root).unwrap();
    let doc = project
        .feature("demo")
        .unwrap()
        .trace
        .documents
        .iter()
        .find(|doc| doc.role == DocRole::Metaplan)
        .unwrap();
    let path = root.join("doc/plans/demo/index.md");
    let text = fs::read_to_string(&path).unwrap();
    fs::write(&path, settle::mark(&text, &doc.current)).unwrap();
}

#[test]
fn draft_all_sections_scopes_only_the_missing_sections_and_enables_teams() {
    let f = team_fixture();
    let tab = f.connect(&browser());
    let s = launch(&f, &tab, "draft-all-sections", Some("demo"));

    assert_eq!(
        scope::load(&s.dir).unwrap(),
        SessionScope::Team {
            run: TeamRun::Draft,
            documents: vec![
                "doc/product/demo/section_02.md".into(),
                "doc/product/demo/section_03.md".into(),
            ],
            bundles: Vec::new(),
        }
    );
    assert_eq!(
        s.extra_env,
        vec![(crate::claude::AGENT_TEAMS_ENV.to_owned(), "1".into())]
    );
}

#[test]
fn draft_all_plans_scopes_the_missing_plans_and_their_bundles() {
    let f = team_fixture();
    let tab = f.connect(&browser());
    let s = launch(&f, &tab, "draft-all-plans", Some("demo"));

    assert_eq!(
        scope::load(&s.dir).unwrap(),
        SessionScope::Team {
            run: TeamRun::Draft,
            documents: vec![
                "doc/plans/demo/PLAN-one.md".into(),
                "doc/plans/demo/PLAN-two.md".into(),
            ],
            bundles: vec![
                "doc/plans/demo/briefs/one".into(),
                "doc/plans/demo/briefs/two".into(),
            ],
        }
    );
    assert_eq!(
        s.extra_env,
        vec![(crate::claude::AGENT_TEAMS_ENV.to_owned(), "1".into())]
    );
}

#[test]
fn address_findings_scopes_the_named_documents_and_the_ledger() {
    let f = team_fixture();
    let files = [
        (
            "doc/product/demo/section_01.md",
            "# Written\n\nThe written section promises this.\n",
        ),
        (
            "doc/plans/demo/findings.md",
            "# Findings\n\n## F-1 Promise\n\nAbout: section_01\n\
             Evidence: written section promises this\nSeverity: high\nStatus: open\n\nWrong.\n",
        ),
    ];
    for (path, text) in files {
        fs::write(f.ctx.root().join(path), text).unwrap();
    }
    *f.ctx.project_mut() = Project::load(f.ctx.root()).unwrap();
    let tab = f.connect(&browser());
    let s = launch(&f, &tab, "address-findings", Some("demo"));

    assert_eq!(
        scope::load(&s.dir).unwrap(),
        SessionScope::Team {
            run: TeamRun::Address {
                ledger: "doc/plans/demo/findings.md".into()
            },
            documents: vec![
                "doc/product/demo/section_01.md".into(),
                "doc/plans/demo/findings.md".into(),
            ],
            bundles: Vec::new(),
        }
    );
    assert_eq!(
        s.extra_env,
        vec![(crate::claude::AGENT_TEAMS_ENV.to_owned(), "1".into())]
    );
}

#[test]
fn draft_all_requires_an_explicit_feature() {
    for id in ["draft-all-sections", "draft-all-plans"] {
        let f = team_fixture();
        let tab = f.connect(&browser());

        assert_eq!(
            f.command(&tab, offer(id, None)),
            refused("a team run needs a feature")
        );
        assert!(f.bridge.chat_session_id().is_none());
        assert!(f.ctx.authoring().holder().is_none());
    }
}

#[test]
fn a_provider_accepting_an_unoffered_team_run_cannot_bypass_draft_run() {
    let f = fixture(false);
    let tab = f.connect(&browser());
    record_guidance(&f);

    let result = f.command(&tab, offer("draft-all-sections", Some("demo")));

    assert_eq!(result["ok"], false);
    assert!(f.bridge.chat_session_id().is_none());
    assert!(f.ctx.authoring().holder().is_none());
}

fn teammate(f: &Fixture, s: &LaunchSpec) {
    let main = f.bridge.transcripts(&s.id).unwrap().main.unwrap();
    let agents = main.with_extension("").join("subagents");
    fs::create_dir_all(&agents).unwrap();
    fs::write(
        agents.join("agent-writer.meta.json"),
        json!({"taskKind": "in_process_teammate", "name": "Writer"}).to_string(),
    )
    .unwrap();
    let record = json!({
        "type": "assistant", "uuid": "teammate-record", "sessionId": s.id,
        "message": {"id": "team-message", "role": "assistant", "content": [
            {"type": "tool_use", "id": "write-section", "name": "Write",
                "input": {"file_path": f.ctx.root().join("doc/product/demo/section_02.md")}}
        ], "stop_reason": "tool_use",
        "usage": {"input_tokens": 12, "output_tokens": 7}}
    });
    let mut file = fs::File::create(agents.join("agent-writer.jsonl")).unwrap();
    writeln!(file, "{record}").unwrap();
}

fn assert_team_replayed_after_chat(f: &Fixture, view: &Value) {
    let opened = f.connect(&browser()).opened;
    let chat = opened
        .iter()
        .position(|frame| frame["type"] == "chat")
        .unwrap();
    let team = opened.iter().position(|frame| frame == view).unwrap();
    assert!(team > chat);
    assert!(!opened.iter().any(|frame| frame["type"] == "kickoff"));
}

#[test]
fn team_events_publish_run_documents_replay_on_open_and_clear_with_the_session() {
    let f = team_fixture();
    let tab = f.connect(&browser());
    let s = launch(&f, &tab, "draft-all-sections", Some("demo"));
    opening(&f, &s);
    f.spool(&s, "UserPromptSubmit", json!({"prompt": "opening"}));
    f.wait_state(&s, SessionState::Working);
    teammate(&f, &s);
    let view = wait_for("the team frame", || {
        tab.frames()
            .into_iter()
            .find(|frame| frame["type"] == "draft_team")
    });

    assert_eq!(
        view,
        json!({"type": "draft_team", "session": s.id,
            "documents": ["doc/product/demo/section_02.md", "doc/product/demo/section_03.md"],
            "teammates": [{"agent_id": "writer", "name": "Writer",
                "documents": ["doc/product/demo/section_02.md"],
                "current": "doc/product/demo/section_02.md", "state": "working",
                "tokens": {"input": 12, "cache_write": 0, "cache_read": 0, "output": 7}}]
        })
    );
    assert!(f
        .events
        .lock()
        .unwrap()
        .iter()
        .any(|event| { matches!(event, BridgeEvent::Team { id, .. } if *id == s.id) }));
    assert_team_replayed_after_chat(&f, &view);

    f.bridge
        .end(&s.id, EndMode::Teardown, EndCause::Caller)
        .unwrap();
    wait_until("the ended team", || !f.ended(&s.id).is_empty());
    let opened = f.connect(&browser()).opened;
    assert!(!opened
        .iter()
        .any(|frame| { frame["type"] == "kickoff" || frame["type"] == "draft_team" }));
    let state = f.ctx.extensions().get::<ChatState>().unwrap();
    assert!(lock(&state.team).is_none());
}

#[test]
fn teammates_outside_team_scopes_are_neither_broadcast_nor_replayed() {
    for scope_file in [None, Some(r#"{"scope":"open"}"#), Some("invalid scope")] {
        let f = fixture(false);
        let tab = f.connect(&browser());
        let s = f.start(&tab);
        if let Some(contents) = scope_file {
            fs::write(s.dir.scope(), contents).unwrap();
        }
        opening(&f, &s);
        f.spool(&s, "UserPromptSubmit", json!({"prompt": "opening"}));
        f.wait_state(&s, SessionState::Working);

        teammate(&f, &s);
        wait_until("the team event", || {
            f.events
                .lock()
                .unwrap()
                .iter()
                .any(|event| matches!(event, BridgeEvent::Team { id, .. } if *id == s.id))
        });

        assert!(!received_frames(&f, &tab)
            .iter()
            .any(|frame| frame["type"] == "draft_team"));
        assert!(!f
            .connect(&browser())
            .opened
            .iter()
            .any(|frame| frame["type"] == "draft_team"));
        let state = f.ctx.extensions().get::<ChatState>().unwrap();
        assert!(lock(&state.team).is_none());
    }
}

/// The `draft_team` frame of a session started outside dispatch with `persisted` as its scope,
/// once a teammate writes.
fn persisted_team_frame(persisted: SessionScope) -> (Fixture, String, Value) {
    let f = fixture(false);
    let tab = f.connect(&browser());
    let ctx = Arc::clone(&f.ctx);
    let id = f
        .bridge
        .start_session(StartRequest {
            kind: SessionKind::Authoring,
            guidance: Box::new(move |dir: &SessionDir| {
                let guidance = FixedGuidance.authoring(&ctx, dir, None, None, None)?;
                scope::write(dir, &persisted)?;
                Ok(guidance)
            }),
            opening_origin: None,
            offer: None,
            feature: None,
            plan: None,
            extra_env: Vec::new(),
        })
        .unwrap();
    let s = f.launched(&id);
    opening(&f, &s);
    f.spool(&s, "UserPromptSubmit", json!({"prompt": "opening"}));
    f.wait_state(&s, SessionState::Working);

    teammate(&f, &s);
    let view = wait_for("the persisted team frame", || {
        tab.frames()
            .into_iter()
            .find(|frame| frame["type"] == "draft_team")
    });
    (f, id, view)
}

#[test]
fn team_events_load_documents_from_persisted_scope_without_dispatch_start() {
    let documents = vec!["doc/product/demo/section_02.md".into()];
    let (f, id, view) = persisted_team_frame(SessionScope::team(documents, Vec::new()).unwrap());
    assert_eq!(view["session"], id);
    assert_eq!(view["documents"], json!(["doc/product/demo/section_02.md"]));
    assert_team_replayed_after_chat(&f, &view);
}

// Regression: an address run's ledger was a document row no teammate ever writes, so the panel
// read it as queued and the run never finished.
#[test]
fn an_address_team_frame_leaves_out_the_ledger() {
    let ledger = PathBuf::from("doc/plans/demo/findings.md");
    let documents = vec!["doc/product/demo/section_02.md".into(), ledger.clone()];
    let (_f, _id, view) = persisted_team_frame(SessionScope::address(documents, ledger).unwrap());
    assert_eq!(view["documents"], json!(["doc/product/demo/section_02.md"]));
}
