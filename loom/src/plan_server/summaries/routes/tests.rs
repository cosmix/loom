//! Summary route contracts through the planning server's loopback HTTP helpers.

use std::collections::BTreeMap;
use std::net::{IpAddr, Ipv4Addr};
use std::sync::{Arc, Mutex};

use serde_json::{json, Value};

use super::routes;
use crate::plan_server::lifecycle::safe;
use crate::plan_server::prereq::ReadOnly;
use crate::plan_server::summaries::rebuilds::Rebuilds;
use crate::plan_server::summaries::scheduler::SessionSpawner;
use crate::plan_server::tests::support::{
    bootstrap, get, json_body, post_json, skip, status, test_routes,
};
use crate::plan_server::wire::server::{MissingPrereq, PrereqKind};
use crate::plan_server::{start_with_routes, RunningServer, ServerOptions};
use crate::planning::summary::claims::Claim;
use crate::planning::summary::cost::Trigger;
use crate::planning::summary::engine::EngineUpdate;
use crate::planning::summary::evaluate::fixtures::{self, Fixture, ID1};
use crate::planning::summary::evaluate::{Evaluation, SummaryEval, SummaryState};
use crate::planning::summary::store::Scope;

const PANEL: &str = "/api/summaries/panel";
const REBUILD: &str = "/api/summaries/rebuild";
const RELEASE: &str = "/api/summaries/release";
const ANSWER: &str = "/api/summaries/answer";

#[derive(Default)]
struct Spawner(Mutex<Vec<Vec<String>>>);

impl SessionSpawner for Spawner {
    fn start_summary(&self, ids: &[String]) -> anyhow::Result<String> {
        self.0.lock().unwrap().push(ids.to_vec());
        Ok("summary-test".to_owned())
    }

    fn stop(&self, _: &str, _: bool) {}
}

fn serve(f: &Fixture, read_only: Option<&str>) -> (RunningServer, Arc<Rebuilds>, Arc<Spawner>) {
    let problems = read_only.map(|message| MissingPrereq {
        what: PrereqKind::Tmux,
        message: message.to_owned(),
        fix: "Install tmux".to_owned(),
    });
    let options = ServerOptions {
        root: f.root.clone(),
        host: IpAddr::V4(Ipv4Addr::LOCALHOST),
        port: Some(0),
        read_only: ReadOnly::from_problems(problems.into_iter().collect()),
    };
    let mut table = routes();
    table.extend(test_routes());
    let server = start_with_routes(options, table).expect("start the planning server");
    let spawner = Arc::new(Spawner::default());
    let rebuilds = Rebuilds::new_for_test(
        spawner.clone(),
        read_only.map(str::to_owned),
        Arc::new(|_| {}),
    );
    server.ctx.extensions().insert(rebuilds.clone());
    (server, rebuilds, spawner)
}

/// An evaluation of one summary that a terminal holds a claim on.
fn terminal_claim_update(id: &str) -> EngineUpdate {
    let claim = Claim {
        summary: id.to_owned(),
        session: None,
        pid: 1,
        created_at: String::new(),
    };
    let eval = SummaryEval {
        id: id.to_owned(),
        source: None,
        state: SummaryState::Current,
        file: None,
        sentences: Vec::new(),
        claim: Some(claim),
        error: None,
    };
    EngineUpdate {
        evaluation: Evaluation {
            summaries: BTreeMap::from([(id.to_owned(), eval)]),
            scope: Scope::default(),
            orphans: Vec::new(),
        },
        rewritten: Vec::new(),
        deleted: Vec::new(),
        invalidated: Vec::new(),
    }
}

fn snapshot(rebuilds: &Rebuilds) -> Value {
    serde_json::to_value(rebuilds.panel()).unwrap()
}

fn refused(response: &str, expected_status: u16, code: &str) {
    assert_eq!(status(response), expected_status, "{}", safe(response));
    assert_eq!(json_body(response)["error"], code, "{}", safe(response));
}

fn ask(rebuilds: &Rebuilds) -> String {
    let ids = (1..=6).map(|n| format!("product/section_{n:02}")).collect();
    rebuilds.enqueue(ids, Trigger::Background);
    rebuilds.panel().question.expect("six ids ask").id
}

fn take(server: &RunningServer, cookies: &str) {
    let response = post_json(server, "/api/test/take", cookies, "");
    assert_eq!(status(&response), 200, "{}", safe(&response));
}

