# W2: hook doctrine and hook checks

Read `doc/plans/token-optim/briefs/doctrine-truth-and-plan-guards/doctrine-and-guards/common.md` first.

## Files you own

- `loom-hooks/post-tool-use.sh`
- `loom-hooks/commit-guard.sh`
- `loom-hooks/learning-validator.sh`
- `loom-hooks/session-start.sh`
- `loom-hooks/spawn-guard.sh`
- `loom-hooks/tests/commit-guard-memory-reminder.sh`
- `loom-hooks/tests/post-tool-use-commit-reminder-tokenized.sh`
- `loom-hooks/tests/learning-validator-stage-journal.sh` (new)
- `loom-hooks/tests/session-start-compact.sh`
- `loom-hooks/tests/run-all.sh`

Hooks must run under bash 3.2 and with BSD tools (macOS): `run-all.sh` has a BSD mode (`LOOM_HOOK_TEST_BSD=1`) that pads `wc` output. Keep `wc -l <"$file" | tr -d '[:space:]'` wherever a line count is compared. Read `doc/loom/knowledge/mistakes/hooks-shell-portability.md` sections "Hook Tests Inherit the Live Session's Environment" and "BSD `wc` Padding ..." before writing a test.

## 1. Post-commit reminder (`post-tool-use.sh`)

`remind_knowledge_update` (about lines 269-298) tells every stage type to run `loom knowledge update mistakes`, which implementation and integration-verify signals forbid, and its item 3 restates the distill procedure. Hooks have no stage-type variable, so the text stays type-neutral.

- Rename the function to `remind_memory_capture` and update its one call (inside the `if [[ "$TOOL_NAME" == "Bash" ]] && [[ -n "$COMMAND" ]]` block, after `loom_tokens_cmd_has_arg_pair 'loom' 'stage' 'commit'`).
- Rewrite the comment header above it to `# === POST-COMMIT MEMORY REMINDER ===` / `# After a \`git commit\` or \`loom stage commit\`, print a non-blocking memory reminder.`
- Replace the heredoc body with this box (keep the right border aligned with the top border, as the current box does):

```text
┌────────────────────────────────────────────────────────────────────┐
│  📝 POST-COMMIT REMINDER: Record Memory                            │
├────────────────────────────────────────────────────────────────────┤
│                                                                    │
│  You just committed. Record what this work taught you:             │
│     loom memory note "mistake: ... Prevention: ..."                │
│     loom memory note "found: ..."  or  "gotcha: ..."               │
│     loom memory decision "chose X because Y" --context "details"   │
│                                                                    │
└────────────────────────────────────────────────────────────────────┘
```

The literal `POST-COMMIT REMINDER` must stay: `tests/post-tool-use-commit-reminder-tokenized.sh` greps it. In that test's case (b) (a real `git commit -m wip`), add an assertion that stderr does not contain `loom knowledge`, and update its header comment to say the reminder is about memory. Touch nothing between the `LOOM_STAGE_ID` guard (about line 229) and the reminder: plans 1 and 2 rewrite the receipt calls there (lines about 248-260).

## 2. Commit-guard checks (`commit-guard.sh`)

- `remind_knowledge_capture` (about lines 331-353) checks `$project_root/$WORK_DIR/knowledge`, a directory that never exists, so it never prints. Delete the function, its two comment lines, and its call with the comment above it in `main()` (about lines 450-451).
- `remind_memory_usage` (about lines 355-394) loops over every `"$memory_dir"/*.md`, so any stage's journal silences the reminder, and its `stage_id` argument is unused. Make it read only `"$memory_dir/$stage_id.md"`: when `stage_id` is empty or contains `/`, return; when that file exists and has more than 10 lines (same `wc -l | tr -d '[:space:]'` count), stay quiet; otherwise print the reminder. Change `No memory entries recorded for this session.` to `No memory entries recorded for this stage.` Keep the heading `⚠️  Memory Recording Reminder` and the `loom memory decision "choice" --context "why"` line (the test greps both).
- The hook stays advisory: it must still reach `COMPLETION CHECKLIST for stage '<id>'`.

`tests/commit-guard-memory-reminder.sh`: every case writes `"$MEMORY_DIR/$STAGE_ID.md"` (that is, `build-api.md`) in place of `session.md`. Add, before the final `echo "PASS"`, a case that removes the stage's journal, writes 120 lines to `"$MEMORY_DIR/other-stage.md"`, and expects the reminder ("another stage's journal does not count"). Update the header comment to say the reminder reads the stage's own journal.

## 3. Learning validator (`learning-validator.sh`)

