# W2 read-hooks: remove the receipt calls from the read hooks, reword the sibling advisory

Read `doc/plans/token-optim/briefs/delivery-truth/delivery-fixes/common.md` first. Load `Skill(skill="loom-skills", args="loom-rust")` before editing the Rust test files. Paths are repository-relative.

W1 deletes `loom hook read-receipt` and `loom/src/context/read_receipts*`. You remove every shell caller of that command, the read-discipline rule that depended on it, and the tests that pin it. The `reads` section of `loom usage` measures rereads from transcripts and is not touched. The sibling ledger (`_shared.tsv`) and its advisory stay; the advisory's wording and the comments that call the ledger "receipts" change, because a ledger row records that an agent asked for a file, never that the content reached it.

## Read before editing

- `loom-hooks/read-guard.sh` lines 95-114 (the `loom_read_discipline_check` call and the `_loom_read_receipt_eligible` block).
- `loom-hooks/_read_discipline.sh` lines 270-381 (`_loom_read_discipline_verdict2`, the rule list comment above `loom_read_discipline_check`, and that function) and the comment at lines 140-147.
- `loom-hooks/_read_ledger.sh` in full (307 lines) and `loom-hooks/_bounded.sh` lines 25-40.
- `loom-hooks/post-tool-use.sh` lines 215-262. Plan 3 renamed the post-commit reminder below this range (`remind_knowledge_update` became `remind_memory_capture`) before you run, and changed nothing in this range; re-read the file and anchor on the quoted text.
- `loom/tests/integration/hooks_read_guard.rs` in full, `hooks_read_guard_repeat.rs`, `hooks_read_guard_receipts.rs`, and `hooks_poll_guard_reads.rs` lines 55-100.
- `skills/loom-orchestration/SKILL.md` lines 115-125 (the **Read receipts.** paragraph).
- The frozen contract `loom/tests/read_intent_contracts.rs` (`read_guard_prepares_no_receipt_intent`).

## 1. `read-guard.sh`

1. The call `loom_read_discipline_check "$FILE_PATH" "$KIND" "$LINES" "$AGENT_ID" "${PAYLOAD_SID:-unknown}" "$INPUT_JSON"` loses its last argument, and the two comment lines above it ("The discipline core can use this original payload only for a bounded receipt check ...") go.
2. Delete the block from the comment "A hard deny exits from the discipline core before reaching this point." through the `fi` that closes `if _loom_read_receipt_eligible "$FILE_PATH"; then ... _loom_read_receipt_prepare "$INPUT_JSON"`. Keep the final `loom_hook_emit_warns` and `exit 0`.

## 2. `_read_discipline.sh`

1. Delete `_loom_read_discipline_verdict2` and its comment block.
2. `loom_read_discipline_check`: the signature comment becomes `<path> <kind:full|range> <lines> <agent_id> <fallback_session_id>`; the `payload="${6:-}"` local goes; the `_loom_read_discipline_verdict2` call and both `LOOM_RD_V2_*` branches go; the sibling condition `[[ "$LOOM_RD_V1_KIND$LOOM_RD_V2_KIND" == "nonenone" ]]` becomes `[[ "$LOOM_RD_V1_KIND" == "none" ]]`.
3. The rule list in the comment above the function: delete item 2 (the receipt-proven repeat). Keep the other items' numbers (0, 0b, 1, 3, 4) so references elsewhere stay valid. Item 3 "OVERRIDES rules 1 and 2" becomes "OVERRIDES rule 1"; item 4 "when rules 1 and 2 are silent" becomes "when rule 1 is silent". The paragraph "Rules 1 and 2 are both computed ... rule 1 wins a tie" becomes one sentence: rule 1 is computed with `_loom_read_discipline_verdict1`, then at most one decision is queued.
4. The comment ending "(_read_ledger.sh's sibling-read receipts)." becomes "(_read_ledger.sh's sibling-read advisory)."

## 3. `_read_ledger.sh` and `_bounded.sh`

