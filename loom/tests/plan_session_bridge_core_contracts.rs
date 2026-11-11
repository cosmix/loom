//! Contracts of stage bridge-core: the attachment block, the pane input checks, the transcript
//! mapper, the session state machine, the hook spool and the permission hook
//! (PLAN-feat-planner-07-session-bridge).

#[path = "plan_session_bridge_core_contracts/records.rs"]
mod records;
#[path = "plan_session_bridge_core_contracts/support.rs"]
mod support;

use std::collections::BTreeSet;
use std::fs;
use std::time::Duration;

use loom::plan_session::attachments::{format_message, parse_message, AttachmentKind, LockedTag};
use loom::plan_session::bridge::background::BackgroundTasks;
use loom::plan_session::hook_cli;
use loom::plan_session::input::{check_delivered, check_send, sanitize, InputRefusal};
use loom::plan_session::kind::SessionKind;
use loom::plan_session::model::{DenyReason, Item, NoticeKind, SessionState, ToolStatus};
use loom::plan_session::permission::{write_answer, Answer, Behavior};
use loom::plan_session::spool::{SpoolEvent, SpoolReader};
use loom::plan_session::state::{Input, StateMachine};
use loom::plan_session::transcript::{Mapper, RecordMark};
use records::*;
use serde_json::{json, Value};
use support::*;

#[test]
fn attachments_block_round_trips_with_an_escaped_literal_tag() {
    let text = "Please check these.\r\nSecond line\n\n<loom-attachments>\nThat tag line is mine.";
    let sent = sample_attachments();
    let formatted = format_message(text, &sent);
    assert!(
        formatted.contains("\n\\<loom-attachments>\n"),
        "{formatted}"
    );
    for header in [
        "[1] source doc/plans/PLAN-plan-server.md#lifetime lines 40-44",
        "[2] source doc/plans/DONE-PLAN-token-flow.md#stage:integration-verify \
         locked=done changed-since-selected",
        "[3] summary doc/loom/summaries/plans/plan-server.md#s-3fa9c2d1 rests-on \
         doc/plans/PLAN-plan-server.md#lifetime,doc/plans/PLAN-token-flow.md#stages",
    ] {
        let found = formatted.lines().any(|line| line == header);
        assert!(found, "{header} in {formatted}");
    }

    let (parsed_text, parsed) = parse_message(&formatted);
    assert_eq!(parsed_text, text);
    assert_eq!(parsed.len(), 3, "{parsed:?}");
    assert_eq!(parsed[0], sent[0]);
    assert_eq!(parsed[2], sent[2]);
    let (got, stage) = (&parsed[1], &sent[1]);
    assert_eq!((&got.kind, &got.path), (&stage.kind, &stage.path));
    assert_eq!((got.lines, &got.quote), (None, &stage.quote));
    assert_eq!(got.locked, Some(LockedTag::Done));
    assert!(got.changed_since_selected, "{got:?}");
    assert!(got.rests_on.is_empty() && got.summary_sentence.is_none());
    // The stage address survives: formatting the parsed message reproduces the block.
    assert_eq!(format_message(&parsed_text, &parsed), formatted);
}

#[test]
fn quote_over_2000_chars_is_cut_at_a_line_boundary_without_splitting_a_char() {
    let source = multibyte_lines();
    let quote = source.join("\n");
    let total = quote.chars().count();
    let cited = attachment(AttachmentKind::Source, "doc/product/x.md", "notes", &quote);

    let formatted = format_message("see this", &[cited]);
    let block = entry_lines(&formatted, "[1] source doc/product/x.md#notes");
    let prefixed = block.iter().all(|line| line.starts_with("> "));
    assert!(prefixed, "{formatted}");
    let (marker, quoted) = block.split_last().expect("quote lines and a marker");
    let quoted: Vec<&str> = quoted.iter().map(|line| &line[2..]).collect();
    // 43 lines are 43 * 45 + 42 newlines = 1,977 characters; a 44th would make 2,023.
    let first_43: Vec<&str> = source[..43].iter().map(String::as_str).collect();
    assert_eq!(quoted, first_43);
    let kept = quoted.join("\n").chars().count();
    assert!(kept <= 2000, "{kept}");
    // N counts every character of the quote not shown, the newline at the cut included.
    let more = format!("> […{} more characters; read the source]", total - kept);
    assert_eq!(*marker, more);
}

