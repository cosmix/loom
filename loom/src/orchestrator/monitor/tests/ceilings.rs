//! The daemon context-ceiling backstop and ceiling resolution.

use crate::models::constants::MIN_CONTEXT_CEILING_TOKENS;
use crate::models::session::{Session, SessionStatus};
use crate::models::stage::{Stage, StageStatus};
use crate::orchestrator::monitor::detection::Detection;
use crate::orchestrator::monitor::handlers::Handlers;
use crate::orchestrator::monitor::{ContextHealth, Monitor, MonitorConfig, MonitorEvent};

use super::ceiling_retries::budget_retry_pair;

/// `Handlers` over an empty `.loom/work`. The temp dir comes back with them so the
/// caller keeps it alive for the length of the test.
fn ceiling_harness() -> (tempfile::TempDir, Handlers) {
    let temp_dir = tempfile::TempDir::new().unwrap();
    let config = MonitorConfig {
        work_dir: temp_dir.path().to_path_buf(),
        ..Default::default()
    };
    (temp_dir, Handlers::new(config, None))
}

/// The plan-wide ceiling the two "never judged against the wrong ceiling"
/// tests configure, and the tokens a session carries against it.
///
/// The ORDER is the whole point, and every value below is chosen to hold it:
///
/// ```text
/// PLAN_WIDE_CEILING  <  SESSION_TOKENS  <  DECLARED_CEILING
///     60,000               250,000            300,000
/// ```
///
/// Judged against the ceiling its stage declares, the session is at 83% —
/// Yellow, nothing fires. Judged against the plan-wide ceiling it never
/// declared, it is Red and far past that ceiling's 75,000 backstop. Only a
/// session sitting between the two can tell the two readings apart.
///
/// The plan-wide value is the smallest a config may legally set, which keeps
/// the fixture as far below the session as the schema allows.
const PLAN_WIDE_CEILING: u32 = MIN_CONTEXT_CEILING_TOKENS;
const SESSION_TOKENS: u32 = 250_000;
const DECLARED_CEILING: u32 = 300_000;

/// The ordering, enforced at compile time rather than trusted. Moving any of
/// the three values out of this relation is what silently emptied these tests
/// once already; here it is a build failure instead.
const _: () = assert!(PLAN_WIDE_CEILING < SESSION_TOKENS && SESSION_TOKENS < DECLARED_CEILING);

/// `Handlers` over a `.loom/work` whose plan-wide `[context] ceiling_tokens` is
/// deliberately tiny.
///
/// The value is written to disk rather than taken from
/// `DEFAULT_CONTEXT_CEILING_TOKENS` **on purpose, and must stay that way**.
/// These tests need a wrong ceiling BELOW the session's token count, and the
/// built-in default is most of a model window — no session can exceed it. When
/// the default was 150,000 these tests were pinned to it and were real; the day
/// it became 800,000 they passed vacuously, and would have kept passing if the
/// guard they cover had been deleted outright. Re-pinning them to the constant
/// reintroduces exactly that hole.
fn plan_wide_ceiling_harness() -> (tempfile::TempDir, Handlers) {
    use crate::fs::work_dir::{write_context_config, ContextConfig};

    let temp_dir = tempfile::TempDir::new().unwrap();
    write_context_config(
        temp_dir.path(),
        &ContextConfig {
            ceiling_tokens: PLAN_WIDE_CEILING,
            subagent_ceiling_tokens: PLAN_WIDE_CEILING,
            model_window_tokens: None,
        },
    )
    .unwrap();
    let config = MonitorConfig {
        work_dir: temp_dir.path().to_path_buf(),
        ..Default::default()
    };
    // `Monitor::new` is what reads the section off disk.
    let handlers = Handlers::new(Monitor::new(config).config().clone(), None);
    (temp_dir, handlers)
}

