//! One worker from admission to `loom subagents result`: a scripted session,
//! the supervisor's settlement, the stage gate and the operator's view.

use crate::native_worker::boundary::Quiescence;
use crate::native_worker::index::{stage_disposition, unit_state, NativeDisposition};
use crate::native_worker::model::{NativeEvent, Outcome, Ownership};
use crate::native_worker::store::read_events;
use crate::native_worker::supervisor::{run_session, settle};

use super::{native_view, ResultArgs};

// The harness is declared privately by each test module that uses it; this
// copy uses only the record fixture and a silent peer.
#[allow(clippy::duplicate_mod, dead_code)]
#[path = "../../native_worker/testkit/mod.rs"]
mod testkit;

use testkit::peer::{agent_message, turn_completed};
use testkit::{invocation, End, ScriptedPeer, SteppingClock, STAGE, UNIT};

#[test]
#[serial_test::serial]
fn a_settled_worker_frees_the_stage_and_its_report_is_readable() {
    let record = invocation();
    let lines = vec![
        agent_message("m1", "All done: parser implemented.", "final_answer"),
        turn_completed("completed"),
    ];
    let mut peer = ScriptedPeer::through_turn_start(lines, End::Silent);
    let (outcome, reason) =
        run_session(&record.dir, &mut peer, &SteppingClock::new(&[])).expect("run_session");
    assert_eq!(outcome, Outcome::Succeeded, "reason: {reason}");
    settle(&record.dir, outcome, &reason, Quiescence::Released).expect("settle");
    assert_settled_tail(&record.dir);
    let state = unit_state(&record.work, STAGE, UNIT)
        .unwrap()
        .expect("unit");
    assert_eq!(state.ownership, Ownership::Released);
    assert_eq!(
        stage_disposition(&record.work, STAGE),
        NativeDisposition::None
    );

    let args = ResultArgs {
        worker: format!("codex:{UNIT}"),
        stage: Some(STAGE.to_string()),
        full: false,
        json: false,
    };
    let (mut out, mut err) = (Vec::new(), Vec::new());
    let code = native_view::result_in(args, None, &record.work, &mut out, &mut err).unwrap();
    assert_eq!(code, 0, "{}", String::from_utf8_lossy(&err));
    let text = String::from_utf8_lossy(&out);
    assert!(text.contains("All done: parser implemented."), "{text}");
}

fn assert_settled_tail(dir: &std::path::Path) {
    let events: Vec<_> = read_events(dir)
        .expect("events")
        .into_iter()
        .map(|record| record.event)
        .collect();
    let tail = &events[events.len() - 3..];
    assert!(
        matches!(tail[0], NativeEvent::ReportStored { .. }),
        "{events:?}"
    );
    assert!(
        matches!(
            tail[1],
            NativeEvent::Terminal {
                outcome: Outcome::Succeeded,
                ..
            }
        ),
        "{events:?}"
    );
    assert!(
        matches!(&tail[2], NativeEvent::OwnershipReleased { by } if by == "supervisor"),
        "{events:?}"
    );
}
