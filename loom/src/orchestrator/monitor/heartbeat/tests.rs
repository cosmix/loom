//! Tests for the heartbeat protocol.

use super::*;
use tempfile::TempDir;

#[test]
fn test_heartbeat_creation() {
    let hb = Heartbeat::new("stage-1".to_string(), "session-abc".to_string())
        .with_context_tokens(91_000)
        .with_transcript_path("/tmp/transcript.jsonl")
        .with_last_tool("Bash".to_string())
        .with_activity("Running tests".to_string());

    assert_eq!(hb.stage_id, "stage-1");
    assert_eq!(hb.session_id, "session-abc");
    assert_eq!(hb.context_tokens, Some(91_000));
    assert_eq!(
        hb.transcript_path,
        Some("/tmp/transcript.jsonl".to_string())
    );
    assert_eq!(hb.last_tool, Some("Bash".to_string()));
    assert_eq!(hb.activity, Some("Running tests".to_string()));
    assert_eq!(hb.progress_at, Some(hb.timestamp));
    assert_eq!(hb.activity_kind, Some(ActivityKind::Progress));
}

#[test]
fn legacy_and_malformed_progress_fall_back_to_observation_time() {
    let legacy: Heartbeat = serde_json::from_str(
        r#"{"stage_id":"legacy","session_id":"session-legacy","timestamp":"2026-09-14T10:00:00Z"}"#,
    )
    .unwrap();
    let mut malformed = legacy.clone();
    malformed.progress_at = Some(legacy.timestamp + chrono::Duration::seconds(1));

    assert_eq!(
        (
            legacy.effective_progress_at(),
            malformed.effective_progress_at()
        ),
        (legacy.timestamp, legacy.timestamp)
    );
}

#[test]
fn optional_liveness_fields_are_omitted_and_activity_kind_is_snake_case() {
    let mut heartbeat = Heartbeat::new("wire".to_string(), "session-wire".to_string());
    heartbeat.progress_at = None;
    heartbeat.activity_kind = None;
    let legacy_json = serde_json::to_value(&heartbeat).unwrap();
    heartbeat.activity_kind = Some(ActivityKind::Observation);
    let observation_json = serde_json::to_value(&heartbeat).unwrap();

    assert_eq!(
        (
            legacy_json.get("progress_at"),
            legacy_json.get("activity_kind"),
            observation_json.get("activity_kind")
        ),
        (None, None, Some(&serde_json::json!("observation")))
    );
}

#[test]
fn test_heartbeat_staleness() {
    let hb = Heartbeat::new("stage-1".to_string(), "session-abc".to_string());

    // Fresh heartbeat should not be stale
    assert!(!hb.is_stale(Duration::from_secs(300)));

    // Any heartbeat is stale with 0 timeout
    assert!(hb.is_stale(Duration::from_secs(0)));
}

#[test]
fn test_write_and_read_heartbeat() -> Result<()> {
    let tmp = TempDir::new()?;
    let work_dir = tmp.path();

    let hb = Heartbeat::new("test-stage".to_string(), "test-session".to_string())
        .with_context_tokens(101_000);

    let path = write_heartbeat(work_dir, &hb)?;
    assert!(path.exists());

    let read_hb = read_heartbeat(&path)?;
    assert_eq!(read_hb.stage_id, "test-stage");
    assert_eq!(read_hb.session_id, "test-session");
    assert_eq!(read_hb.context_tokens, Some(101_000));

    Ok(())
}

#[test]
fn test_heartbeat_watcher_poll() -> Result<()> {
    let tmp = TempDir::new()?;
    let work_dir = tmp.path();

    // Write a heartbeat
    let hb = Heartbeat::new("stage-1".to_string(), "session-1".to_string());
    write_heartbeat(work_dir, &hb)?;

    // Poll should find it
    let mut watcher = HeartbeatWatcher::new();
    let updates = watcher.poll(work_dir)?;

    assert_eq!(updates.len(), 1);
    assert_eq!(updates[0].stage_id, "stage-1");
    assert!(updates[0].is_new);

    // Second poll should not return update (no change)
    let updates = watcher.poll(work_dir)?;
    assert!(updates.is_empty());

    Ok(())
}

#[test]
fn test_heartbeat_watcher_check_hung() {
    let budget = Duration::from_secs(60);
    let mut watcher = HeartbeatWatcher::new();

    // No heartbeat
    assert_eq!(
        watcher.check_session_hung("unknown", "session-1", budget),
        HeartbeatStatus::NoHeartbeat
    );

    // Add a fresh heartbeat
    let hb = Heartbeat::new("stage-1".to_string(), "session-1".to_string());
    watcher.heartbeats.insert("stage-1".to_string(), hb);

    assert_eq!(
        watcher.check_session_hung("stage-1", "session-1", budget),
        HeartbeatStatus::Healthy
    );

    // A heartbeat from a different session for the same stage must not
    // flag the current session — treated as NoHeartbeat.
    assert_eq!(
        watcher.check_session_hung("stage-1", "session-2", budget),
        HeartbeatStatus::NoHeartbeat
    );

    // The same cached heartbeat read against a zero budget is Hung — the
    // threshold is the caller's, not the watcher's.
    let zero = Duration::from_secs(0);
    match watcher.check_session_hung("stage-1", "session-1", zero) {
        HeartbeatStatus::Hung { .. } => (),
        other => panic!("Expected Hung, got {other:?}"),
    }

    // Stale-session guard still wins even when the cached heartbeat is old.
    assert_eq!(
        watcher.check_session_hung("stage-1", "session-2", zero),
        HeartbeatStatus::NoHeartbeat
    );
}

