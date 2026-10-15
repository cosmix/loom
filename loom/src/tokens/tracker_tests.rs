use super::{
    log_change, read_sources, scan_until, spawn_token_tracker, write_flow, Pass, CHECKPOINT_FILE,
};
use crate::tokens::accumulate::{AgentState, MESSAGE_TABLE_CAP};
use crate::tokens::checkpoint::{Checkpoint, MAX_CHECKPOINT_BYTES};
use crate::tokens::discover::Source;
use crate::tokens::flow_build::build_flow;
use crate::tokens::model::{flow_path, read_flow, AgentKind, TokenUsage, BUCKET_SECS, FLOW_FILE};
use crate::tokens::tail::{self, MAX_LINE_BYTES};
use crate::tokens::tests_fixture::{
    agents_of, append, body, claude_line, stage, unix_now, usage, Fixture, SESSION_UUID,
};
use std::collections::BTreeMap;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

const OPUS: &str = "claude-opus-5-5";

fn line(msg_id: &str, ts: i64, cache_write: u64) -> String {
    claude_line(msg_id, ts, OPUS, &usage(0, cache_write, 0, 0))
}

fn alpha_total(fx: &Fixture, now: i64) -> TokenUsage {
    stage(&fx.scan(now), "alpha").total
}

#[test]
fn truncated_or_replaced_transcript_counts_its_new_content() {
    let fx = Fixture::new();
    let now = unix_now();
    let first = [line("m1", now - 90, 100), line("m1b", now - 80, 100)];
    let transcript = fx.add_session("alpha", SESSION_UUID, &body(&first));
    fx.scan(now);

    fs::write(&transcript, "").expect("truncate");
    append(&transcript, &body(&[line("m2", now - 60, 7)]));
    assert_eq!(alpha_total(&fx, now).cache_write, 7);

    let replacement = fx.tmp.path().join("replacement.jsonl");
    let longer = [line("m3", now - 30, 5), line("m4", now - 20, 6)];
    fs::write(&replacement, body(&longer)).expect("replacement");
    fs::rename(&replacement, &transcript).expect("replace");
    assert_eq!(
        alpha_total(&fx, now).cache_write,
        11,
        "a new inode restarts the file"
    );
}

#[test]
fn late_subagent_file_is_picked_up_and_unchanged_files_add_nothing() {
    let fx = Fixture::new();
    let now = unix_now();
    fx.add_session("alpha", SESSION_UUID, &body(&[line("m1", now - 90, 100)]));
    assert_eq!(alpha_total(&fx, now).cache_write, 100);

    let sub = claude_line("s1", now - 60, "claude-sonnet-5-5", &usage(0, 40, 0, 0));
    fx.add_subagent(SESSION_UUID, "agent-a04baa183f888b773.jsonl", &body(&[sub]));
    let flow = fx.scan(now);

    assert_eq!(stage(&flow, "alpha").total.cache_write, 140);
    assert_eq!(
        agents_of(stage(&flow, "alpha"), AgentKind::Subagent).len(),
        1
    );
    assert_eq!(alpha_total(&fx, now).cache_write, 140);
}

#[test]
fn deleted_transcript_keeps_its_agent_totals() {
    let fx = Fixture::new();
    let now = unix_now();
    let transcript = fx.add_session("alpha", SESSION_UUID, &body(&[line("m1", now - 90, 100)]));
    fx.scan(now);

    fs::remove_file(&transcript).expect("delete");

    assert_eq!(alpha_total(&fx, now).cache_write, 100);
}

#[test]
fn corrupt_checkpoint_rescans_without_double_count() {
    let fx = Fixture::new();
    let now = unix_now();
    fx.add_session("alpha", SESSION_UUID, &body(&[line("m1", now - 90, 100)]));
    fx.scan(now);

    fs::write(fx.tokens_dir().join(CHECKPOINT_FILE), "{\"version\":").expect("corrupt");

    assert_eq!(alpha_total(&fx, now).cache_write, 100);
}

