# C1: quota time-left estimator (codex unit)

Read doc/plans/briefs/token-flow/common.md section 6 first. Do not run git. Do not touch any path under .loom.

## Files

- Write: loom/src/tokens/quota_left.rs and loom/src/tokens/quota_left_tests.rs.
- Read only: loom/src/tokens/mod.rs (already declares `pub mod quota_left;`).

## Surface (pinned, other units call it)

    pub const SLOPE_WINDOW_SECS: i64 = 1800;
    pub const MIN_SPAN_SECS: i64 = 360;

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct QuotaEstimate {
        pub minutes_left: Option<u32>,
        pub lasts_to_reset: bool,
    }

    /// points: (observed_at unix secs, used_percent 0..=100), oldest first,
    /// one provider, one five-hour window, one reset period.
    pub fn estimate(points: &[(i64, f64)], resets_at: Option<i64>, now: i64) -> QuotaEstimate

## Steps

1. Implement estimate exactly as common.md section 6 states.
   - Filter to points with observed_at >= now - SLOPE_WINDOW_SECS.
   - Fewer than 2 points, or a span under MIN_SPAN_SECS, returns {None, false}.
   - slope = (last.1 - first.1) / ((last.0 - first.0) as f64 / 60.0).
   - A slope of 0 or less returns {None, true}.
   - Otherwise m = ((100.0 - last.1).max(0.0) / slope - (now - last.0) as f64 / 60.0).max(0.0).floor(), clamped into u32.
   - lasts_to_reset = resets_at is Some(r) and now + m * 60 >= r.
   - Ignore NaN or infinite inputs: such a point is dropped before filtering.
2. Declare the tests from quota_left.rs with `#[cfg(test)] #[path = "quota_left_tests.rs"] mod tests;`.
3. Write unit tests:
   - rising 10 points over 30 minutes at 62 percent, reset in 133 minutes, returns Some(114) and false;
   - points (now-3000, 10.0), (now-1530, 52.0), (now-630, 57.0), (now-30, 62.0), reset in 133 minutes, return Some(94) and false (the first point is outside the window; the last is 30 s old, so the elapsed term subtracts 0.5);
   - the same with reset in 60 minutes returns lasts_to_reset true;
   - flat usage returns {None, true};
   - one point returns {None, false};
   - two points 120 s apart return {None, false};
   - points older than 30 minutes are ignored;
   - used above 100 returns Some(0).

Done when your files compile and your tests are written. Your only check is `cargo test --lib --no-run` from loom/: TMPDIR is read-only inside the codex sandbox, so tests that create temp files cannot run there, and the orchestrator runs `cargo test --lib tokens::` after your wave. A compile error in a file you do not own is not yours: report it and stop; never edit that file and never wait for it. Keep both files under 200 lines, every function under 50 lines, rustfmt clean.
