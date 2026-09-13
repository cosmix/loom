# Context admission: successful Read receipts

## Lane and boundary

Use the Terra implementation lane. This worker owns every read-receipt file below, including the shared post-tool hook; this serializes it after the job-lifecycle stage. Do not run git or proof commands; the stage orchestrator runs the listed commands after all workers finish.

## Owned files

- `loom-hooks/_read_ledger.sh`
- `loom-hooks/_read_discipline.sh`
- `loom-hooks/read-guard.sh`
- `loom-hooks/post-tool-use.sh`
- `loom/src/context/read_receipts.rs` **(new)**
- `loom/src/context/mod.rs`
- `loom/src/commands/hook/read_receipt.rs` **(new)**
- `loom/tests/integration/hooks_read_guard.rs`
- `loom/tests/integration/hooks_read_guard_repeat.rs`
- `loom-hooks/tests/post-tool-use-tool-event.sh`
- `loom-hooks/tests/post-tool-use-empty-output.sh`

Do not edit `loom-hooks/poll-guard.sh` (job-lifecycle owns its polling policy), context-delivery Rust code, skill routing, worker-brief command/dispatch registration, or plan validation. The Sol scoped-worker owner registers `HookCommands::ReadReceipt` and its dispatch/module declaration; this worker owns the called `hook::read_receipt` implementation and `context::read_receipts` storage.

## Grounded seams

`read-guard.sh` classifies `Read` input as full/range and invokes `_read_discipline.sh::loom_read_discipline_check`. That function currently calculates repeat policy from the current TSV then calls `_loom_ledger_append` on every allowed attempt, including paths where the eventual Read may fail. `_read_ledger.sh` stores only path, kind, line interval, and timestamp. It cannot prove returned content, content identity, or source generation.

`post-tool-use.sh` is the existing session hook and sees `tool_name`/`tool_input` plus a transcript path; its documented and currently parsed payload has no result/output field. The local Claude transcript shape is observed: an assistant `tool_use` block has an id, and its following user `tool_result` block has `type`, `tool_use_id`, `content`, and `is_error` (sampled at `/home/dkaponis/.claude/projects/-home-dkaponis-src-loom--worktrees-source-graph/eb12628d-46e9-4743-89d6-491796c6e2f4/subagents/agent-a4f6a2da59b23d384.jsonl`). That supports an exact transcript correlation; it does **not** establish that a PostToolUse payload carries `tool_result` or `tool_response`. An attempt record must never be relabeled a delivery receipt.

## Chosen implementation

Keep the existing TSV only as a bounded attempt/debug ledger; it is never a delivery receipt and it no longer qualifies a repeated Read for warn/deny escalation. Preserve the independent graph-backed large-unbounded policy (`_loom_read_discipline_verdict1`) exactly: a graph-covered text file over the line threshold can still be redirected to outline/ranges on the first attempt.

Also close Fable §8's remaining invalid-stage hint: `_read_discipline.sh` currently
builds `loom knowledge context --stage ${LOOM_STAGE_ID}` from a nonempty environment
value alone. Change that advisory to the always-valid unscoped query form;
do not open shared state or add a subprocess merely to validate the hint.
Add unset, stale/invalid and real-looking stage-ID fixtures proving the hint
never promises a resolvable stage. Preserve actual stage-aware retrieval in the
typed worker-brief path, where stage metadata has been loaded and validated.

Add **new** `context/read_receipts.rs`, registered by this worker from `context/mod.rs`. Its Rust records are atomically stored with existing locked/no-follow filesystem helpers rather than shell TSV. A pending `ReadIntent` contains session/agent, normalized path/range, pre-tool source generation, and a short nonce; a completed `ReadReceipt` contains the same identity plus content class (`text` or `media`), completed-result hash and byte count, source generation, transcript tool-use id, and completion time. The pre-tool source generation is a distinct hash of requested source bytes plus normalized range and current context epoch; it is never inferred from result content. Rust serialization removes shell newline/tab ambiguity and keeps agent-controlled output out of filenames.

