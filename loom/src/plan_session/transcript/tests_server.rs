//! The mapper's server-facing behaviour: delivery matching, origins, subagents, usage-only
//! lines, server items and malformed input.

use serde_json::{json, Value};

use super::tests::{
    at, drive, finished, human, lines, mapper, notice, only_turn, text, tokens, tool, user,
};
use super::RecordMark;
use crate::plan_session::attachments::{Attachment, AttachmentKind};
use crate::plan_session::input::MAX_DELIVERED_BYTES;
use crate::plan_session::model::{
    ChatDelta, Delivery, DenyReason, Item, NoticeKind, Origin, PermissionStatus, ToolStatus,
};

fn cited() -> Attachment {
    Attachment {
        kind: AttachmentKind::Source,
        path: "doc/plans/PLAN-plan-server.md".into(),
        anchor: Some("lifetime".into()),
        lines: Some((40, 44)),
        stage: None,
        summary_sentence: None,
        rests_on: Vec::new(),
        quote: "`loom plan` starts the daemon if it is not running.".into(),
        changed_since_selected: false,
        locked: None,
        file: None,
    }
}

#[test]
fn delivered_prompt_replaces_the_sent_item_and_carries_its_chips() {
    let sent = Item::User {
        id: "s-1".into(),
        text: "Please check this.".into(),
        attachments: vec![cited()],
        delivery: Delivery::Sent,
        origin: Some(Origin::Start),
    };
    let mut mapper = mapper();
    let added = mapper.add_server_item(sent);
    let opened = matches!(
        &added[..],
        [ChatDelta::TurnAdded { .. }, ChatDelta::ItemAdded { .. }]
    );
    assert!(opened, "{added:?}");
    let out = drive(&mut mapper, &lines("attachments.jsonl"));

    let mut delivered = user("u-u1", "Please check this.", Some(Origin::Start));
    if let Item::User { attachments, .. } = &mut delivered {
        *attachments = vec![cited()];
    }
    let turn_id = "t1".to_owned();
    let removed = ChatDelta::ItemRemoved {
        turn_id: turn_id.clone(),
        id: "s-1".into(),
    };
    let item = delivered.clone();
    assert_eq!(
        out.deltas,
        [removed, ChatDelta::ItemAdded { turn_id, item }]
    );
    assert_eq!(only_turn(&mapper).items, [delivered]);
    assert_eq!(out.marks, [human(0, "Please check this.")]);
    let adopted = mapper.begin_turn(at(0)).0;
    assert_eq!(adopted, "t1", "the UserPromptSubmit adopts the turn");
}

fn expected_spawn() -> Item {
    let input = json!({"description": "Find the dashboard stage",
                       "prompt": "Search the plans for the dashboard stage.",
                       "subagent_type": "Explore"});
    let spawn = tool(
        "a-a1-0",
        "toolu_01Ag",
        "Agent",
        "Find the dashboard stage",
        input,
    );
    let mut spawn = finished(spawn, ToolStatus::Done, "Stage integration-verify runs it.");
    if let Item::Tool { nested_tools, .. } = &mut spawn {
        *nested_tools = 1;
    }
    spawn
}

fn expected_nested_grep() -> Item {
    let input = json!({"pattern": "dashboard", "path": "/work/proj/doc/plans"});
    let grep = tool(
        "a-s3-0",
        "toolu_01Gr",
        "Grep",
        "dashboard in doc/plans",
        input,
    );
    let mut grep = finished(grep, ToolStatus::Done, "doc/plans/PLAN-x.md");
    if let Item::Tool {
        subagent, parent, ..
    } = &mut grep
    {
        *subagent = Some("a7f3".into());
        *parent = Some("toolu_01Ag".into());
    }
    grep
}

