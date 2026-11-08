//! Contracts for the busy line in the orchestrator tick: `while_busy` keeps the
//! tick fresh and names the running operation, `tick::read` tolerates a bad
//! third line, the scheduling alerts classify busy against stalled, and the web
//! snapshot carries the busy line.
//!
//! Tick files are written by hand in a TempDir so each scenario controls the
//! tick's age and the operation's start time exactly.

use chrono::{DateTime, Duration, SecondsFormat, Utc};
use loom::commands::status::web::model::{collect_snapshot, SnapshotSource};
use loom::orchestrator::busy::{self, BusyOp};
use loom::orchestrator::scheduling_report::{alerts, Alert, Severity};
use loom::orchestrator::tick::{self, Phase, Tick};
use loom::status::data::StatusData;
use std::panic::AssertUnwindSafe;
use std::path::Path;
use tempfile::TempDir;

const STAGE: &str = "stage-a";

/// An RFC 3339 timestamp `secs_ago` seconds before now, at whole-second
/// precision so a parsed value compares equal to the written one.
fn stamp(secs_ago: i64) -> String {
    (Utc::now() - Duration::seconds(secs_ago)).to_rfc3339_opts(SecondsFormat::Secs, true)
}

/// A busy line for `op` on `STAGE` whose operation started `secs_ago` seconds ago.
fn busy_line(op: &str, secs_ago: i64) -> String {
    format!("busy {op} {} {STAGE}", stamp(secs_ago))
}

/// Write `orchestrator.tick`: a timestamp `tick_age` seconds old, phase
/// `spawning`, and an optional third line.
fn write_tick(work_dir: &Path, tick_age: i64, third: Option<&str>) {
    let mut content = format!("{}\nspawning\n", stamp(tick_age));
    if let Some(line) = third {
        content.push_str(line);
        content.push('\n');
    }
    std::fs::write(work_dir.join("orchestrator.tick"), content).expect("write tick");
}

fn read_tick(work_dir: &Path) -> Tick {
    tick::read(work_dir)
        .expect("tick reads without error")
        .expect("a tick is present")
}

fn texts(alerts: &[Alert], severity: Severity) -> Vec<&str> {
    alerts
        .iter()
        .filter(|a| a.severity == severity)
        .map(|a| a.text.as_str())
        .collect()
}

fn assert_no_text(alerts: &[Alert], needle: &str) {
    for alert in alerts {
        assert!(
            !alert.text.contains(needle),
            "no alert may contain {needle:?}, got {alerts:?}"
        );
    }
}

#[test]
fn busy_line_clears_after_panic() {
    let dir = TempDir::new().unwrap();
    let outcome = std::panic::catch_unwind(AssertUnwindSafe(|| {
        busy::while_busy::<()>(
            dir.path(),
            Phase::Spawning,
            BusyOp::Provisioning,
            STAGE,
            || panic!("provisioning failed"),
        )
    }));
    assert!(
        outcome.is_err(),
        "the panic must propagate out of while_busy"
    );

    let after = read_tick(dir.path());
    assert_eq!(after.phase, Some(Phase::Spawning));
    assert_eq!(after.busy, None, "a panicking operation must not stay busy");

    // A helper thread left running after the unwind would write the busy line back.
    std::thread::sleep(std::time::Duration::from_millis(300));
    let later = read_tick(dir.path());
    assert_eq!(
        later.busy, None,
        "no busy line may reappear after the unwind"
    );
}

#[test]
fn busy_line_names_op_and_stage_while_running() {
    let dir = TempDir::new().unwrap();
    let before = Utc::now() - Duration::seconds(1);
    let inside = busy::while_busy(
        dir.path(),
        Phase::Spawning,
        BusyOp::BriefGeneration,
        STAGE,
        || tick::read(dir.path()).expect("tick reads without error"),
    );

    let inside = inside.expect("the first stamp is written before the operation starts");
    assert_eq!(inside.phase, Some(Phase::Spawning));
    let running = inside.busy.expect("the tick names the running operation");
    assert_eq!(running.op, BusyOp::BriefGeneration);
    assert_eq!(running.stage_id, STAGE);
    assert!(
        running.started_at >= before && running.started_at <= Utc::now(),
        "started_at is the operation's start, got {}",
        running.started_at
    );

    let after = read_tick(dir.path());
    assert_eq!(after.phase, Some(Phase::Spawning));
    assert_eq!(
        after.busy, None,
        "the busy line is cleared when the operation returns"
    );
}

