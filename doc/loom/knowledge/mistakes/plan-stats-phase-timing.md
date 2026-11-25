# Plan Stats Phase Timing

> Phase spans and timeline bars that ended a stage at completed_at

## A Merge Wait After Completion Was Recorded as Zero Time (2026-11-25)

**What happened:** a stage completed at 23:11:58Z, was `merge-blocked` until 07:57:45Z, and its plan stats held a Waiting span from 23:11:59 to 23:11:59. The plan timeline drew the stage as 1h00m with no hatched wait, and 8h45m of the run's wall time had no visible owner.

**Why:** `record_phase_change` (`verify/transitions/persistence.rs`) stamped every terminal close with `completed_at`, which keeps the session's finish time. A span opened after completion (a merge wait) was closed before its start, and `phases::record` clamped that to zero length. Both timelines and `plan_stats::build::stage_end` also ended a stage at `completed_at`, so time after it could not be drawn.

**Prevention:** a stage's `completed_at` is when its session finished, not when the stage left the graph; any code that ends a stage's extent takes the later of `completed_at` and its last phase end.

**Fix:** a terminal close uses `completed_at` only when the open span began at or before it, otherwise now. The TUI `extent`, web `timeline-layout.ts` and `stage_end` take the later of `completed_at` and the last phase end. Regression tests: `merge_wait_after_completion_closes_at_the_time_of_release`, `wall_time_ends_at_a_merge_wait_that_outlasts_every_completion`, `a_merge_wait_after_completion_extends_the_bar`, and the web `timeline-layout` case.
