# W2: forward receipts from the PostToolUse payload

Tier: sonnet (`loom-software-engineer`). Read `common.md` in this directory first.

## Why

`loom hook forward-receipt --transcript <path>` reads the forwarder transcript at PostToolUse and skips every call without a result (`src/commands/hook/forward_receipt.rs:64-69`, `process`). Claude Code writes a call's `tool_result` row only after that call's PostToolUse hooks finish, so the result is never there and no `forward-receipts.jsonl` ever forms. The payload's own `tool_response` carries the result: `loom-hooks/codex-forward-result.sh:31-46` (`load_payload`) and `:103` read `.tool_response.stdout`. A second defect blocks the same path: `exact_forward_argv` (`src/models/forward_receipt/transcript.rs:266-285`) accepts only the 8-word wrapper argv, while forwarders write `--unit-id <unit>` (10 words; 8 of 8 recent recorded forwards on this machine) and `codex-forward-guard.sh` rewrites the command to 12 words with `--invocation-id inv-<32 hex>` (`emit_authorized_input`, `:305-312`; accepted forms in `is_exact_forward_command`, `:106-132`).

## Files you own

- `src/models/forward_receipt/payload.rs` (new), `src/models/forward_receipt/payload_tests.rs` (new)
- `src/models/forward_receipt.rs` (add `pub mod payload;` beside the existing `pub mod transcript;`; nothing else)
- `src/models/forward_receipt/transcript/argv.rs`, `src/models/forward_receipt/transcript/argv_tests.rs` (add `forward_argv` and its tests)
- `src/models/forward_receipt/transcript.rs` (re-export `forward_argv` beside `exact_forward_argv`; `collect_strings` becomes `pub(super)`; nothing else)
- `src/commands/hook/forward_receipt.rs`, `src/commands/hook/tests_forward_receipt.rs`
- `src/cli/types_ops.rs`, `src/cli/dispatch.rs` (the `ForwardReceipt` variant and its arm only)
- `../loom-hooks/post-tool-use.sh`, `../loom-hooks/tests/post-tool-use-forward-receipt.sh`

Do not change `exact_forward_argv`: `commands/subagents/forward_jobs_transcript.rs` reads transcripts through it, and widening it changes `loom subagents` classification, which this plan does not own. Report that gap in your final message (a recorded forward with `--unit-id` is invisible to it).

## Step 1: `payload.rs`

```rust
pub struct PayloadForward {
    pub parent_session_id: String,
    pub agent_id: String,
    pub invocation: ForwardingInvocation,
}
pub fn decode_payload(raw: &str, now: DateTime<Utc>) -> anyhow::Result<PayloadForward>
```

`forward_argv` is not in `payload.rs`. The argv helpers (`exact_forward_argv`, `exact_forward_argv_in`, `is_forward_wrapper`, `safe_shell_syntax`, `bounded_diagnostic`) live in `src/models/forward_receipt/transcript/argv.rs`, pulled in by `mod argv;` in `transcript.rs`, which re-exports only `exact_forward_argv`; `collect_strings` is in `transcript.rs`. Add to `argv.rs`, reusing those helpers:

```rust
pub fn forward_argv(command: &str) -> Option<(String, String)>
```

and re-export it from `transcript.rs` beside `exact_forward_argv`. `forward_argv` returns `(model, effort)` for exactly the forms the guard authorizes: `safe_shell_syntax(command)`, then `shell_words::split`; 8, 10 or 12 words; word 0 accepted only by `is_forward_wrapper(command, &words[0], &crate::assets::install::claude_hooks_dir()?)` (`claude_hooks_dir` returns `Option<PathBuf>`), as `exact_forward_argv_in` does (the absolute `<hooks_dir>/codex-forward.sh` or the bare `~/.claude/hooks/loom/codex-forward.sh`, never `$`); a word 0 that merely contains `/` with file name `codex-forward.sh` is refused, since it would let any `./x/codex-forward.sh` whose output carries markers mint a receipt naming a thread of its choosing; `[1] == "task"`, `[2]` non-empty, `[3] == "--model"`, `[5] == "--effort"`, `[7] == "--write"`, `bounded_diagnostic` on `[4]` and `[6]`; with 10 or more words `[8] == "--unit-id"` and `[9]` matches `^[A-Za-z0-9][A-Za-z0-9._-]{0,63}$`; with 12 words `[10] == "--invocation-id"` and `[11]` matches `^inv-[0-9a-f]{32}$`. Check the characters by hand in small helpers; add no dependency. `exact_forward_argv` is not changed. `payload.rs` calls `super::transcript::forward_argv`.

