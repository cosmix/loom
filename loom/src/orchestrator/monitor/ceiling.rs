//! The ceiling governing a session, and the handoff a session entering Red is owed.
//!
//! Split out of `detection` to keep that file inside the size limit; both
//! answers are about what a session's stage says, so they read together.

use crate::models::session::Session;
use crate::models::stage::Stage;

use super::handlers::Handlers;

/// Write the handoff a session entering the Red band is owed. Best-effort: a
/// missing stage or an unwritable handoff must not abort the tick.
pub(super) fn generate_red_band_handoff(session: &Session, stages: &[Stage], handlers: &Handlers) {
    let Some(stage) = stage_for(session, stages) else {
        return;
    };
    match handlers.handle_context_critical(session, stage) {
        Ok(path) => eprintln!(
            "Generated handoff for session {} at {}",
            session.id,
            path.display()
        ),
        Err(e) => eprintln!(
            "Failed to generate handoff for session '{}': {}",
            session.id, e
        ),
    }
}

/// The stage a session is currently assigned to, if it is in `stages`.
fn stage_for<'a>(session: &Session, stages: &'a [Stage]) -> Option<&'a Stage> {
    let stage_id = session.stage_id.as_deref()?;
    stages.iter().find(|s| s.id == stage_id)
}

/// Resolve the ceiling governing a session, in absolute tokens, or `None` when
/// the session names a stage this poll's snapshot does not contain.
///
/// `ContextConfig::ceiling_for` owns the order (stage value ->
/// `[context] ceiling_tokens` -> the built-in default) so this reader cannot
/// drift from the signal, the launcher and `loom status`. It resolves against
/// the config the monitor read once at startup rather than
/// `resolve_context_ceiling_tokens`, which would re-read `.work/config.toml`
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
