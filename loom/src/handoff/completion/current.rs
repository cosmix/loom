use crate::handoff::schema::{CompletionBlocker, CompletionCheckpoint};
use crate::models::stage::Stage;

pub fn current_blocker<'a>(
    checkpoint: &'a CompletionCheckpoint,
    stage: &Stage,
    current_commit: Option<&str>,
) -> Option<&'a CompletionBlocker> {
    let blocker = checkpoint.blocker.as_ref()?;
    (checkpoint.stage_id == stage.id
        && stage.session.as_deref() == Some(checkpoint.session_id.as_str())
        && checkpoint.is_actionable()
        && current_commit == Some(blocker.commit.as_str()))
    .then_some(blocker)
}