/// Read the tick, wait 1.5 s while the operation runs, and read it again.
fn read_twice_while_running(work_dir: &Path) -> (Tick, Tick) {
    busy::while_busy_every(
        work_dir,
        Phase::Spawning,
        BusyOp::Provisioning,
        STAGE,
        std::time::Duration::from_millis(200),
        || {
            let first = read_tick(work_dir);
            std::thread::sleep(std::time::Duration::from_millis(1500));
            (first, read_tick(work_dir))
        },
    )
}

#[test]
fn busy_line_restamps_while_running() {
    let dir = TempDir::new().unwrap();
    let (first, second) = read_twice_while_running(dir.path());

    let first_busy = first.busy.clone().expect("busy line on the first read");
    let second_busy = second.busy.clone().expect("busy line on the second read");
    assert!(
        second.recorded_at > first.recorded_at,
        "the helper re-stamps the tick: {} then {}",
        first.recorded_at,
        second.recorded_at
    );
    assert_eq!(first_busy.op, second_busy.op);
    assert_eq!(first_busy.stage_id, second_busy.stage_id);
    assert_eq!(
        first_busy.started_at, second_busy.started_at,
        "started_at stays the same across re-stamps"
    );

    std::thread::sleep(std::time::Duration::from_millis(600));
    let after = read_tick(dir.path());
    assert_eq!(
        after.busy, None,
        "no helper writes after the operation returned"
    );
}

#[test]
fn malformed_busy_line_reads_as_not_busy() {
    let dir = TempDir::new().unwrap();
    let now = stamp(0);
    let bad_lines = [
        format!("busy bogus-op {now} {STAGE}"),
        format!("busy brief-generation not-a-time {STAGE}"),
        format!("busy brief-generation {now} "),
        format!("busy brief-generation {now} stage\u{1b}[2Ja"),
        format!("busy brief-generation {now} {}", "a".repeat(129)),
        format!("work brief-generation {now} {STAGE}"),
    ];
    for line in &bad_lines {
        write_tick(dir.path(), 0, Some(line));
        let read = tick::read(dir.path());
        let tick = match read {
            Ok(Some(tick)) => tick,
            other => panic!("third line {line:?} must not hide the tick: {other:?}"),
        };
        assert_eq!(tick.phase, Some(Phase::Spawning), "third line {line:?}");
        assert_eq!(tick.busy, None, "third line {line:?} must read as not busy");
    }

    // The limit is 128 bytes: a 128-byte stage id is still accepted.
    let longest = "a".repeat(128);
    write_tick(
        dir.path(),
        0,
        Some(&format!("busy brief-generation {now} {longest}")),
    );
    let busy = read_tick(dir.path())
        .busy
        .expect("a 128-byte stage id is valid");
    assert_eq!(busy.stage_id, longest);
}

#[test]
fn busy_under_ceiling_is_not_stalled() {
    let dir = TempDir::new().unwrap();
    write_tick(dir.path(), 2, Some(&busy_line("brief-generation", 75)));
    let alerts = alerts(dir.path(), true);

    let info = texts(&alerts, Severity::Info);
    assert_eq!(info.len(), 1, "one Info alert, got {alerts:?}");
    assert!(info[0].contains("orchestrator busy"), "got {info:?}");
    assert!(
        info[0].contains("generating the brief for stage-a"),
        "got {info:?}"
    );
    assert!(
        texts(&alerts, Severity::Critical).is_empty(),
        "got {alerts:?}"
    );
    assert_no_text(&alerts, "stalled");
    assert_no_text(&alerts, "loom stop");
}

