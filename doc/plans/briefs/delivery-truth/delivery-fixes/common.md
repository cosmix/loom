# Delivery truth: shared brief for every worker of stage `delivery-fixes`

Read this file in full, then your own brief. Your brief settles your edits; this file holds the facts, rules and interfaces every worker shares.

## 1. What the stage does

Three delivery proofs loom relies on do not form, and one delivery path pays for text nobody reads. The stage:

1. Removes the read-receipt path (shell callers, the hidden `loom hook read-receipt` command, `loom/src/context/read_receipts*`, its tests). Rule 2 of `_read_discipline.sh` goes with it; the sibling ledger and its advisory stay, reworded.
2. Files each worker-brief delivery record at issue as `<key>.json`, and deletes the SubagentStart bind (`--bind-agent`, `bind_pending`, `bind_nonce`, the transcript scan).
3. Fixes why loom's own stages issued no worker brief (section 2 below): the worker query is capped at 2,048 bytes and the spawn guard's bound on `loom hook worker-brief` rises from 3 to 5 seconds.
4. Stops appending worker briefs to Codex-bound spawns (`loom-codex-forwarder`, `codex:*`).
5. Resets an ordinary session's prompt delivery record after compaction: `knowledge-orient.sh` pipes its SessionStart `compact` payload to `loom hook pre-compact`.
6. Corrects the doc comments and doctrine text that describe the old behaviour.

## 2. Why loom's own stages issued no worker brief (established while planning, 2026-10-18 and 2026-10-19)

- The spawn guard's `PreToolUse:Agent` hook rows (`hook_success` attachments, `durationMs`) in this repository's stage worktree transcripts (`~/.claude/projects/-home-dkaponis-src-loom--worktrees-*/*.jsonl`): of the 174 runs since 2026-10-01 (UTC), none finished under 3,000 ms (median 3,110 ms, maximum 3,852 ms); of the 249 runs before that date, 39 did. Outside stages, where the delegate never runs, the same hook took a median 72 ms over 230 runs.
- `spawn-guard.sh` runs the delegate under `loom_run_bounded 3` (the `WORKER_BRIEF_ENVELOPE=` line in the worker-brief block). A killed delegate prints nothing, and the hook appends nothing.
- A read-only replay of `loom hook worker-brief` (bubblewrap, a discarded overlay over `.loom`) on three recorded token-core spawn payloads took 7.8 to 9.6 s and returned a valid envelope of 12.1 to 12.2 KB, under `max_payload_bytes` (16,384). None of the three `None` returns in `issue` fired, so that return stays as it is.
- Retrieval time grows with query length. `loom knowledge context --budget-tokens 3000` took 1.66 s with a 2,048-byte query, 2.68 s at 4,096, 5.6 s at about 7.6 KB, 13 s at about 15.6 KB and 36 s at about 43.6 KB. Six concurrent 2,048-byte queries finished in 1.75 s wall time. The worker query at `6fee4ecd` is `StageQuery::build_stage_query_text(stage)` (`loom/src/orchestrator/signals/retrieval.rs`: id, type, name, description, working directory, files, artifacts, wiring descriptions, dependency ids) plus the worker's description and prompt (`retrieve_pack` in `loom/src/commands/hook/worker_brief.rs`); token-core's stage description alone was 10,961 bytes.
- The recorded engineer payload is committed as `doc/plans/token-optim/briefs/delivery-truth/delivery-fixes/replay-payload.json`.

## 3. Rules for every worker

- You are a leaf agent: never spawn subagents. Never run git (no add, commit, stash, checkout, rm, restore). Never write under `.loom/`.
- Write only the files your brief's ownership list names. Another worker owns every other file in the stage; a compile error in a file you do not own is not yours: report it and stop.
- Never edit a frozen contract file: `loom/tests/read_intent_contracts.rs`, `loom/tests/spawn_compaction_contracts.rs`, `loom/tests/worker_brief_contracts.rs`. They were written before you started and pin the behaviour your edits must produce. Read the ones your brief names.
- Run exactly the one check your brief names, once. The stage's main agent compiles every target and runs the full gate through a `loom-verifier` afterwards.
- Anchor every edit by symbol and quoted text. Line numbers in briefs are from `6fee4ecd` and drift.
- Record a mistake, a surprise or a decision with `loom memory note "..."` or `loom memory decision "..."` as it happens. Never Claude Code auto-memory. (Codex units report these in their final message instead; the main agent records them.)
- Writing style: plain, short sentences in comments and messages; no marketing words.
- Size limits: 400 lines per file, 50 lines per function (`loom/tests/maintainability/scanner.rs`, enforced by `cargo test`; the same 400-line limit applies to hook scripts by CLAUDE.md Rule 17). `loom/maintainability-baseline.txt` is an exact ledger and a `ratchet_files` entry: no worker edits it, and no edit may make a listed file or function grow or shrink. The one row that touches this stage is `function src/cli/dispatch.rs dispatch 73`: W1 edits `dispatch_hook` in that file and leaves the function `dispatch` byte for byte.

## 4. Interfaces pinned across workers