/// A checkpoint claiming the transcript was read in full for 999 tokens.
fn planted_checkpoint(transcript: &Path) -> Checkpoint {
    let stat = tail::stat(transcript).expect("stat");
    let mut checkpoint = Checkpoint::empty();
    let source = Source {
        stage_id: "alpha".into(),
        key: format!("main-{SESSION_UUID}"),
        kind: AgentKind::Main,
        path: transcript.to_str().expect("utf-8").into(),
        agent_type: None,
    };
    let agent = checkpoint.agent(&source);
    agent.total = usage(0, 999, 0, 0);
    agent.file.inode = stat.inode;
    agent.file.offset = stat.len;
    checkpoint
}

#[test]
fn symlinked_or_oversized_checkpoint_means_a_full_rescan() {
    let fx = Fixture::new();
    let now = unix_now();
    let transcript = fx.add_session("alpha", SESSION_UUID, &body(&[line("m1", now - 90, 100)]));
    let planted = planted_checkpoint(&transcript);
    let outside = fx.tmp.path().join("outside");
    fs::create_dir_all(&outside).expect("outside");
    planted.save(&outside).expect("save outside");
    let checkpoint = fx.tokens_dir().join(CHECKPOINT_FILE);
    std::os::unix::fs::symlink(outside.join(CHECKPOINT_FILE), &checkpoint).expect("symlink");
    assert_eq!(
        alpha_total(&fx, now).cache_write,
        100,
        "read through a symlink"
    );
    assert_eq!(
        Checkpoint::load(&outside),
        planted,
        "wrote through a symlink"
    );

    let mut json = serde_json::to_string(&planted).expect("serialize");
    json.push_str(&" ".repeat(MAX_CHECKPOINT_BYTES));
    fs::write(&checkpoint, json).expect("oversized");
    assert_eq!(
        alpha_total(&fx, now).cache_write,
        100,
        "read an oversized checkpoint"
    );
}

#[test]
fn fifo_checkpoint_is_replaced_without_blocking() {
    let fx = Fixture::new();
    let now = unix_now();
    fx.add_session("alpha", SESSION_UUID, &body(&[line("m1", now - 90, 100)]));
    let fifo = fx.tokens_dir().join(CHECKPOINT_FILE);
    nix::unistd::mkfifo(&fifo, nix::sys::stat::Mode::S_IRWXU).expect("mkfifo");

    assert_eq!(alpha_total(&fx, now).cache_write, 100);
    assert_eq!(alpha_total(&fx, now).cache_write, 100);
}

#[test]
fn directory_checkpoint_fails_the_pass_but_publishes_a_full_rescan() {
    let fx = Fixture::new();
    let now = unix_now();
    fx.add_session("alpha", SESSION_UUID, &body(&[line("m1", now - 90, 100)]));
    fs::create_dir(fx.tokens_dir().join(CHECKPOINT_FILE)).expect("directory");

    for _ in 0..2 {
        assert!(
            fx.try_scan(now).is_err(),
            "an unwritable checkpoint must be reported"
        );
        let flow = read_flow(&fx.work_root).expect("flow.json still written");
        assert_eq!(stage(&flow, "alpha").total.cache_write, 100);
    }
}

#[test]
fn flow_over_the_cap_is_not_written() {
    let fx = Fixture::new();
    let dir = fx.tokens_dir();
    let now = unix_now();
    let agents = (0..2000)
        .map(|n| {
            let mut agent = AgentState::new(AgentKind::Subagent, "/t/agent.jsonl");
            agent.ingest(&line(&format!("m{n}"), now, 100));
            (format!("a{n:016x}"), agent)
        })
        .collect();
    let mut flow = build_flow(&BTreeMap::from([("alpha".to_owned(), agents)]), now);

    assert!(!write_flow(&dir, &mut flow).expect("write_flow"));
    assert!(!dir.join(FLOW_FILE).exists());
}