1. Delete `_loom_read_ranges_overlap`, `_loom_read_attempt_overlaps` (with the comment above them), `_loom_read_receipt_eligible`, `_loom_read_receipt_run`, `_loom_read_receipt_proven`, `_loom_read_last_full_attempt_at` and `_loom_read_receipt_prepare`. Confirm with `rg -n '_loom_read_attempt_overlaps|_loom_read_ranges_overlap|_loom_read_last_full_attempt_at|_loom_read_receipt' loom-hooks loom/tests` that no caller remains.
2. Comments that describe the removed path:
   - Header (lines 2-6): the file holds the TSV ledger read/write/cap helpers for `_read_discipline.sh` (poll counters and the sibling-read scan across one session directory's ledgers) and sources `_bounded.sh`. Name the files that source it after this stage: check with `rg -n '_read_ledger.sh' loom-hooks`; W3 removes the source line from `subagent-start.sh`, so it is not one of them.
   - `_LOOM_LEDGER_MAX_ROWS` comment: "Read receipts use this only as a bounded overlap prefilter; poll counters also scan it." becomes "Poll counters and the sibling-read scan read it."
   - `_loom_read_skip_extension` comment: it marks a binary or image format that text-oriented read advice skips; delete the sentence "Kept beside _loom_read_receipt_eligible ...".
   - `_loom_ledger_append` comment: "outside a stage it is $TMPDIR/loom-reads, which loom's Rust receipt store also walks and rejects when it is exposed." becomes "outside a stage it is $TMPDIR/loom-reads, shared by every session of the user."
   - The block comment that starts "Sibling-read receipts. Every agent of one session" starts "Sibling-read advisory." instead, and its "already read whole" becomes "already requested whole". The same change in the comment above `_loom_read_sibling_receipt` ("that a sibling agent already read whole").
   - `_loom_sibling_full_read` comment: "this agent has read the current file whole itself, so it was told once already or the repeat is rule 2's." loses "or the repeat is rule 2's".
3. The advisory `printf` in `_loom_read_sibling_receipt` becomes exactly:

   ```bash
   printf '%s (%s lines) was requested whole by %s%s (an attempt, not proof of delivery) and is unchanged since - run `loom map --outline %s`, then Read only the range you need with offset/limit.' \
   ```

   with the same arguments. Keep the function and variable names.
4. `_bounded.sh`: the comment line "Shared here so PreToolUse and PostToolUse receipt calls get the same bound." becomes "Shared here so every hook delegate gets the same bound."

## 4. `post-tool-use.sh`

1. Delete the `READ_FILE_PATH=""` assignment and the `if [[ "$TOOL_NAME" == "Read" ]]; then READ_FILE_PATH=... fi` block after it. `TOOL_INPUT` stays (the Write/Edit branch uses it).
2. Delete the block from the comment "Result content is intentionally not parsed or stored in shell." through the `fi` that closes the `hook read-receipt --complete` call.
3. Leave the `loom hook forward-receipt` block exactly as it is (plan 2 edits it), and keep the comment "Tool results are not persisted ...".
4. Keep `source "$(dirname "$0")/_read_ledger.sh"`: the file still uses `loom_run_bounded`, which arrives through it.

## 5. Orchestration skill

`skills/loom-orchestration/SKILL.md`, the paragraph that starts `**Read receipts.**`: the label becomes `**Sibling-read ledger.**`, and "that a sibling already read whole and unchanged" becomes "that a sibling already requested whole and unchanged". Change nothing else in the file: plan 5 slims it later and measures it against `6fee4ecd`.

## 6. Hook tests (bash)

- `loom-hooks/tests/post-tool-use-tool-event.sh`: remove `LOOM_RECEIPT_PAYLOAD` and the stub's `hook read-receipt --complete` branch (the stub only logs `$*` to `$LOOM_CALLS`). Keep the heartbeat and `tool-events.jsonl` checks. Replace the three receipt assertions with one: after both Read payloads (the text file and the PNG), `$LOOM_CALLS` holds no line matching `read-receipt` (a missing file passes). It fails at `6fee4ecd`, where the text-file Read invokes completion.
- `loom-hooks/tests/post-tool-use-empty-output.sh`: delete the final `read-receipt --complete` assertion block; it can no longer fail.
- `loom-hooks/tests/read-guard-sibling-receipts.sh`: the pin `*"$BIG (250 lines) was read whole by agent agent-a"*` becomes `*"$BIG (250 lines) was requested whole by agent agent-a (an attempt, not proof of delivery)"*`; the header's "already read whole" becomes "already requested whole"; the `read_as` comment "(no outline, no receipt proof)" becomes "(no outline)". The `and 1 other agent(s)` pin stays.
- `loom-hooks/tests/read-guard-session-ledgers.sh`: `*"was read whole by the main agent"*` becomes `*"was requested whole by the main agent"*`, `contains("read whole by agent agent-a")` becomes `contains("requested whole by agent agent-a")`, and the header's "(loom's receipt store rejects an exposed $TMPDIR/loom-reads)" becomes "(the directory is shared by every session of the user)".
- Do not edit `loom-hooks/tests/run-all.sh`: every test you change keeps its file name and its `run_test` line.

## 7. Rust integration tests

- `loom/tests/integration/hooks_read_guard_receipts.rs`: delete the file (`rm`), and delete `#[path = "hooks_read_guard_receipts.rs"] mod receipts;` from `hooks_read_guard.rs`.
- `loom/tests/integration/hooks_read_guard.rs`:
  - Module doc: "Integration tests for the read-guard PreToolUse hook."
  - `write_loom_stub`: drop the `hook read-receipt` branch, so the stub prints the outline body and the stub warning. Remove `loom_bin_path` from the `helpers` import if nothing else uses it.
  - Rename `receipt_stub_dir` to `lexical_stub_dir` (directory name `stub-lexical`) here and in `hooks_read_guard_repeat.rs`.
  - Delete `Session::receipt_call_log`, `with_receipt_call_log` and the `LOOM_RECEIPT_CALLS` env line in `run_payload`.
  - `binary_extension_skips_repeat_rule_too` becomes `repeated_binary_reads_stay_silent`: the same three reads and their assertions, with no call log and no final `calls.exists()` assertion.
- `loom/tests/integration/hooks_read_guard_repeat.rs`: module doc "Rule-one, ledger-row and tier-one knowledge-hint cases for `hooks_read_guard.rs`. The parent owns the shared hook harness."
- `loom/tests/integration/hooks_poll_guard_reads.rs`: the comment on test 4b ("Escalation needs a Rust-proven receipt, which a Bash-side `cat` cannot produce, even though both attempts share the ledger.") becomes "A Read after the `cat` stays silent: no rule advises a repeat read."; the assertion message "Read after cat needs a proven receipt to warn" becomes "Read after cat must stay silent".

## Your one check

From the repository root, once, at the end: `bash loom-hooks/tests/post-tool-use-tool-event.sh && bash loom-hooks/tests/post-tool-use-empty-output.sh && bash loom-hooks/tests/read-guard-sibling-receipts.sh && bash loom-hooks/tests/read-guard-session-ledgers.sh`. It runs the hooks directly, so it does not wait on the crate W1 and the codex units are editing. Your Rust test edits compile in the main agent's build after every worker returns.

## Report

The files you changed and deleted, the final line counts of `_read_discipline.sh`, `_read_ledger.sh`, `read-guard.sh` and `post-tool-use.sh`, and anything that did not match this brief.
