# Context admission: scoped worker briefs

## Lane and boundary

Use the Sol implementation lane. This worker owns the scoped worker-admission path below. Do not run git or proof commands; the stage orchestrator runs the listed commands after all workers finish.

## Owned files

- `loom/src/commands/hook/worker_brief.rs` **(new)**
- `loom/src/commands/hook/mod.rs`
- `loom/src/orchestrator/signals/retrieval.rs`
- `loom/src/context/delivery/session.rs`
- `loom/src/context/delivery.rs`
- `loom/src/cli/types_ops.rs`
- `loom/src/cli/dispatch.rs`
- `loom-hooks/spawn-guard.sh`
- `loom-hooks/subagent-start.sh`
- `loom/src/context/tests/delivery.rs`
- `loom/src/context/tests/delivery/session.rs`
- `loom/tests/integration/hooks_spawn_guard.rs`
- `loom/tests/integration/hooks_spawn_guard_gate.rs`
- `loom-hooks/tests/subagent-start-ledger.sh`
- `loom/src/commands/hook/tests_worker_brief.rs` **(new)**

Do not edit `loom-hooks/codex-forward.sh`, any read or poll guard, or plan structural validation. The worker brief must be emitted by the deterministic hook command below, not by duplicating the Codex wrapper's universal navigation preamble.

## Grounded seams

`retrieval.rs::retrieve_stage_pack` already builds a `StageQuery` from stage id/type/name/description/working directory/files/artifacts/wiring/dependencies, applies overlay scope and dependency-path boosts, and calls `retrieve_for_stage`. The existing crate-visible `format/brief.rs::format_knowledge_brief` can render the new command's pack without changing normal signal/recovery/knowledge callers.

`context/delivery.rs::DeliveryRecord` already records recipient id, `context_epoch`, and `(node_id, content_hash)` units through `record_delivery`; `delivery/session.rs::delivered_to_session` suppresses already-delivered stage/prompt units for the same epoch and knows the stage spawn recipient. Those records are real receipt machinery for stage packs, but they do not retrieve or inject task-specific material for a spawned worker. `spawn-guard.sh::record_spawn` and `subagent-start.sh` currently record only spawn identity/model telemetry.

`loom-hooks/codex-forward.sh` already prepends every forwarded Codex task with a navigation kit. Re-emitting that kit in a worker brief would duplicate prompt content and may distort the wrapper contract.

## Chosen implementation

Add **new** `commands/hook/worker_brief.rs` and expose it as deterministic `loom hook worker-brief` in `commands/hook/mod.rs`, `cli/types_ops.rs::HookCommands`, and `cli/dispatch.rs::dispatch_hook`. The normal command reads the authorized typed Task/Agent payload from stdin and returns one bounded JSON envelope with either an empty brief or a nonce plus a worker brief. A `--bind-agent <id>` mode receives the start hook's transcript path and binds a previously issued nonce to the actual child id; both modes are filesystem/string-only and fail closed to empty output.

After the untyped-spawn gate and model resolution have succeeded, `loom-hooks/spawn-guard.sh` invokes `loom hook worker-brief` with the original Task/Agent payload. It merges the returned brief into the original `tool_input.prompt` and emits one `updatedInput` containing both that preserved prompt plus the explicit resolved model when it filled one. A no-brief response preserves current behavior. This is an authorized typed-spawn mutation, not a global automatic prompt hook. The original task body stays byte-for-byte present before the appended scoped section; it must never be shortened or silently drop a stated requirement.

`worker_brief.rs` constructs `StageQuery` from the existing stage retrieval inputs, then adds the worker task and only explicit prompt-declared `Files owned`/`may read` paths as advisory rank inputs. Those prompt declarations never alter sandbox permissions or authorize writes. It calls existing `retrieve_for_stage` and crate-visible brief rendering rather than copying ranker/renderer behavior. The new `worker_recipient_id` helper in `context/delivery/session.rs` takes stage, parent session, and a per-tool-call nonce. It must not use the stage parent recipient: before the child exists, it stores a pending pack against only the nonce and does not mark the parent or prospective child as having received it.

