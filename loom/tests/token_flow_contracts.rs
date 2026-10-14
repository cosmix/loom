//! Contracts for the token tracker, `StatusData.tokens` and quota time left.

#[path = "token_flow_contracts/support.rs"]
mod support;

use loom::tokens::quota_left::{estimate, QuotaEstimate};
use loom::tokens::{
    flow_path, AgentKind, QuotaLeft, QuotaProvider, FLOW_STALE_SECS, STAGE_SERIES_LEN,
};
use serde_json::json;
use std::fs;
use support::*;

#[test]
fn second_pass_counts_nothing_new() {
    let fx = Fx::new();
    let now = unix_now();
    let lines = [
        claude_line("msg_1", now - 90, &usage(3, 100, 50, 7)),
        claude_line("msg_2", now - 60, &usage(4, 200, 60, 9)),
    ];
    fx.add_stage("alpha", &uuid(1), &body(&lines));

    let first = stage(&fx.scan(now), "alpha").total;
    let second = stage(&fx.scan(now), "alpha").total;

    assert_eq!(first, usage(7, 300, 110, 16));
    assert_eq!(second, first, "a pass with no file change added tokens");
}

#[test]
fn repeated_message_id_counts_once() {
    let fx = Fx::new();
    let now = unix_now();
    let same = usage(2, 14477, 4542, 20);
    let lines = [
        claude_line("msg_A", now - 90, &same),
        claude_line("msg_A", now - 90, &same),
        claude_line("msg_A", now - 89, &same),
        claude_line("msg_A", now - 88, &usage(2, 14477, 4542, 50)),
    ];
    fx.add_stage("alpha", &uuid(1), &body(&lines));

    let flow = fx.scan(now);

    assert_eq!(
        main_agent(stage(&flow, "alpha")).total,
        usage(2, 14477, 4542, 50)
    );
}

#[test]
fn torn_last_line_counted_once_after_newline() {
    let fx = Fx::new();
    let now = unix_now();
    let first = claude_line("msg_1", now - 90, &usage(1, 10, 100, 1));
    let second = claude_line("msg_2", now - 60, &usage(2, 20, 200, 2));
    let (head, tail) = second.split_at(second.len() / 2);
    fx.add_stage("alpha", &uuid(1), &format!("{first}\n{head}"));

    let flow = fx.scan(now);
    assert_eq!(stage(&flow, "alpha").total, usage(1, 10, 100, 1));
    assert_eq!(flow.unreadable_lines, 0, "the torn half was parsed");

    append(&fx.transcript(&uuid(1)), &format!("{tail}\n"));
    let flow = fx.scan(now);

    assert_eq!(stage(&flow, "alpha").total, usage(3, 30, 300, 3));
    assert_eq!(flow.unreadable_lines, 0);
}

#[test]
fn rewritten_transcript_is_rescanned() {
    let fx = Fx::new();
    let now = unix_now();
    let lines = [
        claude_line("msg_1", now - 90, &usage(10, 1000, 100, 10)),
        claude_line("msg_2", now - 80, &usage(20, 2000, 200, 20)),
        claude_line("msg_3", now - 70, &usage(30, 3000, 300, 30)),
    ];
    fx.add_stage("alpha", &uuid(1), &body(&lines));
    fx.scan(now);

    let rewritten = claude_line("msg_9", now - 30, &usage(5, 50, 5, 5));
    fs::write(fx.transcript(&uuid(1)), format!("{rewritten}\n")).expect("rewrite");
    let flow = fx.scan(now);

    assert_eq!(stage(&flow, "alpha").total, usage(5, 50, 5, 5));
    assert_eq!(main_agent(stage(&flow, "alpha")).total, usage(5, 50, 5, 5));
}

#[test]
fn backlog_lands_in_its_own_buckets() {
    let fx = Fx::new();
    let now = unix_now();
    let line = claude_line("msg_1", now - 600, &usage(0, 1000, 0, 0));
    fx.add_stage("alpha", &uuid(1), &body(&[line]));

    let flow = fx.scan(now);
    let series = &stage(&flow, "alpha").series;

    assert_eq!(series.len(), STAGE_SERIES_LEN);
    assert_eq!(series[series.len() - 21], 1000);
    assert_eq!(series[series.len() - 1], 0);
}

#[test]
fn codex_job_totals_are_deltas_attributed_to_stage() {
    let fx = Fx::new();
    let now = unix_now();
    let loom_session = fx.add_stage("alpha", &uuid(1), "");
    fx.add_forward_receipt("alpha", &uuid(1), &loom_session, THREAD_ID);
    fx.add_codex_rollout(THREAD_ID, now - 120);

    let flow = fx.scan(now);
    let codex = agents_of(stage(&flow, "alpha"), AgentKind::Codex);

    assert_eq!(codex.len(), 1, "expected one codex agent in stage alpha");
    assert_eq!(codex[0].total, usage(9994, 0, 33280, 206));
    assert!(
        codex[0].id.starts_with("codex-01a1396b"),
        "id {}",
        codex[0].id
    );
}