Add **new** `commands/hook/read_receipt.rs::read_receipt`, invoked by the Sol-owned `loom hook read-receipt` command registration. It has three deterministic modes. `--prepare` accepts the PreToolUse Read input, computes the pre-tool source generation, and writes one pending intent; it claims no delivery. `--check` recomputes the current source generation and reports only a matching proven text receipt for this session/agent/path/range/generation. `--complete` receives the PostToolUse payload, reads only the existing bounded transcript tail, and accepts a receipt only when it finds one assistant Read `tool_use` and its immediately correlated non-error user `tool_result` by the same `tool_use_id`, whose input exactly matches the PostToolUse Read input and exactly one unused matching pending intent. It recomputes source generation after the result; if it differs from the pre-tool intent (file edit, epoch/compaction change, or range mismatch), it discards the intent and records nothing. It classifies and hashes the bounded completed result content, then atomically stores the receipt. Missing/unsafe transcript path, tail truncation, malformed JSON, ambiguous duplicate candidate, error result, unclassifiable media, unreadable source, or any correlation mismatch is a no-op.

Set fixed caps in that Rust module: prepare hashes only a regular text selection whose requested source bytes fit a small fixed receipt cap; complete reads only the existing transcript-tail cap and accepts only result content within the same cap. Oversized full reads, oversized ranges, persisted-output references, images/PDFs with unparseable result shape, and any cap breach return no intent/receipt without a retry or a larger read. `--check` first performs an indexed receipt lookup and only re-hashes when a candidate exists. The three modes perform no network I/O, polling, sleep, or unbounded scan; non-eligible calls retain ordinary guard behavior, so receipt instrumentation cannot add an elapsed wait path to broad or media Reads.

`read-guard.sh` invokes `loom hook read-receipt --prepare` before allowing a Read. `post-tool-use.sh` invokes `--complete` with its current payload and no raw result assumption; the Rust adapter does the bounded, tool-use-id-correlated transcript read. Do not scan a transcript for an uncorrelated result and do not trust `tool_result`/`tool_response` merely because another hook handles those optional fields. The Rust adapter is the only writer of receipt state.

`read-guard.sh` calls `loom hook read-receipt --check` before repeat escalation. `_read_discipline.sh` may issue the repeat advisory/deny only after the check confirms a matching proven receipt. A content/source generation change after a file edit, context rebuild/compaction, range change, result failure, or missing receipt therefore resets eligibility. Media never enters text range/repeat advice and is not converted from serialized bytes to token savings. If the bounded adapter cannot prove correlation, it remains telemetry/advisory-only and never creates a receipt or eligibility for enforcement.

## Tests to add or adjust

- Retain the existing four-field attempt TSV assertion; add Rust helper tests for its separate receipt record, atomic dedup, and hostile newline/tab result content.
- A prepared intent followed by an adjacent correlated transcript `tool_use`/`tool_result` pair creates one receipt; `--check` recognizes only same session/agent/path/range/current source generation. File edit between prepare and completion, changed range, changed context epoch, changed result/source hash, or an ambiguous duplicated Read does not match.
- Failed Read, missing/unsafe transcript path, bounded-tail truncation, malformed result, non-Read tool, and denied pre-tool attempt create no receipt. The post hook itself never persists raw output.
- A PDF/image/media result records `media`, never enables textual range/repeat advice, and is not used to estimate token savings.
- Prove an attempt-only second read does not escalate; prove a matching proven receipt permits the existing repeat policy; prove graph-backed first-read large-file guidance remains independent.

## Orchestrator proof commands

```sh
cargo test --manifest-path loom/Cargo.toml --test integration hooks_read_guard
cargo test --manifest-path loom/Cargo.toml context::read_receipts --lib
bash loom-hooks/tests/post-tool-use-tool-event.sh
bash loom-hooks/tests/post-tool-use-empty-output.sh
```

## Acceptance evidence

The large-file graph guard still governs first attempts. Repeated-read intervention begins only after a Rust-written, completed-result receipt matches the current source generation; an attempt, a stale file, a compaction/rebuild, or media cannot be mislabeled reusable textual delivery.