#[test]
fn id_routes_refuse_bad_unknown_and_over_limit_ids_without_mutation() {
    if skip("id_routes_refuse_bad_unknown_and_over_limit_ids_without_mutation") {
        return;
    }
    let fixture = fixtures::two_sections();
    let (server, rebuilds, spawner) = serve(&fixture, None);
    let cookies = bootstrap(&server);
    let cases = [
        (json!({"ids": [ID1, "../unsafe\u{1b}[31m"]}), "bad_id"),
        (json!({"ids": vec![ID1; 65]}), "too_many_ids"),
    ];
    let before = snapshot(&rebuilds);
    for path in [REBUILD, RELEASE] {
        for (body, code) in &cases {
            refused(
                &post_json(&server, path, &cookies, &body.to_string()),
                400,
                code,
            );
            assert_eq!(snapshot(&rebuilds), before);
        }
    }
    let unknown = json!({"ids": [ID1, "product/section_99"]}).to_string();
    refused(
        &post_json(&server, REBUILD, &cookies, &unknown),
        400,
        "unknown_id",
    );
    assert_eq!(snapshot(&rebuilds), before);
    assert!(spawner.0.lock().unwrap().is_empty());
    assert!(server.ctx.authoring().holder().is_none());
}

#[test]
fn mutations_refuse_an_observer_and_keep_the_question_open() {
    if skip("mutations_refuse_an_observer_and_keep_the_question_open") {
        return;
    }
    let fixture = fixtures::two_sections();
    let (server, rebuilds, spawner) = serve(&fixture, None);
    let holder = bootstrap(&server);
    take(&server, &holder);
    let observer = bootstrap(&server);
    let question = ask(&rebuilds);
    let before = snapshot(&rebuilds);
    let held = server.ctx.authoring().holder();
    let actions = [
        (REBUILD, json!({"ids": [ID1]})),
        (RELEASE, json!({"ids": [ID1]})),
        (ANSWER, json!({"question": question, "rebuild": true})),
    ];
    for (path, body) in actions {
        refused(
            &post_json(&server, path, &observer, &body.to_string()),
            403,
            "not_allowed",
        );
        assert_eq!(snapshot(&rebuilds), before);
        assert_eq!(server.ctx.authoring().holder(), held);
    }
    assert!(spawner.0.lock().unwrap().is_empty());
    assert_eq!(status(&get(&server, PANEL, &observer)), 200);
}

#[test]
fn read_only_refuses_all_mutations_and_keeps_the_panel_readable() {
    if skip("read_only_refuses_all_mutations_and_keeps_the_panel_readable") {
        return;
    }
    let fixture = fixtures::two_sections();
    let (server, rebuilds, spawner) = serve(&fixture, Some("tmux is unavailable"));
    let cookies = bootstrap(&server);
    let before = snapshot(&rebuilds);
    for (path, body) in [
        (REBUILD, json!({"ids": [ID1]})),
        (RELEASE, json!({"ids": [ID1]})),
        (ANSWER, json!({"question": "absent", "rebuild": false})),
    ] {
        refused(
            &post_json(&server, path, &cookies, &body.to_string()),
            409,
            "read_only",
        );
        assert_eq!(snapshot(&rebuilds), before);
    }
    let response = get(&server, PANEL, &cookies);
    assert_eq!(status(&response), 200);
    assert_eq!(json_body(&response), before);
    assert!(spawner.0.lock().unwrap().is_empty());
    assert!(server.ctx.authoring().holder().is_none());
}

#[test]
fn release_refuses_an_id_without_a_terminal_claim() {
    if skip("release_refuses_an_id_without_a_terminal_claim") {
        return;
    }
    let fixture = fixtures::two_sections();
    let (server, rebuilds, _) = serve(&fixture, None);
    let cookies = bootstrap(&server);
    let before = snapshot(&rebuilds);
    let body = json!({"ids": [ID1]}).to_string();
    refused(
        &post_json(&server, RELEASE, &cookies, &body),
        409,
        "not_terminal",
    );
    assert_eq!(snapshot(&rebuilds), before);
}

#[test]
fn release_accepts_a_terminal_claim_on_a_summary_that_is_no_longer_expected() {
    if skip("release_accepts_a_terminal_claim_on_a_summary_that_is_no_longer_expected") {
        return;
    }
    let fixture = fixtures::two_sections();
    let (server, rebuilds, _) = serve(&fixture, None);
    let cookies = bootstrap(&server);
    let gone = "product/section_99";
    rebuilds.on_update(&terminal_claim_update(gone));
    assert_eq!(snapshot(&rebuilds)["terminal_claims"], json!([gone]));

    let body = json!({"ids": [gone]}).to_string();
    let response = post_json(&server, RELEASE, &cookies, &body);
    assert_eq!(status(&response), 200, "{}", safe(&response));
    assert_eq!(json_body(&response)["terminal_claims"], json!([]));
    assert_eq!(snapshot(&rebuilds)["terminal_claims"], json!([]));
}

