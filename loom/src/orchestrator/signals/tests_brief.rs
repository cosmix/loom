//! Fixtures and tests for the per-stage Knowledge Brief delivery pipeline.
//!
//! Split into two cohesive halves, each its own sibling file registered here
//! (not in `tests.rs`, so the split never touches that file's line count):
//! - `tests_brief_rendering.rs` — brief-RENDERING tests that call formatter
//!   functions (`format_signal_content`, `format_recovery_signal`,
//!   `cache::generate_stable_prefix`) directly against a hand-built
//!   `EmbeddedContext`; no real `.work/` tree is involved.
//! - `tests_brief_e2e.rs` — END-TO-END tests that drive the same behaviour
//!   through the real signal-generation entry points (`generate_signal`,
//!   `generate_recovery_signal`, `generate_knowledge_signal`) over a real
//!   project tree, and also check the delivery record each one writes.

use crate::models::stage::Stage;
use crate::orchestrator::signals::recovery_types::RecoverySignalContent;

#[path = "tests_brief_e2e.rs"]
mod tests_brief_e2e;
#[path = "tests_brief_rendering.rs"]
mod tests_brief_rendering;

// `tests_cache.rs` still reaches this fixture as `tests_brief::sample_context_pack`;
// re-exported here so that call site keeps working unchanged even though the
// fixture itself now lives in the rendering half.
pub(super) use tests_brief_rendering::sample_context_pack;

/// A `RecoverySignalContent` for `stage`, as a crash recovery would build it.
///
/// Shared by both halves: the rendering test drives `format_recovery_signal`
/// directly, and the end-to-end test drives `generate_recovery_signal` over a
/// real `.work/` tree — both need the same "a session just crashed" content.
fn crash_recovery_for(stage: &Stage) -> RecoverySignalContent {
    RecoverySignalContent::for_crash(
        "session-recovery".to_string(),
        stage.id.clone(),
        "session-crashed".to_string(),
        None,
        1,
    )
}
