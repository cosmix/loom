use super::{Checkpoint, Saved, CHECKPOINT_VERSION, MAX_CHECKPOINT_BYTES};
use crate::tokens::accumulate::{AgentState, MESSAGE_TABLE_CAP};
use crate::tokens::discover::{Source, ThreadLookup};
use crate::tokens::model::{AgentKind, BUCKET_SECS};
use crate::tokens::tests_fixture::{claude_line, codex_count, usage, Fixture, THREAD_ID};
use crate::tokens::tracker::CHECKPOINT_FILE;
use std::collections::HashSet;
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
fn version_two_maxima_are_rebuilt_instead_of_migrated_without_attribution() {
    let fx = Fixture::new();
    let dir = fx.tokens_dir();
    let mut body = serde_json::to_value(populated()).unwrap();
    body["version"] = serde_json::json!(2);
    for agents in body["stages"].as_object_mut().unwrap().values_mut() {
        for agent in agents.as_object_mut().unwrap().values_mut() {
            for message in agent["file"]["messages"].as_array_mut().unwrap() {
                message.as_array_mut().unwrap().truncate(2);
            }
        }
    }
    fs::write(
        dir.join(CHECKPOINT_FILE),
        serde_json::to_vec(&body).unwrap(),
    )
    .unwrap();
    let (loaded, warning) = Checkpoint::load_reporting(&dir);
    assert_eq!(loaded, Checkpoint::empty());
    assert!(warning.unwrap().starts_with("checkpoint is version 2;"));
}

#[test]
fn version_one_checkpoint_is_refused_with_its_reason() {
    let fx = Fixture::new();
    let dir = fx.tokens_dir();
    let mut checkpoint = populated();
    checkpoint.version = 1;
    checkpoint.save(&dir).expect("save");

    let (loaded, warning) = Checkpoint::load_reporting(&dir);

    assert_eq!(loaded, Checkpoint::empty());
    let warning = warning.expect("a reason");
    assert!(warning.starts_with("checkpoint is version 1;"), "{warning}");
}

#[test]
fn a_real_version_one_body_is_refused_for_its_version_not_a_missing_field() {
    let fx = Fixture::new();
    let dir = fx.tokens_dir();
    // Version 1 agents had neither `effort` nor `by_model`.
    let mut body = serde_json::to_value(populated()).expect("serialize");
    body["version"] = serde_json::json!(1);
    for agents in body["stages"].as_object_mut().expect("stages").values_mut() {
        for agent in agents.as_object_mut().expect("agents").values_mut() {
            let agent = agent.as_object_mut().expect("agent");
            agent.remove("effort");
            agent.remove("by_model");
        }
    }
    assert!(serde_json::from_value::<Checkpoint>(body.clone()).is_err());
    fs::write(dir.join(CHECKPOINT_FILE), body.to_string()).expect("write");

    let (loaded, warning) = Checkpoint::load_reporting(&dir);

    assert_eq!(loaded, Checkpoint::empty());
    let warning = warning.expect("a reason");
    assert!(warning.starts_with("checkpoint is version 1;"), "{warning}");
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

/// Message ids this long make one full table about 5 MB of JSON: one agent
/// fits [`MAX_CHECKPOINT_BYTES`], two do not.
const BULKY_ID_LEN: usize = 1200;

fn fill_table(agent: &mut AgentState) {
    for n in 0..MESSAGE_TABLE_CAP {
        let id = format!("{n:0>width$}", width = BULKY_ID_LEN);
        agent.file.messages.increase(&id, &usage(1, 0, 0, 0));
    }
}

fn bulky(keys: &[&str]) -> Checkpoint {
    let mut checkpoint = Checkpoint::empty();
    for key in keys {
        fill_table(checkpoint.agent(&source(AgentKind::Subagent, key)));
    }
    checkpoint
}

fn table_len(checkpoint: &Checkpoint, key: &str) -> usize {
    let agent = &checkpoint.stages["alpha"][key];
    serde_json::to_value(&agent.file.messages)
        .expect("serialize")
        .as_array()
        .map_or(0, Vec::len)
}

#[test]
fn save_writes_a_checkpoint_that_load_accepts_up_to_the_cap() {
    let fx = Fixture::new();
    let dir = fx.tokens_dir();
    let checkpoint = bulky(&["a1"]);

    assert_eq!(checkpoint.save(&dir).expect("save"), Saved::Written);

    assert_eq!(Checkpoint::load(&dir), checkpoint);
}

#[test]
fn save_refuses_a_checkpoint_load_would_refuse() {
    let fx = Fixture::new();
    let dir = fx.tokens_dir();

    let saved = bulky(&["a1", "a2"]).save(&dir).expect("save");

    let Saved::OverCap(bytes) = saved else {
        panic!("a checkpoint over the cap must not be written: {saved:?}");
    };
    assert!(bytes > MAX_CHECKPOINT_BYTES);
    assert!(!dir.join(CHECKPOINT_FILE).exists());
}

#[test]
fn clearing_tables_spares_the_listed_agents_and_keeps_their_totals() {
    let mut checkpoint = bulky(&["a1", "a2"]);
    checkpoint.agent(&source(AgentKind::Subagent, "a2")).total = usage(7, 0, 0, 0);
    let keep = HashSet::from([("alpha", "a1")]);

    checkpoint.clear_tables_except(&keep);

    assert_eq!(table_len(&checkpoint, "a1"), MESSAGE_TABLE_CAP);
    assert_eq!(table_len(&checkpoint, "a2"), 0);
    assert_eq!(checkpoint.stages["alpha"]["a2"].total, usage(7, 0, 0, 0));
}

#[test]
fn load_reporting_names_why_it_fell_back() {
    let fx = Fixture::new();
    let dir = fx.tokens_dir();
    let file = dir.join(CHECKPOINT_FILE);
    let reason = |dir: &std::path::Path| {
        let (checkpoint, reason) = Checkpoint::load_reporting(dir);
        assert_eq!(checkpoint, Checkpoint::empty());
        reason
    };
    assert_eq!(reason(&dir), None, "a first run has nothing to explain");

    let mut json = serde_json::to_string(&populated()).expect("serialize");
    json.push_str(&" ".repeat(MAX_CHECKPOINT_BYTES));
    fs::write(&file, &json).expect("oversized");
    assert!(reason(&dir).expect("reason").contains("over"));

    fs::write(&file, "{\"version\":").expect("torn");
    assert!(reason(&dir).expect("reason").contains("unparseable"));

    fs::remove_file(&file).expect("remove");
    fs::create_dir(&file).expect("directory");
    assert!(reason(&dir).expect("reason").contains("unreadable"));

    fs::remove_dir(&file).expect("remove");
    let mut other = populated();
    other.version = CHECKPOINT_VERSION + 1;
    other.save(&dir).expect("save");
    assert!(reason(&dir).expect("reason").contains("version"));

    populated().save(&dir).expect("save");
    assert_eq!(Checkpoint::load_reporting(&dir).1, None);
}
