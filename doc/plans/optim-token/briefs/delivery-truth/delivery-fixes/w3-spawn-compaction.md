# W3 spawn-compaction: no briefs for Codex-bound spawns, a 5-second bound, no SubagentStart bind, a compaction reset for ordinary sessions

Read `doc/plans/token-optim/briefs/delivery-truth/delivery-fixes/common.md` first. Load `Skill(skill="loom-skills", args="loom-rust")` before editing the Rust test file. Paths are repository-relative.

## Read before editing

- `loom-hooks/spawn-guard.sh` lines 1-30 (header), 150-160 (`resolve_defined_tier` comment) and 266-330 (preamble block and the worker-brief block that starts with the comment "A scoped brief is optional"). Plan 3 rewrote header item 5 of this file before you run (it may have added one line); anchor on the quoted text, not on line numbers.
- `loom-hooks/subagent-start.sh` in full (141 lines).
- `loom-hooks/knowledge-orient.sh` in full (86 lines) and `loom-hooks/pre-compact.sh` lines 1-45.
- `loom/src/commands/hook/pre_compact.rs` and `loom/src/commands/hook/target.rs` (read only: `reset_for_payload`, `HookTarget::from_environment`, `HookTarget::for_checkout`).
- `loom-hooks/tests/subagent-start-ledger.sh`, `loom-hooks/tests/knowledge-orient-silent.sh`, and `loom/tests/integration/hooks_spawn_guard_gate.rs` (the helpers at the top and the last test, `codex_forwarder_keeps_marker_first_without_navigation_kit`).
- The frozen contracts you must turn green: `loom/tests/spawn_compaction_contracts.rs` (`codex_bound_spawns_receive_no_worker_brief`, `slow_worker_brief_still_reaches_the_spawn`, `compacted_ordinary_session_loses_only_its_prompt_record`).

## 1. `spawn-guard.sh`

The file is over the limit before you start: 403 lines at `4cc085b0` (commit `e747e9dc` added the own-root agents loop in `resolve_defined_tier`, after plan 3 was written against 398), and plan 3's item-5 rewrite may add one more. It must end at or under 400 (`wc -l`); the stage verifier fails it otherwise. Edits 1-4 add no line; edits 5-6 remove lines until the count is at or under 400. Never cut a code line other than the merge in edit 5. Report the final `wc -l`.

