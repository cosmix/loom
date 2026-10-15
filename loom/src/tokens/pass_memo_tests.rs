use super::{json_hash, FileStamp, PassMemo, FLOW_HEARTBEAT_SECS};
use crate::tokens::accumulate::MESSAGE_TABLE_CAP;
use crate::tokens::checkpoint::Checkpoint;
use crate::tokens::discover::Source;
use crate::tokens::model::{flow_path, read_flow, AgentKind, BUCKET_SECS, FLOW_FILE};
use crate::tokens::tests_fixture::{
    append, body, claude_line, stage, unix_now, usage, Fixture, SESSION_UUID,
};
use crate::tokens::tracker::CHECKPOINT_FILE;
use std::collections::HashSet;
use std::fs;

const OPUS: &str = "claude-opus-5-5";

fn line(msg_id: &str, ts: i64, cache_write: u64) -> String {
    claude_line(msg_id, ts, OPUS, &usage(0, cache_write, 0, 0))
}

/// A fixture whose one transcript line is older than every series, so its
/// flow does not change from pass to pass, and a bucket-aligned `now`.
fn idle_fixture(cache_write: u64) -> (Fixture, i64) {
    let fx = Fixture::new();
    let now = unix_now().div_euclid(BUCKET_SECS) * BUCKET_SECS;
    let text = body(&[line("m1", now - 7200, cache_write)]);
    fx.add_session("alpha", SESSION_UUID, &text);
    (fx, now)
}

fn stamps(fx: &Fixture) -> [Option<FileStamp>; 2] {
    let dir = fx.tokens_dir();
    [CHECKPOINT_FILE, FLOW_FILE].map(|name| FileStamp::of(&dir.join(name)))
}

fn written_at(fx: &Fixture) -> i64 {
    read_flow(&fx.work_root).expect("flow.json").observed_at
}

#[test]
fn unchanged_pass_writes_neither_file() {
    let (fx, now) = idle_fixture(100);
    let first = fx.scan(now);
    let before = stamps(&fx);

    let second = fx.scan(now + 5);

    assert!(before.iter().all(Option::is_some), "{before:?}");
    assert_eq!(
        stamps(&fx),
        before,
        "checkpoint.json and flow.json rewritten"
    );
    assert_eq!(second.plan.total, first.plan.total);
    assert_eq!(
        second.observed_at,
        now + 5,
        "the pass returns the flow it built"
    );
}

#[test]
fn checkpoint_replaced_between_passes_is_loaded_from_disk() {
    let (fx, now) = idle_fixture(100);
    fx.scan(now);
    let path = fx.tokens_dir().join(CHECKPOINT_FILE);
    let mut planted = Checkpoint::load(&fx.tokens_dir());
    for agent in planted
        .stages
        .values_mut()
        .flat_map(|agents| agents.values_mut())
    {
        agent.total.cache_write = 99_999;
    }
    let json = serde_json::to_string(&planted).expect("serialize");
    fs::write(&path, json).expect("rewrite in place");

    let flow = fx.scan(now + 5);

    assert_eq!(stage(&flow, "alpha").total.cache_write, 99_999);
}

#[test]
fn idle_flow_is_rewritten_once_a_heartbeat() {
    let (fx, now) = idle_fixture(100);
    let heartbeat = FLOW_HEARTBEAT_SECS;
    let seen: Vec<i64> = [now, now + 5, now + heartbeat - 1, now + heartbeat]
        .into_iter()
        .map(|at| {
            fx.scan(at);
            written_at(&fx)
        })
        .collect();

    assert_eq!(seen, [now, now, now, now + heartbeat]);
}

#[test]
fn changed_flow_is_written_within_the_heartbeat() {
    let (fx, now) = idle_fixture(100);
    fx.scan(now);

    append(
        &fx.transcript(SESSION_UUID),
        &body(&[line("m2", now - 7100, 7)]),
    );
    fx.scan(now + 5);

    let flow = read_flow(&fx.work_root).expect("flow.json");
    assert_eq!(flow.observed_at, now + 5);
    assert_eq!(stage(&flow, "alpha").total.cache_write, 107);
}