#[test]
fn two_stages_sum_into_the_plan_and_decay() {
    let fx = Fixture::new();
    // The last second of a bucket: the rate's span is the full 300 s there.
    let now = unix_now().div_euclid(BUCKET_SECS) * BUCKET_SECS - 1;
    fx.add_session("alpha", SESSION_UUID, &body(&[line("a", now - 60, 3000)]));
    let beta = body(&[line("b", now - 60, 1500)]);
    fx.add_session("beta", "02ed9f4c-608e-4e2c-8edf-05d26adcd4d6", &beta);
    assert_eq!(fx.scan(now).plan.rate, usage(0, 900, 0, 0));

    let later = fx.scan(now + 600);

    assert_eq!(later.plan.rate, TokenUsage::default());
    assert_eq!(later.plan.total.cache_write, 4500);
    for id in ["alpha", "beta"] {
        assert!(stage(&later, id).agents.iter().all(|agent| !agent.active));
    }
}

#[test]
fn stopped_pass_reads_nothing_yet_still_publishes() {
    let fx = Fixture::new();
    let now = unix_now();
    fx.add_session("alpha", SESSION_UUID, &body(&[line("m1", now - 90, 100)]));
    let codex = fx.codex_sessions();

    let stopped = scan_until(&fx.work_root, Some(&codex), now, &AtomicBool::new(true));

    assert!(stopped.expect("pass").flow.stages.is_empty());
    assert!(flow_path(&fx.work_root).is_file());
    assert_eq!(alpha_total(&fx, now).cache_write, 100);
}

#[test]
fn repeated_problem_is_logged_once_until_it_changes() {
    let mut last = None;
    log_change(&mut last, Some("boom".to_string()));
    log_change(&mut last, Some("boom".to_string()));
    assert_eq!(last.as_deref(), Some("boom"));

    log_change(&mut last, None);

    assert_eq!(last, None, "a clean pass forgets the last problem");
}

#[test]
fn tracker_thread_exits_within_a_second_of_shutdown() {
    let fx = Fixture::new();
    let shutdown = Arc::new(AtomicBool::new(false));
    let handle = spawn_token_tracker(fx.work_root.clone(), Arc::clone(&shutdown));
    let deadline = Instant::now() + Duration::from_secs(10);
    while !flow_path(&fx.work_root).is_file() && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(20));
    }
    assert!(
        flow_path(&fx.work_root).is_file(),
        "the first pass runs at once"
    );

    let stopped_at = Instant::now();
    shutdown.store(true, Ordering::SeqCst);
    handle.join().expect("tracker thread");

    assert!(stopped_at.elapsed() < Duration::from_secs(1));
}

/// A transcript of [`MESSAGE_TABLE_CAP`] lines with distinct 1200-character
/// message ids: its agent's checkpointed table is about 5 MB of JSON, so one
/// fits [`MAX_CHECKPOINT_BYTES`] and two do not.
fn bulky_body(tag: &str, now: i64) -> String {
    let lines: Vec<String> = (0..MESSAGE_TABLE_CAP)
        .map(|n| line(&format!("{tag}{n:0>1200}"), now - 60, 1))
        .collect();
    body(&lines)
}

fn pass_at(fx: &Fixture, now: i64) -> Pass {
    let codex = fx.codex_sessions();
    scan_until(&fx.work_root, Some(&codex), now, &AtomicBool::new(false)).expect("pass")
}

fn saved_table_len(fx: &Fixture, agent_key: &str) -> usize {
    let checkpoint = Checkpoint::load(&fx.tokens_dir());
    let agent = &checkpoint.stages["alpha"][agent_key];
    let table = serde_json::to_value(&agent.file.messages).expect("serialize");
    table.as_array().map_or(0, Vec::len)
}

