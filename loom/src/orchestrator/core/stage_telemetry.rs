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
    let deliveries = crate::context::delivery::observe_deliveries(
        &orchestrator.config.work_dir,
        plan,
        &stage.id,
    );
    let event = context_event(&stage.id, session_id, deliveries);

    let _ = crate::telemetry::emit(&orchestrator.config.work_dir, &event);
}

fn context_event(
    stage_id: &str,
    session_id: &str,
    deliveries: anyhow::Result<Vec<crate::context::delivery::DeliveryRecord>>,
) -> crate::telemetry::TelemetryEvent {
    use crate::telemetry::TelemetryEvent;
    let reason = match deliveries {
        Ok(records) => match records
            .into_iter()
            .find(|record| record.recipient_id == session_id)
        {
            Some(record) => {
                return TelemetryEvent::ContextDelivered {
                    stage_id: stage_id.to_string(),
                    session_id: session_id.to_string(),
                    context_epoch: record.context_epoch,
                    items: record.delivered.len(),
                }
            }
            None => "no delivery record for this session",
        },
        Err(_) => "delivery records could not be read",
    };
    TelemetryEvent::ContextUnavailable {
        stage_id: stage_id.to_string(),
        session_id: session_id.to_string(),
        reason: reason.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::telemetry::TelemetryEvent;

    #[test]
    fn unavailable_reasons_distinguish_read_failure() {
        let missing = context_event("stage", "session", Ok(Vec::new()));
        let failed = context_event(
            "stage",
            "session",
            Err(anyhow::anyhow!("secret ledger text")),
        );
        assert_eq!(
            missing,
            TelemetryEvent::ContextUnavailable {
                stage_id: "stage".into(),
                session_id: "session".into(),
                reason: "no delivery record for this session".into(),
            }
        );
        assert_eq!(
            failed,
            TelemetryEvent::ContextUnavailable {
                stage_id: "stage".into(),
                session_id: "session".into(),
                reason: "delivery records could not be read".into(),
            }
        );
    }

    #[test]
    fn matching_delivery_reports_epoch_and_count() {
        let record = crate::context::delivery::DeliveryRecord {
            recipient_id: "session".into(),
            launch_id: "launch".into(),
            context_epoch: "epoch".into(),
            delivered: vec![],
            written_at: chrono::Utc::now(),
        };
        assert_eq!(
            context_event("stage", "session", Ok(vec![record])),
            TelemetryEvent::ContextDelivered {
                stage_id: "stage".into(),
                session_id: "session".into(),
                context_epoch: "epoch".into(),
                items: 0,
            }
        );
    }
}
