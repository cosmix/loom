# fix2: review round 2 findings

Read `doc/plans/briefs/stage-exits-and-guards/common.md` first; its rules bind you. Three findings, all yours. Locate seams by symbol.

## Own

- `loom/src/commands/hook/idle_stop.rs`
- `loom/src/commands/hook/idle_stop_tests.rs`
- `loom/.githooks/pre-commit`
- `scripts/test-pre-commit-partial-staging.sh`
- `CONTRIBUTING.md`

Frozen, read-only: `loom/src/commands/hook/idle_stop_contract_tests.rs`, `loom/tests/ledger_hook_contracts.rs` (both must still pass unchanged). Edit no other file.

## Finding 1 (major): idle-stop gives up when the newest heartbeat is a subagent's

`idle_stop.rs`, `block_line` (about line 134): it returns `None` whenever the stage's current heartbeat has `subagent == true`, before the marker is read. `SubagentStop` and every subagent tool call write `subagent: true` into the same `heartbeat/<stage>.json`. Scenario: the main agent spawns a background subagent; its stop writes the newest heartbeat; the main agent's notification turn answers in text only and ends; children are quiet, no host job is live, no marker exists, yet nothing blocks and the unattended session ends idle (the incident this unit prevents).

Fix: keep the stage and session-id match of the heartbeat (a heartbeat of another session or stage still lets the session stop), drop the early return on `subagent`, and compute `worked_since_last_block` as: true when no marker exists; otherwise true when the heartbeat `timestamp` differs from the one recorded in the marker. The 3-block cap and `stop_hook_active` still bound repeats. Update the test `a_newer_heartbeat_from_a_subagent_or_another_session_lets_the_session_stop` in `idle_stop_tests.rs`: another session's heartbeat still stops; a subagent heartbeat of this session with no marker blocks, and with a marker holding the same timestamp does not.

## Finding 2 (major): pre-commit stages a ledger lowered from unstaged work

`loom/.githooks/pre-commit`, the `LOOM_LEDGER_TIGHTEN=1 cargo test` block and the staging step (about lines 107-124). Tighten measures the working tree; the partial-staging guard only covers staged paths. Scenario: the ledger has `file src/a.rs 450`; the developer shrinks `src/a.rs` to 420 but leaves it unstaged and commits only `src/b.rs`. Tighten lowers the row to 420 and the hook stages the ledger; the commit holds the 450-line `a.rs` with a 420 row and CI's strict maintainability test fails.

Fix: stage the lowered ledger only when no tracked file under `loom/` other than the ledger itself has unstaged edits (for example `git diff --quiet -- loom/ ':!loom/maintainability-baseline.txt'`, adapted to how the hook already names paths and runs git; Bash 3.2 syntax). When tracked files have unstaged edits, leave the lowered ledger unstaged and print one line saying it must be committed together with the change that shrank the code. The gate-failure exit stays as it is now (staging before it when allowed). Keep every behavior `loom/tests/ledger_hook_contracts.rs` pins: an edit staged alone with a clean tree stages the ledger; a ledger with unstaged edits refuses the commit and is not staged. Add a case to `run_ledger_case` in `scripts/test-pre-commit-partial-staging.sh`: a second tracked file under `loom/` has an unstaged edit, the lowered ledger is not staged.

Also: when the ledger has unstaged edits AND the gate also failed, the hook exits before printing the maintainability failure message (about line 119). Print the gate failure message in that path as well.

## Finding 3 (minor): CONTRIBUTING.md

`CONTRIBUTING.md` (about line 116) says the hook stages the updated ledger after the gate passes. State the real rule: it stages the lowered ledger whenever it lowered rows and nothing it measured is unstaged (also when the gate then fails), refuses a ledger with unstaged edits, and leaves the ledger unstaged with a notice when tracked files under `loom/` have unstaged edits.

## Proof and report

`bash -n` on the shell files; `cargo check --all-targets` from `loom/` (a PATH avoiding the build script's git lookup, as earlier workers did). Run no tests. Report files changed, tests added or changed and why, anything that contradicted this brief.
