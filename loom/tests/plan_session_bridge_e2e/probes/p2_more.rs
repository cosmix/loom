//! P2 (plan step B1), continued: local and dialog commands (`/cost`, `/usage`, `/context`,
//! `/effort`, `/model`, `/help`, `/status`), an unlisted name, the paste lengths and
//! `/compact`, each pasted after Idle with Escape after a possible dialog.

use std::fs;

use serde_json::{json, Value};
use serial_test::serial;

use super::p2::{install_skills, PERSONAL};
use super::paste::{paste, Spec};
use super::probe::Probe;
use super::secs;

/// `/probe-personal` followed by ` w001 w002 …` up to exactly 300 characters on one line.
fn long_line() -> String {
    let mut line = format!("/{PERSONAL}");
    let mut word = 1;
    while line.len() + 5 <= 300 {
        line.push_str(&format!(" w{word:03}"));
        word += 1;
    }
    line.push_str(&"x".repeat(300 - line.len()));
    line
}

/// The top-level keys whose values differ, `env` values withheld (it may hold secrets).
fn settings_diff(before: &Value, after: &Value) -> String {
    let empty = serde_json::Map::new();
    let (old, new) = (
        before.as_object().unwrap_or(&empty),
        after.as_object().unwrap_or(&empty),
    );
    let mut keys: Vec<&String> = old.keys().chain(new.keys()).collect();
    keys.sort();
    keys.dedup();
    let changed = keys
        .into_iter()
        .filter(|key| old.get(*key) != new.get(*key));
    let line = |key: &String| match key.as_str() {
        "env" => "env: changed (values withheld)".to_owned(),
        _ => format!("{key}: {:?} -> {:?}", old.get(key), new.get(key)),
    };
    let lines: Vec<String> = changed.map(line).collect();
    if lines.is_empty() {
        "changed: false".to_owned()
    } else {
        format!("changed: true\n{}", lines.join("\n"))
    }
}

#[test]
fn settings_diff_names_changed_keys_and_withholds_env() {
    let before = json!({"model": "opus", "env": {"TOKEN": "a"}, "theme": "dark"});
    let after = json!({"model": "haiku", "env": {"TOKEN": "b"}, "theme": "dark"});
    let want = "changed: true\nenv: changed (values withheld)\n\
                model: Some(String(\"opus\")) -> Some(String(\"haiku\"))";
    assert_eq!(settings_diff(&before, &after), want);
    assert_eq!(settings_diff(&before, &before), "changed: false");
}

#[test]
fn the_long_slash_line_is_300_characters_on_one_line() {
    let line = long_line();
    assert_eq!(line.len(), 300);
    assert!(!line.contains('\n') && line.starts_with("/probe-personal w001"));
}

#[test]
#[ignore = "operator probe: needs a logged-in Claude Code and the network"]
#[serial]
fn p2_cost_usage_and_context() {
    let mut p = Probe::open("p2-usage-context", install_skills);
    paste(&mut p, &Spec::local("/cost", 15));
    paste(&mut p, &Spec::local("/usage", 30));
    paste(&mut p, &Spec::local("/context", 30));
    p.finish();
}

#[test]
#[ignore = "operator probe: needs a logged-in Claude Code and the network"]
#[serial]
fn p2_effort_and_model_with_arguments() {
    let mut p = Probe::open("p2-effort-model", install_skills);
    paste(&mut p, &Spec::local("/effort high", 15));
    // An unknown level: its answer names the accepted ones.
    paste(&mut p, &Spec::local("/effort turbo", 15));
    let settings = p.home.join(".claude/settings.json");
    let read = || {
        fs::read_to_string(&settings)
            .ok()
            .and_then(|t| serde_json::from_str(&t).ok())
    };
    let before: Value = read().unwrap_or_default();
    paste(&mut p, &Spec::local("/model haiku", 15));
    let after: Value = read().unwrap_or_default();
    let body = settings_diff(&before, &after);
    p.out
        .note("Copied ~/.claude/settings.json around /model haiku", &body);
    p.finish();
}

#[test]
#[ignore = "operator probe: needs a logged-in Claude Code and the network"]
#[serial]
fn p2_dialog_commands() {
    let mut p = Probe::open("p2-dialogs", install_skills);
    for command in ["/model", "/help", "/status"] {
        paste(&mut p, &Spec::local(command, 5));
    }
    p.finish();
}

#[test]
#[ignore = "operator probe: needs a logged-in Claude Code and the network"]
#[serial]
fn p2_unlisted_name_and_paste_lengths() {
    let mut p = Probe::open("p2-unlisted-lengths", install_skills);
    paste(&mut p, &Spec::local("/probe-unlisted-zz", 10));
    let short = format!("/{PERSONAL} abc defgh ijk");
    assert!((21..=40).contains(&short.len()), "{short:?}");
    paste(&mut p, &Spec::record(short, PERSONAL));
    let long = long_line();
    let added = paste(&mut p, &Spec::record(long.clone(), PERSONAL));
    let last_word = long.rsplit(' ').next().unwrap_or_default().to_owned();
    let whole = added.iter().any(|line| line.contains(&last_word));
    let body = format!("a record holds the last argument {last_word:?}: {whole}");
    p.out.note("The 300-character line's arguments", &body);
    p.finish();
}

#[test]
#[ignore = "operator probe: needs a logged-in Claude Code and the network"]
#[serial]
fn p2_compact_through_the_queue() {
    let mut p = Probe::open("p2-compact", install_skills);
    let compact = Spec {
        wait: secs(120),
        ..Spec::record("/compact", "compact_boundary")
    };
    paste(&mut p, &compact);
    p.finish();
}