| Interface | Owner | Shape after this stage |
| --- | --- | --- |
| `loom hook worker-brief` | W1 (CLI), C1 (function) | No flags. `HookCommands::WorkerBrief` is a unit variant dispatched as `HookCommands::WorkerBrief => hook::worker_brief::worker_brief(),`; C1 defines `pub fn worker_brief() -> Result<()>` |
| Worker query cap | C1 | `pub const WORKER_QUERY_MAX_BYTES: usize = 2_048;` and `pub fn worker_query_text(stage: &Stage, description: &str, prompt: &str) -> String` in `loom/src/commands/hook/worker_brief.rs`: description, prompt and `StageQuery::build_stage_query_text(stage)` joined with `"\n"` in that order, cut to at most 2,048 bytes at a character boundary |
| Record writer | C1 | `fn store_record(config: &Config, nonce: &str, pack: &ContextPack) -> Result<Option<()>>` writes `<key>.json` in the worker delivery directory; `Ok(None)` when the file exists. `store_pending`, `bind_pending`, `bind_nonce`, `read_transcript`, `transcript_nonce`, `collect_value_markers` are gone; `marker_nonce` stays |
| `loom hook read-receipt` | W1 | Removed. W2 removes every shell caller and every Rust integration test that spawns it |
| `loom hook pre-compact` | C2 | Unchanged CLI: reads the hook payload on stdin, resolves the target through `HookTarget::from_environment()`, deletes the payload session's prompt record. It no longer touches a read-receipt epoch |
| `loom_read_discipline_check <path> <kind> <lines> <agent_id> <fallback_session_id>` | W2 | The sixth `payload` argument is removed. `poll-guard.sh` already passes five arguments and is not edited |
| `_loom_read_receipt_*` shell helpers | W2 | Deleted. No file outside W2's list calls them |
| `loom-hooks/subagent-start.sh` | W3 | Never calls `loom hook worker-brief` |

## 5. Files other plans change

- Plan 3 (`PLAN-01-doctrine-truth-and-plan-guards`) runs and merges before this plan. In `loom-hooks/post-tool-use.sh` it renames `remind_knowledge_update` to `remind_memory_capture` and rewrites that reminder, changing nothing between the `LOOM_STAGE_ID` guard and the reminder; in `loom-hooks/tests/run-all.sh` it rewords one description and adds one `run_test` line; in `loom-hooks/spawn-guard.sh` it rewrites header item 5 (the brief is appended only inside a stage). `spawn-guard.sh` was 403 lines at `4cc085b0`, already over the 400-line limit (commit `e747e9dc`); W3 brings it to 400 or fewer. Re-read those files before editing them; if a block your brief names is not where it says, find it by the quoted text.
- Plan 2 (`PLAN-03-usage-evidence`) runs after this plan and edits the `loom hook forward-receipt` call in `post-tool-use.sh`. Leave that call and its `if` block exactly as they are. It also edits the `ForwardReceipt { transcript }` variant in `src/cli/types_ops.rs` and its `HookCommands::ForwardReceipt { transcript } =>` arm in `src/cli/dispatch.rs`; leave both as they are.
- Plan 4 (`PLAN-04-dependency-findings`) runs later, does not edit `worker_brief.rs`, and builds a test on `loom/src/commands/hook/worker_brief/test_support.rs` (`Fixture`, `payload`, `pack`, `item`) declared beside the `mod support;` line in `tests_worker_brief.rs`. Keep those helpers' names and signatures.
- Plan 5 (`PLAN-05-guidance-and-gate-output`) slims `skills/loom-orchestration/SKILL.md` later; W2 changes only the label and one phrase of its **Read receipts.** paragraph.

## 6. Who owns what

| Worker | Lane | Files |
| --- | --- | --- |
| W1 rust-delivery | Claude sonnet | `loom/src/context/read_receipts.rs` and `loom/src/context/read_receipts/` (deleted), `loom/src/commands/hook/read_receipt.rs` (deleted), `loom/src/context/mod.rs`, `loom/src/commands/hook/mod.rs`, `loom/src/commands/hook/tests_cli_surface.rs` (new), `loom/src/cli/types_ops.rs`, `loom/src/cli/dispatch.rs`, `loom/src/commands/hook/worker_brief/test_support.rs`, `loom/src/commands/hook/tests_worker_brief.rs`, `loom/src/context/delivery.rs`, `loom/src/context/delivery/session.rs`, `loom/src/context/tests/delivery/session.rs` |
| W2 read-hooks | Claude sonnet | `loom-hooks/read-guard.sh`, `loom-hooks/_read_discipline.sh`, `loom-hooks/_read_ledger.sh`, `loom-hooks/_bounded.sh`, `loom-hooks/post-tool-use.sh`, `skills/loom-orchestration/SKILL.md`, `loom-hooks/tests/post-tool-use-tool-event.sh`, `loom-hooks/tests/post-tool-use-empty-output.sh`, `loom-hooks/tests/read-guard-sibling-receipts.sh`, `loom-hooks/tests/read-guard-session-ledgers.sh`, `loom/tests/integration/hooks_read_guard.rs`, `loom/tests/integration/hooks_read_guard_repeat.rs`, `loom/tests/integration/hooks_read_guard_receipts.rs` (deleted), `loom/tests/integration/hooks_poll_guard_reads.rs` |
| W3 spawn-compaction | Claude sonnet | `loom-hooks/spawn-guard.sh`, `loom-hooks/subagent-start.sh`, `loom-hooks/knowledge-orient.sh`, `loom-hooks/pre-compact.sh`, `loom-hooks/tests/subagent-start-ledger.sh`, `loom-hooks/tests/knowledge-orient-silent.sh`, `loom/tests/integration/hooks_spawn_guard_gate.rs` |
| C1 worker-brief | Codex gpt-5.6-terra | `loom/src/commands/hook/worker_brief.rs` |
| C2 pre-compact | Codex gpt-5.6-terra | `loom/src/commands/hook/pre_compact.rs`, `loom/src/commands/hook/tests_pre_compact.rs` |
