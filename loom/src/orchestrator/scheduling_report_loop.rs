//! The scheduler loop's alert for `loom status`, the TUI and the web dashboard:
//! busy while a long operation runs, stalled when the tick or the operation says
//! the loop is stuck. The state comes from `busy::loop_state`, so every surface
//! agrees on the thresholds.

use std::path::Path;

use chrono::{DateTime, Utc};

use crate::orchestrator::busy::{self, Busy, LoopState, GLOBAL_BUSY_CEILING_SECS};
use crate::orchestrator::tick::{self, STALL_THRESHOLD_SECS};
use crate::utils::format_elapsed;

use super::{Alert, Severity};

/// Elapsed time before a wrapped operation is worth an Info alert; shorter runs
/// are routine and the headline already names them.
const BUSY_NOTICE_SECS: i64 = 60;

const RESTART_ADVICE: &str = "restart with `loom stop`, then `loom run`";

/// The loop alert for the tick in `work_dir`, if any. Only `Stalled` is Critical
/// and carries the restart advice.
pub(super) fn loop_alert(work_dir: &Path, now: DateTime<Utc>) -> Option<Alert> {
    let tick = tick::read(work_dir).ok().flatten()?;
    let phase = tick.phase.map(|p| p.as_str());
    match busy::loop_state(Some(&tick), now) {
        LoopState::Unknown | LoopState::Ticking { .. } => None,
        LoopState::Busy {
            busy,
            elapsed_secs,
            over_ceiling,
        } => busy_alert(&busy, elapsed_secs, over_ceiling),
        LoopState::Stalled {
            age_secs,
            busy,
            elapsed_secs,
        } => Some(stalled_alert(age_secs, phase, busy.as_ref(), elapsed_secs)),
    }
}

fn busy_alert(busy: &Busy, elapsed_secs: i64, over_ceiling: bool) -> Option<Alert> {
    if elapsed_secs < BUSY_NOTICE_SECS {
        return None;
    }
    let what = busy.op.describe(&busy.stage_id);
    let elapsed = format_elapsed(elapsed_secs);
    let (severity, text) = if over_ceiling {
        let ceiling = format_elapsed(busy.op.ceiling_secs());
        (
            Severity::Warning,
            format!("orchestrator busy: {what} for {elapsed}, longer than the expected {ceiling}"),
        )
    } else {
        (
            Severity::Info,
            format!("orchestrator busy: {what} for {elapsed}"),
        )
    };
    Some(Alert { severity, text })
}

fn stalled_alert(
    age_secs: i64,
    phase: Option<&str>,
    busy: Option<&Busy>,
    elapsed_secs: Option<i64>,
) -> Alert {
    let text = match (busy, elapsed_secs) {
        // A fresh tick: only the operation's own age can have made it stalled.
        (Some(busy), Some(elapsed)) if age_secs < STALL_THRESHOLD_SECS => format!(
            "orchestrator loop stalled: {} has run {}, past the {} limit — {RESTART_ADVICE}",
            busy.op.describe(&busy.stage_id),
            format_elapsed(elapsed),
            format_elapsed(GLOBAL_BUSY_CEILING_SECS),
        ),
        _ => {
            let phase = phase.map(|p| format!(", stuck in {p}")).unwrap_or_default();
            let while_busy = busy
                .map(|b| format!(" while {}", b.op.describe(&b.stage_id)))
                .unwrap_or_default();
            format!(
                "orchestrator loop stalled: no tick for {age_secs}s{phase}{while_busy} \
                 — {RESTART_ADVICE}"
            )
        }
    };
    Alert {
        severity: Severity::Critical,
        text,
    }
}

#[cfg(test)]
mod tests {
    use super::super::alerts;
    use super::*;
    use chrono::{Duration, SecondsFormat};
    use tempfile::TempDir;

    fn stamp(secs_ago: i64) -> String {
        (Utc::now() - Duration::seconds(secs_ago)).to_rfc3339_opts(SecondsFormat::Secs, true)
    }

    /// Write a tick `tick_age` seconds old; `busy` is `(op, secs since it started)`.
    fn write_tick(dir: &Path, tick_age: i64, busy: Option<(&str, i64)>) {
        let mut content = format!("{}\nevents\n", stamp(tick_age));
        if let Some((op, started)) = busy {
            content.push_str(&format!("busy {op} {} stage-a\n", stamp(started)));
        }
        std::fs::write(dir.join("orchestrator.tick"), content).unwrap();
    }

