//! Delivering a crash again until its stage has moved on.
//!
//! A crash is seen once: the record turns `Crashed`, and the next poll finds
//! no transition. The handler can still fail after that (a stage file it could
//! not read or write) and leave the stage `Executing` with a dead session, so
//! the monitor keeps a crash it reported until the stage no longer owes it. The
//! handler latches what it applied (`reported_crashes`), which makes a repeat
//! harmless. Each failed delivery logs, and a state file that stays unwritable
//! fails them all, so the waits between deliveries double up to a limit.

use std::collections::HashMap;
use std::path::PathBuf;

use crate::models::session::{Session, SessionStatus};
use crate::models::stage::{Stage, StageStatus};

use super::detection::Detection;
use super::events::MonitorEvent;

/// The most polls between two deliveries of one crash.
const MAX_POLLS_BETWEEN_DELIVERIES: u32 = 64;

/// A crash reported to the handler that its stage may still owe.
pub(super) struct OwedCrash {
    report: Option<PathBuf>,
    polls_to_skip: u32,
    next_wait: u32,
}

/// The crashes reported to the handler, by session.
pub(super) type UndeliveredCrashes = HashMap<String, OwedCrash>;

impl Detection {
    /// Remember each crash among `events`.
    pub(super) fn note_reported_crashes(&mut self, events: &[MonitorEvent]) {
        for event in events {
            if let MonitorEvent::SessionCrashed {
                session_id,
                crash_report_path,
                ..
            } = event
            {
                let owed = OwedCrash {
                    report: crash_report_path.clone(),
                    polls_to_skip: 0,
                    next_wait: 0,
                };
                self.undelivered_crashes.insert(session_id.clone(), owed);
            }
        }
    }

    /// The event owed to `session`, whose status did not change: its crash
    /// again while its stage still runs under it. A record no longer `Crashed`
    /// is a new attempt that reused the id, which owes nothing. A stage missing
    /// from this snapshot may just be unreadable now, so the crash stays
    /// remembered.
    pub(super) fn redeliver_crash(
        &mut self,
        session: &Session,
        stages: &[Stage],
    ) -> Vec<MonitorEvent> {
        let Some(owed) = self.undelivered_crashes.get_mut(&session.id) else {
            return Vec::new();
        };
        let owner = session
            .stage_id
            .as_ref()
            .and_then(|stage_id| stages.iter().find(|stage| &stage.id == stage_id));
        let still_owed = session.status == SessionStatus::Crashed
            && match owner {
                Some(stage) => {
                    stage.status == StageStatus::Executing
                        && stage.session.as_deref() == Some(session.id.as_str())
                }
                None => session.stage_id.is_some(),
            };
        if !still_owed {
            self.undelivered_crashes.remove(&session.id);
            return Vec::new();
        }
        if owner.is_none() {
            return Vec::new();
        }
        if owed.polls_to_skip > 0 {
            owed.polls_to_skip -= 1;
            return Vec::new();
        }
        owed.polls_to_skip = owed.next_wait;
        owed.next_wait = (owed.next_wait * 2).clamp(1, MAX_POLLS_BETWEEN_DELIVERIES);
        vec![MonitorEvent::SessionCrashed {
            session_id: session.id.clone(),
            stage_id: session.stage_id.clone(),
            crash_report_path: owed.report.clone(),
        }]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ID: &str = "session-1";

    fn record(status: SessionStatus) -> Session {
        let mut session = Session::new();
        session.id = ID.to_string();
        session.assign_to_stage("alpha".to_string());
        session.status = status;
        session
    }

    fn stage_under_session() -> Stage {
        let mut stage = Stage::new("alpha".to_string(), None);
        stage.id = "alpha".to_string();
        stage.status = StageStatus::Executing;
        stage.session = Some(ID.to_string());
        stage
    }

    fn with_owed_crash() -> Detection {
        let mut detection = Detection::new();
        detection.note_reported_crashes(&[MonitorEvent::SessionCrashed {
            session_id: ID.to_string(),
            stage_id: Some("alpha".to_string()),
            crash_report_path: None,
        }]);
        detection
    }

    #[test]
    fn a_crash_that_stays_undelivered_comes_back_ever_more_rarely() {
        let mut detection = with_owed_crash();
        let (crashed, stages) = (record(SessionStatus::Crashed), [stage_under_session()]);

        let delivered: Vec<bool> = (0..16)
            .map(|_| !detection.redeliver_crash(&crashed, &stages).is_empty())
            .collect();

        let polls: Vec<usize> = (0..16).filter(|&poll| delivered[poll]).collect();
        assert_eq!(polls, [0, 1, 3, 6, 11]);
    }

    /// A respawn under a reused recovery id can run before the poll that would
    /// have dropped the first crash: its live record must not be told it died.
    #[test]
    fn a_new_attempt_under_a_reused_id_owes_nothing() {
        let mut detection = with_owed_crash();

        let events =
            detection.redeliver_crash(&record(SessionStatus::Running), &[stage_under_session()]);

        assert!(events.is_empty());
        assert!(detection.undelivered_crashes.is_empty());
    }
}