#[test]
fn fresh_observations_report_stale_progress_and_observation_age_separately() {
    let now = Utc::now();
    let mut watcher = HeartbeatWatcher::with_now(now);
    let mut heartbeat = Heartbeat::new("stage-observe".to_string(), "session-observe".to_string());
    heartbeat.timestamp = now - chrono::Duration::seconds(2);
    heartbeat.progress_at = Some(now - chrono::Duration::seconds(120));
    heartbeat.activity_kind = Some(ActivityKind::Observation);
    watcher
        .heartbeats
        .insert("stage-observe".to_string(), heartbeat);

    assert_eq!(
        watcher.check_session_hung("stage-observe", "session-observe", Duration::from_secs(60)),
        HeartbeatStatus::Hung {
            stale_duration_secs: 120,
            observation_age_secs: 2,
        }
    );
}

fn observed_heartbeat(now: DateTime<Utc>, timestamp_age: i64, context_tokens: u32) -> Heartbeat {
    let mut heartbeat = Heartbeat::new("stage-observe".to_string(), "session-observe".to_string());
    heartbeat.timestamp = now - chrono::Duration::seconds(timestamp_age);
    heartbeat.progress_at = Some(now - chrono::Duration::seconds(120));
    heartbeat.activity_kind = Some(ActivityKind::Observation);
    heartbeat.context_tokens = Some(context_tokens);
    heartbeat
}

fn assert_observation_update(updates: &[HeartbeatUpdate], context_tokens: u32) {
    assert_eq!(updates.len(), 1);
    assert_eq!(
        (
            updates[0].heartbeat.context_tokens,
            updates[0].progress_advanced,
        ),
        (Some(context_tokens), false)
    );
}

fn assert_observation_hung(
    watcher: &HeartbeatWatcher,
    budget: Duration,
    observation_age_secs: u64,
) {
    assert_eq!(
        watcher.check_session_hung("stage-observe", "session-observe", budget),
        HeartbeatStatus::Hung {
            stale_duration_secs: 120,
            observation_age_secs,
        }
    );
}

#[test]
fn advancing_observations_keep_context_current_while_progress_remains_hung() -> Result<()> {
    let temp = TempDir::new()?;
    let now = Utc::now();
    let mut watcher = HeartbeatWatcher::with_now(now);
    let budget = Duration::from_secs(60);

    write_heartbeat(temp.path(), &observed_heartbeat(now, 3, 10_000))?;
    let first = watcher.poll(temp.path())?;
    assert_observation_update(&first, 10_000);
    assert_observation_hung(&watcher, budget, 3);

    write_heartbeat(temp.path(), &observed_heartbeat(now, 2, 20_000))?;
    let second = watcher.poll(temp.path())?;
    assert_observation_update(&second, 20_000);
    assert_observation_hung(&watcher, budget, 2);

    write_heartbeat(temp.path(), &observed_heartbeat(now, 1, 30_000))?;
    let third = watcher.poll(temp.path())?;
    assert_observation_update(&third, 30_000);
    assert_observation_hung(&watcher, budget, 1);
    Ok(())
}

#[test]
fn a_new_session_does_not_inherit_progress_from_the_previous_owner() -> Result<()> {
    let temp = TempDir::new()?;
    let now = Utc::now();
    let mut watcher = HeartbeatWatcher::with_now(now);
    let mut heartbeat = Heartbeat::new("owned-stage".to_string(), "session-old".to_string());
    heartbeat.timestamp = now - chrono::Duration::seconds(100);
    heartbeat.progress_at = Some(heartbeat.timestamp);
    write_heartbeat(temp.path(), &heartbeat)?;
    let old = watcher.poll(temp.path())?;
    assert_eq!(
        (old.len(), old[0].is_new, old[0].progress_advanced),
        (1, true, false)
    );

    heartbeat.session_id = "session-new".to_string();
    heartbeat.timestamp = now;
    heartbeat.progress_at = Some(now);
    write_heartbeat(temp.path(), &heartbeat)?;
    let new = watcher.poll(temp.path())?;
    assert_eq!(
        (
            new.len(),
            new[0].heartbeat.session_id.as_str(),
            new[0].progress_advanced
        ),
        (1, "session-new", false)
    );
    Ok(())
}

/// `loom handoff` stamps the context figure it finds here into the handoff it
/// writes, so an absent or unmeasured heartbeat must read as "no reading"
/// rather than as zero tokens.
#[test]
fn stage_context_tokens_reads_the_latest_reading() -> Result<()> {
    let tmp = TempDir::new()?;
    let work_dir = tmp.path();

    assert_eq!(stage_context_tokens(work_dir, "absent"), None);

    let unmeasured = Heartbeat::new("quiet".to_string(), "session-1".to_string());
    write_heartbeat(work_dir, &unmeasured)?;
    assert_eq!(stage_context_tokens(work_dir, "quiet"), None);

    let measured =
        Heartbeat::new("loud".to_string(), "session-2".to_string()).with_context_tokens(147_000);
    write_heartbeat(work_dir, &measured)?;
    assert_eq!(stage_context_tokens(work_dir, "loud"), Some(147_000));

    Ok(())
}
