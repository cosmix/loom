//! Slash command records of the P2 fixtures: what each shape marks for the bridge and the notice
//! its output leaves in the chat.

use serde_json::{json, Value};

use super::notification::BackgroundEvidence;
use super::tests::{at, drive, lines, mapper, user};
use super::{CommandMark, Mapper, RecordMark};
use crate::plan_session::model::{Item, NoticeKind};

/// The lines of transcript fixture `name` whose record `pick` accepts.
fn picked(name: &str, pick: impl Fn(&Value) -> bool) -> Vec<String> {
    let all = lines(&format!("{name}.jsonl")).into_iter();
    let parsed = all.map(|line| (serde_json::from_str::<Value>(&line).unwrap(), line));
    parsed
        .filter(|(record, _)| pick(record))
        .map(|(_, line)| line)
        .collect()
}

fn content(record: &Value) -> &str {
    let text = record.pointer("/message/content").or(record.get("content"));
    text.and_then(Value::as_str).unwrap_or_default()
}

fn commands(marks: &[RecordMark]) -> Vec<CommandMark> {
    let commands = marks.iter().filter_map(|mark| match mark {
        RecordMark::Command(command) => Some(command.clone()),
        _ => None,
    });
    commands.collect()
}

fn notices(mapper: &Mapper) -> Vec<String> {
    let items = mapper.turns().iter().flat_map(|turn| &turn.items);
    let notices = items.filter_map(|item| match item {
        Item::Notice {
            kind: NoticeKind::Info,
            text,
            ..
        } => Some(text.clone()),
        _ => None,
    });
    notices.collect()
}

fn invoked(name: &str, args: &str, prompt: bool) -> CommandMark {
    let (name, args) = (name.to_owned(), args.to_owned());
    CommandMark::Invoked { name, args, prompt }
}

fn run(command: &str, args: &str) -> CommandMark {
    let run = Some((command.to_owned(), args.to_owned()));
    CommandMark::Output { run }
}

#[test]
fn a_prompt_type_record_marks_its_command_and_shows_nothing() {
    let records = picked("p2-skills", |r| {
        content(r).starts_with("<command-message>loom-plan:args")
    });
    let mut mapper = mapper();
    mapper.begin_turn(at(0));
    let out = drive(&mut mapper, &records);
    let prompt = invoked("loom-plan:args", "alpha beta", true);
    assert_eq!(commands(&out.marks), [prompt]);
    assert!(mapper.turns()[0].items.is_empty(), "{:?}", mapper.turns());
}

#[test]
fn context_records_mark_the_command_and_its_run_and_skip_the_markdown_copy() {
    let records = picked("p2-usage-context", |r| {
        r["subtype"] == "local_command" || r["isMeta"] == true
    });
    let mut mapper = mapper();
    let out = drive(&mut mapper, &records);
    let marks = [invoked("context", "", false), run("context", "")];
    assert_eq!(commands(&out.marks), marks);
    let shown = notices(&mapper);
    assert_eq!(shown.len(), 1, "{shown:?}");
    assert!(shown[0].starts_with("Context Usage"), "{:.80}", shown[0]);
    assert!(!shown.iter().any(|text| text.contains("## Context Usage")));
}

#[test]
fn local_user_records_trim_their_indented_elements() {
    let records = picked("p2-effort-model", |r| {
        let text = content(r);
        text.starts_with("<command-name>/effort") || text.starts_with("<local-command-stdout>Set")
    });
    let mut mapper = mapper();
    let out = drive(&mut mapper, &records);
    assert!(
        records[0].contains("\\n            <command-message>"),
        "{}",
        records[0]
    );
    let marks = [
        invoked("effort", "high", false),
        CommandMark::Output { run: None },
    ];
    assert_eq!(commands(&out.marks)[..2], marks);
    let shown = notices(&mapper);
    assert!(
        shown[0].starts_with("Set effort level to high"),
        "{shown:?}"
    );
}

