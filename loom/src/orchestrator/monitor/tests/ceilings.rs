//! The daemon context-ceiling backstop and ceiling resolution.

use crate::models::session::{Session, SessionStatus};
use crate::models::stage::{Stage, StageStatus};
use crate::orchestrator::monitor::detection::Detection;
use crate::orchestrator::monitor::handlers::Handlers;
use crate::orchestrator::monitor::{ContextHealth, Monitor, MonitorConfig, MonitorEvent};

/// `Handlers` over an empty `.work`. The temp dir comes back with them so the
/// caller keeps it alive for the length of the test.
fn ceiling_harness() -> (tempfile::TempDir, Handlers) {
    let temp_dir = tempfile::TempDir::new().unwrap();
    let config = MonitorConfig {
        work_dir: temp_dir.path().to_path_buf(),
        ..Default::default()
    };
    (temp_dir, Handlers::new(config, None))
}

/// A running session at 250_000 tokens: past 125% of the 150k built-in
/// default, and only 83% of the 300k ceiling its stage declares.
fn session_at_250k() -> Session {
    let mut session = Session::new();
    session.id = "session-1".to_string();
    session.status = SessionStatus::Running;
    session.stage_id = Some("stage-1".to_string());
    session.context_tokens = 250_000;
    session
}

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

/// `Detection`'s fire-once latch (`last_budget_exceeded`) is in-memory only, so
/// a session record left on disk at a high reading re-fires on the first tick
/// after every `loom run` restart — and that event kills the healthy successor
/// the stage has already moved on to, not the corpse that earned it.
#[test]
fn a_dead_sessions_stale_reading_never_fires_the_backstop() {
    let (_temp_dir, handlers) = ceiling_harness();
    let mut detection = Detection::new();

    let mut stage = Stage::new("test".to_string(), Some("Ceiling test stage".to_string()));
    stage.id = "stage-1".to_string();
    stage.status = StageStatus::Executing;
    stage.context_ceiling_tokens = Some(100_000);
    stage.session = Some("session-new".to_string());

    let mut corpse = Session::new();
    corpse.id = "session-old".to_string();
    corpse.status = SessionStatus::Crashed;
    corpse.stage_id = Some("stage-1".to_string());
    corpse.context_tokens = 300_000;

    let events = detection.detect_session_changes(&[corpse], &[stage], &handlers);
    assert!(
        !events
            .iter()
            .any(|e| matches!(e, MonitorEvent::BudgetExceeded { .. })),
        "a corpse's stale reading must not fire the backstop: {events:?}"
    );
    assert!(
        !events
            .iter()
            .any(|e| matches!(e, MonitorEvent::SessionContextCritical { .. })),
        "a corpse's stale reading must not fire a Red-band critical either: {events:?}"
    );
}

/// A session that names a stage may only be judged against THAT stage's own
/// ceiling. `list_all_stages` skips a stage file it cannot read, so a stage
/// missing from the snapshot this tick means "unknown" — defaulting there
/// would re-judge a session against the 150k built-in default and kill it at
/// a backstop it never had. A session with no declared stage is different:
/// nothing was declared, so nothing is missing, and the plan-wide ceiling
/// still governs it (covered by the other tests in this file).
#[test]
fn a_session_whose_stage_is_missing_from_the_snapshot_is_not_judged() {
    let (_temp_dir, handlers) = ceiling_harness();
    let mut detection = Detection::new();

    // The stage file for "stage-1" could not be read this tick: the snapshot
    // is empty, not merely missing an entry with no ceiling set.
    let events = detection.detect_session_changes(&[session_at_250k()], &[], &handlers);
    assert!(
        !events
            .iter()
            .any(|e| matches!(e, MonitorEvent::BudgetExceeded { .. })),
        "an unresolvable stage must not fall back to the plan-wide default: {events:?}"
    );
    assert!(
        !events
            .iter()
            .any(|e| matches!(e, MonitorEvent::SessionContextCritical { .. })),
        "an unresolvable stage must not fire a Red-band critical either: {events:?}"
    );
}

/// The other half of the same rule, and the proof that the reading itself is
/// unremarkable: with the stage present and declaring its own 300_000 ceiling,
/// 250_000 tokens is 83% of it — Yellow, so at most a warning fires. Judged
/// against the 150_000 default instead, the very same session is at 166% (Red)
/// and past the 187_500 backstop, which is precisely the killing the missing
/// stage above must not license.
#[test]
fn a_session_under_its_stages_declared_ceiling_is_never_judged_against_the_default() {
    let (_temp_dir, handlers) = ceiling_harness();
    let mut detection = Detection::new();

    let mut stage = Stage::new("test".to_string(), Some("Ceiling test stage".to_string()));
    stage.id = "stage-1".to_string();
    stage.context_ceiling_tokens = Some(300_000);

    let events = detection.detect_session_changes(&[session_at_250k()], &[stage], &handlers);
    assert!(
        !events
            .iter()
            .any(|e| matches!(e, MonitorEvent::BudgetExceeded { .. })),
        "250k under a declared 300k ceiling must not cross the backstop: {events:?}"
    );
    assert!(
        !events
            .iter()
            .any(|e| matches!(e, MonitorEvent::SessionContextCritical { .. })),
        "250k under a declared 300k ceiling is Yellow, not Red: {events:?}"
    );
}