    fn only_alert(dir: &Path) -> Alert {
        let mut found = alerts(dir, true);
        assert_eq!(found.len(), 1, "got {found:?}");
        found.remove(0)
    }

    fn assert_quiet(dir: &Path) {
        assert!(alerts(dir, true).is_empty());
    }

    #[test]
    fn test_stall_alert_requires_a_running_daemon() {
        let temp = TempDir::new().unwrap();
        write_tick(temp.path(), 36_000, None);

        assert!(alerts(temp.path(), false).is_empty(), "stopped daemon");

        let alert = only_alert(temp.path());
        assert_eq!(alert.severity, Severity::Critical);
        assert!(alert.text.contains("stuck in events"));
        assert!(alert.text.contains("`loom stop`"));
        assert!(!alert.text.contains(" while "));
    }

    #[test]
    fn test_ticking_and_missing_tick_raise_nothing() {
        let temp = TempDir::new().unwrap();
        assert_quiet(temp.path());
        write_tick(temp.path(), 2, None);
        assert_quiet(temp.path());
    }

    #[test]
    fn test_short_busy_run_raises_nothing() {
        let temp = TempDir::new().unwrap();
        write_tick(temp.path(), 2, Some(("brief-generation", 59)));
        assert_quiet(temp.path());
    }

    #[test]
    fn test_busy_past_a_minute_is_info_without_restart_advice() {
        let temp = TempDir::new().unwrap();
        write_tick(temp.path(), 2, Some(("merge-verification", 187)));
        let alert = only_alert(temp.path());
        assert_eq!(alert.severity, Severity::Info);
        assert!(alert
            .text
            .starts_with("orchestrator busy: verifying the merge of stage-a for 3m"));
        assert!(!alert.text.contains("loom stop"));
        assert!(!alert.text.contains("stalled"));
        assert!(!alert.text.contains("longer than"));
    }

    #[test]
    fn test_busy_over_its_ceiling_is_a_warning_naming_the_ceiling() {
        let temp = TempDir::new().unwrap();
        write_tick(temp.path(), 2, Some(("brief-generation", 400)));
        let alert = only_alert(temp.path());
        assert_eq!(alert.severity, Severity::Warning);
        assert!(alert
            .text
            .contains("generating the brief for stage-a for 6m"));
        assert!(alert.text.ends_with("longer than the expected 5m0s"));
        assert!(!alert.text.contains("loom stop"));
        assert!(!alert.text.contains("stalled"));
    }

    #[test]
    fn test_ceiling_is_per_operation() {
        let temp = TempDir::new().unwrap();
        write_tick(temp.path(), 2, Some(("merge-verification", 400)));
        assert_eq!(only_alert(temp.path()).severity, Severity::Info);
    }

    #[test]
    fn test_global_ceiling_is_critical_with_restart_advice() {
        let temp = TempDir::new().unwrap();
        write_tick(temp.path(), 2, Some(("provisioning", 1900)));
        let alert = only_alert(temp.path());
        assert_eq!(alert.severity, Severity::Critical);
        assert!(alert.text.starts_with(
            "orchestrator loop stalled: provisioning stage-a has run 31m40s, past the 30m0s limit"
        ));
        assert!(alert
            .text
            .ends_with("restart with `loom stop`, then `loom run`"));
    }

    #[test]
    fn test_stale_tick_while_busy_names_the_operation_after_the_phase() {
        let temp = TempDir::new().unwrap();
        write_tick(temp.path(), 90, Some(("provisioning", 1900)));
        let alert = only_alert(temp.path());
        assert_eq!(alert.severity, Severity::Critical);
        assert!(alert.text.contains("no tick for 90s"));
        assert!(alert
            .text
            .contains("stuck in events while provisioning stage-a — restart"));
        assert!(!alert.text.contains("has run"));
    }

    #[test]
    fn test_only_stalled_alerts_say_stalled() {
        let temp = TempDir::new().unwrap();
        for (tick_age, started) in [(2, 75), (2, 400), (2, 1900), (90, 100)] {
            write_tick(temp.path(), tick_age, Some(("provisioning", started)));
            let alert = only_alert(temp.path());
            let stalled = alert.severity == Severity::Critical;
            assert_eq!(alert.text.contains("stalled"), stalled, "{alert:?}");
            assert_eq!(alert.text.contains("loom stop"), stalled, "{alert:?}");
        }
    }
}
