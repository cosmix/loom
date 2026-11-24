# fix1: review round 1 findings

Read `doc/plans/briefs/stage-exits-and-guards/common.md` first; its rules bind you. All of its
"Own", boundary, test and report rules apply. Four findings from the first review round, all
yours to fix. Locate every seam by symbol; line numbers are advisory.

## Own

- `loom/.githooks/pre-commit`
- `scripts/test-pre-commit-partial-staging.sh`
- `skills/loom-plan-writer/SKILL.md`
- `loom/src/commands/hook/idle_stop.rs`
- `loom/src/commands/hook/idle_stop_tests.rs`
- `loom/src/orchestrator/core/loop_recovery/park.rs` (and a new file beside it if it nears 400 lines)
- `loom/src/relay/emit/stderr_text.rs`

Read-only (frozen): `loom/src/commands/hook/idle_stop_contract_tests.rs`,
`loom/tests/ledger_hook_contracts.rs`. Do not edit any other file.

## Finding 1 (major): pre-commit loses the lowered ledger rows when the gate fails

`loom/.githooks/pre-commit`, around the `LOOM_LEDGER_TIGHTEN=1 cargo test` block (about lines
111-132). When a commit has growth in file A and a stale row B, tighten lowers B in the working
tree, validation then fails on A, and the hook exits (about line 118) before the staging block
(about 121-132). B's lowering stays an unstaged edit. After A is fixed and the commit retried,
`LEDGER_DIFF_STATUS` is 1 (unstaged edits), tighten changes nothing more, the before and after
hashes match, and the ledger is neither staged nor refused: HEAD keeps the stale row while the
working-tree test and pre-push pass, and CI fails.

Fix: the ledger-changed comparison and the staging step (only when the ledger had no unstaged
edits before the hook ran, `LEDGER_DIFF_STATUS` 0) must happen BEFORE the gate-failure exit, so a
failing gate still stages the lowering it made; the failure message stays accurate (it says the
rows were lowered and staged). A ledger that had unstaged edits stays refused and unstaged, as
today. Keep Bash 3.2 syntax. Add a case to `run_ledger_case` in
`scripts/test-pre-commit-partial-staging.sh` where the gate fails (growth in a second file) and
the lowered ledger is staged anyway; follow that script's existing case style. Do not change the
behavior the frozen `loom/tests/ledger_hook_contracts.rs` pins.

Proof: `bash -n loom/.githooks/pre-commit scripts/test-pre-commit-partial-staging.sh`. You cannot
run the script (no git); the orchestrator runs it.

## Finding 2 (minor): SKILL.md ratchet_files row contradicts the doctrine

`skills/loom-plan-writer/SKILL.md`, the `ratchet_files` table row (about line 252) says baseline
and ledger files change only through an accepted integrity dispute. The doctrine in
`skills/loom-plan-writer/references/v2-contracts.md` (about lines 117-119) and
`loom/src/orchestrator/signals/v2_section_review.rs` (about line 80) says lowering or removing a
row raises nothing and is always allowed; only loosening needs a dispute. Reword the row to say
exactly that, in the same words the reference uses. Keep the existing line
"A commit hook is a gate every stage must pass" untouched.

## Finding 3 (minor): `block_line` fact gathering has no test

`loom/src/commands/hook/idle_stop.rs`, `block_line` and its fact readers. Only `decide()` and the
marker are tested. Add tests in `idle_stop_tests.rs` that go through `block_line` (or the smallest
function that maps on-disk evidence to `StopFacts`) with a temp work directory holding a stage
file, a session file, a heartbeat and a host-job store, and pin: a host-job store read error maps
to live (session stops being blocked); only the stage-work and knowledge session types count (a
contract session type is never blocked); a latest heartbeat that is a subagent's or another
session's lets the session stop; an unknown native child disposition maps to `None`. Use the
fixture helpers the existing tests in that file and `loom/src/test_support` / `tempfile::TempDir`
provide; tests that change an environment variable carry `#[serial]`. If a seam is private and
needs a small `pub(crate)` or parameter change to be testable, make the smallest one.

## Finding 4 (minor): `children_quiet` duplicates existing code

`idle_stop.rs` (about lines 152-166) re-implements `Orchestrator::session_child_disposition` in
`loom/src/orchestrator/core/loop_recovery/park.rs` (about lines 69-87): native worker
disposition plus journal replay. That function needs only `work_dir`. Extract its body as a free
function in the same module (or a new sibling file when `park.rs` would pass 360 lines), keep the
`Orchestrator` method as a thin caller, and have `idle_stop.rs` call the free function, mapping
`NoChildren` to `Some(true)`, `Active` to `Some(false)`, `Unknown(_)` to `None`. Check with
`loom map --impact session_child_disposition` or `rg` that no other caller breaks. Remove the
duplicated code in `idle_stop.rs`.

## Suggestion (include it)

`loom/src/relay/emit/stderr_text.rs` (about line 39): the advice says a rerun on an unchanged
tree follows the same request. A replacement trial now follows its request on a changed tree too;
state that in one added clause. Any test that pins the old sentence changes with it (report it).

## Proof and report

Compile with `cargo check --all-targets` from `loom/` (use a PATH that avoids the build script's
git lookup if needed, as earlier workers did) and `bash -n` on shell files. Run no tests. Report
files changed, tests added, existing tests changed and why, and anything that contradicted this
brief.