#[test]
fn unreadable_usage_line_is_counted() {
    let fx = Fx::new();
    let now = unix_now();
    let valid = claude_line("msg_1", now - 60, &usage(1, 2, 3, 4));
    let bad = claude_line("msg_2", now - 50, &usage(1, 2, 3, 4))
        .replace("\"output_tokens\":4", "\"output_tokens\":\"many\"");
    assert!(bad.contains("\"many\""), "fixture did not corrupt the line");
    let user = json!({"type": "user", "timestamp": iso(now - 40),
        "message": {"role": "user", "content": "how is usage counted?"}})
    .to_string();
    fx.add_stage("alpha", &uuid(1), &body(&[valid, bad, user]));

    let flow = fx.scan(now);

    assert_eq!(flow.unreadable_lines, 1);
}

#[test]
fn symlinked_tokens_dir_is_refused() {
    let fx = Fx::new();
    let now = unix_now();
    let outside = fx.path("outside");
    fs::create_dir_all(&outside).expect("outside dir");
    std::os::unix::fs::symlink(&outside, fx.work_root.join("tokens")).expect("symlink");
    let line = claude_line("msg_1", now - 60, &usage(1, 2, 3, 4));
    fx.add_stage("alpha", &uuid(1), &body(&[line]));

    let result = fx.try_scan(now);

    assert!(result.is_err(), "scan_pass followed a symlinked tokens dir");
    let written: Vec<_> = fs::read_dir(&outside).expect("read outside").collect();
    assert!(
        written.is_empty(),
        "files written through the symlink: {written:?}"
    );
}

#[test]
fn agent_labels_are_sanitized() {
    let fx = Fx::new();
    let now = unix_now();
    let agent = "a04baa183f888b773";
    let loom_session = fx.add_stage("alpha", &uuid(1), "");
    let spent = usage(2, 14477, 4542, 20);
    let line = subagent_line(
        agent,
        "msg_S1",
        now - 60,
        "claude\u{1b}[31m-x\u{202e}",
        &spent,
    );
    fx.add_subagent(&uuid(1), agent, &body(&[line]));
    fx.add_start_row(
        "alpha",
        agent,
        "evil\u{1b}[2J\nline",
        &uuid(1),
        &loom_session,
    );

    let flow = fx.scan(now);
    let subagents = agents_of(stage(&flow, "alpha"), AgentKind::Subagent);

    assert_eq!(subagents.len(), 1, "expected one subagent agent");
    assert_eq!(subagents[0].total, spent);
    let agent_type = subagents[0]
        .agent_type
        .as_deref()
        .expect("agent_type resolved");
    let model = subagents[0].model.as_deref().expect("model recorded");
    assert_clean("agent_type", agent_type);
    assert_clean("model", model);
}

#[test]
fn status_data_carries_fresh_flow() {
    let fx = Fx::new();
    let now = unix_now();
    let line = claude_line("msg_1", now - 60, &usage(1, 2, 3, 4));
    fx.add_stage("alpha", &uuid(1), &body(&[line]));
    fx.scan(now);

    let tokens = fx.status_tokens().expect("fresh flow published");

    assert_eq!(stage(&tokens, "alpha").total, usage(1, 2, 3, 4));
}

#[test]
fn status_data_drops_stale_flow() {
    let fx = Fx::new();
    let then = unix_now() - FLOW_STALE_SECS - 60;
    let line = claude_line("msg_1", then - 60, &usage(1, 2, 3, 4));
    fx.add_stage("alpha", &uuid(1), &body(&[line]));
    fx.scan(then);
    assert!(
        flow_path(&fx.work_root).is_file(),
        "the tracker wrote no flow"
    );

    assert_eq!(fx.status_tokens(), None, "a stale flow was published");
}

