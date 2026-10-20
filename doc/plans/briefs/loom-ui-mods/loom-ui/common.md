# loom-ui: shared contract for every worker

Read this file in full before your own brief. Where your brief and this file differ, this file wins.

## Where the stage runs

The stage's `working_dir` is `loom/`: loom runs each contract as `cargo test <name> -- --exact` from the stage working directory (`loom/src/testrun/adapters/cargo_test.rs` `single_test_command`, `loom/src/verify/contracts/site.rs` `stage_site`), and the repository root has no `Cargo.toml`. Shell commands therefore run from `loom/`: `cargo test ...` needs no `--manifest-path`, and files outside the crate are `../claude-plugins/...`, `../scripts/...`, `../install.sh`. Paths in these briefs are repo-relative (`loom/src/...`, `claude-plugins/...`), counted from the worktree root.

## What the stage builds

1. `loom hook ui-snapshot` (Rust, W1): a hidden, read-only command that prints ONE line of compact JSON describing the loom stage the calling Claude Code session belongs to, or `{"schema":1,"in_stage":false,"reason":"<code>"}` when it belongs to none. It always exits 0.
2. The `loom-ui` Claude Code plugin (TypeScript, W2) in `claude-plugins/loom-ui/`: a stage band above the prompt drawn only in a stage session, a context gauge in the status line, a `/loom` pane, subagent tracking in every session (a subagent band with elapsed time, model and effort and context use per running agent, plus a `/loom-agents` pane), and a guard-denial feed. It raises no desktop notification: the web UI and Claude Code already do. It reads loom state ONLY by running `<LOOM_BIN> hook ui-snapshot`, where `<LOOM_BIN>` is the absolute path in the session's `LOOM_BIN` variable.
3. Installation (Rust + shell, W3): `loom install-assets` (which `install.sh` and `loom update` both run) embeds `claude-plugins/loom-ui/` and places it at `<claude-dir>/skills/loom-ui/`. Claude Code auto-loads a plugin folder there as `loom-ui@skills-dir`, enabled at user scope, with no settings.json change (verified with Claude Code 2.1.295: `claude plugin list --json` reports `{"id":"loom-ui@skills-dir","scope":"user","enabled":true,...}` for a folder under `skills/` that holds `.claude-plugin/plugin.json`, also under a relocated `CLAUDE_CONFIG_DIR`, and a headless session loads it under every capsule shape loom passes: `--setting-sources user,project` with or without `--settings` carrying `enabledPlugins`; only `enabledPlugins: {"loom-ui@skills-dir": false}` disables it).

## Security rules (all workers)

