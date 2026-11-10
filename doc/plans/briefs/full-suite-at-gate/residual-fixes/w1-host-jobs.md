# W1: host-job fixes (R1)

Agent: `loom-software-engineer`. Read `doc/plans/briefs/full-suite-at-gate/residual-fixes/common.md` first.

Your specification is the plan's `### R1. Host-job fixes` section, items R1.1 to R1.5. Read the whole section before editing anything.

## Files you own

- `loom/src/host_verify/submit.rs`: fn `cancel` only (R1.1)
- `loom/src/commands/stage/merge.rs`: fn `withdraw_or_refuse_host_job` and its doc comment only (R1.2)
- `loom/src/host_verify/tests_submit_pending.rs`: two new tests (R1.3)
- `loom/src/host_verify/tests_submit.rs`: one `pub(super)` removal (R1.4)
- `loom/src/commands/verify.rs`: one test moved into `mod tests` (R1.4)
- `loom/src/orchestrator/core/inbox_drain/check.rs`: the `no_cache` line in fn `apply_check` (R1.5)
- `loom/src/orchestrator/core/inbox_drain/check_merge_tests.rs`: one new test (R1.5)

## What is already there

- **R1.2's test is not yours to write.** Loom's contract session has already added `a_waiting_check_behind_an_ended_one_refuses_then_is_withdrawn` to `loom/src/commands/stage/merge/live_job_tests.rs` and frozen that file. It fails until your R1.2 code change is in. Make only the code change in `merge.rs`.
- **R1.3's tests pass on the current code.** They pin behaviour the existing code already has, so they are green before and after. They fail only on the wrong implementations the plan names. Before writing them, read `request`, `subset`, `queued`, `waiting`, `superseded` and `state_root` in `tests_submit.rs`, and `pair` and `assert_cancelled` in `tests_submit_pending.rs`. Confirm that `subset` uses `HEAD_A` and that both use the same `STAGE`, session and `DIGEST`. Adjust the head argument if `subset` uses another head.
- **R1.5's test fails before your change.** Without the `no_cache` change, the resolver's request joins the ordinary job and `assert_ne!(job.id, cached.id)` fails. Write the test first and watch it fail, then change `apply_check` and watch it pass. Resolve `SubmitRequest` and `Submission` with `loom map --find-all SubmitRequest`; use the path `check.rs` already imports if there is one, else the full `crate::host_verify::submit::` path the plan's test uses. `request(head)` in `check.rs`'s test module sends tree digest `"0".repeat(64)`; confirm that before relying on it.

## Order

1. R1.1: the read-order swap in `submit::cancel`, with its comment.
2. R1.2: the code change in `withdraw_or_refuse_host_job`. If the doc comment names only the current job, add "or waiting".
3. R1.3: the two tests.
4. R1.4: the test move in `verify.rs` (inside `mod tests`, reached through the module's existing `use super::...`). Then run `rg -n joined_requests_append_and_keep_the_last_sixteen loom/src` and drop `pub(super)` from that test only if it has no other reference.
5. R1.5: the test, then the `no_cache` line.
6. `rustfmt --edition 2021` on every Rust file you changed.

## Your checks

Run R1's acceptance block from the plan, from `loom/`, exactly as written: four `cargo test --lib` runs to logs, the `rg` over the logs, and the `wc -l`. Each run prints `exit=0`, each `test result` line shows zero failed and at least one passed, and every file is at or under 400 lines (`check.rs` is 387 after R1.5). `cargo test --lib live_job_tests` must include the frozen contract test passing.
