//! The ceiling governing a session, and the handoff a session entering Red is owed.
//!
//! Split out of `detection` to keep that file inside the size limit; both
//! answers are about what a session's stage says, so they read together.

use crate::handoff::HandoffOrigin;
use crate::models::session::Session;
use crate::models::stage::{Stage, StageStatus};

use super::detection::Detection;
use super::handlers::Handlers;

/// Write the handoff a session entering the Red band is owed. Best-effort: a
/// missing stage or an unwritable handoff must not abort the tick.
pub(super) fn generate_red_band_handoff(
    session: &Session,
    stages: &[Stage],
    handlers: &Handlers,
    reuse_existing: bool,
) -> bool {
    let Some(stage) = stage_for(session, stages) else {
        return false;
    };
    let result = if reuse_existing {
        handlers.ensure_context_handoff(session, stage, HandoffOrigin::RedBand)
    } else {
        handlers
            .generate_context_handoff(session, stage, HandoffOrigin::RedBand)
            .map(Some)
    };
    match result {
        Ok(Some(path)) => {
            eprintln!(
                "Generated handoff for session {} at {}",
                session.id,
                path.display()
            );
            true
        }
        Ok(None) => true,
        Err(e) => {
            eprintln!(
                "Failed to generate handoff for session '{}': {}",
                session.id, e
            );
            false
        }
    }
}

impl Detection {
    pub(super) fn record_red_handoff_ready(
        &mut self,
        session: &Session,
        stages: &[Stage],
        handlers: &Handlers,
        reuse_existing: bool,
    ) {
        if generate_red_band_handoff(session, stages, handlers, reuse_existing) {
            self.red_handoff_ready.insert(session.id.clone());
        }
    }
}

/// The stage a session is currently assigned to, if it is in `stages`.
fn stage_for<'a>(session: &Session, stages: &'a [Stage]) -> Option<&'a Stage> {
    let stage_id = session.stage_id.as_deref()?;
    stages.iter().find(|s| s.id == stage_id)
}

pub(super) fn session_has_current_assignment(session: &Session, stages: &[Stage]) -> bool {
    let Some(stage_id) = session.stage_id.as_deref() else {
        return true;
    };
    stages.iter().any(|stage| {
        stage.id == stage_id
            && stage.session.as_deref() == Some(session.id.as_str())
            && matches!(
                stage.status,
                StageStatus::Executing | StageStatus::NeedsHandoff
            )
    })
}

/// Resolve the ceiling governing a session, in absolute tokens, or `None` when
/// the session names a stage this poll's snapshot does not contain.
///
/// `ContextConfig::ceiling_for` owns the order (stage value ->
/// `[context] ceiling_tokens` -> the built-in default) so this reader cannot
/// drift from the signal, the launcher and `loom status`. It resolves against
/// the config the monitor read once at startup rather than
/// `resolve_context_ceiling_tokens`, which would re-read `.loom/work/config.toml`
/// for every session on every tick.
///
/// A session that NAMES a stage may only be judged against THAT stage's
/// ceiling. `list_all_stages` skips a stage file it cannot read, so a stage
/// missing from the snapshot means "unknown", and defaulting there would
/// re-judge a session with a 300k ceiling against the 150k default and kill it
/// at a backstop it never had. A session that names no stage is a different
/// case: nothing was declared, so nothing is missing, and the plan-wide
/// ceiling governs it.
pub(super) fn resolve_ceiling_tokens(
    session: &Session,
    stages: &[Stage],
    handlers: &Handlers,
) -> Option<u32> {
    let declared = match session.stage_id {
        Some(_) => Some(stage_for(session, stages)?.context_ceiling_tokens),
        None => None,
    };
    Some(handlers.context_config().ceiling_for(declared.flatten()))
}
