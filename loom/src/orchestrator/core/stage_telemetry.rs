//! Telemetry emission for freshly spawned stage sessions.

use crate::models::stage::Stage;

use super::Orchestrator;

/// Emit one telemetry event for a session just spawned for `stage`.
///
/// Derives every field from the `DeliveryRecord` signal generation already
/// wrote for `session_id` — no second retrieval — falling back to
/// `ContextUnavailable` when no record is found for this session. Telemetry
/// is best-effort by contract: `telemetry::emit`'s own result is discarded,
/// so a telemetry failure can never fail a spawn.
pub(super) fn record_context_telemetry(
    orchestrator: &Orchestrator,
    stage: &Stage,
    session_id: &str,
) {
    // Derived through the shared helper, never by hand: the plan component is
    // the join key between the writer of a delivery record and its readers, and
    // a second derivation reads an empty directory rather than a missing record.
    let plan = crate::context::delivery::plan_key(stage);
    let event =
        crate::context::delivery::load_deliveries(&orchestrator.config.work_dir, plan, &stage.id)
            .ok()
            .and_then(|records| {
                records
                    .into_iter()
                    .find(|record| record.recipient_id == session_id)
            })
            .map(
                |record| crate::telemetry::TelemetryEvent::ContextDelivered {
                    stage_id: stage.id.clone(),
                    session_id: session_id.to_string(),
                    context_epoch: record.context_epoch,
                    items: record.delivered.len(),
                },
            )
            .unwrap_or_else(|| crate::telemetry::TelemetryEvent::ContextUnavailable {
                stage_id: stage.id.clone(),
                session_id: session_id.to_string(),
                reason: "no delivery record for this session".to_string(),
            });

    let _ = crate::telemetry::emit(&orchestrator.config.work_dir, &event);
}