- Read-only. The snapshot command opens no daemon socket, makes no network call, reads no `admin.token`/`user.token`, and writes nothing of its own. It reuses the status collector (`build_stage_summary`), whose completion view for an `executing` or `needs-human-review` stage verifies the session's completion checkpoint: that reads `completion-attestation.key` under the work dir and creates it (random, 0600, race-safe) when it is absent (`loom/src/handoff/completion/attest.rs` `attestation_key`, reached from `handoff::load_trusted_session_checkpoint`), and runs one git `rev-parse` when a checkpoint carries a blocker (`handoff::expected_stage_commit`). This is the same work `loom status` does from any shell; the key never reaches the output. Nothing else is written and no other git runs. When the key cannot be read or created (a symlink or other non-regular file at its path, a denied read), `load_trusted_session_checkpoint` errors and the completion view's `.ok()` drops it: `completion_blocker` is `null` and the snapshot stays in stage, as `loom status` shows it in the same state. That degrade is intended; never turn it into `read-error`.
- Every string that originates in a stage file, plan file, session record, ledger or review record is untrusted. Rust flattens it with `crate::context::untrusted::inline_safe` (controls, whitespace runs, bidi and zero-width characters become single spaces, backticks become U+02CB, 200-character cap including the ellipsis) before it reaches the JSON. The plugin treats every snapshot field as display text: it validates the type of every field it reads, truncates, and never feeds snapshot text into the model's context (no `prompt.compose`, `prompt.context`, `prompt.submit` or `session.append` hooks, no `$.prompt.*`, `$.model.*`, no `$.ui.ask`). A `command.run` answer `{ text }` is stored as a transcript row the model reads, so the plugin's commands never answer with snapshot text.
- The plugin observes; it never refuses or rewrites model or tool execution. Every observing hook (`session.start`, `tool.call`, `agent.spawn`, `turn.step`, `turn.complete`) passes the event on unchanged with `next(e)` and returns what `next` returned. Two hooks answer instead: `command.run` for the plugin's own `loom` and `loom-agents` commands answers without calling `next` (a `command.run` hook that reads `next` becomes a gate), and `ui.render` returns the plugin's own tree for its band and panes, or `next(e)` when it draws nothing. It registers no tool and spawns no agent.
- The only process the plugin runs is the loom binary named by `LOOM_BIN`, with argv `[loomBin, 'hook', 'ui-snapshot']`, no shell, no `cwd` or `env` option. `LOOM_BIN` is the daemon's verified binary that every loom session carries (`loom/src/orchestrator/terminal/native/wrapper/host_env.rs`); loom's own hooks run `"${LOOM_BIN:-loom}"` for the same reason. A bare `'loom'` is forbidden: Claude Code resolves `argv[0]` through the session's PATH, relative entries included, against the session cwd (verified with 2.1.295), and the plugin's child must be treated as unsandboxed (with sandbox settings on, a probe child still wrote outside its cwd and reached the network), so a session-writable or relative PATH entry would let an agent plant a `loom` the plugin runs every 5 s. The plugin polls only when `LOOM_BIN` is an absolute path.

## Snapshot JSON, schema 1

The command prints exactly one line: `serde_json::to_string` of `UiSnapshot`, then `\n`. Key order below is the serialization order. `null` fields are serialized as `null` (never skipped) except that an out-of-stage snapshot carries only `schema`, `in_stage` and `reason`.

Out of stage: `{"schema":1,"in_stage":false,"reason":"<code>"}` where `<code>` is one of the nine below. The plugin rejects any other `reason` value.

| Code | When |
| --- | --- |
| `no-stage-env` | any of `LOOM_STAGE_ID`, `LOOM_SESSION_ID`, `LOOM_WORK_DIR` is unset or empty |
| `unsafe-id` | `LOOM_STAGE_ID` or `LOOM_SESSION_ID` fails `crate::models::forward_receipt::is_safe_id` |
| `work-dir-invalid` | `LOOM_WORK_DIR` does not canonicalize to a directory |
| `session-missing` | `crate::fs::session_files::load_session_exact(work_dir, session_id)` returns `Ok(None)` or `Err` |
| `session-mismatch` | the session record's `stage_id` is not `LOOM_STAGE_ID`, or `LOOM_SESSION_TYPE` is set and differs from the record's `session_type` Display string |
| `stage-missing` | `crate::verify::load_stage(stage_id, work_dir)` fails, or the loaded record's `id` is not `LOOM_STAGE_ID` (`load_stage` matches by file name only) |
| `stage-not-owned` | the stage record's `session` is not `Some(LOOM_SESSION_ID)` |
| `outside-worktree` | a non-knowledge session whose canonical cwd is not inside the canonical `session.worktree_path` (or the session has no worktree path), or `LOOM_WORKTREE_PATH` is set and its canonical form differs from it; a knowledge session whose canonical cwd is not inside the work dir's main project root |
| `read-error` | a later read of the stage, session, workspace config or stage list failed |

The knowledge branch is decided from the session RECORD (`session.session_type == SessionType::Knowledge`), never from `LOOM_SESSION_TYPE` alone (`doc/loom/knowledge/mistakes/session-identity-env.md` "Presence of an Env Var Is Not Membership"). Knowledge sessions run in the main checkout with no worktree, so their cwd must lie inside `WorkDir::main_project_root()` instead. Every other check still applies.