1. The `if` that guards the worker-brief call, `if [[ $GATE_PASSED -eq 1 ]] && command -v "${LOOM_BIN:-loom}" &>/dev/null &&`, becomes `if [[ $GATE_PASSED -eq 1 && "$AGENT_TYPE_REQ" != loom-codex-forwarder && "$AGENT_TYPE_REQ" != codex:* ]] && command -v "${LOOM_BIN:-loom}" &>/dev/null &&` on the same line (the right-hand `codex:*` is an unquoted glob, the same predicate the preamble `case` uses). The continuation line stays.
2. In the line that sets `WORKER_BRIEF_ENVELOPE`, `loom_run_bounded 3` becomes `loom_run_bounded 5`, the bound `user-prompt-context.sh` gives `loom hook user-prompt` (`timeout 5`).
3. The three comment lines above the block become three lines saying: a scoped brief is optional, and a missing command, a timeout, a bad envelope or an empty selection changes nothing; Codex-bound types get none, because Codex units read loom's Codex UserPromptSubmit brief (`fs/permissions/codex_hooks.rs`); the prompt is read from `TOOL_INPUT` so its trailing newlines survive the merge.
4. Header item 5 (plan 3's rewrite: "... inside a stage ... it also appends an optional scoped worker brief"): add "except to loom-codex-forwarder and codex:* spawns" to the worker-brief clause, rewrapping within the item's existing lines. After edit 1 the old text is false for Codex-bound types.
5. Merge the `elif [[ -z "$WORKER_BRIEF_ENVELOPE" ]]; then` branch and its `loom_debug "spawn-guard: worker-brief empty output"` line into the first test: `if ! WORKER_BRIEF_ENVELOPE=$(... loom_run_bounded 5 ... hook worker-brief 2>/dev/null) || [[ -z "$WORKER_BRIEF_ENVELOPE" ]]; then` with `loom_debug "spawn-guard: worker-brief timeout, nonzero exit or empty output"`. Saves 2 lines; behaviour is unchanged (both branches only log).
6. If the file is still over 400: in the comment above `resolve_defined_tier`, fold "<root>/agents/, then ~/.claude/agents/)," and "else a built-in table." onto fewer lines (saves 1); then, only if still over, shorten other comment blocks inside the worker-brief or preamble sections.

Why: forwarders received a brief of about 10,000 characters in 94 of 187 transcripts and relayed it in 18, and every loom stage spawn since 2026-10-01 hit the 3-second bound (`common.md` section 2).

## 2. `subagent-start.sh`

1. Delete the final `case "$TRANSCRIPT_PATH" in */subagents/agent-"$AGENT_ID".jsonl) ... esac` block that calls `loom hook worker-brief --bind-agent`. Keep `TRANSCRIPT_PATH`: the agent-id fallback above uses it.
2. Delete the header bullet "Binds a pending worker brief when a safe child transcript path is present".
3. Delete `source "$(dirname "$0")/_read_ledger.sh"` once `rg -n '_loom_|loom_run_bounded' loom-hooks/subagent-start.sh` shows nothing from it remains (`loom_read_input` comes from `_bounded.sh`, sourced further down; `loom_debug` from `_common.sh`).

## 3. `knowledge-orient.sh`

In the branch `if [[ "$SOURCE" == "compact" ]] || [[ "$SOURCE" == "resume" ]]; then`, before its `exit 0`, add:

```bash
 if [[ "$SOURCE" == "compact" ]] && command -v "${LOOM_BIN:-loom}" &>/dev/null; then
  printf '%s' "$INPUT_JSON" | LOOM_HOOK_CONTEXT=1 loom_run_bounded 3 "${LOOM_BIN:-loom}" hook pre-compact >/dev/null 2>&1 || true
 fi
```

with a two-line comment: a compaction may drop what this session's prompt delivery record still credits, and no PreCompact hook runs outside a stage, so the record is reset here (`loom hook pre-compact`). `loom_run_bounded` comes from `_bounded.sh`, already sourced. In the header's "Skipped entirely when" list, the `"compact" or "resume"` bullet gains: on `compact` the hook first resets this session's prompt delivery record through `loom hook pre-compact`. Every path still prints nothing. The script runs under `set -euo pipefail`; the `|| true` covers the whole pipeline.

How the reset finds the record: `loom hook pre-compact` resolves its target with `HookTarget::from_environment()` (`LOOM_WORK_DIR`, else the process working directory, which for this hook is the session's project directory), the same resolution `loom hook user-prompt` uses when it writes the record, and derives the recipient from the payload's `session_id` the same way (`hook_recipient_id`). The payload's `cwd` is not read. Codex registers `knowledge-orient.sh` for SessionStart only on `^(startup|clear)$` (`fs/permissions/codex_hooks.rs`), so the new branch never runs under Codex. A stage session exits at the `LOOM_STAGE_ID` check before this branch and keeps `pre-compact.sh`.

## 4. `pre-compact.sh`

Rewrite the comment above `source "$(dirname "${BASH_SOURCE[0]}")/_bounded.sh"` (from "Capture stdin" through "nothing here may ever block compaction over delivery-record bookkeeping."). The statement "this hook is registered globally regardless of LOOM_STAGE_ID" is false: it is registered only in stage worktree settings (`loom/src/hooks/config.rs`, `HookEvent::PreCompact`), and `fs/permissions/hooks/config.rs::build` registers no PreCompact hook. The new comment says: the payload goes to `loom hook pre-compact` before anything else, which deletes this session's own prompt delivery record so a post-compaction prompt can redeliver; it runs before the env-var guard; an ordinary session gets the same reset from `knowledge-orient.sh` on its SessionStart `compact` event; the delegate is fail-open and `|| true` guards the call. No code line changes.

## 5. Tests

- `loom-hooks/tests/subagent-start-ledger.sh`: rename `BIND_CALLS` to `LOOM_CALLS` (the stub and every `env` line). Replace the two bind-argv assertions with one, placed after the first run (the one with a safe child transcript path): `$LOOM_CALLS` holds no line containing `worker-brief` (a missing file passes). It fails at `6fee4ecd`, where that run passes the bind argv. Keep the ledger-row assertions and the no-parent case.
- `loom-hooks/tests/knowledge-orient-silent.sh`: case (c) passes `LOOM_BIN="$TMP/no-loom"` as an extra env assignment to `assert_silent`, so the compact case never reaches an installed `loom`. The case still asserts silence.
- `loom/tests/integration/hooks_spawn_guard_gate.rs`: convert `codex_forwarder_keeps_marker_first_without_navigation_kit` into `codex_forwarder_spawn_gets_no_prompt_rewrite`. Same setup and input (subagent_type `loom-codex-forwarder`, model `sonnet`, the `LOOM-CODEX-FORWARD-ONLY` prompt). A forwarder spawn now gets no `updatedInput`, but its first spawn of a session still prints the `loom-orchestration` advisory as `additionalContext`, so stdout is not empty. Assert exit 0; that stdout parses as JSON with no `hookSpecificOutput.updatedInput`; and that stdout contains neither `PREAMBLE_LINE`, nor `loom-worker-brief`, nor `LOOM_HOOK_WARN`. The file is at 400 lines: the converted test must be no longer than the one it replaces. Remove any helper the change orphans.
- Do not edit `loom-hooks/tests/run-all.sh`.

## Your one check

From the repository root, once, at the end: `bash loom-hooks/tests/subagent-start-ledger.sh && bash loom-hooks/tests/knowledge-orient-silent.sh && bash loom-hooks/tests/knowledge-orient-emits.sh`. It runs the hooks directly, so it does not wait on the crate W1 and the codex units are editing. The contracts and your Rust test edit run in the verifier round.

## Report

The files you changed, `wc -l` of `spawn-guard.sh`, `knowledge-orient.sh` and `hooks_spawn_guard_gate.rs`, and anything that did not match this brief.
