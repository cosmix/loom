# Deny-once read gate

The stage description defines the behaviour. The frozen contracts in `loom/tests/read_gate_contracts.rs` must pass. Never edit them. Read `loom-hooks/_read_discipline.sh`, `_read_ledger.sh` and `_containment.sh` first. All hooks must stay bash 3.2 compatible: see the header of `_read_discipline.sh`.

## Changes

**`_containment.sh`:** add `loom_read_gate_enabled`, returning 0 unless a `[hooks]` table in `$LOOM_WORK_DIR/config.toml` holds exactly `read_gate = false`. Share the awk table scan with `loom_deny_enabled` through a small helper rather than copying it. Note the inverted default: `loom_deny_enabled` answers yes only on an unambiguous `true`, and this answers no only on an unambiguous `false`. It is only ever consulted inside the live-stage check.

**`_read_ledger.sh`:** add `loom_read_deny_once <path> <message> <agent_id> <fallback_sid>`. This file has room (226 lines), and adding a new sourced helper would cost four hook-registration surfaces.

1. If the stage is not live or `loom_read_gate_enabled` fails, call `loom_hook_note_warn "$message"` and return 0. Live means `LOOM_STAGE_ID` and `LOOM_MAIN_AGENT_PID` are both set and `is_ancestor "$LOOM_MAIN_AGENT_PID"` succeeds, the same gate as `spawn-guard.sh:112`.
2. Compute the absolute path through `cd` and `pwd -P` in a subshell, for both channels, so `cat ../x` and a Read of the absolute path are one key, then the ledger with `_loom_ledger_file "read-denials" "$agent_id" "$fallback_sid"`.
3. If `_loom_polls_count "$ledger" "$abs"` is above 0, return 0. This repeat is allowed, with no warning. When live, check the ledger before running the outline, so an allowed repeat does not pay up to 2 s for `_loom_outline_covered_rows`.
4. If the ledger already holds `_LOOM_LEDGER_MAX_ROWS` rows, note the message as a warning and return 0. `_loom_ledger_append` trims to the newest rows, so appending to a full ledger would evict an earlier denial and deny that path again. Otherwise call `_loom_ledger_append "$ledger" "$abs"`, then count again. If the row is not there, note the message as a warning and return 0. `_loom_ledger_append` swallows every failure, including a symlinked directory, so only the re-count proves the denial was recorded.
5. Print `⛔ BLOCKED: <message>` plus the literal line `Repeating this read is allowed if you need the whole file.` on stderr, and `exit 2`.

**`_read_discipline.sh`:**

- Add `_loom_is_read_gate_exempt <path>`, matching `*/.loom/*`, `.loom/*`, a path under `$LOOM_WORK_DIR` (guard an empty value, or every absolute path matches), `*/doc/plans/*` and `doc/plans/*`. An exempt path skips rule 1 entirely and still reaches the sibling advisory and the ledger append.
- In `loom_read_discipline_check`, replace the `if [[ "$LOOM_RD_V1_KIND" == "deny" ]]` block so both verdict kinds go through `loom_read_deny_once`. Leave `loom_hook_deny_or_warn` as it is: `poll-guard.sh:90` and `:107` still use it with their `deny_enabled` meaning.
- The file is 343 lines. Keep it under 400.
- Reword `_loom_read_discipline_verdict1`'s uncovered message to "no outline available", because the 2-second bound on `_loom_outline_covered_rows` can time out on a covered file, and stop assuming Markdown in its `rg -n` advice.
- Keep `rg -n` and `offset/limit` in the no-outline warning, and `Outline` and `Read the ranges you need with offset/limit` in the covered message: existing tests assert them.
- Keep the kind names in `verdict1`, or simplify them if nothing else needs the distinction.
- Update the rule list in the comment above `loom_read_discipline_check`, and the exit-code comment in `read-guard.sh:16-21`.

**`poll-guard.sh`:** line 3's comment says only a configured live stage can deny, which stops being true: reword it.

## Traps

- **The exit happens before the reads ledger append.** Today a denied read never reaches `_loom_ledger_append` at the end of `loom_read_discipline_check`. A repeat that is allowed does reach it, which is correct: the sibling advisory should see it.
- **`poll-guard.sh:197-198` shares this core**, so a Bash `cat` denied once and retried through Read must pass. The key is path-based, so this holds. Add `cat_denied_once_then_read_allowed` to `hooks_poll_guard_reads.rs` with `setup_both_hooks`, a live PID and no config, in both orders.
- **`set -u` and empty values.** `_loom_polls_count` reads two columns from a row of path and timestamp: the shape `_loom_ledger_append` writes when given one field.
- **Tests.** `uncovered_file_warns_and_allows_even_with_deny_on` runs with no `LOOM_MAIN_AGENT_PID` and passes unchanged: do not edit it. With this brief applied to a scratch copy and no test edited, `hooks_read_guard` and `hooks_poll_guard` pass, so no existing test needs to change. `skip_unless_gate_visible` stays for the integration tests; a frozen contract never self-skips.

Do not raise `READ_GUARD_LINE_LIMIT`, and do not touch the sibling-read advisory.

## Check

Run `cargo test --manifest-path loom/Cargo.toml --test integration hooks_read_guard` and `--test integration hooks_poll_guard` once each. Report the result to the orchestrator.