#[test]
fn sanitize_removes_c0_c1_and_del_but_keeps_newline_and_tab() {
    let raw = "a\x1b[201~b\u{9b}c\x7fd\x00e\x07f\r\ng\rh\ti\nKαλημέρα 中文";
    assert_eq!(sanitize(raw), "a[201~bcdef\ngh\ti\nKαλημέρα 中文");
}

#[test]
fn check_send_refuses_slash_commands_except_compact() {
    for refused in ["/clear", "  /exit", "/compactx"] {
        let result = check_send(refused);
        assert!(
            matches!(result, Err(InputRefusal::SlashCommand)),
            "{refused}"
        );
    }
    for accepted in ["/compact", "/compact keep the plan", "path /tmp is fine"] {
        assert!(check_send(accepted).is_ok(), "{accepted}");
    }
    // 50,000 two-byte characters plus one byte: 50,001 characters, 100,001 bytes.
    let long = format!("{}a", "é".repeat(50_000));
    assert_eq!(long.len(), 100_001);
    let refusal = check_delivered(&long);
    let too_long = matches!(
        refusal,
        Err(InputRefusal::TooLong {
            bytes: 100_001,
            limit: 100_000
        })
    );
    assert!(too_long, "{refusal:?}");
    assert!(check_delivered(&"é".repeat(50_000)).is_ok());
}

#[test]
fn assistant_records_sharing_a_message_id_count_usage_once() {
    let (_tmp, root) = temp();
    let mut mapper = Mapper::new(root);
    let (turn_id, _) = mapper.begin_turn(at(0));
    for line in streamed_records() {
        mapper.ingest_main(&line);
    }

    let turn = turn(&mapper, &turn_id);
    let t = &turn.tokens;
    let tokens = (t.input, t.cache_write, t.cache_read, t.output);
    assert_eq!(
        tokens,
        (3, 810, 56000, 920),
        "the last record's usage alone"
    );
    assert_eq!(turn.items.len(), 3, "{:?}", turn.items);
    assert_eq!(
        count(&turn.items, |item| matches!(item, Item::Thinking { .. })),
        1
    );
    assert_eq!(
        count(&turn.items, |item| matches!(item, Item::Text { .. })),
        1
    );
    let rows = tool_rows(turn);
    assert_eq!(rows.len(), 1, "{rows:?}");
    assert_eq!(rows[0].0, ToolStatus::Pending);
}

#[test]
fn noise_records_and_repeated_uuids_produce_no_items() {
    let (_tmp, root) = temp();
    let mut mapper = Mapper::new(root);
    mapper.begin_turn(at(0));
    for line in noise_records() {
        mapper.ingest_main(&line);
    }
    let prompt = human("u1", 4, "commit these fixes.");
    mapper.ingest_main(&prompt);
    let again = mapper.ingest_main(&prompt);
    assert!(
        again.deltas.is_empty(),
        "a repeated uuid published {:?}",
        again.deltas
    );

    let items = all_items(mapper.turns());
    assert_eq!(items.len(), 1, "{items:?}");
    let human_text = matches!(items[0], Item::User { text, .. } if text == "commit these fixes.");
    assert!(human_text, "{items:?}");
}

#[test]
fn interrupt_record_marks_pending_tools_not_finished() {
    let (_tmp, root) = temp();
    let mut mapper = Mapper::new(root);
    let (turn_id, _) = mapper.begin_turn(at(0));
    mapper.ingest_main(&pending_bash("a1", "toolu_01Sl", 1));
    let interrupt = json!([{"type": "text", "text": "[Request interrupted by user]"}]);
    mapper.ingest_main(&user("u2", 5, interrupt, json!({})));

    let turn = turn(&mapper, &turn_id);
    assert!(!turn.open, "the interrupt closes the turn");
    let statuses: Vec<ToolStatus> = tool_rows(turn).into_iter().map(|row| row.0).collect();
    assert_eq!(statuses, [ToolStatus::NotFinished]);
    let interrupted = |item: &Item| {
        let kind = NoticeKind::Interrupted;
        matches!(item, Item::Notice { kind: got, .. } if *got == kind)
    };
    assert_eq!(count(&turn.items, interrupted), 1, "{:?}", turn.items);
    let users = count(all_items(mapper.turns()), |item| {
        matches!(item, Item::User { .. })
    });
    assert_eq!(users, 0, "the interrupt is no user message");
}