The appended section contains only selected excerpts/pointers and epoch/status metadata; it excludes plan overview, general stage doctrine, and the Codex navigation kit already injected by `loom-hooks/codex-forward.sh`. It includes a stable nonce marker. `subagent-start.sh` reads that marker from the child transcript, calls `loom hook worker-brief --bind-agent`, and only then lets the command atomically bind the pending receipt to the actual child recipient. If no single safe marker is found, it writes its existing start record but binds nothing. This prevents concurrent prospective workers from being conflated. A successful normal command records no delivery until it has emitted the envelope and later binds; empty selection records nothing. Required material that cannot fit is rendered as the bounded existing `ContextPack::unmet_required` line, never silently truncated.

For `loom-codex-forwarder`, that actual child recipient is the Claude forwarder, not the Codex rollout that its one Bash call later starts. Binding the nonce proves only that the forwarder received the normal Agent prompt; it must not create a receipt for, or suppress a later brief to, the downstream Codex process. The forwarder copies the scoped section once through its normal Agent prompt and `loom-hooks/codex-forward.sh` keeps its existing one navigation preamble. No worker-content deduplication decision may be based on a forwarder receipt for the unobservable Codex rollout.

The same Sol worker owns the central hook-command registration for the Terra read-receipt command: add **new** `HookCommands::ReadReceipt` and dispatch it to `hook::read_receipt::read_receipt`. Do not implement its receipt storage here; the Terra receipt worker owns that new module and the context helper. This single ownership avoids a cross-worker conflict in `mod.rs`, `types_ops.rs`, and `dispatch.rs`.

`delivery/session.rs` is private. Re-export the narrow new helper through the
owned `context/delivery.rs` facade and consume
`context::delivery::worker_recipient_id` from the hook command. Do not expose
the whole session module. Exercise that public facade in worker-brief tests;
distinct nonces must remain distinct and binding one must not credit another.

## Tests to add or adjust

- Add command tests proving the worker task plus owned path selects scoped material, returns a nonce envelope, and creates only a pending record.
- Prove a same nonce cannot bind twice; two concurrent nonces isolate; a missing/ambiguous transcript marker binds nothing; and a changed epoch/content reopens an otherwise delivered pack.
- Extend spawn-guard integration coverage to prove an authorized typed spawn gets one `updatedInput` containing both the preserved prompt/brief and model fill, while an untyped denied spawn never runs the command. Add the start-hook shell case binding child id from its nonce marker.
- Add a forwarder-specific case proving its bound receipt names the Claude forwarder only and never claims a Codex rollout delivery or changes a subsequent rollout's admission.
- Prove a missing/invalid payload and empty selection emit no brief and write no receipt. Prove a required item over budget is shown as unmet rather than silently absent.
- Command tests must prove the worker rendering excludes the Codex navigation-kit wording; existing stage/recovery/knowledge rendering is not modified.
- Keep delivery record tests for atomic merge and recipient validation; add worker nonce/child recipient collision and isolation cases next to the current session-recipient tests.

## Orchestrator proof commands

```sh
cargo test --manifest-path loom/Cargo.toml commands::hook::worker_brief --lib
cargo test --manifest-path loom/Cargo.toml context::tests::delivery --lib
cargo test --manifest-path loom/Cargo.toml --test integration hooks_spawn_guard
bash loom-hooks/tests/subagent-start-ledger.sh
```

## Acceptance evidence

Only an authorized typed Task/Agent spawn receives a scoped, receipt-pending brief. The actual child receives credit only after `SubagentStart` binds its nonce; stage-parent delivery behavior remains intact. A forwarded Codex task receives no second navigation kit, empty selection is free, and required facts have an explicit failure surface.
