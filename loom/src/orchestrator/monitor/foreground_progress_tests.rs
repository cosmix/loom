use super::*;
use chrono::TimeDelta;

fn sample(ticks: u64) -> CommandProgress {
    let pid = std::process::id();
    CommandProgress {
        pid,
        started: crate::process::process_start_time(pid),
        cpu_ticks: ticks,
    }
}

#[test]
fn independent_readers_share_growth_idle_expiry_and_absolute_deadline() {
    let work = tempfile::TempDir::new().unwrap();
    let start = Utc::now();
    for (offset, ticks, expected) in [
        (301, 10, true),
        (601, 10, false),
        (601, 11, true),
        (899, 12, true),
        (900, 13, true),
        (1000, 14, true),
        (1560, 15, false),
    ] {
        let now = start + TimeDelta::seconds(offset);
        for _reader in ["monitor", "status"] {
            let observed =
                record(work.path(), "session-1", sample(ticks), start, 300, now).unwrap();
            assert_eq!(observed.holds(start, 300, now), expected, "at {offset}");
        }
    }
}

#[test]
fn reordered_samples_preserve_ticks_and_do_not_restart_idle_grace() {
    let start = Utc::now();
    let newer = start + TimeDelta::seconds(700);
    let initial = ForegroundObservation::advance(None, sample(10), start, 300, start).unwrap();
    let grown =
        ForegroundObservation::advance(Some(initial), sample(12), start, 300, newer).unwrap();
    let delayed = ForegroundObservation::advance(
        Some(grown),
        sample(11),
        start,
        300,
        newer - TimeDelta::seconds(1),
    )
    .unwrap();
    assert_eq!(delayed.cpu_ticks, 12);
    assert_eq!(delayed.progress_at, newer);
    let mut reused = sample(100);
    reused.started = Some(0);
    let replaced =
        ForegroundObservation::advance(Some(delayed), reused, start, 300, newer).unwrap();
    assert!(!replaced.holds(start, 300, newer));
}

#[test]
fn shared_wall_window_honors_zero_small_large_and_overflow_budgets() {
    let start = Utc::now();
    for (budget, seconds, expected) in [
        (0, 0, false),
        (60, 1559, true),
        (60, 1560, false),
        (900, 1559, true),
        (900, 1560, false),
        (2000, 1999, true),
        (2000, 2000, false),
    ] {
        assert_eq!(
            within_window(start, budget, start + TimeDelta::seconds(seconds)),
            expected
        );
    }
    assert!(!within_window(start, 300, start - TimeDelta::seconds(1)));
    assert!(ForegroundObservation::advance(None, sample(1), start, u64::MAX, start).is_none());
}

#[test]
fn checkpoint_refuses_recycled_identity_corruption_and_symlink_paths() {
    let work = tempfile::TempDir::new().unwrap();
    let now = Utc::now();
    let mut recycled = sample(10);
    recycled.started = Some(0);
    assert!(record(work.path(), "session-1", recycled, now, 300, now).is_err());
    assert!(record(work.path(), "../escape", sample(10), now, 300, now).is_err());
    let path = work
        .path()
        .join("heartbeat/command-progress/session-1.json");
    assert!(!path.exists());
    record(work.path(), "session-1", sample(10), now, 300, now).unwrap();
    std::fs::write(&path, "broken").unwrap();
    assert!(record(work.path(), "session-1", sample(10), now, 300, now).is_err());
    std::fs::remove_file(&path).unwrap();
    std::os::unix::fs::symlink(work.path().join("missing"), &path).unwrap();
    assert!(record(work.path(), "session-1", sample(10), now, 300, now).is_err());
}

#[test]
fn status_can_read_a_checkpoint_without_waiting_for_another_publisher() {
    let work = tempfile::TempDir::new().unwrap();
    let now = Utc::now();
    record(work.path(), "session-1", sample(10), now, 300, now).unwrap();
    let (_, dir) = checkpoint_directory(work.path()).unwrap();
    let held = std::fs::File::from(dir);
    held.lock_exclusive().unwrap();
    let observed = record(work.path(), "session-1", sample(10), now, 300, now).unwrap();
    assert!(observed.holds(now, 300, now));
}

/// A tool that used more CPU than the next one must not hide the next one's
/// work: the tree's total falls when the first exits, then rises again.
#[test]
fn work_after_a_larger_finished_tool_earns_credit() {
    let start = Utc::now();
    let at = |seconds: i64| start + TimeDelta::seconds(seconds);
    let budget = 300;
    let mut seen = ForegroundObservation::advance(None, sample(1000), start, budget, at(0));
    seen = ForegroundObservation::advance(seen, sample(50), start, budget, at(400));
    assert!(!seen.as_ref().unwrap().holds(start, budget, at(400)));
    seen = ForegroundObservation::advance(seen, sample(60), start, budget, at(500));

    let seen = seen.unwrap();
    assert_eq!(seen.progress_at, at(500));
    assert!(seen.holds(start, budget, at(700)));
    assert!(!seen.holds(start, budget, at(800)));
}

/// A total that stays low earns nothing: only a rise is work.
#[test]
fn a_total_that_fell_and_stays_earns_no_credit() {
    let start = Utc::now();
    let at = |seconds: i64| start + TimeDelta::seconds(seconds);
    let budget = 300;
    let mut seen = ForegroundObservation::advance(None, sample(1000), start, budget, at(0));
    seen = ForegroundObservation::advance(seen, sample(50), start, budget, at(100));
    seen = ForegroundObservation::advance(seen, sample(50), start, budget, at(200));

    assert_eq!(seen.unwrap().progress_at, at(0));
}

/// The command exits between finding it and sampling it. The stall decision
/// sees the observation already on record, not a sudden absence of work.
#[test]
fn a_command_that_exits_mid_sample_keeps_its_recorded_observation() {
    use super::super::foreground::observe_commands;
    let work = tempfile::TempDir::new().unwrap();
    let now = Utc::now();
    let later = now + TimeDelta::seconds(60);
    let mut sampled = std::process::Command::new("sleep")
        .arg("30")
        .spawn()
        .unwrap();
    let pid = sampled.id();

    let first = observe_commands(work.path(), "session-1", &[pid], now, 300, now);
    assert!(first.is_some(), "a live command is sampled");
    sampled.kill().unwrap();
    sampled.wait().unwrap();

    let kept = observe_commands(work.path(), "session-1", &[pid], now, 300, later)
        .expect("the recorded observation stands");
    assert!(kept.holds(now, 300, later));
    assert!(!kept.holds(now, 300, now + TimeDelta::seconds(300)));

    let mut other = std::process::Command::new("sleep")
        .arg("30")
        .spawn()
        .unwrap();
    let other_pid = other.id();
    other.kill().unwrap();
    other.wait().unwrap();
    let unseen = observe_commands(work.path(), "session-1", &[other_pid], now, 300, later);
    assert!(unseen.is_none(), "a process never observed earns nothing");
}
