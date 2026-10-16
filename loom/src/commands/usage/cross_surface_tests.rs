use super::{discovery::DiscoveredFile, time_range::TimeRange, transcript};
use crate::tokens::accumulate::AgentState;
use crate::tokens::{AgentKind, TokenUsage};
use serde_json::{json, Value};

fn row(stamp: &str, usage: Value) -> String {
    json!({"type":"assistant", "timestamp":stamp,
        "message":{"id":"same", "model":"model", "content":[], "usage":usage}})
    .to_string()
}

fn compare(rows: &[String], expected: TokenUsage) -> AgentState {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("session.jsonl");
    std::fs::write(&path, rows.join("\n") + "\n").unwrap();
    let file = DiscoveredFile {
        path,
        project_slug: "project".into(),
        scope: transcript::Scope::Main,
        session_id: "session".into(),
        agent_id: None,
    };
    let range = TimeRange {
        since: "2026-08-01T00:00:00Z".parse().unwrap(),
        until: None,
    };
    let offline = transcript::parse(&file, &range).unwrap();
    let mut live = AgentState::new(AgentKind::Main, "session.jsonl");
    for row in rows {
        live.ingest(row);
    }
    let mut total = TokenUsage::default();
    for request in offline.requests() {
        total.add(&TokenUsage {
            input: request.usage.input,
            cache_write: request.usage.cache_creation,
            cache_read: request.usage.cache_read,
            output: request.usage.output,
        });
    }
    assert_eq!(total, expected);
    assert_eq!(live.total, total);
    let sum = |values: Vec<TokenUsage>| {
        values.iter().fold(TokenUsage::default(), |mut sum, value| {
            sum.add(value);
            sum
        })
    };
    assert_eq!(sum(live.buckets.values().copied().collect()), total);
    assert_eq!(
        sum(live.by_model.iter().map(|entry| entry.total).collect()),
        total
    );
    live
}

#[test]
fn nested_cache_writes_agree_across_live_and_offline() {
    compare(
        &[row(
            "2026-08-02T00:00:00Z",
            json!({"input_tokens":5,"output_tokens":2,
        "cache_creation":{"ephemeral_5m_input_tokens":4,"ephemeral_1h_input_tokens":6}}),
        )],
        TokenUsage {
            input: 5,
            output: 2,
            cache_write: 10,
            cache_read: 0,
        },
    );
}

#[test]
fn partial_observations_replace_the_whole_vector_on_both_surfaces() {
    compare(
        &[
            row(
                "2026-08-02T00:00:00Z",
                json!({"input_tokens":10,"output_tokens":4}),
            ),
            row("2026-08-02T00:00:00Z", json!({"output_tokens":8})),
        ],
        TokenUsage {
            output: 8,
            ..TokenUsage::default()
        },
    );
}

#[test]
fn downward_and_reordered_revisions_agree_across_live_and_offline() {
    let old = row(
        "2026-08-02T00:00:00.100Z",
        json!({"input_tokens":10,
        "cache_creation_input_tokens":100,"output_tokens":4}),
    );
    let new = row(
        "2026-08-02T00:00:00.200Z",
        json!({"input_tokens":5,
        "cache_read_input_tokens":100,"output_tokens":8}),
    );
    let expected = TokenUsage {
        input: 5,
        cache_write: 0,
        cache_read: 100,
        output: 8,
    };
    compare(&[old.clone(), new.clone()], expected);
    compare(&[new, old], expected);
}

#[test]
fn revisions_retract_the_original_bucket_and_model() {
    let old = row(
        "2026-08-02T00:00:00Z",
        json!({"input_tokens":10,"output_tokens":4}),
    );
    let new = row(
        "2026-08-02T00:01:00Z",
        json!({"input_tokens":5,"output_tokens":8}),
    )
    .replace("\"model\":\"model\"", "\"model\":\"new-model\"");
    let expected = TokenUsage {
        input: 5,
        output: 8,
        ..TokenUsage::default()
    };
    let live = compare(&[old, new], expected);
    let old_bucket = "2026-08-02T00:00:00Z"
        .parse::<chrono::DateTime<chrono::Utc>>()
        .unwrap()
        .timestamp()
        .div_euclid(crate::tokens::BUCKET_SECS);
    assert_eq!(live.buckets[&old_bucket], TokenUsage::default());
    assert_eq!(live.buckets[&(old_bucket + 2)], expected);
    assert_eq!(
        live.by_model
            .iter()
            .find(|entry| entry.model == "model")
            .unwrap()
            .total,
        TokenUsage::default()
    );
    assert_eq!(
        live.by_model
            .iter()
            .find(|entry| entry.model == "new-model")
            .unwrap()
            .total,
        expected
    );
}

#[test]
fn restored_live_checkpoint_retracts_revision_attribution() {
    let mut live = AgentState::new(AgentKind::Main, "session.jsonl");
    live.ingest(&row(
        "2026-08-02T00:00:00Z",
        json!({"input_tokens":10,"output_tokens":4}),
    ));
    let encoded = serde_json::to_vec(&live).unwrap();
    let mut restored: AgentState = serde_json::from_slice(&encoded).unwrap();
    restored.ingest(&row("2026-08-02T00:01:00Z", json!({"output_tokens":2})));
    assert_eq!(
        restored.total,
        TokenUsage {
            output: 2,
            ..TokenUsage::default()
        }
    );
    assert_eq!(restored.by_model[0].total, restored.total);
    assert_eq!(
        restored
            .buckets
            .values()
            .map(TokenUsage::fresh)
            .sum::<u64>(),
        2
    );
}

#[test]
fn status_and_tracker_consume_the_same_handoff_inventory() {
    use crate::fs::{session_files::save_session, work_dir::WorkDir};
    use crate::models::session::Session;

    let dir = tempfile::tempdir().unwrap();
    let work = WorkDir::new(dir.path().join("work")).unwrap();
    work.initialize().unwrap();
    for id in ["first", "second"] {
        let path = dir.path().join(format!("{id}.jsonl"));
        std::fs::write(
            &path,
            row("2026-08-02T00:00:00Z", json!({"output_tokens":2})) + "\n",
        )
        .unwrap();
        let mut session = Session::new();
        session.stage_id = Some("alpha".into());
        session.transcript_path = Some(path.to_string_lossy().into_owned());
        save_session(&session, work.root()).unwrap();
    }
    let inventory = crate::provider_data::sessions::load_all_sessions(&work).unwrap();
    let status = crate::commands::status::data::load_all_sessions(&work).unwrap();
    let now = "2026-08-02T00:01:00Z"
        .parse::<chrono::DateTime<chrono::Utc>>()
        .unwrap()
        .timestamp();
    let flow = crate::tokens::scan_pass(work.root(), Some(&dir.path().join("codex")), now).unwrap();
    assert_eq!(inventory.len(), 2);
    let mut expected: Vec<_> = inventory.iter().map(|session| &session.id).collect();
    let mut actual: Vec<_> = status.iter().map(|session| &session.id).collect();
    expected.sort();
    actual.sort();
    assert_eq!(actual, expected);
    assert_eq!(flow.stages.len(), 1);
    assert_eq!(flow.stages[0].agents.len(), 2);
    assert_eq!(flow.stages[0].total.output, 4);
}
