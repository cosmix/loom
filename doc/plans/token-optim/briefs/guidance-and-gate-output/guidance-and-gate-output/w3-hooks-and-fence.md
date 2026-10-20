# W3: main-session size advisory, heredoc message, integration-verify fence

Tier: sonnet (`loom-software-engineer`). Working directory for cargo: `loom/`. Paths below are repo-relative.

## Files you own

- `loom-hooks/post-tool-use.sh`
- `loom-hooks/loom-control-complete.sh` (`completion_rejection_reason` only)
- `loom-hooks/tests/loom-control-complete-heredoc.sh` (one assertion)
- `loom-hooks/_subagent-preamble.txt` (the integration-verify exception only)
- `loom/src/orchestrator/signals/tests_doctrine_prefixes.rs` (one new test)

Read-only: `loom/tests/guidance_gate_contracts.rs` (frozen contract file; never edit it), `loom-hooks/_post-tool-heartbeat.sh:92-116`, `loom-hooks/_read_discipline.sh:127-157` (`_loom_sanitize_agent_id`, `_loom_ledger_file`), `loom-hooks/_read_ledger.sh:85-120,198-209` (`_loom_ledger_append`, `_loom_polls_count`), `loom-hooks/_common.sh:1433-1475` (`loom_payload_agent_verdict`), `loom-hooks/loom-relay.sh:26-30` (the PostToolUse `additionalContext` emitter shape), `doc/loom/knowledge/mistakes/hooks-shell-portability.md` (bash 3.2 and BSD traps).

## Earlier plans touch `post-tool-use.sh` first

Plans 3, 1 and 2 of this programme edit `loom-hooks/post-tool-use.sh` before this plan runs: plan 3 renames `remind_knowledge_update` (`:269-298` at 6fee4ecd) to `remind_memory_capture` and deletes items of its `POST-COMMIT REMINDER` box, plan 1 deletes the read-receipt completion branch (`:254-260`), and plan 2 changes the `loom hook forward-receipt` call (`:248-252`) to pipe the payload. Re-read the whole file before editing and anchor by function name and comment text. At 6fee4ecd the file is 348 lines; after those plans expect about 330. Keep it at or under 400 lines (CLAUDE.md Rule 17) and every function under 50.

## Part A: the main-session size advisory (`loom-hooks/post-tool-use.sh`)

Operator decision of 2026-09-18: when a stage's main session changes more than 20 lines or more than 2 files itself (BLOCK-B point 1), it gets one advisory. It never denies, and no hook script or registration is added: `post-tool-use.sh` already runs on every tool call of a stage session (`"*"` matcher, `loom/src/hooks/config.rs:152-160`), and hooks run outside the sandbox, so they can write `.loom/work/hooks/`.

Add one commented section with two functions, placed after the `=== EDIT RECORDING ===` block, and call both on the line before the script's final `exit 0` (after the context-ceiling checks, which `exit 2` and would discard stdout JSON):

```bash
_loom_main_edit_tally
_loom_main_edit_advisory
```

`_loom_main_edit_tally` returns 0 without writing unless every condition holds:

1. `TOOL_NAME` is `Write`, `Edit`, `MultiEdit` or `NotebookEdit`, and `jq` is on PATH. Do not require the `loom` binary: the edit-recording branch above does, the tally must not.
2. `LOOM_SESSION_TYPE` is exactly `stage` (merge, base_conflict, knowledge, adjudication and contract sessions are excluded; `relay/emit/session_type.rs:13-23` lists the spellings).
3. `IS_SUBAGENT` is `0` (set by `loom_post_tool_write_heartbeat`) and the payload's `.agent_id` is empty.
4. `LOOM_WORKTREE_PATH` is set, and the edited path (`.tool_input.file_path`, or `.tool_input.notebook_path` for NotebookEdit) lies under `${LOOM_WORKTREE_PATH%/}/` and not under `${LOOM_WORKTREE_PATH%/}/.loom/`. Briefs the main agent writes to `$LOOM_SCRATCH_DIR` do not count.

