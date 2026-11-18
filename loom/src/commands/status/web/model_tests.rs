use super::*;
use crate::commands::status::data::StageSummary;
use crate::models::stage::StageStatus;
use crate::quota::{ProviderQuota, QuotaSnapshot, QuotaWindow, WindowKind};
use chrono::TimeZone;

#[path = "model_tests_busy.rs"]
mod busy;
#[path = "model_tests_stages.rs"]
mod stages;
#[path = "model_tests_tokens.rs"]
mod tokens;

const FIXTURE: &str = include_str!("../../../../../web/src/api/fixtures/snapshot.json");
const STATUSES_FIXTURE: &str = include_str!("../../../../../web/src/api/fixtures/statuses.json");

fn fixture_event(
    at: &str,
    from: StageStatus,
    to: StageStatus,
    session: Option<&str>,
) -> crate::fs::stage_events::StageEvent {
    crate::fs::stage_events::StageEvent {
        at: at.parse().unwrap(),
        stage_id: "server".to_owned(),
        from,
        to,
        session: session.map(str::to_owned),
        session_type: None,
        reason: None,
    }
}

fn fixture_activity() -> Vec<crate::fs::stage_events::StageEvent> {
    vec![
        fixture_event(
            "2026-09-04T11:50:00Z",
            StageStatus::Executing,
            StageStatus::NeedsAdjudication,
            Some("session-server-1"),
        ),
        fixture_event(
            "2026-09-04T11:58:00Z",
            StageStatus::NeedsAdjudication,
            StageStatus::Queued,
            None,
        ),
    ]
}

fn fixed_time() -> DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 9, 4, 12, 0, 0).single().unwrap()
}

/// The [`StatusData`] behind [`fixture_snapshot`], wrapping `stages`.
fn fixture_status(stages: Vec<StageSummary>) -> StatusData {
    StatusData {
        stages,
        merge: crate::commands::status::data::MergeSummary {
            merged: vec![],
            pending: vec!["docs".to_owned()],
            conflicts: vec!["docs".to_owned()],
        },
        progress: crate::commands::status::data::ProgressSummary {
            total: 9,
            completed: 1,
            executing: 1,
            pending: 3,
            blocked: 4,
        },
        plan_name: Some("Web Dashboard Fixture".to_owned()),
        activity: fixture_activity(),
        quota: QuotaSnapshot {
            claude: Some(ProviderQuota {
                observed_at: 1_788_523_200,
                windows: vec![
                    QuotaWindow {
                        kind: WindowKind::FiveHour,
                        used_percent: 48.0,
                        resets_at: Some(1_788_531_180),
                    },
                    QuotaWindow {
                        kind: WindowKind::SevenDay,
                        used_percent: 31.0,
                        resets_at: Some(1_788_876_000),
                    },
                ],
                plan: None,
                error: None,
            }),
            codex: Some(ProviderQuota {
                observed_at: 1_788_522_960,
                windows: vec![QuotaWindow {
                    kind: WindowKind::SevenDay,
                    used_percent: 63.0,
                    resets_at: Some(1_788_728_400),
                }],
                plan: Some("pro".to_owned()),
                error: None,
            }),
        },
        tokens: Some(tokens::fixture_tokens()),
    }
}

/// The scheduler alerts behind [`fixture_snapshot`].
fn fixture_alerts() -> Vec<WebAlert> {
    vec![
        WebAlert {
            severity: WebSeverity::Info,
            text: "1 stage waiting on a free slot".to_owned(),
        },
        WebAlert {
            severity: WebSeverity::Warning,
            text: "client failed acceptance; retrying in 30s".to_owned(),
        },
        WebAlert {
            severity: WebSeverity::Critical,
            text: "orchestrator loop stalled 75s".to_owned(),
        },
    ]
}

fn fixture_snapshot() -> WebSnapshot {
    let stages = stages::fixture_stages();
    let attention = crate::commands::status::render::attention_entries(&stages)
        .iter()
        .map(WebAttention::from)
        .collect();
    let status = fixture_status(stages);
    WebSnapshot {
        status,
        attention,
        alerts: fixture_alerts(),
        daemon: DaemonState::Running,
        tick_age_secs: Some(4),
        busy: None,
        source: SnapshotSource::Daemon,
        terminals: false,
        notice: None,
        generated_at: fixed_time(),
        version: "0.0.0-fixture".to_owned(),
    }
}

