//! The daemon context-ceiling backstop and ceiling resolution.

use crate::models::session::{Session, SessionStatus};
use crate::models::stage::Stage;
use crate::orchestrator::monitor::detection::Detection;
use crate::orchestrator::monitor::handlers::Handlers;
use crate::orchestrator::monitor::{ContextHealth, Monitor, MonitorConfig, MonitorEvent};

/// The daemon backstop is deliberately NOT gated on a health-band change.
///
/// A session is already Red at 90% of its ceiling, so by the time it reaches
/// the daemon's 125% backstop there is no band left to transition into. If the
/// crossing check shared the band gate it would never fire at all — which is
/// exactly how the old percentage-based check behaved once `context_tokens`
/// stopped moving.
///
/// It must also fire exactly ONCE per crossing: this event kills a live agent,
/// and re-emitting it every 5-second tick would kill each successor in turn.
#[test]
fn daemon_backstop_fires_once_at_125_percent_of_the_stage_ceiling() {
    use tempfile::TempDir;

    let temp_dir = TempDir::new().unwrap();
    let config = MonitorConfig {
        work_dir: temp_dir.path().to_path_buf(),
        ..Default::default()
    };
    let handlers = Handlers::new(config, None);
    let mut detection = Detection::new();

    // Pre-seed the band so no band transition can be confused for the backstop.
    detection
        .last_context_levels
        .insert("session-1".to_string(), ContextHealth::Red);

    let mut session = Session::new();
    session.id = "session-1".to_string();
    session.status = SessionStatus::Running;
    session.stage_id = Some("stage-1".to_string());

    let mut stage = Stage::new("test".to_string(), Some("Ceiling test stage".to_string()));
    stage.id = "stage-1".to_string();
    stage.context_ceiling_tokens = Some(100_000); // backstop at 125_000

    // Over the ceiling, under the backstop: the agent's own hook governs here,
    // and the daemon stays out of it.
    session.context_tokens = 120_000;
    let events = detection.detect_session_changes(&[session.clone()], &[stage.clone()], &handlers);
    assert!(
        !events
            .iter()
            .any(|e| matches!(e, MonitorEvent::BudgetExceeded { .. })),
        "the daemon must not fire between 100% and 125% of the ceiling"
    );

    // Past the backstop: fire, naming the STAGE ceiling, not the multiplied one.
    session.context_tokens = 130_000;
    let events = detection.detect_session_changes(&[session.clone()], &[stage.clone()], &handlers);
    let fired = events
        .iter()
        .find_map(|e| match e {
            MonitorEvent::BudgetExceeded {
                context_tokens,
                ceiling_tokens,
                ..
            } => Some((*context_tokens, *ceiling_tokens)),
            _ => None,
        })
        .expect("the backstop must fire past 125% of the ceiling");
    assert_eq!(fired, (130_000, 100_000));

    // Still past it on the next tick: must NOT fire again.
    let events = detection.detect_session_changes(&[session.clone()], &[stage.clone()], &handlers);
    assert!(
        !events
            .iter()
            .any(|e| matches!(e, MonitorEvent::BudgetExceeded { .. })),
        "the backstop must fire on the crossing, not on every tick above it"
    );
}

/// A stage without its own ceiling falls back to the plan-wide `[context]`
/// value, and only then to the built-in default.
#[test]
fn a_stage_without_a_ceiling_inherits_the_plan_wide_one() {
    use crate::fs::work_dir::{write_context_config, ContextConfig};
    use tempfile::TempDir;

    let temp_dir = TempDir::new().unwrap();
    let work_dir = temp_dir.path().to_path_buf();
    write_context_config(
        &work_dir,
        &ContextConfig {
            ceiling_tokens: 40_000,
            subagent_ceiling_tokens: 30_000,
        },
    )
    .unwrap();

    let config = MonitorConfig {
        work_dir,
        ..Default::default()
    };
    // `Monitor::new` is what reads the section off disk.
    let handlers = Handlers::new(Monitor::new(config).config().clone(), None);
    let mut detection = Detection::new();

    let mut session = Session::new();
    session.id = "session-1".to_string();
    session.status = SessionStatus::Running;
    session.stage_id = Some("stage-1".to_string());
    // Under the 150k built-in default, but past 125% of the configured 40k.
    session.context_tokens = 60_000;

    let mut stage = Stage::new("test".to_string(), None);
    stage.id = "stage-1".to_string();
    assert_eq!(stage.context_ceiling_tokens, None);

    let events = detection.detect_session_changes(&[session], &[stage], &handlers);
    assert!(
        events.iter().any(|e| matches!(
            e,
            MonitorEvent::BudgetExceeded {
                ceiling_tokens: 40_000,
                ..
            }
        )),
        "the plan-wide [context] ceiling must govern a stage that sets none: {events:?}"
    );
}