#[test]
fn over_cap_checkpoint_empties_the_tables_of_files_that_did_not_advance() {
    let fx = Fixture::new();
    let now = unix_now();
    fx.add_session("alpha", SESSION_UUID, &bulky_body("main", now));
    let main_key = format!("main-{SESSION_UUID}");
    assert_eq!(pass_at(&fx, now).warning, None);
    assert_eq!(saved_table_len(&fx, &main_key), MESSAGE_TABLE_CAP);

    fx.add_subagent(SESSION_UUID, "agent-a1.jsonl", &bulky_body("sub", now));
    let second = pass_at(&fx, now);

    assert_eq!(second.warning, None, "shedding idle tables made it fit");
    assert_eq!(saved_table_len(&fx, &main_key), 0, "the idle file's table");
    assert_eq!(saved_table_len(&fx, "a1"), MESSAGE_TABLE_CAP);
    assert_eq!(
        stage(&second.flow, "alpha").total.cache_write,
        2 * MESSAGE_TABLE_CAP as u64
    );
}

#[test]
fn checkpoint_over_the_cap_with_nothing_to_shed_is_not_written_and_warned_about() {
    let fx = Fixture::new();
    let now = unix_now();
    fx.add_session("alpha", SESSION_UUID, &bulky_body("main", now));
    fx.add_subagent(SESSION_UUID, "agent-a1.jsonl", &bulky_body("sub", now));

    let pass = pass_at(&fx, now);

    let warning = pass
        .warning
        .expect("the skipped save is the pass's warning");
    assert!(warning.contains("not written"), "{warning}");
    assert!(!fx.tokens_dir().join(CHECKPOINT_FILE).exists());
    assert!(flow_path(&fx.work_root).is_file());
    assert_eq!(
        stage(&pass.flow, "alpha").total.cache_write,
        2 * MESSAGE_TABLE_CAP as u64
    );
}

#[test]
fn both_failed_writes_are_reported_together() {
    let fx = Fixture::new();
    let now = unix_now();
    fx.add_session("alpha", SESSION_UUID, &body(&[line("m1", now - 90, 100)]));
    fs::create_dir(fx.tokens_dir().join(CHECKPOINT_FILE)).expect("checkpoint directory");
    fs::create_dir(fx.tokens_dir().join(FLOW_FILE)).expect("flow directory");

    let error = fx.try_scan(now).expect_err("both writes fail");

    let message = format!("{error:#}");
    assert!(message.contains("token checkpoint"), "{message}");
    assert!(message.contains("token flow"), "{message}");
}

#[test]
fn overlong_transcript_line_is_counted_as_unreadable_and_the_rest_is_read() {
    let fx = Fixture::new();
    let now = unix_now();
    let mut text = body(&[line("m1", now - 90, 100)]);
    text.push_str(&"x".repeat(MAX_LINE_BYTES + 10));
    text.push('\n');
    text.push_str(&body(&[line("m2", now - 80, 5)]));
    fx.add_session("alpha", SESSION_UUID, &text);

    let flow = fx.scan(now);

    assert_eq!(stage(&flow, "alpha").total.cache_write, 105);
    assert_eq!(flow.unreadable_lines, 1);
}

#[test]
fn unreadable_file_warning_flattens_the_untrusted_path() {
    // SAFETY: geteuid has no arguments and only reads process credentials.
    if unsafe { libc::geteuid() } == 0 {
        return; // root reads a mode 0 file, so there is no failure to report
    }
    let fx = Fixture::new();
    let path = fx.tmp.path().join("claude").join("a\u{1b}[31m\nb.jsonl");
    fs::write(&path, "one\n").expect("write");
    fs::set_permissions(&path, fs::Permissions::from_mode(0o000)).expect("chmod");
    let source = Source {
        stage_id: "alpha".into(),
        key: "main-x".into(),
        kind: AgentKind::Main,
        path: path.to_str().expect("utf-8").into(),
        agent_type: None,
    };

    let (advanced, failure) = read_sources(
        &mut Checkpoint::empty(),
        std::slice::from_ref(&source),
        &AtomicBool::new(false),
    );

    let failure = failure.expect("the open is refused");
    assert!(!failure.contains(['\u{1b}', '\n']), "{failure:?}");
    assert!(advanced.contains(&("alpha", "main-x")));
}
