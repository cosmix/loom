# usage-accounting: shared pins

Stage `usage-accounting` of `doc/plans/token-optim/PLAN-03-usage-evidence.md` (metaplan plan 2, steps 1, 2 and 5). Every worker reads this file first, then its own brief. Paths are relative to `loom/` unless they start with `../` or `doc/`. Line numbers are from `6fee4ecd`; anchor every edit by the quoted symbol, never by the number alone.

## 1. What this stage changes

- `loom usage` reads the transcripts of removed stage worktrees, accepts RFC3339 instants for `--since`, and reports each provider as `measured` or `partial` with reasons.
- `loom hook forward-receipt` takes the forward's result from the PostToolUse payload on stdin instead of the transcript, so forward receipts form; `post-tool-use.sh` pipes the payload.
- Codex rows of a project are the threads its forward receipts name; every other Codex row in the window is foreign and leaves the totals.
- Sections: Bash output bytes by command (`tools`), tokens on observation-only requests (`polling`), orientation for main sessions and subagents in requests and tokens (`lifecycle`), and the requested model of each spawn (`agents`).

## 2. Earlier plans change files this stage edits

Plans 3 (`PLAN-01-doctrine-truth-and-plan-guards`) and 1 (`PLAN-02-delivery-truth`) merge before this plan runs. Re-read each of these files at run time before editing it:

| File | Expected change by the earlier plan | Your anchor |
| --- | --- | --- |
| `../loom-hooks/post-tool-use.sh` | Plan 3 renamed `remind_knowledge_update` to `remind_memory_capture` and deleted items of its POST-COMMIT REMINDER box. Plan 1 deleted the `READ_FILE_PATH` lines and the `hook read-receipt --complete` block (`:219-222`, `:254-260` at 6fee4ecd) | The block under the comment `# Rust revalidates this bounded, best-effort forward lifecycle notification.` that runs `hook forward-receipt --transcript` |
| `src/cli/types_ops.rs`, `src/cli/dispatch.rs` | Plan 1 deleted the `ReadReceipt` hook variant and the `WorkerBrief` `--bind-agent`/`--agent-type`/`--transcript` flags | The `ForwardReceipt {` variant and the `HookCommands::ForwardReceipt { transcript } =>` arm in `dispatch_hook` |
| `../loom-hooks/tests/run-all.sh` | Plan 3 may add `run_test` lines | Not edited here: the forward-receipt hook test keeps its file and its `run_test` line (`:61`) |

If a block a brief names is gone or moved, find it by its quoted text; if it no longer exists, report that instead of guessing.

## 3. Interfaces workers share (pinned; compile together at the end)

- `src/commands/usage/args.rs` (W1, moved verbatim from `usage/mod.rs:55-163`): `pub enum ProviderSelection`, `pub struct UsageArgs`. `usage/mod.rs` re-exports both with `pub use args::{ProviderSelection, UsageArgs};`. `ProviderSelection::includes` and the `UsageArgs` helpers `since`, `provider`, `windows`, `has_explicit_selection` become `pub(super)`.
- `Transcript.requested_model: Option<String>` (W3 adds the field in `src/commands/usage/transcript_types.rs`, after `agent_type`), doc: the model the spawn guard recorded for this agent, joined from `spawns.jsonl`; `ambiguous` when the order join cannot decide.
- `src/commands/usage/spawn_join.rs` (W4, codex): `pub(crate) const AMBIGUOUS_MODEL: &str = "ambiguous";`, `pub(crate) struct RequestedModels`, `impl RequestedModels { pub(crate) fn load(work_dir: Option<&Path>) -> Self; pub(crate) fn requested_model(&self, agent_id: &str, parent_session_id: &str) -> Option<&str>; }`. W1 declares `mod spawn_join;` in `usage/mod.rs` and calls it from `parse_all`.
- `src/commands/usage/completeness.rs` (W1): `#[derive(Debug, Clone, Serialize)] pub(crate) struct Completeness { pub(crate) state: CompletenessState, pub(crate) reasons: Vec<&'static str> }`; `#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)] #[serde(rename_all = "kebab-case")] pub(crate) enum CompletenessState { Measured, Partial }`. Stage 2 compares the state with `==`. `ProviderReport` gains `pub(crate) completeness: Completeness`.
- `ToolShape` gains `pub bash_output: BashOutput` (W5, codex, in `sections/tools.rs`).

## 4. JSON surface the contracts read (pinned)

- `.provider_ledger.providers[] | select(.provider == "claude" or "codex")` gains `completeness: {"state": "measured" | "partial", "reasons": [..]}`. Reason codes, in this order when several apply: `missing-roots`, `unreadable-files`, `missed-session-transcripts`, `unknown-usage-rows`, `ambiguous-rows`, `codex-scope-unavailable`, `forward-thread-missing`, `forward-receipt-unthreaded`. Fallback coverage rows are not a reason; `fallback_coverage_rows` stays a coverage count. `state` is `partial` exactly when `reasons` is non-empty.
- `.provider_ledger.providers[].diagnostics` gains `missed_session_transcripts`, `foreign_codex_rows`, `missing_forward_threads`, `unthreaded_forward_receipts`, `codex_scope_unavailable` (all counts).
- Totals stay where they are: `.provider_ledger.providers[].totals.output.value`, `.coverage.measured_responses`.

## 5. Rules for every worker

- Error handling follows the file you edit: `anyhow::Result` with `.context(...)`; the usage pipeline warns on stderr and skips unreadable inputs, except where a brief says a failure aborts.
- Size limits (CLAUDE.md Rule 17; `tests/maintainability/scanner.rs`): every `.rs` file under `src/` and `tests/` at most 400 lines and every function at most 50, measured after `cargo fmt`; the hard limit is 400 lines (`cargo test --test maintainability`, `FILE_LINE_LIMIT`). Keep a 10% margin, except where a brief names a lower per-file cap (`forward_join.rs` 380, `sections/agents.rs` 360), which overrides the margin. `discovery.rs` (326 today) stays at or under 380; if the slug scan would pass that, W1 puts it in `src/commands/usage/discovery_slugs.rs` (W1's territory). `loom/maintainability-baseline.txt` must not change: it fails on growth and on shrinkage of a listed entry, and it is a ratchet file of this plan. No file or function this stage touches is listed in it today; do not create one that would be.
- Never run git. Never write under `.loom/`. Run only the narrowly scoped check your brief names, once; the orchestrator runs the gate.
- Record mistakes, decisions and surprises with `loom memory note` / `loom memory decision` as they happen (a codex unit reports them in its final message instead). Never Claude Code auto-memory.

## 6. The gate the orchestrator runs (from `loom/`)

```bash
cargo build
cargo test --all-targets --no-fail-fast
cargo test --doc --workspace --all-features -- --test-threads=1
cargo clippy --all-targets -- -D warnings
cargo fmt --check
RUSTDOCFLAGS='-D warnings' cargo doc --workspace --all-features --no-deps
cd .. && ./scripts/check-hook-syntax.sh
cd .. && bash loom-hooks/tests/run-all.sh
cd .. && LOOM_HOOK_TEST_BSD=1 bash loom-hooks/tests/run-all.sh
```