#[test]
fn collector_flattens_flow_strings() {
    let fx = Fx::new();
    let evil = |base: &str| format!("{base}\u{1b}[31m\u{202e}\nx");
    let zero = json!({"input": 0, "cache_write": 0, "cache_read": 0, "output": 0});
    let agent = json!({"id": evil("main"), "kind": "main", "agent_type": evil("type"),
        "model": evil("model"), "active": true, "total": zero, "rate": zero, "series": [0]});
    let flow = json!({"observed_at": unix_now(), "bucket_secs": 30,
        "plan": {"total": zero, "rate": zero, "series": [0]},
        "stages": [{"stage_id": evil("alpha"), "total": zero, "rate": zero, "series": [0],
            "agents": [agent]}],
        "quota_left": [], "unreadable_lines": 0});
    fs::write(fx.tokens_dir().join("flow.json"), flow.to_string()).expect("flow.json");

    let tokens = fx.status_tokens().expect("flow published");

    assert_eq!(tokens.stages.len(), 1);
    let stage = &tokens.stages[0];
    assert_clean("stage_id", &stage.stage_id);
    assert_eq!(stage.agents.len(), 1);
    assert_clean("id", &stage.agents[0].id);
    assert_clean(
        "agent_type",
        stage.agents[0].agent_type.as_deref().expect("agent_type"),
    );
    assert_clean("model", stage.agents[0].model.as_deref().expect("model"));
}

#[test]
fn corrupt_checkpoint_rescans() {
    let fx = Fx::new();
    let now = unix_now();
    let lines = [
        claude_line("msg_1", now - 90, &usage(0, 1000, 0, 0)),
        claude_line("msg_2", now - 60, &usage(0, 1000, 0, 0)),
    ];
    fx.add_stage("alpha", &uuid(1), &body(&lines));
    fs::write(fx.tokens_dir().join("checkpoint.json"), "{").expect("checkpoint");

    let flow = fx
        .try_scan(now)
        .expect("a corrupt checkpoint must mean a full rescan");

    assert_eq!(stage(&flow, "alpha").total, usage(0, 2000, 0, 0));
}

#[test]
fn checkpoint_resumes_from_offset() {
    let fx = Fx::new();
    let now = unix_now();
    let a = claude_line("msg_A1", now - 90, &usage(0, 0, 0, 10));
    fx.add_stage("alpha", &uuid(1), &body(std::slice::from_ref(&a)));
    fx.scan(now);

    let a2 = claude_line("msg_A2", now - 90, &usage(0, 0, 0, 99));
    assert_eq!(a2.len(), a.len(), "A2 must keep A's byte length");
    let b = claude_line("msg_B1", now - 60, &usage(0, 0, 0, 5));
    overwrite_in_place(&fx.transcript(&uuid(1)), &body(&[a2, b]));
    let flow = fx.scan(now);

    assert_eq!(main_agent(stage(&flow, "alpha")).total.output, 15);
}

#[test]
fn rate_is_five_minute_average() {
    let fx = Fx::new();
    let now = unix_now();
    let lines = [
        claude_line("msg_1", now - 420, &usage(0, 6000, 0, 0)),
        claude_line("msg_2", now - 60, &usage(0, 3000, 0, 0)),
    ];
    fx.add_stage("alpha", &uuid(1), &body(&lines));

    let flow = fx.scan(now);
    let alpha = stage(&flow, "alpha");
    let main = main_agent(alpha);

    assert_eq!(alpha.rate, usage(0, 600, 0, 0));
    assert_eq!(main.rate, usage(0, 600, 0, 0));
    assert_eq!(alpha.total.cache_write, 9000);
    assert!(main.active, "an agent with usage a minute ago is active");
}

#[test]
fn quota_left_estimates_from_slope() {
    let now = 1_792_000_000;
    let points = [
        (now - 3000, 10.0),
        (now - 1530, 52.0),
        (now - 630, 57.0),
        (now - 30, 62.0),
    ];

    let QuotaEstimate {
        minutes_left,
        lasts_to_reset,
    } = estimate(&points, Some(now + 133 * 60), now);

    assert_eq!(minutes_left, Some(94));
    assert!(!lasts_to_reset);
}

#[test]
fn quota_left_flat_usage_lasts_to_reset() {
    let now = 1_792_000_000;

    let QuotaEstimate {
        minutes_left,
        lasts_to_reset,
    } = estimate(&[(now - 1200, 40.0), (now, 40.0)], Some(now + 3600), now);

    assert_eq!(minutes_left, None);
    assert!(lasts_to_reset);
}

#[test]
fn quota_left_survives_reset_jitter() {
    let fx = Fx::new();
    let now = unix_now();
    let reset = now + 133 * 60;
    let week = Some((20.0, now + 400_000));
    fx.write_claude_history(&[
        history_row(now - 1530, (52.0, reset - 1), week),
        history_row(now - 630, (57.0, reset), week),
        history_row(now - 30, (62.0, reset - 1), week),
    ]);

    let flow = fx.scan(now);

    let expected = QuotaLeft {
        provider: QuotaProvider::Claude,
        minutes_left: Some(94),
        lasts_to_reset: false,
    };
    assert_eq!(flow.quota_left, vec![expected]);
}
