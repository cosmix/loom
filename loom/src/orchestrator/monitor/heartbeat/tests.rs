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