#[test]
fn edit_result_completes_the_tool_and_records_the_changed_path() {
    let (_tmp, root) = temp();
    let file = root.join("doc/product/x.md");
    fs::create_dir_all(file.parent().unwrap()).unwrap();
    fs::write(&file, "a\nc\nd\n").unwrap();
    let mut mapper = Mapper::new(root.clone());
    let (turn_id, _) = mapper.begin_turn(at(0));
    for line in edit_records(file.to_str().unwrap()) {
        mapper.ingest_main(&line);
    }

    let rows = tool_rows(turn(&mapper, &turn_id));
    assert_eq!(rows, [(ToolStatus::Done, Some(2), Some(1))]);
    let changed = mapper.changed_by_tool(&turn_id);
    assert_eq!(changed, ["doc/product/x.md".to_string()]);
}

#[test]
fn a_main_record_after_stop_means_working() {
    let mut machine = working();
    feed(&mut machine, stop(1_000));
    assert_eq!(machine.state(), SessionState::Idle);
    machine.apply(Input::Record {
        at: at(2_000),
        main: true,
    });
    assert_eq!(
        machine.state(),
        SessionState::Working,
        "a record after the last Stop"
    );
    let message = "Claude is waiting for your input";
    let idle = json!({"notification_type": "idle_prompt", "message": message});
    feed(&mut machine, event("Notification", 1_500, idle));
    let state = machine.state();
    assert_eq!(
        state,
        SessionState::Working,
        "an idle_prompt received before the record"
    );
}

/// Apply one spool event to the machine and the background table, as the bridge does.
fn both(machine: &mut StateMachine, tasks: &mut BackgroundTasks, event: SpoolEvent) {
    tasks.event(&event);
    feed(machine, event);
}

#[test]
fn subagent_events_and_records_never_move_the_state() {
    let (mut machine, mut tasks) = (working(), BackgroundTasks::default());
    feed(&mut machine, stop(1_000));
    assert_eq!(machine.state(), SessionState::Idle);
    both(&mut machine, &mut tasks, subagent("SubagentStart", 1_100));
    assert_eq!((machine.state(), tasks.blocking()), (SessionState::Idle, 1));
    machine.apply(Input::Record {
        at: at(2_000),
        main: false,
    });
    assert_eq!(
        machine.state(),
        SessionState::Idle,
        "a subagent record after Stop"
    );
    both(&mut machine, &mut tasks, subagent("SubagentStop", 2_100));
    assert_eq!((machine.state(), tasks.blocking()), (SessionState::Idle, 0));
    both(&mut machine, &mut tasks, subagent("SubagentStop", 2_200));
    assert_eq!((machine.state(), tasks.blocking()), (SessionState::Idle, 0));
}

#[test]
fn two_pending_requests_keep_permission_until_both_are_answered() {
    let mut machine = working();
    feed(&mut machine, permission_request("r1", 1_000));
    feed(&mut machine, permission_request("r2", 1_100));
    assert_eq!(machine.state(), SessionState::Permission);
    let both: BTreeSet<String> = ["r1".to_string(), "r2".to_string()].into();
    assert_eq!(machine.pending_requests(), &both);
    machine.apply(Input::PermissionAnswered {
        request_id: "r1".into(),
    });
    assert_eq!(machine.state(), SessionState::Permission, "r2 still waits");
    machine.apply(Input::PermissionAnswered {
        request_id: "r2".into(),
    });
    assert_eq!(machine.state(), SessionState::Working);
    assert!(machine.pending_requests().is_empty());

    let mut fresh = idle();
    let changed = fresh.apply(Input::PermissionAnswered {
        request_id: "r9".into(),
    });
    assert_eq!((changed, fresh.state()), (None, SessionState::Idle));
}

