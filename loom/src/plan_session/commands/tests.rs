//! The native table, the source markers and the merge of `initialize` rows.

use std::collections::BTreeMap;

use serde_json::{json, Value};

use super::list::{CatalogueState, CommandClass, CommandSource, CommandTable};

const INITIALIZE: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/tests/fixtures/plan_session/probe-notes/p2-side-initialize.side-loom-ui-disabled.jsonl"
));
const REASONS: [&str; 4] = [
    "Changes the session outside the planner",
    "Opens a terminal dialog",
    "Saves your default for new sessions",
    "Not verified in the planner",
];

/// Line `index` of the fixture, and its rows at `pointer`.
fn fixture_rows(index: usize, pointer: &str) -> Vec<Value> {
    let line = INITIALIZE.lines().nth(index).expect("a fixture line");
    let value: Value = serde_json::from_str(line).expect("a JSON line");
    value
        .pointer(pointer)
        .and_then(Value::as_array)
        .unwrap()
        .clone()
}

fn answer_rows() -> Vec<Value> {
    fixture_rows(0, "/response/response/commands")
}

fn changed_rows() -> Vec<Value> {
    fixture_rows(1, "/commands")
}

fn counts(table: &CommandTable) -> BTreeMap<String, usize> {
    let mut counts = BTreeMap::new();
    for entry in table.entries() {
        *counts.entry(format!("{:?}", entry.source)).or_default() += 1;
    }
    counts
}

#[test]
fn every_native_name_is_in_the_table_once_with_one_class() {
    let table = CommandTable::loading();
    assert_eq!(table.state(), &CatalogueState::Loading);
    assert_eq!(table.entries().len(), 57);
    for entry in table.entries() {
        let named = table.entries().iter().filter(|e| e.name == entry.name);
        assert_eq!(named.count(), 1, "{} is listed twice", entry.name);
        assert_eq!(entry.source, CommandSource::BuiltIn, "{entry:?}");
        match entry.class {
            CommandClass::TerminalOnly => {
                let reason = entry.reason.as_deref().unwrap_or_default();
                assert!(REASONS.contains(&reason), "{entry:?}");
            }
            CommandClass::RunWithHandling => assert_eq!(entry.reason, None, "{entry:?}"),
            CommandClass::Run => panic!("a native row runs plainly: {entry:?}"),
        }
        assert!(!entry.description.is_empty(), "{entry:?}");
    }
    let handled: Vec<&str> = table
        .entries()
        .iter()
        .filter(|e| e.class == CommandClass::RunWithHandling)
        .map(|e| e.name.as_str())
        .collect();
    assert_eq!(handled, ["compact", "context"]);
}

#[test]
fn aliases_find_their_canonical_rows() {
    let table = CommandTable::unavailable("gone");
    let reason = "gone".to_owned();
    assert_eq!(table.state(), &CatalogueState::Unavailable { reason });
    for (alias, name) in [("reset", "clear"), ("new", "clear"), ("cost", "usage")] {
        assert_eq!(table.lookup(alias).map(|e| e.name.as_str()), Some(name));
    }
    assert_eq!(table.lookup("stats").unwrap().name, "usage");
    assert_eq!(table.lookup("clear").unwrap().name, "clear");
    assert!(table.lookup("probe-unlisted-zz").is_none());
    assert!(
        table.lookup("/clear").is_none(),
        "lookup takes a name without its slash"
    );
}

#[test]
fn a_listed_row_never_takes_a_native_name_or_alias() {
    let rows = [
        json!({"name": "probe:new", "description": "(probe) Starts", "aliases": ["new", "cost", "own"]}),
        json!({"name": "reset", "description": "A skill named like an alias"}),
    ];
    let table = CommandTable::from_initialize(&rows, 1);
    for (typed, native) in [("new", "clear"), ("cost", "usage"), ("reset", "clear")] {
        let entry = table.lookup(typed).unwrap();
        assert_eq!(
            (entry.name.as_str(), &entry.source),
            (native, &CommandSource::BuiltIn)
        );
        assert_eq!(entry.class, CommandClass::TerminalOnly, "{typed}");
    }
    let plugin = table.lookup("own").unwrap();
    assert_eq!(plugin.name, "probe:new");
    assert_eq!(plugin.aliases, ["own"]);
    let listed = table
        .entries()
        .iter()
        .filter(|e| e.source != CommandSource::BuiltIn);
    assert_eq!(listed.count(), 1, "the row named `reset` gave way");
}

