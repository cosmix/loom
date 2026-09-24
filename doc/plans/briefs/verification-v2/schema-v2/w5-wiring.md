# schema-v2 / W5 — v2 wiring semantics and version threading

Read first: `doc/plans/briefs/verification-v2/DESIGN.md` D0, D11 (the glob and literal bullets
only; definition-site exclusion and `reachable` belong to the `wiring-hardening` stage).
Knowledge: `mistakes/pinned-literals-ledgers-and-wiring.md` "Goal-Backward Wiring Checks Pin a
PATTERN to a PATH"; `mistakes/doctrine-and-acceptance.md` "Aggregated Wiring Re-Verification:
Double-Applied working_dir".

Wave 2: W1's `WiringCheck.literal` and `Stage.plan_version` exist.

## Files you own

`loom/src/verify/goal_backward/wiring.rs`, `loom/src/verify/goal_backward/wiring_v2.rs` (new),
`loom/src/verify/goal_backward/mod.rs`, `loom/src/commands/stage/complete_verification.rs`,
`loom/src/commands/verify.rs`.

## Tasks

1. `verify_wiring(wiring, working_dir)` (wiring.rs:23-87; the function is ledgered at 65 lines)
   becomes `verify_wiring(wiring, working_dir, plan_version: u32)`. For `plan_version == 1`
   the behaviour is byte-for-byte today's. For 2, dispatch to `wiring_v2.rs`, which implements
   the glob `source` rule and `literal` escaping of DESIGN D11 with the `glob` crate (already a
   dependency). Reuse today's bounded read (`fs::safe_read::read_to_string_bounded`, 10 MB, with
   outbound-symlink rejection) and the regex builder. A glob result outside `working_dir` after
   canonicalisation is skipped. Restructure `verify_wiring` into helpers so it ends at or below
   its ledger count; lower or remove the ledger entry and report the exact new count.
2. Thread `plan_version` to every call site:
   - `verify/goal_backward/mod.rs:29` `run_goal_backward_verification` gains a `plan_version`
     parameter;
   - `commands/verify.rs:168` (its caller, inside ledgered `execute`, 89 lines) passes the
     runtime stage's `plan_version`;
   - `commands/stage/complete_verification.rs:240` (aggregated IV wiring) passes
     `checks.stage.plan_version` (every stage of one plan shares the version);
   - grep for any other caller (`rg -n "run_goal_backward_verification|verify_wiring\(" loom/src loom/tests`)
     and thread it there too.

   Pay for lines added inside a ledgered function by extraction, and report the counts.

## Named tests (binding), in `wiring_v2.rs`'s test module (declare it with `#[path]` if the file

would pass 400 lines)

- `v2_glob_source_matches_any_file`: `source: "src/**/*.rs"` where only `src/a/b.rs` contains the
  pattern → no gap.
- `v1_glob_source_stays_a_literal_path`: the same check with `plan_version = 1` → the gap "Wiring
  source file missing" exactly as today.
- `v2_literal_pattern_matches_metacharacters_literally`: `pattern: "run("`, `literal: true`,
  file containing `run(` → no gap; the same with `literal: false` → a gap (today's invalid-regex
  gap).

## Proof (one command, once)

`cargo test --manifest-path loom/Cargo.toml --lib verify::goal_backward::`

## Report

Files changed; exact new counts for `verify_wiring`, `execute` and any other ledgered item; every
call site you threaded; the proof result.