#[test]
fn fixture_matches_serde_output() {
    let actual = serde_json::to_value(fixture_snapshot()).unwrap();
    let expected = serde_json::from_str::<serde_json::Value>(FIXTURE).unwrap();
    assert_eq!(
        actual,
        expected,
        "fixture out of date; expected:\n{}",
        serde_json::to_string_pretty(&expected).unwrap()
    );
}

#[test]
fn fixture_deserializes_into_web_snapshot() {
    let snapshot = serde_json::from_str::<WebSnapshot>(FIXTURE).unwrap();
    assert_eq!(snapshot.status.stages.len(), 9);
}

#[test]
fn daemon_state_maps_every_variant() {
    assert_eq!(
        DaemonState::from(DaemonStatus::Running),
        DaemonState::Running
    );
    assert_eq!(
        DaemonState::from(DaemonStatus::ProcessOnly),
        DaemonState::ProcessOnly
    );
    assert_eq!(
        DaemonState::from(DaemonStatus::NotRunning),
        DaemonState::NotRunning
    );
    assert_eq!(
        DaemonState::from(DaemonStatus::Unreachable),
        DaemonState::Unreachable
    );
    assert!(DaemonState::Running.is_running());
    assert!(!DaemonState::ProcessOnly.is_running());
    assert!(!DaemonState::NotRunning.is_running());
    assert!(DaemonState::Unreachable.is_running());
}

#[test]
fn attention_conversion_keeps_failure_label() {
    let entry = AttentionEntry {
        id: "server".to_owned(),
        name: "Rust server".to_owned(),
        label: "BLOCKED",
        status: crate::models::stage::StageStatus::Blocked,
        command: Some("loom stage retry server".to_owned()),
        command_when: None,
        alternatives: Vec::new(),
        acceptance: Vec::new(),
        note: None,
        automatic: false,
        failure_type: Some(FailureType::TestFailure),
        evidence: vec!["test failed".to_owned()],
        review_reason: None,
        reason: None,
        failing_criteria: Vec::new(),
        terminal_history: None,
        cleanup_warning: None,
        has_human_review_choices: false,
        dispute_count: None,
        judge_heartbeat_secs: None,
        completion_blocker: None,
        outgoing_session_exit_reason: None,
    };
    let attention = WebAttention::from(&entry);
    assert_eq!(attention.status, crate::models::stage::StageStatus::Blocked);
    assert_eq!(attention.label, "BLOCKED");
    assert_eq!(attention.failure_label.as_deref(), Some("test"));
    let wire = serde_json::to_value(&attention).unwrap();
    assert_eq!(wire["command"], "loom stage retry server");
    assert!(wire["note"].is_null());
    assert_eq!(wire["automatic"], false);
    assert!(wire.get("hint").is_none());
}

#[test]
fn attention_conversion_carries_the_block_facts_and_flags_the_history() {
    let stages = stages::fixture_stages();
    let blocked = stages
        .iter()
        .find(|stage| stage.id == "local-runtime")
        .unwrap();
    let entries = crate::commands::status::render::attention_entries(std::slice::from_ref(blocked));

    let wire = serde_json::to_value(WebAttention::from(&entries[0])).unwrap();

    assert_eq!(wire["terminal_history"], true);
    assert_eq!(wire["failing_criteria"][0]["index"], 4);
    assert!(wire.get("disputes").is_none());
    assert_eq!(wire["acceptance"].as_array().unwrap().len(), 5);
    assert_eq!(
        wire["reason"],
        wire["note"]
            .as_str()
            .unwrap()
            .strip_prefix("blocked by the stage agent: ")
            .unwrap()
    );
    assert!(!wire["note"].as_str().unwrap().contains("less -R"));
}