#[test]
fn the_fixture_rows_read_as_their_sources() {
    let table = CommandTable::from_initialize(&answer_rows(), 1);
    let expected = [
        ("Account", 13),
        ("BuiltIn", 57),
        ("Bundled", 19),
        ("Personal", 32),
        ("Plugin", 17),
    ];
    let expected: BTreeMap<String, usize> = expected
        .iter()
        .map(|(k, n)| ((*k).to_owned(), *n))
        .collect();
    assert_eq!(counts(&table), expected);
    // A `plugin:name` row without a `(<plugin>)` prefix reads by its suffix.
    let research = table.lookup("<PLUGIN-2>:deep-research").unwrap();
    assert_eq!(research.source, CommandSource::Account);
    let bundled = table.lookup("deep-research").unwrap();
    assert_eq!(bundled.source, CommandSource::Bundled);
    assert!(
        bundled.description.ends_with("cited report."),
        "{bundled:?}"
    );
    let plugin = table
        .lookup("<PLUGIN-1>:building-<PLUGIN-8>-agents")
        .unwrap();
    assert!(
        plugin.description.starts_with("Build AI agents"),
        "{plugin:?}"
    );
    for entry in table
        .entries()
        .iter()
        .filter(|e| e.source != CommandSource::BuiltIn)
    {
        assert_eq!((&entry.class, &entry.reason), (&CommandClass::Run, &None));
        for marker in ["(user)", "(claude.ai sync)", "(dynamic workflow)"] {
            assert!(!entry.description.ends_with(marker), "{entry:?}");
        }
    }
}

#[test]
fn a_commands_changed_row_reads_as_personal() {
    let table = CommandTable::from_initialize(&changed_rows(), 2);
    assert_eq!(table.state(), &CatalogueState::Ready { generation: 2 });
    assert_eq!(counts(&table)["Personal"], 33);
    let added = table.lookup("probe-added").expect("the added skill");
    assert_eq!(added.source, CommandSource::Personal);
    assert_eq!(added.description, "Probe skill probe-added");
}

#[test]
fn listed_rows_keep_their_order_before_the_native_rows() {
    let table = CommandTable::from_initialize(&answer_rows(), 1);
    let names: Vec<&str> = table.entries().iter().map(|e| e.name.as_str()).collect();
    assert_eq!(names[0], "<SKILL-2>");
    assert_eq!(
        names[81], "advisor",
        "the native rows follow the 81 listed rows"
    );
    assert_eq!(names.last(), Some(&"statusline"));
}

#[test]
fn rows_are_capped_skipped_and_replaced() {
    let rows = [
        json!({"name": "n".repeat(300), "description": "d".repeat(2_000),
               "argumentHint": "h".repeat(300), "aliases": ["a".repeat(300), 7]}),
        json!({"description": "no name"}),
        json!({"name": 5}),
        json!({"name": ""}),
        json!({"name": "plain", "description": "A project skill"}),
        json!({"name": "plain", "description": "A repeat"}),
        json!({"name": "help", "description": "An impostor (user)"}),
        json!({"name": "x:y", "description": "(p) Plugin", "builtin": false}),
    ];
    let table = CommandTable::from_initialize(&rows, 3);
    let listed: Vec<_> = table.entries().iter().take(3).collect();
    assert_eq!(table.entries().len(), 3 + 57);
    let long = listed[0];
    assert_eq!(long.name.chars().count(), 200);
    assert_eq!(long.description.chars().count(), 1_000);
    assert_eq!(long.argument_hint.chars().count(), 200);
    assert_eq!(long.aliases, ["a".repeat(200)]);
    assert_eq!(long.source, CommandSource::Skill);
    assert_eq!(
        (listed[1].name.as_str(), &listed[1].source),
        ("plain", &CommandSource::Skill)
    );
    assert_eq!(listed[1].description, "A project skill");
    assert_eq!(
        (listed[2].description.as_str(), &listed[2].source),
        ("Plugin", &CommandSource::Plugin)
    );
    let help = table.lookup("help").unwrap();
    assert_eq!(
        help.source,
        CommandSource::BuiltIn,
        "the native row replaces a listed one"
    );
}

#[test]
fn entries_serialize_with_snake_case_keys_and_kebab_case_values() {
    let table = CommandTable::loading();
    let clear = serde_json::to_value(table.lookup("clear").unwrap()).unwrap();
    assert_eq!(
        clear,
        json!({
            "name": "clear", "aliases": ["reset", "new"],
            "description": "Start a new session with empty context; previous session stays on disk (resumable with /resume)",
            "argument_hint": "[name]", "source": "built-in", "class": "terminal-only",
            "reason": "Changes the session outside the planner",
        })
    );
    let context = serde_json::to_value(table.lookup("context").unwrap()).unwrap();
    assert_eq!(context["class"], "run-with-handling");
    assert_eq!(context["reason"], Value::Null);
}