#[test]
fn subagent_tool_rows_nest_under_the_spawning_call_and_add_tokens() {
    let meta: Value = serde_json::from_str(&lines("subagent-agent.meta.json")[0]).unwrap();
    let spawned_by = meta["toolUseId"].as_str();
    let main = lines("subagent-main.jsonl");
    let agent = lines("subagent-agent.jsonl");
    let mut mapper = mapper();
    drive(&mut mapper, &main[..2]);
    let mut marks = Vec::new();
    for line in &agent {
        marks.extend(mapper.ingest_subagent("a7f3", spawned_by, line).marks);
    }
    drive(&mut mapper, &main[2..]);

    let answer = text("a-a3-0", "Integration verify runs the dashboard.");
    let first = user("u-u1", "Find the stage that runs the dashboard.", None);
    let turn = only_turn(&mapper);
    assert_eq!(
        turn.items,
        [first, expected_spawn(), expected_nested_grep(), answer]
    );
    assert_eq!(turn.tokens, tokens(4, 0, 410, 41));
    let agent_id = "a7f3".to_owned();
    assert_eq!(
        marks,
        [RecordMark::SubagentFinal {
            agent_id,
            at: at(6)
        }]
    );

    let crossed = mapper.ingest_main(&agent[2]);
    assert!(
        crossed.deltas.is_empty() && crossed.marks.is_empty(),
        "uuids are shared"
    );
    let again = mapper.ingest_subagent("a7f3", spawned_by, &main[1]);
    assert!(again.deltas.is_empty());
}

fn assistant_tools(uuid: &str, secs: u32, calls: &[(&str, &str)]) -> String {
    let content: Vec<Value> = calls
        .iter()
        .map(|(id, name)| {
            json!({"type": "tool_use", "id": id, "name": name,
                                 "input": {"description": format!("{name} {id}")}})
        })
        .collect();
    let record = json!({"type": "assistant", "uuid": uuid,
        "timestamp": format!("2026-10-27T10:00:{secs:02}.000Z"),
        "message": {"id": format!("msg_{uuid}"), "role": "assistant", "content": content}});
    record.to_string()
}

#[test]
fn subagent_rows_name_their_own_spawning_call_when_one_message_spawns_two() {
    let mut mapper = mapper();
    mapper.begin_turn(at(0));
    let spawn = assistant_tools("a1", 1, &[("toolu_A1", "Agent"), ("toolu_A2", "Agent")]);
    mapper.ingest_main(&spawn);
    let first = assistant_tools("s1", 2, &[("toolu_G1", "Grep")]);
    let second = assistant_tools("s2", 3, &[("toolu_G2", "Glob")]);
    mapper.ingest_subagent("agent-1", Some("toolu_A1"), &first);
    mapper.ingest_subagent("agent-2", Some("toolu_A2"), &second);

    let rows: Vec<_> = only_turn(&mapper)
        .items
        .iter()
        .filter_map(|item| match item {
            Item::Tool {
                tool_use_id,
                subagent,
                parent,
                nested_tools,
                ..
            } => Some((
                tool_use_id.as_str(),
                subagent.as_deref(),
                parent.as_deref(),
                *nested_tools,
            )),
            _ => None,
        })
        .collect();
    assert_eq!(
        rows,
        [
            ("toolu_A1", None, None, 1),
            ("toolu_A2", None, None, 1),
            ("toolu_G1", Some("agent-1"), Some("toolu_A1"), 0),
            ("toolu_G2", Some("agent-2"), Some("toolu_A2"), 0),
        ]
    );
}

#[test]
fn only_paths_under_doc_join_the_changed_paths() {
    let mut mapper = mapper();
    mapper.begin_turn(at(0));
    let paths = [
        "/home/u/.local/state/loom/drafts/x.md",
        "/work/proj/src/lib.rs",
        "/work/proj/doc/../src/lib.rs",
        "/work/proj/doc/product/x.md",
        "doc/plans/PLAN-y.md",
    ];
    for (n, path) in paths.iter().enumerate() {
        let id = format!("toolu_W{n}");
        mapper.ingest_main(&assistant_tools(&format!("a{n}"), 1, &[(&id, "Write")]));
        let result = json!([{"tool_use_id": id, "type": "tool_result", "content": "ok"}]);
        let record = json!({"type": "user", "uuid": format!("u{n}"),
            "timestamp": "2026-10-27T10:00:02.000Z",
            "message": {"role": "user", "content": result},
            "toolUseResult": {"filePath": path}});
        mapper.ingest_main(&record.to_string());
    }

    assert_eq!(
        mapper.changed_by_tool("t1"),
        ["doc/plans/PLAN-y.md", "doc/product/x.md"]
    );
}