#[test]
fn per_operation_ceilings_differ() {
    let dir = TempDir::new().unwrap();
    write_tick(dir.path(), 2, Some(&busy_line("brief-generation", 400)));
    let brief = alerts(dir.path(), true);
    let warnings = texts(&brief, Severity::Warning);
    assert!(
        warnings
            .iter()
            .any(|t| t.contains("longer than the expected")),
        "a 400 s brief generation is past its ceiling, got {brief:?}"
    );
    assert!(
        texts(&brief, Severity::Critical).is_empty(),
        "got {brief:?}"
    );
    assert_no_text(&brief, "loom stop");

    write_tick(dir.path(), 2, Some(&busy_line("provisioning", 400)));
    let provisioning = alerts(dir.path(), true);
    assert!(
        texts(&provisioning, Severity::Info)
            .iter()
            .any(|t| t.contains("provisioning stage-a")),
        "a 400 s provisioning run is under its ceiling, got {provisioning:?}"
    );
    assert!(
        texts(&provisioning, Severity::Warning).is_empty(),
        "got {provisioning:?}"
    );
    assert!(
        texts(&provisioning, Severity::Critical).is_empty(),
        "got {provisioning:?}"
    );
}

#[test]
fn global_ceiling_is_critical() {
    let dir = TempDir::new().unwrap();
    write_tick(dir.path(), 2, Some(&busy_line("provisioning", 1900)));
    let alerts = alerts(dir.path(), true);

    let critical = texts(&alerts, Severity::Critical);
    assert_eq!(critical.len(), 1, "one Critical alert, got {alerts:?}");
    assert!(
        critical[0].contains("orchestrator loop stalled"),
        "got {critical:?}"
    );
    assert!(
        critical[0].contains("provisioning stage-a"),
        "got {critical:?}"
    );
    assert!(critical[0].contains("loom stop"), "got {critical:?}");
    assert_no_text(&alerts, "orchestrator busy");
}

#[test]
fn stale_tick_is_critical_while_busy() {
    let dir = TempDir::new().unwrap();
    write_tick(dir.path(), 90, Some(&busy_line("provisioning", 100)));
    let alerts = alerts(dir.path(), true);

    let critical = texts(&alerts, Severity::Critical);
    assert_eq!(critical.len(), 1, "one Critical alert, got {alerts:?}");
    assert!(
        critical[0].contains("orchestrator loop stalled"),
        "got {critical:?}"
    );
    assert!(
        critical[0].contains(" while provisioning stage-a"),
        "got {critical:?}"
    );
    assert!(critical[0].contains("loom stop"), "got {critical:?}");
    assert_no_text(&alerts, "orchestrator busy");
}

fn snapshot_json(work_dir: &Path) -> serde_json::Value {
    let snapshot = collect_snapshot(
        work_dir,
        StatusData::default(),
        SnapshotSource::Files,
        false,
    );
    serde_json::to_value(snapshot).expect("snapshot serializes")
}

#[test]
fn web_snapshot_carries_the_busy_line() {
    let dir = TempDir::new().unwrap();
    let started = stamp(30);
    let line = format!("busy provisioning {started} {STAGE}");
    write_tick(dir.path(), 0, Some(&line));

    let json = snapshot_json(dir.path());
    let busy = &json["busy"];
    assert_eq!(busy["op"], "provisioning", "got {busy}");
    assert_eq!(busy["what"], "provisioning stage-a");
    assert_eq!(busy["stage_id"], STAGE);
    assert_eq!(busy["ceiling_secs"], 600);
    assert_eq!(busy["global_ceiling_secs"], 1800);
    let started_at = busy["started_at"].as_str().expect("started_at is a string");
    let parse = |s: &str| DateTime::parse_from_rfc3339(s).expect("RFC 3339");
    assert_eq!(parse(started_at), parse(&started));

    write_tick(dir.path(), 0, None);
    let json = snapshot_json(dir.path());
    let object = json.as_object().expect("snapshot is an object");
    assert!(object.contains_key("busy"), "busy always serializes");
    assert!(
        json["busy"].is_null(),
        "no busy line gives null, got {}",
        json["busy"]
    );
}
