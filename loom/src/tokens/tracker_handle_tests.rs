use super::TokenTracker;
use crate::tokens::plan_totals::plan_tokens;
use crate::tokens::tests_fixture::{append, body, claude_line, unix_now, usage, Fixture};
use crate::tokens::tracker::{final_scan, CHECKPOINT_FILE};
use std::collections::BTreeSet;
use std::time::{Duration, Instant};

#[test]
fn a_line_written_after_the_tracker_ends_is_counted_by_the_final_scan() {
    let fx = Fixture::new();
    let now = unix_now();
    let first = claude_line("m1", now - 90, "claude-opus-5-5", &usage(0, 100, 0, 0));
    let transcript = fx.add_session(
        "alpha",
        "a1b2c3d4-0000-4000-8000-000000000010",
        &body(&[first]),
    );
    let checkpoint = fx.tokens_dir().join(CHECKPOINT_FILE);

    let tracker = TokenTracker::spawn(fx.work_root.clone());
    let deadline = Instant::now() + Duration::from_secs(20);
    while !checkpoint.exists() && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(20));
    }
    assert!(
        checkpoint.exists(),
        "the tracker's first pass wrote no checkpoint"
    );

    let stopping = Instant::now();
    tracker.stop_and_join();
    assert!(
        stopping.elapsed() < Duration::from_secs(5),
        "the tracker outlived its stop by {:?}",
        stopping.elapsed()
    );
    assert!(tracker.take_thread().is_none(), "the thread was joined");
    tracker.stop_and_join();

    let last = claude_line("m2", now - 30, "claude-opus-5-5", &usage(0, 7, 0, 0));
    append(&transcript, &last);
    final_scan(&fx.work_root, Some(&fx.codex_sessions()), now).expect("final scan");

    let ids: BTreeSet<String> = ["alpha".to_owned()].into();
    let plan = plan_tokens(&fx.work_root, &ids).expect("plan tokens");
    assert_eq!(plan.totals.cache_write, 107);
}
