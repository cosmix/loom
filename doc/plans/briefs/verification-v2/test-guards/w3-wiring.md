# test-guards / W3 — completion wiring and IV reachable re-verification

Read first: `doc/plans/briefs/verification-v2/DESIGN.md` D0, D11 (last bullet), D13, D14.
Knowledge: `mistakes/doctrine-and-acceptance.md` "Aggregated Wiring Re-Verification:
Double-Applied working_dir". Code: `commands/stage/complete_verification.rs`
(`run` L36-49, `run_aggregated_check` L180-189, `aggregated_wiring`/`stage_wiring_gaps` L191-245;
review-harvest-gate added the review gate call after contract-phase's contract check).

Pinned from others: `verify::integrity::check(stage, work_dir, worktree, target_branch)` (W1),
`verify::impact_tests::run(stage, working_dir, criteria_config)` (W2),
`verify::goal_backward::reachable::verify_reachable(checks, graph)` and
`context::worktree_graph::build_for_worktree(working_dir)` (wiring-hardening).

## Files you own

`loom/src/verify/mod.rs` (declare `integrity` and `impact_tests`),
`loom/src/commands/stage/complete_verification.rs`,
`loom/src/commands/stage/complete_verification_v2.rs` (new),
`loom/src/commands/stage/complete_verification_v2_tests.rs` (new).

## Tasks

1. Move every v2 check into `complete_verification_v2.rs::run_v2(checks, target_branch)`: the
   contract check (contract-phase), the review gate (review-harvest-gate), the integrity gate
   (W1), impact-selected tests (W2, standard stages only, after the contract check), and the IV
   reachable re-verification below. `complete_verification::run` then makes exactly one v2 call,
   gated on `plan_version == 2`. The file shrinks, and later v2 checks go into the new file.
   Order: contract check, integrity gate, impact tests, review gate. Cheap, deterministic
   failures come first; the review gate is last because every edit made to fix an earlier
   failure needs a re-review.
2. IV reachable re-verification: for a v2 integration-verify stage, for every `Completed` stage
   whose definition has `reachable`, run `verify_reachable` against one worktree graph of the IV
   worktree. Build the graph from `worktree_root`, never from an already-resolved
   `acceptance_dir` joined with another `working_dir`: that is the double-applied `working_dir`
   trap. Gaps fail completion with the originating stage named.

## Named test (binding), in `complete_verification_v2_tests.rs`

- `aggregated_reachable_reverified_in_iv`: an IV stage and a completed stage whose `reachable`
  check names a symbol that the merged tree no longer reaches → `run_v2` fails naming that
  stage. Inject the graph or build a small temp crate.

## Proof (one command, once, after W1 and W2 report)

`cargo test --manifest-path loom/Cargo.toml --lib commands::stage::complete_verification_v2`

## Report

Files changed; `complete_verification.rs` line count before and after; the proof result.