`decode_payload` parses the JSON object and fails (with a short reason, never echoing payload text) unless: `tool_name == "Bash"`; `tool_input.command` is a string accepted by `forward_argv`; `tool_use_id`, `agent_id` and `session_id` are strings that pass `is_safe_id`; and a result object exists under `tool_response`, else `tool_result`. From the result object: `text` = its `stdout` string, else its `output` string, else empty; `background_task_id` = `backgroundTaskId`, else `background_task_id`, kept only when `is_safe_id`; `task_output_path_text` = `text` followed by every string inside the result object (`collect_strings`, which `transcript.rs` exposes as `pub(super)`). Build `ForwardingResult { id: tool_use_id, text, background_task_id, task_output_path_text, timestamp: now }` and `ForwardingInvocation { id: tool_use_id, model, effort, timestamp: now, result: Some(result) }`. `parent_session_id` is the payload `session_id` (in a subagent's hook payload it is the parent session, as `codex-forward-result.sh` and `forward_join.rs::join_transcript` already assume).

Tests in `argv_tests.rs` add the 10 and 12-word forms and the rejected word 0 cases `./x/codex-forward.sh` and `$HOME/.claude/hooks/loom/codex-forward.sh`. Tests in `payload_tests.rs` (declared from `payload.rs` with `#[cfg(test)] #[path = "payload_tests.rs"] mod tests;`): the 8, 10 and 12-word forms parse; a 12-word form with `inv-` plus 31 hex, an 11-word form, `--unit-id ../x`, a `;` in the command, and a non-Bash tool all fail; `stdout` wins over `output`; a missing `tool_response` and `tool_result` fails.

## Step 2: `forward_receipt.rs`

- `pub fn forward_receipt() -> Result<()>` takes no argument. It returns `Ok(())` silently when `Config::from_env()` is `None` (unchanged), reads stdin through `fn read_payload(reader: impl Read, limit: u64) -> Result<String>` (limit `MAX_PAYLOAD_BYTES = 4 * 1024 * 1024` in production; read `take(limit + 1)`, refuse when longer without echoing payload text, as `worker_brief.rs::read_stdin` does), passes `Utc::now()` to `process`, and logs a failure to stderr exactly as today (`:35-38`).
- `fn process(raw: &str, config: &Config, now: DateTime<Utc>) -> Result<()>`: the `ensure!` checks stay (`:58-63`); `decode_payload(raw, now)` replaces `transcript::read`; one identity `ForwardIdentity::new(&forward.parent_session_id, &forward.agent_id, &forward.invocation.id, &config.stage_id, &config.loom_session_id)`; then `observe(...)` and `persist(...)` exactly as today. Delete the transcript import and the loop. `observe`, `persist`, the lock and the `O_NOFOLLOW` append do not change. `encode_new_observations` skips an observation when `existing` already holds one with the same `receipt_id`, `state`, `exit_code`, `codex_thread_id` and `locator`, ignoring `observed_at`; without this, `rerun_is_idempotent_and_raw_result_is_not_persisted` gets 4 lines where it asserts 2, and a hook retry double-appends.
- `terminal_evidence`, companion backend: an END marker with outcome `timed_out` records `ForwardState::TimedOut` with the END exit code, whatever the job record reads. The wrapper cancels a job at its cap, so the record reads `cancelled` (or still `running` if the cancel has not landed), and an `end.outcome == state` match would drop the terminal line, leaving the receipt `running` for good: `poll-guard.sh` would then advise `loom subagents wait --receipt` on a unit the stage has re-delegated.
- `src/cli/types_ops.rs`: `ForwardReceipt` loses its `transcript` field; its doc line says it reads the forward's PostToolUse payload on stdin. `src/cli/dispatch.rs`: `HookCommands::ForwardReceipt => hook::forward_receipt::forward_receipt(),` in `dispatch_hook`. `dispatch_hook` is not in the maintainability ledger; `dispatch` (69 in `maintainability-baseline.txt`) is, so do not touch `dispatch`.

## Step 3: tests in `tests_forward_receipt.rs`

Convert every test from a transcript fixture to a payload fixture; keep each test name, each scenario and every assertion. Replace `write_transcript(tool_id, result)` with a builder that returns the payload JSON string: `{"session_id": PARENT, "agent_id": AGENT, "tool_use_id": tool_id, "cwd": "/w", "transcript_path": "<temp>/t.jsonl", "hook_event_name": "PostToolUse", "tool_name": "Bash", "tool_input": {"command": <12-word forward>}, "tool_response": {"stdout": text, "stderr": "", "interrupted": false, "isImage": false}}`, with the metadata the old test put in `toolUseResult` merged into `tool_response` (`backgroundTaskId` and the output-path strings), and no `tool_response` for `absent_tool_result_writes_nothing`. `structured_stdout_takes_precedence_over_tool_result_text` becomes `stdout` against `output`. The `assert!(process(...).is_err())` line in `symlinked_receipts_file_is_refused` necessarily changes text: that raises the test-integrity event `TI-edit-loom/src/commands/hook/tests_forward_receipt.rs` (event ids carry repository-root paths), which the main agent disputes once after the final review round; you file nothing. The reason is "the hook input moved from a transcript path to the PostToolUse payload; every scenario and assertion is kept". Every `process(...)` call in the tests passes one fixed instant. Keep the number of tests and assertions at least what it is at base (`TI-decl-rust`, `TI-assert-rust` fire when they fall).

New tests: `read_payload` reads exactly `limit` bytes and refuses `limit + 1` bytes without echoing payload text; a companion payload whose END is `timed_out`/124 with a job record `cancelled` writes a `running` line and a `timed_out` line with exit code 124.

## Step 4: the hook call

In `../loom-hooks/post-tool-use.sh`, the block under `# Rust revalidates this bounded, best-effort forward lifecycle notification.` becomes:

```bash
# Rust revalidates this bounded, best-effort forward lifecycle notification. The
# call's tool_result row is written after this hook, so the payload carries it.
if [[ "$TOOL_NAME" == "Bash" && "$COMMAND" == *"codex-forward.sh task"* && -n "$INPUT_JSON" ]] \
 && command -v "${LOOM_BIN:-loom}" &>/dev/null; then
 printf '%s' "$INPUT_JSON" | LOOM_HOOK_CONTEXT=1 loom_run_bounded 5 "${LOOM_BIN:-loom}" hook forward-receipt >/dev/null 2>&1 || true
fi
```

`loom_run_bounded` (`loom-hooks/_bounded.sh:37-54`) passes stdin through on every branch, as plan 1's former `read-receipt --complete` call relied on. Keep the file at its current line count or within one line of it; tabs for indentation; bash 3.2 compatible (`doc/loom/knowledge/mistakes/hooks-shell-portability.md`).

`../loom-hooks/tests/post-tool-use-forward-receipt.sh`: the stub `loom` also copies its stdin to `$FORWARD_STDIN` (`cat >"$FORWARD_STDIN"`) for `hook forward-receipt`. Expected argv becomes `hook forward-receipt`; assert the recorded stdin equals the forwarding payload byte for byte; the ordinary Bash and Edit payloads still record nothing; add a forwarding payload without `transcript_path` that still records one call. The `run_test` line in `run-all.sh` (`:61`) keeps its description and file.

## Done means

Run once each: `cargo test --lib forward_receipt` and `bash ../loom-hooks/tests/post-tool-use-forward-receipt.sh`. Report both results, every changed file and its line count after `cargo fmt`, and the `exact_forward_argv` gap named above. Also report for knowledge-distill: a PostToolUse payload whose stdout Claude Code replaced with a "Full output saved to:" notice decodes as `Absent` (`marker.rs` needs START on line 0), so no receipt forms; `codex-forward-result.sh` handles that case and the payload writer does not.