#[test]
fn answer_refuses_a_missing_question_without_closing_the_open_one() {
    if skip("answer_refuses_a_missing_question_without_closing_the_open_one") {
        return;
    }
    let fixture = fixtures::two_sections();
    let (server, rebuilds, spawner) = serve(&fixture, None);
    let cookies = bootstrap(&server);
    ask(&rebuilds);
    let before = snapshot(&rebuilds);
    let body = json!({"question": "absent", "rebuild": true}).to_string();
    refused(
        &post_json(&server, ANSWER, &cookies, &body),
        404,
        "no_such_question",
    );
    assert_eq!(snapshot(&rebuilds), before);
    assert!(spawner.0.lock().unwrap().is_empty());
}

#[test]
fn malformed_bodies_are_refused_without_mutation() {
    if skip("malformed_bodies_are_refused_without_mutation") {
        return;
    }
    let fixture = fixtures::two_sections();
    let (server, rebuilds, _) = serve(&fixture, None);
    let cookies = bootstrap(&server);
    let before = snapshot(&rebuilds);
    for path in [REBUILD, RELEASE, ANSWER] {
        for body in ["{", "{}"] {
            refused(
                &post_json(&server, path, &cookies, body),
                400,
                "bad_request",
            );
            assert_eq!(snapshot(&rebuilds), before);
        }
    }
}

#[test]
fn rebuild_accepts_64_ids_as_a_button_without_taking_authoring() {
    if skip("rebuild_accepts_64_ids_as_a_button_without_taking_authoring") {
        return;
    }
    let fixture = fixtures::two_sections();
    let expected: Vec<String> = (1..=6).map(|n| format!("product/section_{n:02}")).collect();
    for n in 3..=6 {
        fixtures::put(
            &fixture.root,
            &format!("doc/product/section_{n:02}.md"),
            &fixtures::section(n, "Alpha.", "Beta."),
        );
    }
    let (server, rebuilds, spawner) = serve(&fixture, None);
    let cookies = bootstrap(&server);
    let ids: Vec<_> = expected.iter().cycle().take(64).cloned().collect();
    let body = json!({"ids": ids}).to_string();
    let response = post_json(&server, REBUILD, &cookies, &body);
    assert_eq!(status(&response), 202, "{}", safe(&response));
    assert_eq!(json_body(&response), snapshot(&rebuilds));
    assert_eq!(*spawner.0.lock().unwrap(), vec![expected]);
    assert!(rebuilds.panel().question.is_none());
    assert!(server.ctx.authoring().holder().is_none());
}

#[test]
fn holder_can_rebuild_without_changing_authoring() {
    if skip("holder_can_rebuild_without_changing_authoring") {
        return;
    }
    let fixture = fixtures::two_sections();
    let (server, _, spawner) = serve(&fixture, None);
    let cookies = bootstrap(&server);
    take(&server, &cookies);
    let held = server.ctx.authoring().holder();
    let body = json!({"ids": [ID1]}).to_string();
    let response = post_json(&server, REBUILD, &cookies, &body);
    assert_eq!(status(&response), 202, "{}", safe(&response));
    assert_eq!(*spawner.0.lock().unwrap(), vec![vec![ID1.to_owned()]]);
    assert_eq!(server.ctx.authoring().holder(), held);
}

#[test]
fn holder_or_free_browser_can_answer_and_release_without_taking_authoring() {
    if skip("holder_or_free_browser_can_answer_and_release_without_taking_authoring") {
        return;
    }
    let fixture = fixtures::two_sections();
    for held in [false, true] {
        let (server, rebuilds, spawner) = serve(&fixture, None);
        let cookies = bootstrap(&server);
        if held {
            take(&server, &cookies);
        }
        let holder = server.ctx.authoring().holder();
        let question = ask(&rebuilds);
        let body = json!({"question": question, "rebuild": held}).to_string();
        let response = post_json(&server, ANSWER, &cookies, &body);
        assert_eq!(status(&response), 200, "{}", safe(&response));
        assert_eq!(json_body(&response), snapshot(&rebuilds));
        assert!(rebuilds.panel().question.is_none());
        assert_eq!(spawner.0.lock().unwrap().len(), usize::from(held));
        let response = post_json(&server, RELEASE, &cookies, r#"{"ids":[]}"#);
        assert_eq!(status(&response), 200, "{}", safe(&response));
        assert_eq!(json_body(&response), snapshot(&rebuilds));
        assert_eq!(server.ctx.authoring().holder(), holder);
    }
}