/// An executing stage whose `loom stage complete` failed a gate stays
/// `executing` on the wire. The dashboard tells it from a running stage by
/// the stage's `completion_blocker`, the summary `loom status` lists it by;
/// its attention entry is filed under the generic blocked status.
#[test]
fn a_completion_pending_stage_reaches_the_wire_with_its_blocker() {
    let stages = stages::fixture_stages();
    let mut pending = stages.iter().find(|s| s.id == "server").unwrap().clone();
    pending.completion_blocker = Some(crate::status::data::CompletionBlockerSummary {
        state: crate::status::data::CompletionBlockerState::Pending,
        fingerprint: "feedface1234".to_owned(),
        failure_code: "daemon_rejected".to_owned(),
        summary: Some("test-integrity gate failed".to_owned()),
        commit: "0123456789ab".to_owned(),
        repeat_count: 1,
        first_observed_at: None,
        last_observed_at: None,
        next_action: "watching: a repeat of daemon_rejected parks this stage".to_owned(),
    });
    let entries = crate::status::attention_model::attention_entries(std::slice::from_ref(&pending));

    let entry = serde_json::to_value(WebAttention::from(&entries[0])).unwrap();
    let stage = serde_json::to_value(&pending).unwrap();

    assert_eq!(entry["label"], "COMPLETION PENDING");
    assert_eq!(entry["status"], "blocked");
    assert_eq!(entry["automatic"], true);
    assert_eq!(stage["status"], "executing");
    assert_eq!(stage["completion_blocker"]["state"], "pending");
    assert_eq!(
        stage["completion_blocker"]["summary"],
        "test-integrity gate failed"
    );
    let mut running = pending.clone();
    running.completion_blocker = None;
    assert!(serde_json::to_value(&running)
        .unwrap()
        .get("completion_blocker")
        .is_none());
}

/// Every [`StageStatus`], ordered as `web/src/api/fixtures/statuses.json` lists
/// them.
///
/// The `match` below is exhaustive and has no wildcard arm, so a new variant
/// stops the build here. Without it the variant compiles cleanly, the page's
/// `stageStatusSchema` then rejects the whole snapshot at runtime, and the
/// dashboard renders nothing.
fn every_stage_status() -> Vec<StageStatus> {
    let statuses = vec![
        StageStatus::WaitingForDeps,
        StageStatus::Queued,
        StageStatus::Executing,
        StageStatus::WaitingForInput,
        StageStatus::NeedsHandoff,
        StageStatus::Completed,
        StageStatus::Skipped,
        StageStatus::Blocked,
        StageStatus::CompletedWithFailures,
        StageStatus::MergeConflict,
        StageStatus::MergeBlocked,
        StageStatus::NeedsHumanReview,
        StageStatus::NeedsAdjudication,
    ];
    for status in &statuses {
        match status {
            StageStatus::WaitingForDeps
            | StageStatus::Queued
            | StageStatus::Executing
            | StageStatus::WaitingForInput
            | StageStatus::NeedsHandoff
            | StageStatus::Completed
            | StageStatus::Skipped
            | StageStatus::Blocked
            | StageStatus::CompletedWithFailures
            | StageStatus::MergeConflict
            | StageStatus::MergeBlocked
            | StageStatus::NeedsHumanReview
            | StageStatus::NeedsAdjudication => {}
        }
    }
    statuses
}

#[test]
fn statuses_fixture_matches_stage_status() {
    #[derive(Deserialize)]
    struct StatusFixtureEntry {
        status: StageStatus,
        icon: String,
        label: String,
        legend: String,
    }
    let fixture = serde_json::from_str::<Vec<StatusFixtureEntry>>(STATUSES_FIXTURE).unwrap();
    let statuses = every_stage_status();
    let legend = &crate::commands::status::ui::tui::ledger::legend::LEGEND;
    assert_eq!(fixture.len(), statuses.len());
    for (status, entry) in statuses.iter().zip(fixture.iter()) {
        assert_eq!(entry.status, *status);
        assert_eq!(entry.icon, status.icon());
        assert_eq!(entry.label, status.label());
        let expected = legend
            .iter()
            .find(|(candidate, _)| candidate == status)
            .unwrap()
            .1;
        assert_eq!(entry.legend, expected);
    }
}