`check_memory_usage` (about lines 56-88) counts files with a `^##` line, which only the memory summary writer produced; that writer is deleted in this stage, so the check would warn in every session. Journal entries are `###` headings (`### 📝 Note [2026-09-12 20:38:06] id=...`).

- After the environment guard, add the same id guard `post-tool-use.sh` uses: `case "$LOOM_STAGE_ID" in *[!A-Za-z0-9._-]* | "") exit 0 ;; esac`.
- `check_memory_usage` reads only `"${LOOM_WORK_DIR}/memory/${LOOM_STAGE_ID}.md"` and warns when that file is missing or holds no line starting `###` (`grep -q '^### '`). Drop the `shopt` loop. Change the box's first text line to `No memories recorded for this stage`, keeping the box borders aligned. It stays advisory and always exits 0.
- Update the header comment's check 1 description.

New test `tests/learning-validator-stage-journal.sh`, modelled on `tests/post-tool-use-commit-reminder-tokenized.sh` (scratch dir from `mktemp -d "${TMPDIR:-/tmp}/loom-hooktest.XXXXXX"`, `trap` cleanup, the hook run with `env -u` scrubbing of `LOOM_*` and explicit `LOOM_STAGE_ID=s1 LOOM_SESSION_ID=sess LOOM_WORK_DIR=$TMP/work`, stdin `{}`, stderr captured). Cases, each with a fresh memory dir:

1. `memory/s1.md` holds `# Memory Journal: s1` and one `### 📝 Note [2026-10-18 10:00:00] id=abc` entry: no `No memories recorded` in stderr.
2. No `memory/s1.md`: the warning prints.
3. `memory/s1.md` holds only the `# Memory Journal: s1` header: the warning prints.
4. Only `memory/other.md`, holding an entry: the warning prints.

The test prints `PASS` and exits 0 on success, `FAIL: <case>` and exits 1 otherwise.

## 4. Recovery text (`session-start.sh`)

The compact/resume `additionalContext` (about line 153) says `run 'loom memory list'`, which lists every journal in the plan. Change it to `run 'loom memory list --stage ${LOOM_STAGE_ID}'` (the hook has already validated `LOOM_STAGE_ID` at about line 37). In `tests/session-start-compact.sh`, add an assertion after the signal-path check that the additionalContext contains `loom memory list --stage test-stage` (use `grep -qF`, as the file's other checks do).

## 5. Spawn-guard header (`spawn-guard.sh`)

Header item 5 (lines 19-20) says the guard prepends the preamble to every typed spawn and appends a worker brief, in every session. Two parts are false: the preamble is skipped for `loom-codex-forwarder` and `codex:*` spawns (the `case` around `spawn-guard.sh:271-290`), and the brief is appended only past the stage gate (`GATE_PASSED`, about lines 108-111). Rewrite item 5 to say: the guard PREPENDS `_subagent-preamble.txt` to a typed spawn's prompt that lacks it, in every session, except `loom-codex-forwarder` and `codex:*` spawns; inside a stage (the `GATE_PASSED` gate below) it also APPENDS an optional scoped worker brief. Do not say Codex-bound spawns skip the worker brief: at this stage they still get it (plan 1 adds that exclusion). Use at most three lines for the item. The file is 403 lines at `4cc085b0` (398 at `6fee4ecd`; commit `e747e9dc` added five lines in `resolve_defined_tier`), so it ends at 403 or 404; plan 1 trims it to 400 or fewer. Change nothing else in this file: plan 1 edits its brief append and trims it later. Report the final `wc -l`.

## 6. Register the new test (`tests/run-all.sh`)

- Change the description of the `commit-guard-memory-reminder.sh` line to `commit-guard: the memory reminder reads this stage's journal, shows for thin or absent, hides for 11+ lines, under padded wc`.
- Add directly after it: `run_test "learning-validator: warns unless this stage's own journal holds an entry" "$SCRIPT_DIR/learning-validator-stage-journal.sh"`.

Add no other line: later plans run this suite and expect 103 tests.

## 7. Notes for every edit above

- In the default relay mode the daemon appends a recorded entry to the journal when it drains the relay (`doc/loom/knowledge/architecture/memory-spool.md`, "The Design"), so a Stop hook can read a journal that does not yet hold a note recorded seconds earlier. Both checks stay advisory; that spurious reminder is accepted.
- W7's doctrine test (wave 2) reads every hook line whose text, after leading whitespace and box characters (`│`), starts with `loom`, and resolves it against the CLI. Every such line you write in a box must be a real command (`loom memory note ...`, `loom memory decision ...`).

## Check (run once, at the end, from the repository root)

```bash
bash loom-hooks/tests/run-all.sh
```

Report the result line. The verifier runs `./scripts/check-hook-syntax.sh` and the BSD mode.