/// A running session at [`SESSION_TOKENS`]: 83% of the ceiling its stage
/// declares, and far past the backstop of the plan-wide ceiling it did not.
fn session_at_250k() -> Session {
    let mut session = Session::new();
    session.id = "session-1".to_string();
    session.status = SessionStatus::Running;
    session.stage_id = Some("stage-1".to_string());
    session.context_tokens = SESSION_TOKENS;
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
/// It must retry while this exact assignment remains over budget. The handler
/// verifies the same identity before it can take the session down, so a stale
/// record cannot make a successor eligible for a retry.
#[test]
fn daemon_backstop_retries_past_125_percent_of_the_stage_ceiling() {
    let (_temp_dir, handlers) = ceiling_harness();
    let mut detection = Detection::new();
    // Pre-seed the band so no band transition can be confused for the backstop.
    detection
        .last_context_levels
        .insert("session-1".to_string(), ContextHealth::Red);
    let (mut session, mut stage) = budget_retry_pair();
    stage.status = StageStatus::Executing;
    // Over the ceiling, under the backstop: the agent's own hook governs here.
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

    // The first handoff handler might fail before it can persist
    // `NeedsHandoff`; retry while the same assignment remains Executing.
    let events = detection.detect_session_changes(&[session.clone()], &[stage.clone()], &handlers);
    assert!(
        events
            .iter()
            .any(|e| matches!(e, MonitorEvent::BudgetExceeded { .. })),
        "an over-budget current Executing assignment must retry after a failed handler"
    );
}

/// The clamp that keeps the backstop reachable at the built-in ceiling.
///
/// `DAEMON_CEILING_MULTIPLIER` alone puts the backstop for an 800,000-token
/// ceiling at 1.25 x 800,000 = 1,000,000 — the entire model window. Resident
/// tokens never get there: the session dies or compacts first, so the daemon's
/// last-resort takedown would sit permanently unarmed and
/// `handle_budget_exceeded` would be unreachable code. `backstop_tokens`
/// clamps it to 95% of the window, 950,000, which a session can cross.
#[test]
fn the_backstop_is_clamped_to_a_reading_a_session_can_actually_reach() {
    let (_temp_dir, handlers) = ceiling_harness();
    let mut detection = Detection::new();

    let mut stage = Stage::new("test".to_string(), None);
    stage.id = "stage-1".to_string();
    stage.status = StageStatus::Executing;
    stage.session = Some("session-1".to_string());
    stage.context_ceiling_tokens = Some(800_000);

    let mut session = session_at_250k();
    session.context_tokens = 940_000;
    let events = detection.detect_session_changes(&[session.clone()], &[stage.clone()], &handlers);
    assert!(
        !events
            .iter()
            .any(|e| matches!(e, MonitorEvent::BudgetExceeded { .. })),
        "under the clamped backstop the agent's own governance still owns it: {events:?}"
    );

    session.context_tokens = 960_000;
    let events = detection.detect_session_changes(&[session], &[stage], &handlers);
    assert!(
        events.iter().any(|e| matches!(
            e,
            MonitorEvent::BudgetExceeded {
                ceiling_tokens: 800_000,
                ..
            }
        )),
        "past the clamp the daemon must fire, naming the stage ceiling: {events:?}"
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
            model_window_tokens: None,
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
    stage.status = StageStatus::Executing;
    stage.session = Some(session.id.clone());
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
/// would re-judge a session against a ceiling it never had and kill it at a
/// backstop it never had. A session with no declared stage is different:
/// nothing was declared, so nothing is missing, and the plan-wide ceiling
/// still governs it (covered by the other tests in this file).
///
/// The plan-wide ceiling comes from `plan_wide_ceiling_harness`, written to
/// disk, because it has to sit BELOW this session's tokens for the fallback to
/// be observable at all; see that helper before substituting a constant.
#[test]
fn a_session_whose_stage_is_missing_from_the_snapshot_is_not_judged() {
    let (_temp_dir, handlers) = plan_wide_ceiling_harness();
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
/// unremarkable: with the stage present and declaring its own ceiling, the
/// session is at 83% of it — Yellow, so at most a warning fires. Judged
/// against the plan-wide ceiling the stage never declared, the same session is
/// Red and past that ceiling's backstop, which is precisely the killing the
/// missing stage above must not license.
///
/// That plan-wide ceiling is written to disk by `plan_wide_ceiling_harness`
/// rather than read from the built-in default, and has to be: the default is
/// larger than the ceiling this stage declares, so pinning the test to it
/// makes both assertions hold no matter what the code does.
#[test]
fn a_session_under_its_stages_declared_ceiling_is_never_judged_against_the_plan_wide_one() {
    let (_temp_dir, handlers) = plan_wide_ceiling_harness();
    let mut detection = Detection::new();

    let mut stage = Stage::new("test".to_string(), Some("Ceiling test stage".to_string()));
    stage.id = "stage-1".to_string();
    stage.status = StageStatus::Executing;
    stage.session = Some("session-1".to_string());
    stage.context_ceiling_tokens = Some(DECLARED_CEILING);

    let events = detection.detect_session_changes(&[session_at_250k()], &[stage], &handlers);
    assert!(
        !events
            .iter()
            .any(|e| matches!(e, MonitorEvent::BudgetExceeded { .. })),
        "a session under its stage's declared ceiling must not cross a backstop: {events:?}"
    );
    assert!(
        !events
            .iter()
            .any(|e| matches!(e, MonitorEvent::SessionContextCritical { .. })),
        "83% of the declared ceiling is Yellow, not Red: {events:?}"
    );
}

/// The mirror that makes the two tests above falsifiable: the SAME session and
/// the SAME plan-wide ceiling, with the stage no longer declaring one. Now the
/// plan-wide value legitimately governs, and it fires — Red, and past its
/// backstop. If this test ever goes quiet, the fixture has drifted back to a
/// plan-wide ceiling the session cannot exceed, and its two neighbours have
/// stopped proving anything.
#[test]
fn the_same_session_is_judged_when_the_plan_wide_ceiling_legitimately_governs() {
    let (_temp_dir, handlers) = plan_wide_ceiling_harness();
    let mut detection = Detection::new();

    let mut stage = Stage::new("test".to_string(), Some("Ceiling test stage".to_string()));
    stage.id = "stage-1".to_string();
    stage.status = StageStatus::Executing;
    stage.session = Some("session-1".to_string());
    assert_eq!(stage.context_ceiling_tokens, None);

    let events = detection.detect_session_changes(&[session_at_250k()], &[stage], &handlers);
    assert!(
        events.iter().any(|e| matches!(
            e,
            MonitorEvent::BudgetExceeded {
                ceiling_tokens: PLAN_WIDE_CEILING,
                ..
            }
        )),
        "a stage declaring no ceiling of its own is governed by the plan-wide one: {events:?}"
    );
    assert!(
        events
            .iter()
            .any(|e| matches!(e, MonitorEvent::SessionContextCritical { .. })),
        "far past the plan-wide ceiling is Red: {events:?}"
    );
}
