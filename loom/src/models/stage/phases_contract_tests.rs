//! Frozen contract for the persisted phase history of a stage.
//!
//! Every step goes through `update_stage`, the one persisted stage write, and
//! reads the spans back from the stage file. Phases are compared by their
//! serde names.

use std::path::Path;

use chrono::Utc;
use tempfile::TempDir;

use crate::fs::session_files::save_session;
use crate::models::session::{Session, SessionType};
use crate::models::stage::{Stage, StageStatus};
use crate::verify::transitions::{create_stage, load_stage, update_stage};

const STAGE_ID: &str = "phased";

/// Save a real session record of `kind` for the stage and return its id.
fn saved_session(work_dir: &Path, kind: SessionType) -> String {
    let session = match kind {
        SessionType::Contract => Session::new_contract(STAGE_ID),
        _ => {
            let mut session = Session::new();
            session.stage_id = Some(STAGE_ID.to_string());
            session.tracking_key = Session::derive_tracking_key(STAGE_ID, SessionType::Stage);
            session
        }
    };
    save_session(&session, work_dir).unwrap();
    session.id
}

/// Apply one persisted update, then reload the stage and list its spans as
/// `(phase, closed)`.
fn step(work_dir: &Path, apply: impl FnOnce(&mut Stage)) -> Vec<(String, bool)> {
    update_stage(STAGE_ID, work_dir, |stage| {
        apply(stage);
        Ok(())
    })
    .unwrap();
    let stage = load_stage(STAGE_ID, work_dir).unwrap();
    for pair in stage.phases.windows(2) {
        let closed = pair[0].ended_at.expect("an earlier span is closed");
        assert!(pair[0].started_at <= closed && closed <= pair[1].started_at);
    }
    stage
        .phases
        .iter()
        .map(|span| {
            let phase = serde_json::to_value(span).unwrap()["phase"].clone();
            (phase.as_str().unwrap().to_string(), span.ended_at.is_some())
        })
        .collect()
}

fn spans(entries: &[(&str, bool)]) -> Vec<(String, bool)> {
    entries
        .iter()
        .map(|(phase, closed)| (phase.to_string(), *closed))
        .collect()
}

type Step = (
    &'static str,
    Box<dyn FnOnce(&mut Stage)>,
    Vec<(&'static str, bool)>,
);

const CONTRACT: (&str, bool) = ("contract", true);
const EXECUTING: (&str, bool) = ("executing", true);
const ADJUDICATION: (&str, bool) = ("adjudication", true);
const WAITING: (&str, bool) = ("waiting", true);

fn step_of(
    label: &'static str,
    update: impl FnOnce(&mut Stage) + 'static,
    expected: Vec<(&'static str, bool)>,
) -> Step {
    (label, Box::new(update), expected)
}

/// Queued, the contract writer, then the implementer while still Executing.
fn first_attempt(writer: String, implementer: String) -> Vec<Step> {
    let queue = |s: &mut Stage| s.status = StageStatus::Queued;
    let start = move |s: &mut Stage| {
        s.status = StageStatus::Executing;
        s.session = Some(writer);
    };
    let handover = move |s: &mut Stage| s.session = Some(implementer);
    vec![
        step_of("queued before any work", queue, vec![]),
        step_of("contract writer starts", start, vec![("contract", false)]),
        step_of(
            "same-status handover",
            handover,
            vec![CONTRACT, ("executing", false)],
        ),
    ]
}

/// Adjudication, a requeue for a retry, the retry and completion.
fn after_adjudication(retrier: String) -> Vec<Step> {
    let adjudicate = |s: &mut Stage| {
        s.status = StageStatus::NeedsAdjudication;
        s.session = None;
    };
    let requeue = |s: &mut Stage| {
        s.status = StageStatus::Queued;
        s.retry_count = 1;
    };
    let retry = move |s: &mut Stage| {
        s.status = StageStatus::Executing;
        s.session = Some(retrier);
    };
    let complete = |s: &mut Stage| {
        s.status = StageStatus::Completed;
        s.completed_at = Some(Utc::now());
    };
    let judged = vec![CONTRACT, EXECUTING, ADJUDICATION];
    let waited = [judged.clone(), vec![WAITING]].concat();
    vec![
        step_of(
            "adjudication",
            adjudicate,
            vec![CONTRACT, EXECUTING, ("adjudication", false)],
        ),
        step_of(
            "requeued for a retry",
            requeue,
            [judged, vec![("waiting", false)]].concat(),
        ),
        step_of(
            "retry starts",
            retry,
            [waited.clone(), vec![("retry", false)]].concat(),
        ),
        step_of(
            "completed",
            complete,
            [waited, vec![("retry", true)]].concat(),
        ),
    ]
}

#[test]
fn contract_writing_and_adjudication_get_their_own_spans() {
    let temp = TempDir::new().unwrap();
    let work_dir = temp.path();
    let stage = Stage {
        id: STAGE_ID.to_string(),
        name: "Phased".to_string(),
        status: StageStatus::WaitingForDeps,
        ..Stage::default()
    };
    create_stage(&stage, work_dir).unwrap();
    let writer = saved_session(work_dir, SessionType::Contract);
    let implementer = saved_session(work_dir, SessionType::Stage);
    let retrier = saved_session(work_dir, SessionType::Stage);

    let steps = [
        first_attempt(writer, implementer),
        after_adjudication(retrier),
    ];
    for (label, update, expected) in steps.into_iter().flatten() {
        assert_eq!(step(work_dir, update), spans(&expected), "{label}");
    }
}