#[test]
fn a_compact_leaves_its_command_mark_and_a_notice_without_escapes() {
    let records = picked("p2-compact", |r| {
        let text = content(r);
        text.starts_with("<command-name>/compact") || text.starts_with("<local-command-stdout>")
    });
    let mut mapper = mapper();
    let out = drive(&mut mapper, &records);
    let marks = [
        invoked("compact", "", false),
        CommandMark::Output { run: None },
    ];
    assert_eq!(commands(&out.marks), marks);
    let shown = notices(&mapper);
    assert!(
        shown[0].starts_with("Compacted (ctrl+o to see full summary)"),
        "{shown:?}"
    );
    assert!(!shown[0].contains('\u{1b}'), "{shown:?}");
}

#[test]
fn a_fork_launch_marks_its_skill_beside_its_background_evidence() {
    let records = picked("p2-skills", |r| {
        content(r) == "/probe-fork" || content(r).contains("<forked-skill-launch>")
    });
    let mut mapper = mapper();
    let out = drive(&mut mapper, &records);
    let skill = CommandMark::Forked {
        skill: "probe-fork".to_owned(),
    };
    assert_eq!(commands(&out.marks), [skill]);
    let fork = out.marks.iter().any(|mark| {
        let evidence = match mark {
            RecordMark::Background { evidence, .. } => Some(evidence),
            _ => None,
        };
        matches!(evidence, Some(BackgroundEvidence::ForkLaunch { .. }))
    });
    assert!(fork, "{:?}", out.marks);
    assert_eq!(
        notices(&mapper),
        ["Running in the background as @probe-fork"]
    );
    let typed = mapper.turns()[0].items.iter().any(
        |item| matches!(item, Item::User { text, command: None, .. } if text == "/probe-fork"),
    );
    assert!(typed, "the plain /probe-fork record is the user's line");
}

#[test]
fn an_unknown_name_marks_it_and_leaves_its_warning() {
    let records = picked("p2-unlisted-lengths", |r| r["subtype"] == "informational");
    let mut mapper = mapper();
    let out = drive(&mut mapper, &records);
    let unknown = CommandMark::Unknown {
        name: "probe-unlisted-zz".to_owned(),
    };
    assert_eq!(commands(&out.marks), [unknown]);
    assert_eq!(notices(&mapper), ["Unknown command: /probe-unlisted-zz"]);
}

fn line(uuid: &str, record: Value) -> String {
    let mut record = record;
    record["uuid"] = json!(uuid);
    record["timestamp"] = json!("2026-10-27T10:00:03Z");
    record.to_string()
}

#[test]
fn output_loses_osc_links_and_c1_controls_but_keeps_tabs_and_lines() {
    let stderr = "<local-command-stderr> \u{1b}]8;;https://x.test\u{7}link\u{1b}]8;;\u{7}\u{9b}\n\tnext \u{1b}[2mdim\u{1b}[22m </local-command-stderr>";
    let record = json!({"type": "user", "message": {"role": "user", "content": stderr}});
    let mut mapper = mapper();
    let out = drive(&mut mapper, &[line("u1", record)]);
    assert_eq!(commands(&out.marks), [CommandMark::Output { run: None }]);
    assert_eq!(notices(&mapper), ["link\n\tnext dim"]);
}

#[test]
fn meta_and_untagged_records_are_no_command_records() {
    let caveat = "<local-command-caveat>Caveat</local-command-caveat>";
    let shaped =
        "<command-name>/effort</command-name>\n            <command-args>high</command-args>";
    let records = [
        line(
            "u1",
            json!({"type": "user", "isMeta": true, "message": {"content": shaped}}),
        ),
        line(
            "u2",
            json!({"type": "user", "message": {"content": caveat}}),
        ),
        line(
            "u3",
            json!({"type": "system", "subtype": "local_command", "content": "plain"}),
        ),
        line(
            "u4",
            json!({"type": "user", "origin": {"kind": "human"}, "message": {"content": "/x y"}}),
        ),
    ];
    let mut mapper = mapper();
    let out = drive(&mut mapper, &records);
    assert_eq!(commands(&out.marks), []);
    assert_eq!(mapper.turns()[0].items, [user("u-u4", "/x y", None)]);
}