Which sessions report in-stage: stage sessions (including integration-verify and knowledge-distill, which are stage-kind sessions in a worktree), knowledge sessions (knowledge-bootstrap) and a contract session while it runs (the orchestrator assigns `stage.session` to it, `loom/src/orchestrator/core/stage_spawn.rs`, and exports `LOOM_WORKTREE_PATH` for it). Merge and adjudication sessions, and a superseded contract session, report `stage-not-owned`; that is the intended degrade. `session.worktree_path` is written just after the terminal spawn, so the first polls of a new session can answer `outside-worktree`; the plugin keeps polling.

In stage (field meanings):

- `stage`: `id`, `name`, `plan` (the plan name `load_plan_name` reads from the workspace `config.toml` plan source, with its leading `Plan:` stripped, or `null`), `status` (the `StageStatus` serde string, e.g. `executing`, `needs-human-review`), `stage_type` (`StageType` serde string), `session_type` (lowercase `SessionType` Display of the session record, or `null`), `held`, `retry_count`, `max_retries` (`null` means the global default), `elapsed_secs` and `execution_secs` (from the summary's timing, each `null` when unknown), `model` (resolved model string, `null` when empty), `reasoning_effort` (resolved effort, `null` when empty).
- `context`: `ceiling_tokens` = `resolve_context_ceiling_tokens(work_dir, stage.context_ceiling_tokens)`; `warn_tokens` = `ceiling * 60 / 100` and `red_tokens` = `ceiling * 90 / 100` in integer arithmetic (the display bands of `orchestrator/monitor/context.rs` `context_health`); `backstop_tokens` = `ContextConfig::backstop_tokens(ceiling)` (the daemon's forced-handoff tier; 250000 for a 200000 ceiling, 950000 for the default 800000); `subagent_ceiling_tokens` = the context config's `subagent_ceiling_tokens` (default 800000); `last_reported_tokens` = the session record's `context_tokens` when > 0, else `null`. A `ceiling_tokens` of 0 means "no gauge" to the plugin.
- `dependencies`: one `{id, status, merged}` per id in `stage.dependencies`, at most 32, in plan order; an id that fails `is_safe_id` is dropped; a dependency whose record cannot be loaded gets `status: "unknown"`, `merged: false`.
- `dependents`: one `{id, status}` per stage whose `dependencies` contains this stage, at most 32, sorted by id.
- `attention`: the `AttentionEntry` that `commands::status::render::attention_entries` yields for this stage's sanitized `StageSummary`, as `{label, note, command, automatic}` (`note` and `command` nullable), or `null`.
- `review_reason`: `StageSummary.review_reason` (sanitized) or `null`.
- `checklist`: `acceptance` (each criterion's `AcceptanceCriterion::command()` line, `inline_safe`, at most 64 entries), `acceptance_total` (the real count), `wiring_checks` (`stage.wiring.len() + stage.wiring_tests.len()`), `contracts` (`stage.contracts.len()`), `verification_status` (`stage.verification_status` mapped by hand: `NotRun` to `not-run`, `Passed` to `passed`, `GapsFound { .. }` to `gaps-found`, `HumanNeeded` to `human-needed`; never serialize the enum, whose `GapsFound` is a struct variant), `review_gate` (`crate::verify::review::gate::covers(&stage)`), `review_rounds` and `review_rounds_well_formed` (from `verify::review::store::load_rounds` and `ReviewRound::is_well_formed`), `open_findings` (`store::open_findings(..).len()`), `completion_blocker` (`{summary, next_action}` from `StageSummary.completion_blocker`, `summary` nullable, or `null`). A failure reading the review store sets `review_rounds`, `review_rounds_well_formed` and `open_findings` to 0; it never produces `read-error`.
- `daemon`: `tick_age_secs` from `orchestrator::tick::read` (`null` when there is no tick file) and `stalled` = age >= `orchestrator::tick::STALL_THRESHOLD_SECS`.

Nullable fields, all of which the plugin's parser must accept as `null`: `stage.plan`, `stage.session_type`, `stage.max_retries`, `stage.elapsed_secs`, `stage.execution_secs`, `stage.model`, `stage.reasoning_effort`, `context.last_reported_tokens`, `attention` (and its `note`, `command`), `review_reason`, `checklist.completion_blocker` (and its `summary`), `daemon.tick_age_secs`.

## Snapshot fixture

`loom/tests/fixtures/loom_ui_snapshot_fixture.ts` is a HARNESS file written by the contract session and frozen with the contracts. Nobody edits it afterwards. W2 copies it byte for byte to `claude-plugins/loom-ui/hooks/fixtures/snapshot-fixture.ts` (the harness must live inside the crate, because `harness:` paths reject `..` under `working_dir: "loom"`, and the plugin's tests must import a file inside the plugin folder); the stage gate runs `cmp` on the two. Its exact content is the comment line, then the export line, then the JSON object to end of file, with no trailing semicolon:

```typescript
// Pinned by loom/tests/loom_ui_contracts.rs; claude-plugins/loom-ui/hooks/fixtures/snapshot-fixture.ts is a byte-for-byte copy.
export const SNAPSHOT_FIXTURE = {
  "schema": 1,
  "in_stage": true,
  "stage": {
    "id": "loom-ui",
    "name": "Loom UI plugin",
    "plan": "Loom UI mods",
    "status": "executing",
    "stage_type": "standard",
    "session_type": "stage",
    "held": false,
    "retry_count": 1,
    "max_retries": 3,
    "elapsed_secs": 754,
    "execution_secs": 690,
    "model": "opus",
    "reasoning_effort": "medium"
  },
  "context": {
    "ceiling_tokens": 800000,
    "warn_tokens": 480000,
    "red_tokens": 720000,
    "backstop_tokens": 950000,
    "subagent_ceiling_tokens": 800000,
    "last_reported_tokens": 312000
  },
  "dependencies": [
    { "id": "ui-snapshot", "status": "completed", "merged": true }
  ],
  "dependents": [
    { "id": "integration-verify", "status": "waiting-for-deps" }
  ],
  "attention": {
    "label": "NEEDS INPUT",
    "note": "answer the question in the stage's terminal",
    "command": null,
    "automatic": false
  },
  "review_reason": null,
  "checklist": {
    "acceptance": [
      "cargo build --manifest-path loom/Cargo.toml",
      "claude plugin test claude-plugins/loom-ui"
    ],
    "acceptance_total": 2,
    "wiring_checks": 3,
    "contracts": 8,
    "verification_status": "not-run",
    "review_gate": true,
    "review_rounds": 1,
    "review_rounds_well_formed": 1,
    "open_findings": 2,
    "completion_blocker": null
  },
  "daemon": { "tick_age_secs": 4, "stalled": false }
}
```

The Rust contract `ui_snapshot_fixture_matches_serializer` reads `tests/fixtures/loom_ui_snapshot_fixture.ts` relative to `CARGO_MANIFEST_DIR`, takes the text after `export const SNAPSHOT_FIXTURE =`, parses it as JSON and compares it with `serde_json::to_value(loom::commands::hook::ui_snapshot::fixture_snapshot())`. W1 writes `fixture_snapshot()` to build exactly this value. W2's tests import `SNAPSHOT_FIXTURE` from `./fixtures/snapshot-fixture` as their in-stage sample. The contract `ui_snapshot_real_collection_emits_schema_strings` ties the real collector to the same key set and enum strings, because `fixture_snapshot()` is hand-built.

## Proof commands

Every worker runs only the ONE check its brief names, from `loom/`. The orchestrator runs the full stage gate through a `loom-verifier`.
