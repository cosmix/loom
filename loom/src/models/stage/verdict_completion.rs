//! The two ways a stage reaches `Completed`: every ordinary completer goes
//! through `try_transition_to_completed`, which refuses a stage in
//! adjudication, and the daemon's run after a verdict goes through
//! `complete_from_verdict`, the only caller of the `NeedsAdjudication ->
//! Completed` edge.

use anyhow::{bail, ensure, Result};

use super::types::{Stage, StageStatus};

impl Stage {
    /// `Completed` for every completer except the verdict completion: a stage in adjudication waits for its verdict.
    ///
    /// The transition table permits `NeedsAdjudication -> Completed` for
    /// `complete_from_verdict`; ordinary completers (a session's `loom stage
    /// complete`, a merge landing) rely on being refused there, because a
    /// dispute filed meanwhile must be judged before the stage closes.
    pub(super) fn try_transition_to_completed(&mut self) -> Result<()> {
        if self.status == StageStatus::NeedsAdjudication {
            bail!(
                "stage '{}' is awaiting adjudication; only the completion run after its verdict completes it",
                self.id
            );
        }
        self.try_transition(StageStatus::Completed)
    }

    /// NeedsAdjudication -> Completed for the daemon's verdict completion; refuses any other status or a stage that names a session.
    pub fn complete_from_verdict(&mut self) -> Result<()> {
        ensure!(
            self.status == StageStatus::NeedsAdjudication,
            "stage '{}' is {}, not awaiting adjudication; the verdict completion completes only a held stage",
            self.id,
            self.status
        );
        ensure!(
            self.session.is_none(),
            "stage '{}' names session '{}'; the verdict completion completes only a sessionless stage",
            self.id,
            self.session.as_deref().unwrap_or_default()
        );
        self.try_transition(StageStatus::Completed)?;
        self.close_reason = None;
        self.stamp_completed();
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn stage_in(status: StageStatus) -> Stage {
        let mut stage = Stage::new("held".to_string(), None);
        stage.status = status;
        stage
    }

    #[test]
    fn generic_completers_refuse_a_stage_in_adjudication() {
        let mut stage = stage_in(StageStatus::NeedsAdjudication);
        let err = stage.try_complete(None).unwrap_err().to_string();
        assert!(err.contains("awaiting adjudication"), "{err}");
        assert_eq!(stage.status, StageStatus::NeedsAdjudication);
        assert!(stage.completed_at.is_none());

        let mut stage = stage_in(StageStatus::NeedsAdjudication);
        assert!(stage.try_complete_merge().is_err());
        assert_eq!(stage.status, StageStatus::NeedsAdjudication);
        assert!(!stage.merged);
    }

    #[test]
    fn generic_completers_still_complete_an_executing_stage() {
        let mut stage = stage_in(StageStatus::Executing);
        stage.try_complete(Some("done".to_string())).unwrap();
        assert_eq!(stage.status, StageStatus::Completed);
        assert!(stage.completed_at.is_some());

        let mut stage = stage_in(StageStatus::Executing);
        stage.try_complete_merge().unwrap();
        assert_eq!(stage.status, StageStatus::Completed);
        assert!(stage.merged);
    }

    #[test]
    fn verdict_completion_refuses_an_executing_stage() {
        let mut stage = stage_in(StageStatus::Executing);
        let err = stage.complete_from_verdict().unwrap_err().to_string();
        assert!(err.contains("is Executing"), "{err}");
        assert_eq!(stage.status, StageStatus::Executing);
    }

    #[test]
    fn verdict_completion_refuses_a_stage_naming_a_session() {
        let mut stage = stage_in(StageStatus::NeedsAdjudication);
        stage.session = Some("session-1".to_string());
        let err = stage.complete_from_verdict().unwrap_err().to_string();
        assert!(err.contains("session-1"), "{err}");
        assert_eq!(stage.status, StageStatus::NeedsAdjudication);
        assert!(stage.completed_at.is_none());
    }

    #[test]
    fn verdict_completion_completes_a_sessionless_held_stage() {
        let mut stage = stage_in(StageStatus::NeedsAdjudication);
        stage.close_reason = Some("an earlier close".to_string());
        stage.complete_from_verdict().unwrap();
        assert_eq!(stage.status, StageStatus::Completed);
        assert!(stage.completed_at.is_some());
        assert_eq!(stage.close_reason, None);
    }
}