#[test]
fn deleted_flow_is_written_on_the_next_pass() {
    let (fx, now) = idle_fixture(100);
    fx.scan(now);

    fs::remove_file(flow_path(&fx.work_root)).expect("delete flow.json");
    fx.scan(now + 5);

    assert_eq!(written_at(&fx), now + 5);
}

#[test]
fn the_checkpoint_hash_is_stable_within_the_process_and_follows_the_content() {
    let mut checkpoint = Checkpoint::empty();
    let first = json_hash(&checkpoint).expect("hash");

    assert_eq!(json_hash(&checkpoint).expect("hash"), first);
    assert_eq!(json_hash(&checkpoint.clone()).expect("hash"), first);
    checkpoint.version += 1;
    assert_ne!(json_hash(&checkpoint).expect("hash"), first);
}

/// A checkpoint of two agents with full tables of 1200-character message
/// ids: about 10 MB of JSON, over the cap even after shedding idle tables.
fn over_cap_checkpoint(now: i64) -> (Checkpoint, Vec<Source>) {
    let mut checkpoint = Checkpoint::empty();
    let sources: Vec<Source> = ["a1", "a2"]
        .map(|key| Source {
            stage_id: "alpha".into(),
            key: key.into(),
            kind: AgentKind::Subagent,
            path: format!("/t/{key}.jsonl"),
            agent_type: None,
        })
        .into();
    for source in &sources {
        let agent = checkpoint.agent(source);
        for n in 0..MESSAGE_TABLE_CAP {
            agent.ingest(&line(&format!("{}{n:0>1200}", source.key), now - 60, 1));
        }
    }
    (checkpoint, sources)
}

#[test]
fn over_cap_save_forgets_the_checkpoint() {
    let fx = Fixture::new();
    let dir = fx.tokens_dir();
    let now = unix_now();
    let mut memo = PassMemo::new(&fx.work_root);
    let (first, _) = memo.take_checkpoint(&dir);
    let saved = memo.save_checkpoint(first, &dir, &HashSet::new());
    assert_eq!(saved.expect("first save"), None);
    let on_disk = FileStamp::of(&dir.join(CHECKPOINT_FILE));

    let (from_memo, _) = memo.take_checkpoint(&dir);
    assert_eq!(
        from_memo,
        Checkpoint::empty(),
        "the memo's saved checkpoint"
    );
    let (bulky, sources) = over_cap_checkpoint(now);
    let advanced: HashSet<(&str, &str)> = sources
        .iter()
        .map(|source| (source.stage_id.as_str(), source.key.as_str()))
        .collect();
    let warning = memo.save_checkpoint(bulky, &dir, &advanced).expect("save");

    assert!(warning.is_some_and(|warning| warning.contains("not written")));
    assert_eq!(FileStamp::of(&dir.join(CHECKPOINT_FILE)), on_disk);
    let (next, note) = memo.take_checkpoint(&dir);
    assert_eq!(
        next,
        Checkpoint::empty(),
        "the file's checkpoint, not the memo's"
    );
    assert_eq!(note, None);
}

#[test]
fn work_roots_on_one_thread_never_share_state() {
    let (a, now) = idle_fixture(100);
    let b = Fixture::new();
    b.add_session("alpha", SESSION_UUID, &body(&[line("m1", now - 7200, 7)]));
    let twin = Fixture::new();
    twin.add_session("alpha", SESSION_UUID, &body(&[line("m1", now - 7200, 100)]));

    let totals: Vec<u64> = [(&a, now), (&b, now), (&a, now + 5), (&twin, now + 5)]
        .into_iter()
        .map(|(fx, at)| stage(&fx.scan(at), "alpha").total.cache_write)
        .collect();

    assert_eq!(totals, [100, 7, 100, 100]);
    for (fx, cache_write) in [(&a, 100), (&b, 7), (&twin, 100)] {
        let flow = read_flow(&fx.work_root).expect("flow.json");
        assert_eq!(stage(&flow, "alpha").total.cache_write, cache_write);
    }
}
