use super::{Checkpoint, CHECKPOINT_VERSION, MAX_CHECKPOINT_BYTES};
use crate::tokens::discover::{Source, ThreadLookup};
use crate::tokens::model::{AgentKind, BUCKET_SECS};
use crate::tokens::tests_fixture::{claude_line, codex_count, usage, Fixture, THREAD_ID};
use crate::tokens::tracker::CHECKPOINT_FILE;
use std::fs;

const NOW: i64 = 1_788_523_200;

fn source(kind: AgentKind, key: &str) -> Source {
    Source {
        stage_id: "alpha".into(),
        key: key.into(),
        kind,
        path: format!("/t/{key}.jsonl"),
        agent_type: None,
    }
}

/// A checkpoint exercising every stored field.
fn populated() -> Checkpoint {
    let mut checkpoint = Checkpoint::empty();
    let main = checkpoint.agent(&source(AgentKind::Main, "main-01ed9f4c"));
    main.ingest(&claude_line(
        "msg_1",
        NOW,
        "claude-opus-5-5",
        &usage(1, 2, 3, 4),
    ));
    main.file.offset = 100;
    main.file.inode = 42;
    main.unreadable = 3;
    let codex = checkpoint.agent(&source(AgentKind::Codex, "codex-01a1396b"));
    codex.ingest(&codex_count(NOW, [21255, 12160, 110]));
    let lookup = ThreadLookup {
        rollout: Some("/r/rollout.jsonl".into()),
        checked_at: NOW,
    };
    checkpoint.threads.insert(THREAD_ID.into(), lookup);
    checkpoint
}

#[test]
fn saved_checkpoint_loads_back() {
    let fx = Fixture::new();
    let dir = fx.tokens_dir();
    let checkpoint = populated();

    checkpoint.save(&dir).expect("save");

    assert_eq!(Checkpoint::load(&dir), checkpoint);
}

#[test]
fn other_version_loads_empty() {
    let fx = Fixture::new();
    let dir = fx.tokens_dir();
    let mut checkpoint = populated();
    checkpoint.version = CHECKPOINT_VERSION + 1;
    checkpoint.save(&dir).expect("save");

    assert_eq!(Checkpoint::load(&dir), Checkpoint::empty());
}

#[test]
fn symlinked_checkpoint_is_not_read_through() {
    let fx = Fixture::new();
    let dir = fx.tokens_dir();
    let outside = fx.tmp.path().join("outside");
    fs::create_dir_all(&outside).expect("outside");
    populated().save(&outside).expect("save outside");
    std::os::unix::fs::symlink(outside.join(CHECKPOINT_FILE), dir.join(CHECKPOINT_FILE))
        .expect("symlink");

    assert_eq!(Checkpoint::load(&dir), Checkpoint::empty());
}

#[test]
fn oversized_checkpoint_loads_empty() {
    let fx = Fixture::new();
    let dir = fx.tokens_dir();
    let mut json = serde_json::to_string(&populated()).expect("serialize");
    json.push_str(&" ".repeat(MAX_CHECKPOINT_BYTES));
    fs::write(dir.join(CHECKPOINT_FILE), json).expect("write");

    assert_eq!(Checkpoint::load(&dir), Checkpoint::empty());
}

#[test]
fn directory_or_fifo_checkpoint_loads_empty_without_blocking() {
    let fx = Fixture::new();
    let dir = fx.tokens_dir();
    fs::create_dir(dir.join(CHECKPOINT_FILE)).expect("directory");
    assert_eq!(Checkpoint::load(&dir), Checkpoint::empty());

    fs::remove_dir(dir.join(CHECKPOINT_FILE)).expect("remove");
    let fifo = dir.join(CHECKPOINT_FILE);
    nix::unistd::mkfifo(&fifo, nix::sys::stat::Mode::S_IRWXU).expect("mkfifo");
    assert_eq!(Checkpoint::load(&dir), Checkpoint::empty());
}

#[test]
fn prune_drops_old_buckets_of_every_agent() {
    let mut checkpoint = populated();

    checkpoint.prune(NOW.div_euclid(BUCKET_SECS) + 1);

    let agents = checkpoint
        .stages
        .values()
        .flat_map(|agents| agents.values());
    assert!(agents.clone().all(|agent| agent.buckets.is_empty()));
    assert!(agents.map(|agent| agent.total.fresh()).sum::<u64>() > 0);
}