#[test]
fn tokens_only_lines_count_the_last_usage_per_message_and_add_no_item() {
    let usage = |n: u64| {
        let usage = json!({"input_tokens": n, "cache_creation_input_tokens": n,
                           "cache_read_input_tokens": n, "output_tokens": n});
        json!({"message": {"id": "msg_T", "usage": usage}}).to_string()
    };
    let mut mapper = mapper();
    assert!(
        mapper.ingest_tokens_only(&usage(1)).is_empty(),
        "no open turn"
    );
    mapper.begin_turn(at(0));
    let first = mapper.ingest_tokens_only(&usage(1));
    assert!(
        matches!(&first[..], [ChatDelta::TurnUpdated { .. }]),
        "{first:?}"
    );
    mapper.ingest_tokens_only(&usage(5));
    let turn = only_turn(&mapper);
    assert_eq!(
        turn.tokens,
        tokens(5, 5, 5, 5),
        "the last record per id wins"
    );
    assert!(turn.items.is_empty());
}

#[test]
fn closing_origin_and_server_item_replacement() {
    let mut mapper = mapper();
    mapper.begin_turn(at(0));
    mapper.expect_closing("Thanks, that is all.".into());
    let closing = json!({"type": "user", "uuid": "u9", "timestamp": "2026-10-27T10:00:07.000Z",
        "message": {"role": "user", "content": "Thanks, that is all."},
        "origin": {"kind": "human"}});
    mapper.ingest_main(&closing.to_string());
    let expected = user("u-u9", "Thanks, that is all.", Some(Origin::Closing));
    assert_eq!(only_turn(&mapper).items, std::slice::from_ref(&expected));

    let card = |status| Item::Permission {
        id: "p-r1".into(),
        request_id: "r1".into(),
        tool: "Bash".into(),
        input: "{}".into(),
        status,
        deny_reason: Some(DenyReason::Holder),
        note: None,
        expires_at: "2026-10-27T10:05:00.000Z".into(),
    };
    mapper.add_server_item(card(PermissionStatus::Pending));
    let replaced = mapper.replace_server_item(card(PermissionStatus::Denied));
    let item = card(PermissionStatus::Denied);
    let turn_id = "t1".to_owned();
    let published = ChatDelta::ItemReplaced {
        turn_id,
        item: item.clone(),
    };
    assert_eq!(replaced, [published]);
    assert_eq!(only_turn(&mapper).items, [expected, item]);
    let unknown = notice("n-x", NoticeKind::Info, "x");
    assert!(mapper.replace_server_item(unknown).is_empty());
}

#[test]
fn a_wrapped_delivery_at_the_size_limit_keeps_no_wrapper() {
    let body = "a".repeat(MAX_DELIVERED_BYTES);
    let content = format!("<pasted_content id=\"f3a0\">\n{body}\n</pasted_content id=\"f3a0\">");
    let record = json!({"type": "user", "uuid": "u1", "timestamp": "2026-10-27T10:00:00.000Z",
        "message": {"role": "user", "content": content}, "origin": {"kind": "human"}});
    let mut mapper = mapper();
    let out = mapper.ingest_main(&record.to_string());
    assert_eq!(only_turn(&mapper).items, [user("u-u1", &body, None)]);
    assert_eq!(out.marks, [human(0, &body)]);
}

#[test]
fn malformed_or_oversized_records_never_panic() {
    let mut mapper = mapper();
    let long_uuid = "u".repeat(200);
    let refused = [
        String::new(),
        "not json".into(),
        r#"{"type":"assistant","uuid":"a1","message":{"content":[{"type":"te"#.into(),
        "[]".into(),
        r#"{"type":"user","message":{"role":"user","content":"no uuid"}}"#.into(),
        json!({"type": "user", "uuid": long_uuid, "message": {"content": "hi"}}).to_string(),
    ];
    for line in &refused {
        let out = mapper.ingest_main(line);
        assert!(out.deltas.is_empty() && out.marks.is_empty(), "{line}");
    }
    assert!(mapper.turns().is_empty());

    let block = json!({"type": "text", "text": "é".repeat(70_000)});
    let message = json!({"content": [block, 7, null]});
    let record = json!({"type": "assistant", "uuid": "a2", "message": message});
    mapper.ingest_main(&record.to_string());
    let Item::Text { markdown, .. } = &only_turn(&mapper).items[0] else {
        panic!("{:?}", mapper.turns());
    };
    assert_eq!(markdown.chars().count(), 64 * 1024);
}