Then it counts the changed lines with one `jq` call over `TOOL_INPUT`, using this definition (state it in the section's comment):

```text
def n: if type == "string" and length > 0 then (split("\n") | length) else 0 end;
def pair: [(.old_string | n), (.new_string | n)] | max;
if (.edits | type) == "array" then ([.edits[] | pair] | add // 0)
elif has("content") then (.content | n)
elif has("new_source") then (.new_source | n)
else pair end
```

(an Edit counts the larger side of its replacement, MultiEdit sums its edits, Write counts the whole content, NotebookEdit its new source), and appends one row with `_loom_ledger_append "$ledger" "edit" "$path" "$lines"`, where `ledger=$(_loom_ledger_file "mainedits" "main" "$LOOM_SESSION_ID")`, which resolves to `${LOOM_WORK_DIR}/hooks/mainedits/<session>/main.tsv`. The helpers already create mode-0700 directories and refuse symlinks.

`_loom_main_edit_advisory` returns 0 silently when the ledger is unreadable or already holds a `warned` row (`_loom_polls_count "$ledger" "warned"`). Otherwise it sums the `edit` rows' line counts and counts distinct paths, with a `while IFS=$'\t' read -r kind path lines` loop (bash 3.2 has no associative arrays: track seen paths in a newline-delimited string). When the sum is over 20 or the path count over 2, it appends a `warned` row first, then prints one JSON object on stdout, built as `loom-relay.sh:26-30` builds it:

```bash
jq -nc --arg message "$message" '{hookSpecificOutput: {hookEventName: "PostToolUse", additionalContext: $message}}'
```

with `message` exactly:

```text
LOOM_HOOK_WARN: this main session has itself changed ${lines} lines across ${files} file(s). Past 20 changed lines or 2 files the main agent delegates (CLAUDE.md Rule 7, hard stop 6), unless the stage signal says the stage runs single-agent. Advisory only; nothing was blocked.
```

The contract looks for `7 lines across 3 file(s)` and `25 lines across 1 file(s)`.

Stdout, never stderr: on exit 0 Claude Code passes a PostToolUse hook's JSON `additionalContext` to the model and does not pass its stderr. Every path through both functions returns 0; neither may change the script's exit code. Under `set -euo pipefail` a function that ends in `cond && action` aborts the script (`hooks-shell-portability.md`, "A Bash Function Ending in `cond && action` Aborts a `set -e` Script"): end each function with an explicit `return 0`.

The frozen contract `main_session_edit_advisory_fires_once_past_the_limit` (`loom/tests/guidance_gate_contracts.rs`) drives exactly this behaviour; read it before writing code.

## Part B: the heredoc refusal message (`loom-hooks/loom-control-complete.sh`)

`heredoc_openers_are_plain` (`:154-170`) refuses unquoted heredoc openers, because an unquoted body expands `$(...)`. The refusal stays; today its message is a generic reason from `completion_rejection_reason` (`:216-234`). The file is 399 lines, so the change is two lines at most:

1. Fold a regex into the existing `local` line: `local trusted=$1 heredoc_re='(^|[^<])<<-?[[:space:]]*[A-Za-z_]'`.
2. As the function's first check, before `ATTEMPT_TOKENIZE_FAILED`:
   `[[ ! "$COMMAND" =~ $heredoc_re ]] || { printf '%s' "an unquoted heredoc delimiter expands \$(...) in its body; quote the delimiter: <<'EOF'"; return; }`

`[^<]` keeps a here-string (`<<<word`) from matching; a quoted (`<<'EOF'`, `<<"EOF"`) or escaped (`<<\EOF`) delimiter does not start with a letter or `_`, so it does not match either. An arithmetic shift (`$((1<<x))`) does match and gets the heredoc advice; the command is still refused, which is all its test asserts. Keep the regex in the variable (a literal regex inside `[[ =~ ]]` parses differently across bash versions). The file must end at or under 400 lines.

In `loom-hooks/tests/loom-control-complete-heredoc.sh`, after `expect_refused "an unquoted delimiter whose body expands a command"` (`:183`), add, in the shape of `:145`:

```bash
[[ "$PRE_OUTPUT" == *'quote the delimiter'* ]] || fail "unquoted heredoc reason: $PRE_OUTPUT"
```

## Part C: the integration-verify fence (`loom-hooks/_subagent-preamble.txt`)

The preamble's exception (`:18-20`) tells every integration-verify subagent to run the full build, suite and linter, while the IV prefix (`INTEGRATION_VERIFY_OVERRIDE`, `loom/src/orchestrator/signals/cache.rs:115-126`) assigns ONE canonical verifier and has other reviewers run targeted checks. Replace those three lines with exactly:

```text
- EXCEPTION - integration-verify stages: if its orchestrator named you
  the ONE canonical verifier, IGNORE this block; running the FULL build,
  test suite, and linter IS your job. Other reviewers run targeted checks.
```

Do not touch BLOCK-A (`:11-17`): `block_a_agrees_across_every_surface` (`tests_doctrine.rs:112-133`) requires it byte-identical across the prefixes, this file and `subagent-verify-guard.sh`. The loom-verifier exception (`:21-23`) and every other line stay. The preamble is prepended to every typed spawn, so the rewording costs 22 bytes per spawn (197 to 219); keep it to these three lines. `subagent-verify-guard.sh` is unchanged: it lets every IV subagent run full suites, and the fence is guidance.

In `loom/src/orchestrator/signals/tests_doctrine_prefixes.rs`, next to `integration_verify_prefix_assigns_one_canonical_gate_owner` (`:215-242`), add:

```rust
#[test]
fn subagent_preamble_names_the_one_canonical_iv_verifier()
```

asserting that `crate::fs::permissions::constants::HOOK_SUBAGENT_PREAMBLE` contains `the ONE canonical verifier, IGNORE this block` and `Other reviewers run targeted checks.`, does not contain `if you were spawned to review or`, and that `generate_integration_verify_stable_prefix()` contains `ONE canonical verifier`. The existing needles at `:168` and `:219-231` stay unchanged.

## Your one check

`cargo test --test guidance_gate_contracts main_session_edit_advisory_fires_once_past_the_limit`, once, from `loom/`. The main agent runs `bash -n`, the hook suite (plain and BSD) and the gate.

## Traps

- `TOOL_INPUT` is the JSON text of `.tool_input` (`post-tool-use.sh:218`); feed it to `jq`, never to shell word splitting.
- Paths may contain spaces; `_loom_ledger_append` strips tabs and newlines from fields, so the TSV stays parseable.
- `wc` pads its output on BSD; do not use it here. Count with `jq` and bash arithmetic.
- The large-output notice (`_loom_bash_output_notice`) prints on stderr with exit 0; do not copy that channel.