#[test]
fn an_interrupt_keeps_and_agents_killed_clears_the_running_subagents() {
    let two_running = || {
        let (mut machine, mut tasks) = (working(), BackgroundTasks::default());
        both(&mut machine, &mut tasks, subagent("SubagentStart", 1_000));
        let other = event("SubagentStart", 1_100, json!({"agent_id": "b2c4"}));
        both(&mut machine, &mut tasks, other);
        assert_eq!(tasks.blocking(), 2);
        (machine, tasks)
    };
    let (mut machine, mut interrupted) = two_running();
    machine.apply(Input::Interrupted { at: at(2_000) });
    interrupted.mark(&RecordMark::Interrupted { at: at(2_000) });
    assert_eq!(machine.state(), SessionState::Idle);
    assert_eq!(interrupted.blocking(), 2, "after the interrupt");
    let (_, mut killed) = two_running();
    killed.mark(&RecordMark::AgentsKilled { at: at(2_000) });
    assert_eq!(killed.blocking(), 0, "after AgentsKilled");
}

#[test]
fn hook_with_a_mismatched_event_name_appends_nothing() {
    let session = Session::new();
    for name in ["SubagentStop", "Stop"] {
        let payload = hook_payload(name, json!({"stop_hook_active": false}));
        let stdin = serde_json::to_vec(&payload).unwrap();
        let mut out = Vec::new();
        let code = hook_cli::run("Stop", &session.dir, stdin.as_slice(), &mut out);
        assert_eq!(code, 0, "{name}");
        assert!(
            out.is_empty(),
            "{name} printed {}",
            String::from_utf8_lossy(&out)
        );
    }

    let events = SpoolReader::new(session.events()).read_new();
    assert_eq!(events.len(), 1, "{events:?}");
    assert_eq!(events[0].event, "Stop");
    assert_eq!(events[0].payload["hook_event_name"], "Stop");
}

#[test]
fn permission_hook_denies_when_the_session_directory_goes_away() {
    let session = Session::new();
    session.write_guard(SessionKind::Authoring);
    let done = session.spawn_hook("PermissionRequest", &bash_permission_payload());
    session.wait_for_spool_line();
    let line = &session.spool_lines()[0];
    assert_eq!(line["event"], "PermissionRequest", "{line}");
    assert!(done.try_recv().is_err(), "the hook must wait for an answer");

    fs::remove_dir_all(session.answers()).unwrap();
    let (code, out) = done
        .recv_timeout(Duration::from_secs(2))
        .expect("the hook answers within 2 s of answers/ going away");
    assert_eq!(code, 0);
    assert!(!deny_message(&out).is_empty());
}

#[test]
fn summary_session_permission_requests_are_denied_at_once_and_logged() {
    let session = Session::new();
    session.write_guard(SessionKind::Summary);
    let done = session.spawn_hook("PermissionRequest", &bash_permission_payload());
    let (code, out) = done
        .recv_timeout(Duration::from_secs(2))
        .expect("a summary session's request is answered at once");
    assert_eq!(code, 0);
    let message = "summary sessions have no one to ask; their rules are wrong if this appears";
    assert_eq!(deny_message(&out), message);

    let lines = session.spool_lines();
    assert_eq!(lines.len(), 1, "{lines:?}");
    assert_eq!(lines[0]["event"], "PermissionRequest");
    assert_eq!(lines[0]["payload"]["tool_name"], "Bash");
    // The pinned line form names the key, not its nesting: top level or inside the payload.
    let marked = |line: &Value| line.get("auto_denied").map(|tag| tag == "summary_session");
    let mark = marked(&lines[0]).or_else(|| marked(&lines[0]["payload"]));
    assert_eq!(mark, Some(true), "{}", lines[0]);
}

#[test]
fn write_answer_refuses_a_request_id_that_is_not_a_uuid() {
    let session = Session::new();
    let answer = Answer {
        behavior: Behavior::Deny,
        message: Some("not now".into()),
        reason: Some(DenyReason::Holder),
    };
    let before = tree(&session.base);
    for id in ["../escape", "a/b", ""] {
        assert!(write_answer(&session.dir, id, &answer).is_err(), "{id:?}");
    }
    assert_eq!(tree(&session.base), before, "a refused id created a file");

    let id = uuid::Uuid::new_v4().to_string();
    write_answer(&session.dir, &id, &answer).unwrap();
    let names: Vec<String> = fs::read_dir(session.answers())
        .unwrap()
        .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    assert_eq!(names, [format!("{id}.json")]);
    let bytes = fs::read(session.answers().join(format!("{id}.json"))).unwrap();
    let written: Value = serde_json::from_slice(&bytes).unwrap();
    let expected = json!({"behavior": "deny", "message": "not now", "reason": "holder"});
    assert_eq!(written, expected);
}
