//! The commands view against its web fixtures: `session-commands.json` is the serializer's
//! text, and `slash-vectors.json` holds the cases the send gate and the web gate both pass.

use serde::Deserialize;
use serde_json::{json, Value};

use super::{frame, view};
use crate::plan_server::context::PlanContext;
use crate::plan_server::ids::{BrowserId, PublicId};
use crate::plan_server::request::{ApiRequest, ApiResponse, QueryParams};
use crate::plan_server::wire::commands::{CatalogueWireState, CommandsView};
use crate::plan_server::wire::message::PlanMessage;
use crate::plan_session::commands::{CommandSource, CommandTable};
use crate::plan_session::input::check_send;
use crate::web_server::http::parse_head;

const VIEW: &str = include_str!("../../../../web/src/plan/api/fixtures/session-commands.json");
const VECTORS: &str = include_str!("../../../../web/src/plan/api/fixtures/slash-vectors.json");
const INITIALIZE: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/tests/fixtures/plan_session/probe-notes/p2-side-initialize.side-loom-ui-disabled.jsonl"
));

/// `GET /api/session/commands` with `query` (`""` or `?session=<id>`) against `ctx`.
pub(crate) fn call_route(ctx: &PlanContext, query: &str) -> ApiResponse {
    let route = super::routes().remove(0);
    let raw = format!(
        "GET {}{query} HTTP/1.1\r\nHost: localhost\r\n\r\n",
        route.path
    );
    let head = parse_head(raw.as_bytes()).unwrap().unwrap();
    let browser = BrowserId::parse(&"ab".repeat(16)).unwrap();
    let public_id = PublicId::mint();
    let request = ApiRequest {
        ctx,
        head: &head,
        browser: &browser,
        public_id: &public_id,
        query: QueryParams::parse(head.query.as_deref()).unwrap(),
        body: Vec::new(),
    };
    (route.handler)(&request)
}

/// The rows of the recorded `initialize` answer.
fn probe_rows() -> Vec<Value> {
    let answer: Value = serde_json::from_str(INITIALIZE.lines().next().unwrap()).unwrap();
    answer["response"]["response"]["commands"]
        .as_array()
        .unwrap()
        .clone()
}

/// A table over a command list's rows, as a session's catalogue holds it.
fn table_of(view: &CommandsView) -> CommandTable {
    match view.state {
        CatalogueWireState::Loading => CommandTable::loading(),
        CatalogueWireState::Unavailable => {
            CommandTable::unavailable(view.reason.as_deref().unwrap())
        }
        CatalogueWireState::Ready => {
            let listed = view
                .entries
                .iter()
                .filter(|e| e.source != CommandSource::BuiltIn);
            let rows: Vec<Value> = listed
                .map(
                    |e| json!({"name": e.name, "description": e.description, "aliases": e.aliases}),
                )
                .collect();
            CommandTable::from_initialize(&rows, view.generation.unwrap())
        }
    }
}

#[test]
fn the_session_commands_fixture_carries_the_serializers_values() {
    let fixture: CommandsView = serde_json::from_str(VIEW).unwrap();
    // The web formatter lays out short arrays inline, so the file's text can differ
    // from the serializer's; the parsed values are what the two sides share.
    let reserialized: Value = serde_json::to_value(&fixture).unwrap();
    assert_eq!(reserialized, serde_json::from_str::<Value>(VIEW).unwrap());
    assert!(VIEW.ends_with('\n'));
    assert_eq!(fixture.state, CatalogueWireState::Ready);
    assert_eq!(fixture.generation, Some(1));
    let names: Vec<&str> = fixture.entries.iter().map(|e| e.name.as_str()).collect();
    let expected = [
        "simplify",
        "loom-plan:args",
        "probe-personal",
        "docs",
        "context",
        "compact",
        "clear",
        "usage",
        "effort",
        "help",
    ];
    assert_eq!(names, expected);
}

#[test]
fn every_fixture_entry_is_a_row_of_the_probe_table() {
    let fixture: CommandsView = serde_json::from_str(VIEW).unwrap();
    let probe = view(&CommandTable::from_initialize(&probe_rows(), 1));
    for entry in &fixture.entries {
        let row = probe.entries.iter().find(|e| e.name == entry.name);
        assert_eq!(row, Some(entry), "{}", entry.name);
    }
}

#[test]
fn the_vectors_hold_the_fixture_view_and_the_native_tables() {
    let vectors: Vectors = serde_json::from_str(VECTORS).unwrap();
    let fixture: CommandsView = serde_json::from_str(VIEW).unwrap();
    assert_eq!(vectors.tables.ready, fixture);
    assert_eq!(vectors.tables.loading, view(&CommandTable::loading()));
    let unavailable = vectors.tables.unavailable;
    assert_eq!(unavailable.state, CatalogueWireState::Unavailable);
    assert_eq!(unavailable.entries, view(&CommandTable::loading()).entries);
}

#[test]
fn the_send_gate_gives_every_vector_its_answer() {
    let vectors: Vectors = serde_json::from_str(VECTORS).unwrap();
    assert!(vectors.cases.len() >= 24, "{} cases", vectors.cases.len());
    for case in &vectors.cases {
        let table = table_of(match case.table.as_str() {
            "ready" => &vectors.tables.ready,
            "loading" => &vectors.tables.loading,
            "unavailable" => &vectors.tables.unavailable,
            other => panic!("no table {other}"),
        });
        let got = check_send(&case.text, &table)
            .err()
            .map(|refusal| refusal.to_string());
        let want = (case.expect != "ok").then(|| case.expect.clone());
        assert_eq!(got, want, "{:?} on {}", case.text, case.table);
    }
}

#[derive(Deserialize)]
struct Vectors {
    tables: Tables,
    cases: Vec<Case>,
}

#[derive(Deserialize)]
struct Tables {
    ready: CommandsView,
    loading: CommandsView,
    unavailable: CommandsView,
}

#[derive(Deserialize)]
struct Case {
    text: String,
    table: String,
    expect: String,
}

#[test]
fn the_view_flattens_every_string_of_a_hostile_row() {
    let hostile = json!({
        "name": "evil",
        "description": "line one\nline two\u{1b}[31m `tick`",
        "argumentHint": "[a\nb]",
        "aliases": ["al\tias"],
    });
    let table = CommandTable::from_initialize(&[hostile], 3);
    let view = view(&table);
    let evil = view.entries.iter().find(|e| e.name == "evil").unwrap();
    for text in [&evil.description, &evil.argument_hint, &evil.aliases[0]] {
        assert!(!text.contains(['\n', '\t', '\u{1b}', '`']), "{text:?}");
    }
    assert_eq!(view.generation, Some(3));
    let unavailable = super::view(&CommandTable::unavailable("gone\nfor good"));
    assert_eq!(unavailable.reason.as_deref(), Some("gone for good"));
    assert_eq!(unavailable.generation, None);
}

#[test]
fn the_frame_carries_the_view_under_the_session() {
    let table = CommandTable::from_initialize(&probe_rows(), 2);
    let PlanMessage::Commands {
        session,
        state,
        generation,
        reason,
        entries,
    } = frame("s1", &table)
    else {
        panic!("not a commands frame");
    };
    let expected = view(&table);
    assert_eq!((session.as_str(), state), ("s1", expected.state));
    assert_eq!((generation, reason), (Some(2), None));
    assert_eq!(entries, expected.entries);
    let text = frame("s1", &table).to_frame();
    assert_eq!(
        serde_json::from_str::<Value>(&text).unwrap()["type"],
        "commands"
    );
}
