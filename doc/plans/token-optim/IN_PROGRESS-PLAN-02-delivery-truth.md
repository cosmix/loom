# Plan: Delivery Truth

## Overview

Three delivery proofs loom relies on do not form, and one delivery path pays for text nobody reads. This plan removes the read-receipt path, files each worker-brief delivery record when the brief is issued, fixes the cause of loom's own stages issuing no worker brief, stops appending worker briefs to Codex-bound spawns, and resets an ordinary session's prompt delivery record after compaction. It is plan 1 of the token-optimization metaplan (`doc/plans/optim-token-metaplan.md`, section 1) and targets per-spawn resident bytes and the proofs no later plan may count while they are broken.

- **Runs after:** plan 3, `PLAN-01-doctrine-truth-and-plan-guards`. In `loom-hooks/post-tool-use.sh` it renames `remind_knowledge_update` to `remind_memory_capture` and changes nothing between the `LOOM_STAGE_ID` guard and the reminder; in `loom-hooks/tests/run-all.sh` it adds one `run_test` line; in `loom-hooks/spawn-guard.sh` it rewrites header item 5. `spawn-guard.sh` is 403 lines at `4cc085b0` (commit `e747e9dc` added the own-root agents loop after plan 3 was written against 398), so it is over the 400-line limit before this plan starts; W3 trims it to 400 or fewer. This plan's stage re-reads those files at run time and anchors every edit by quoted text. `before_stage` fails when plan 3 has not merged (`remind_memory_capture` absent from `post-tool-use.sh`).
- **Runs before:** plan 2, `PLAN-03-usage-evidence`, which edits the `loom hook forward-receipt` call in `post-tool-use.sh`; this plan leaves that call as it is. Plan 4, `PLAN-04-dependency-findings`, depends on this plan's worker-brief records and on `loom/src/commands/hook/worker_brief/test_support.rs` keeping `Fixture`, `payload`, `pack` and `item`. Plan 5, `PLAN-05-guidance-and-gate-output`, slims `skills/loom-orchestration/SKILL.md` after this plan relabels one paragraph of it.
- **Lanes:** `delivery-fixes` lists `["claude", "codex"]`. Two pieces are single-file codex units with pinned interfaces and text-checkable done conditions: C1 rewrites `worker_brief.rs` (query cap, record at issue, bind deletion) and C2 drops the receipt epoch from `pre_compact.rs` and its test. Everything else stays on Claude sonnet: the receipt-module deletion and CLI surface change several files that compile only together, and the hook edits are doctrine-bearing shell changes that converge by running fixture-backed hook tests. Integration-verify and knowledge-distill list no lanes (bookends).
- **`loom plan verify --strict` warnings:** none accepted once plan 3 has merged. Plan 3 moves the knowledge-bootstrap heuristic out of `plan/schema/validation.rs` into `check_knowledge_recommendations` (`plan/schema/validation/knowledge_recommendations.rs`), which stays silent for this populated knowledge tree. Before plan 3 merges, the missing knowledge-bootstrap stage warns; this plan does not run before plan 3.
- **Where the pre-push gate runs.** The metaplan requires every code stage to run the full pre-push gate (`loom/.githooks/pre-push`: markdownlint :35-91, fmt :28, clippy :97, doc :107, tests :126) plus the hook steps. Markdownlint runs because W2 edits the tracked `skills/loom-orchestration/SKILL.md`. Three pre-push steps are left out of the stages on purpose, so the stage gate is the pre-push gate minus these: `cargo audit` (:115-122; no dependency changes, and its advisory database host is not in `allowed_domains`), `scripts/flake-check.sh` (:135-145; no quota, process or verdict module changes), and the `GIT_INDEX_FILE`/`GIT_DIR`/`GIT_WORK_TREE` scrubbing around the test run (:126-127; it matters only when tests run inside a git hook). Their owner is the operator's `git push`, which runs the pre-push hook in full. `loom plan verify` warns on an unfiltered `cargo test` in a standard stage's acceptance (`warn_full_suite_outside_integration_verify`, `plan/schema/validation_suite.rs:148-171`), so `delivery-fixes` runs the whole gate, unfiltered suite included, in its `loom-verifier` round, and its acceptance keeps every other pre-push step and the hook steps plus filters that select this stage's tests. Integration-verify's acceptance runs the unfiltered suite.
- **Before `loom run` (operator):** commit this plan and every file under `doc/plans/token-optim/briefs/delivery-truth/` (the replay payload included; plan 2's replay also copies it). `extract_brief_paths` (`loom/src/plan/schema/structural_checks.rs:78`) and the committed-input bundle (`loom/src/commands/run/plan_inputs/discovery.rs:22`) recognise only `doc/plans/briefs/`, so neither `loom plan verify` nor `loom run` notices an uncommitted brief here, and a stage worktree is cut from HEAD.
- **Evidence class:** a byte change that leaves agent behaviour alone (metaplan, "How a saving is shown", class 1). No paired run is needed. Plan 2's replay measures the byte difference.
- **Operator check after merge.** No stage runs this check; the operator does, once PLAN-03's `usage-accounting` workers have spawned. After `./dev-install.sh`, the next plan run in this repository (plan 2) must show, for at least one worker of its first standard stage: the worker transcript's first user message carries the line `<!-- loom-worker-brief nonce=<32 hex> -->` and, below it, the brief's first pack item in full (its heading and body; a marker alone does not prove the body arrived, because `render_brief` writes the marker and `format_knowledge_brief`'s output separately); and `.loom/work/context/<plan>/<stage>/session-retrieval/worker-brief/` holds a `worker-<hex>.json` record whose first `delivered[].node_id` names that item. Commands: `rg -l -F 'loom-worker-brief nonce=' ~/.claude/projects/-home-dkaponis-src-loom--worktrees-<stage>/*/subagents/*.jsonl`, then `jq` on the first `type == "user"` row of one hit, then `fd -e json . .loom/work/context/<plan>/<stage>/session-retrieval/worker-brief`. The operator records the result with `loom memory note` (or hands it to that stage's main agent to record), and a failure is reported against this plan.

## Goals

- No `loom hook read-receipt`, no `context/read_receipts*`, no receipt intent written by any session; rule 2 of `_read_discipline.sh` goes with it. The sibling ledger stays, and its advisory says the file was requested, not delivered.
- A worker-brief delivery record `<key>.json` exists for every brief the spawn guard appends, and `load_deliveries` returns it. No SubagentStart bind. A record means the brief was issued: a spawn later denied by a sibling PreToolUse hook (`stage-terminal-guard.sh`, `codex-forward-guard.sh` run on Task/Agent too, `fs/permissions/hooks/config.rs:63-75`) or by the user, or a delegate killed after its write, still leaves one, and `dependency_chunk_ids` (`context/delivery.rs`) counts it. That over-credit is accepted.
- Loom's own stages receive worker briefs: the worker query is capped at 2,048 bytes and the delegate's bound is 5 seconds.
- `loom-codex-forwarder` and `codex:*` spawns carry no worker brief.
- An ordinary session's prompt delivery record is deleted after compaction.
- Non-goals: repairing receipts, a stage compaction marker, a new hook script, recording `loom knowledge context` output, Codex-side retrieval or binding, Codex read suppression (all dropped by the metaplan, reasons below), and every item of metaplan section 6.

## Why loom's stages issued no worker brief (step 3, established while planning)

The cause is the 3-second bound around `loom hook worker-brief` in `spawn-guard.sh` (`loom_run_bounded 3` on the `WORKER_BRIEF_ENVELOPE=` line, `:300` at `6fee4ecd`, `:305` at `4cc085b0`), which the delegate exceeds in this repository because retrieval time grows with query length and the worker query carries the whole stage description.

| Measurement (2026-10-18 and 2026-10-19) | Result |
| --- | --- |
| Spawn-guard `PreToolUse:Agent` hook `durationMs` in this repository's stage worktree transcripts since 2026-10-01 (UTC) | 0 of 174 under 3,000 ms; median 3,110 ms, maximum 3,852 ms |
| The same, before 2026-10-01 | 39 of 249 under 3,000 ms; in the integration-verify transcripts, the spawns under about 3,000 ms were the ones whose child transcript carries the marker |
| `PreToolUse:Agent` outside stages (the delegate never runs) | median 72 ms over 230 runs |
| Other projects | stencilizer 145 of 145 under 3,000 ms (median 410); troofoo 49 of 104 and cartolyth 54 of 128 |
| Read-only replay of `loom hook worker-brief` (installed `loom 1.2.5 (6fee4ecd)`, bubblewrap with a discarded overlay over `.loom`) on three recorded token-core spawns | 7.8 to 9.6 s each; a valid envelope of 12.1 to 12.2 KB, under `max_payload_bytes` 16,384; none of the three `None` returns in `issue` fired; nothing written to the real `.loom` |
| `loom knowledge context --budget-tokens 3000` by query length | 1.66 s at 2,048 bytes; 2.68 s at 4,096; 5.6 s at about 7.6 KB; 13 s at about 15.6 KB; 36 s at about 43.6 KB; six concurrent 2,048-byte queries 1.75 s wall |
| token-core's stage description (part of the worker query through `StageQuery::build_stage_query_text`) | 10,961 bytes |

The recorded engineer payload is committed as `doc/plans/token-optim/briefs/delivery-truth/delivery-fixes/replay-payload.json`. The stage replays it once against its own stage before any edit, and integration-verify replays it with the built binary.

The fix the metaplan anticipated (dropping pack items when the envelope exceeds `max_payload_bytes`) is not built: the replay measured envelopes well under the limit, so that return did not fire.

## Execution Diagram

```mermaid
graph LR
    delivery-fixes --> integration-verify
    integration-verify --> knowledge-distill
```

## Stages

### 1. delivery-fixes (`working_dir: loom`)

One implementation stage. No Q1-Q4 split applies: every change is a deletion or a local edit over disjoint files, no later stage needs a checkpoint, and the work fits one session. The main agent replays the recorded spawn, spawns three Claude workers in the background and two codex units in the foreground in one message, compiles every target, runs the gate through a `loom-verifier`, runs the review rounds, files the expected test-integrity dispute and commits.

| Worker | Lane | Territory |
| --- | --- | --- |
| W1 rust-delivery | Claude sonnet | Deletes `context/read_receipts*`, `commands/hook/read_receipt.rs` and the CLI variant; the flag-free `WorkerBrief` variant and dispatch arm; a CLI parse test; the worker-brief unit tests and test support; delivery doc comments |
| W2 read-hooks | Claude sonnet | Removes the receipt calls from `read-guard.sh`, `_read_discipline.sh` (rule 2), `_read_ledger.sh` and `post-tool-use.sh`; rewords the sibling advisory, its comments, the `_bounded.sh` comment and the orchestration skill's label; updates the bash and Rust tests that pinned the receipt path |
| W3 spawn-compaction | Claude sonnet | `spawn-guard.sh` skips Codex-bound types, bounds the delegate at 5 s, corrects header item 5 and trims the file from 403 to 400 lines or fewer; `subagent-start.sh` loses the bind; `knowledge-orient.sh` resets the prompt record on `compact`; `pre-compact.sh` comment; their tests |
| C1 worker-brief | Codex gpt-5.6-terra | `worker_brief.rs`: `WORKER_QUERY_MAX_BYTES` (2,048) and `worker_query_text`; `store_pending` becomes `store_record` writing `<key>.json`; the bind path deleted; `worker_brief()` takes no arguments |
| C2 pre-compact | Codex gpt-5.6-terra | `pre_compact.rs` without the receipt epoch and with a corrected module doc; the epoch test deleted from `tests_pre_compact.rs` |

Risk-checklist walk (v2):

| Area | Applies | Contract |
| --- | --- | --- |
| Untrusted input | The worker prompt, set by the parent agent, is cut to a byte cap | `worker-query-cuts-on-a-char-boundary`, `worker-query-stays-under-the-cap` |
| Filesystem paths | Records keep the hashed `worker-<hex>` key and the locked atomic writer; no new path handling | none needed |
| Process I/O and time | The spawn guard's bound on the delegate | `slow-worker-brief-still-reaches-the-spawn` |
| Configuration | No new setting | none needed |
| Lifecycle | A record filed once per issued brief; the compaction reset fires once per SessionStart `compact` | `issued-worker-brief-files-a-loadable-record`, `compacted-ordinary-session-loses-only-its-prompt-record` |
| Reachability | The new reset and the forwarder exclusion are reached only through hook scripts | the two hook contracts drive the scripts; `after_stage` checks the two call sites, and `wiring` checks that `retrieve_pack` calls `worker_query_text` |
| External data | Claude Code's PreToolUse and SessionStart payload shapes | fixtures copy the field set of the recorded payload |
| Behaviour the stage exists to add | No receipt intent; no brief for Codex-bound spawns | `read-guard-prepares-no-receipt-intent`, `codex-bound-spawns-receive-no-worker-brief` |

Expected test-integrity events, disputed once after the final review round: `TI-decl-rust` and `TI-assert-rust` (receipt, bind and epoch tests deleted), and `TI-edit` events (event ids carry repo-relative paths, `TI-edit-loom/src/...`, because `base_test_files` lists the tree with `git ls-tree --full-tree`, `verify/integrity/count.rs:53-56`; the paths below are package-relative) on `src/context/read_receipts/tests.rs`, `src/context/read_receipts/tests_extra.rs`, `tests/integration/hooks_read_guard_receipts.rs` (deleted), `tests/integration/hooks_read_guard.rs`, `tests/integration/hooks_read_guard_repeat.rs`, `tests/integration/hooks_poll_guard_reads.rs`, `tests/integration/hooks_spawn_guard_gate.rs`, `src/commands/hook/tests_worker_brief.rs`, `src/commands/hook/tests_pre_compact.rs` and `src/context/tests/delivery/session.rs`, plus any event on the six bash tests the stage edits. No `TI-ratchet` event: no ledger row changes.

### 2. integration-verify (`working_dir: .`)

The full gate on the merged tree, the hook steps, parallel reviews, and functional checks: the recorded spawn replayed with the built binary finishes under the 5-second bound, the built binary rejects `hook read-receipt` and `hook worker-brief --bind-agent`, and no removed name survives outside the tests that assert its absence.

### 3. knowledge-distill (`working_dir: .`)

Rewrites the knowledge sections that describe the removed path and the binding, and records the cause found in step 3.

## Coverage of metaplan section 1

| Metaplan item | Where |
| --- | --- |
| Bullet: read receipts cannot form | delivery-fixes W1, W2, C2 (removed) |
| Bullet: worker-brief records never bind | delivery-fixes C1 (record at issue, bind deleted), W1 (CLI flags, tests), W3 (bind call removed) |
| Bullet: no worker brief reached a subagent in loom's runs | Cause above; delivery-fixes C1 (query cap), W3 (bound) |
| Bullet: Codex forwarders carry a brief they rarely relay | delivery-fixes W3 |
| Bullet: ordinary sessions keep their prompt record across compaction | delivery-fixes W3 (`knowledge-orient.sh`), C2 (`pre_compact.rs`) |
| Step 1 remove the read-receipt path | delivery-fixes W1, W2, C2; advisory rewording W2 |
| Step 2 file worker-brief records at issue | delivery-fixes C1, W1, W3 |
| Step 3 find why no brief was issued | Evidence above; delivery-fixes main-agent replay and `loom memory note`; regression contract `slow-worker-brief-still-reaches-the-spawn` |
| Step 4 stop appending briefs to forwarders | delivery-fixes W3 |
| Step 5 reset ordinary sessions after compaction | delivery-fixes W3, C2 |
| Step 6 correct the text | delivery-fixes C2 (`pre_compact.rs`), W1 (`delivery/session.rs`, `delivery.rs`), W3 (`pre-compact.sh`); knowledge-distill (three knowledge sections and four summaries) |
| Acceptance: read-guard writes no intent | contract `read-guard-prepares-no-receipt-intent` |
| Acceptance: forwarder vs engineer marker | contract `codex-bound-spawns-receive-no-worker-brief` |
| Acceptance: issued brief returned by `load_deliveries` | W1 unit tests `issued_brief_files_one_record_load_deliveries_returns` and `capped_query_still_issues_a_scoped_brief` (populated overlay, query past the cap); contract `issued-worker-brief-files-a-loadable-record` |
| Artifacts (existence, non-empty, no stub markers) | `delivery-fixes.artifacts`: `worker_brief.rs` (contracts `worker-query-*`, `issued-worker-brief-*`, wiring `let text = worker_query_text(&config.stage`), `pre_compact.rs` (contract `compacted-ordinary-session-*`), `tests_cli_surface.rs` (CLI removal). `loom plan verify` rejects `..` in artifacts, so the two hook scripts outside `working_dir` are proven only by their contracts and after_stage checks: `spawn-guard.sh` (`codex-bound-*`, `slow-worker-brief-*`, the bound check) and `knowledge-orient.sh` (`compacted-ordinary-session-*`, the call check) |
| Acceptance: compact payload deletes the prompt record | contract `compacted-ordinary-session-loses-only-its-prompt-record` |
| Acceptance: step 3 regression test and operator check | contract `slow-worker-brief-still-reaches-the-spawn`; operator check in the overview |
| Acceptance: the gate with the hook steps | delivery-fixes verifier round and acceptance; integration-verify acceptance |
| Dropped: repairing receipt completion; a stage compaction marker; `compaction-reset.sh`; recording `loom knowledge context` output; wrapper-side Codex retrieval, nonce binding to rollouts and a credentialed cross-OS test; Codex read suppression and compaction reset | Not built, for the metaplan's reasons (receipts under 0.5% of Read input; stage sessions have not compacted; `knowledge-orient.sh` is already registered; at most 25 overlapping prompt briefs in 102 sessions; Codex already receives a brief and the wrapper cannot write `.loom`; Codex has no Read tool and a unit is one prompt bounded at 540 s) |
| Disposition rows naming plan 1: census D (reread guard); review #3 P8 (scoped worker brief); #4 content-aware read receipts; 4.5 tree-wide read receipts; "push small, pull large; delivery records; PreCompact epoch"; review P4 (runtime correlation and provider coverage); delivery gaps 1, 2 and 3 | Covered by the rows above. The P8 and gap-3 rows' interactive and source-window halves are section 6 (E5, E6), outside this plan |

## Deviations from the metaplan

- **The step 3 cause is the time bound.** The metaplan named three `None` returns in `issue`; the replay shows none fires. The fix caps the query and raises the bound; it does not trim pack items.
- **`codex:*` spawns are excluded with `loom-codex-forwarder`.** The preamble `case` in `spawn-guard.sh` already treats both as Codex-bound, and Codex's own UserPromptSubmit hook briefs both.
- **The bound line also changes in `spawn-guard.sh`** (in place). The file also loses at least 3 lines (403 at `4cc085b0`): the empty-output `elif` folds into the timeout test, and a comment is rewrapped.
- **The worker query drops the stage text on loom's own spawns.** The order is description, prompt, stage text; a recorded loom spawn prompt is 3,580 bytes, so the 2,048-byte cut removes the stage text entirely. Accepted: the stage text reaches the stage session through its signal, and the worker's own task is the better retrieval key.
- **`loom hook pre-compact` does not read the payload's `cwd`.** It resolves `HookTarget::from_environment()` (`LOOM_WORK_DIR`, else the process working directory, `commands/hook/target.rs:54-67`), the same resolution `loom hook user-prompt` uses to write the record (`user_prompt.rs:106-114`); the contract proves the deletion end to end.
- **The advisory keeps naming the reader**: "was requested whole by agent X and N other agent(s) (an attempt, not proof of delivery)", so its pins keep their shape.
- **More text is corrected than step 6 lists**: the `_bounded.sh` comment that names receipt calls, the ledger comments that call sibling rows receipts, and the orchestration skill's **Read receipts.** label (`skills/loom-orchestration/SKILL.md:119`), which becomes **Sibling-read ledger.**
- **The forwarder gate test is converted, not deleted**: `codex_forwarder_keeps_marker_first_without_navigation_kit` becomes `codex_forwarder_spawn_gets_no_prompt_rewrite`, keeping its no-preamble pin.
- **`loom-hooks/tests/run-all.sh` is not edited**: no new bash test is added; the new hook tests are Rust contracts, because v2 contracts need a test runner adapter.
- **The unfiltered suite runs in the verifier round, not in acceptance** (overview, "Where the pre-push gate runs").

## Shared files and sibling plans

| File | Other plan | What this plan assumes |
| --- | --- | --- |
| `loom-hooks/post-tool-use.sh` | 3 (before), 2 and 5 (after) | Plan 3 renamed `remind_knowledge_update` to `remind_memory_capture` and touched nothing above it; this plan deletes the `READ_FILE_PATH` lines and the `read-receipt --complete` block and keeps the `forward-receipt` block for plan 2 |
| `loom/src/cli/types_ops.rs`, `loom/src/cli/dispatch.rs` | 2 (after) | This plan deletes the `ReadReceipt` hook variant and the `WorkerBrief` bind flags and their arms; the `ForwardReceipt { transcript }` variant and its `HookCommands::ForwardReceipt { transcript } =>` arm stay as they are for plan 2 |
| `loom-hooks/spawn-guard.sh` | 3 (before) | Plan 3 rewrote header item 5 only; the file is 403-404 lines when this plan starts. This plan edits the worker-brief block in place, adds the Codex exclusion to item 5, and trims comments and one redundant branch until the file is at or under 400 lines |
| `loom-hooks/tests/run-all.sh` | 3 (before), 2 | Not edited here; plan 3's added test makes the suite 103 tests |
| `loom/src/commands/hook/worker_brief.rs`, `worker_brief/test_support.rs`, `tests_worker_brief.rs` | 4 (after) | `retrieve_pack` keeps `StageQuery::dependency_paths` and the declared paths; the query text is `worker_query_text` (2,048 bytes); `store_pending` is renamed `store_record`; `marker_nonce` stays (`render_brief` uses it); `Fixture`, `payload`, `pack`, `item` and the `mod support;` line keep their shape; `Fixture::transcript`, the `Fixture::root` field and `marker` are deleted (dead after the bind tests go) |
| `skills/loom-orchestration/SKILL.md` | 5 (after) | Only the **Read receipts.** label and one phrase change; plan 5 measures the file against `6fee4ecd` |

## Environment inventory

| Need | State |
| --- | --- |
| crates registry for a cold build | allowed (`crates.io`, `index.crates.io`, `static.crates.io`) |
| `bunx markdownlint-cli2` in the repository pre-commit hook | allowed (`registry.npmjs.org`) |
| `web/` dependencies (v2 provision lint) | provisioned (hardened `bun install`) |
| localhost binding by the full suite | allowed (`allow_local_binding: true`) |
| tmux e2e sockets | not granted; those tests self-skip under a session sandbox |
| `rg`, `jq`, `bash` for the hook tests | host tools, checked by `before_stage` |
| Codex lane | host: `codex` at `~/.local/bin/codex`, plugin `codex@openai-codex`, `exclude_slash_tmp = true` in `~/.codex/config.toml`; when missing at run time, the signal reroutes C1 and C2 to sonnet |
| Recorded payload for the replay | committed under `doc/plans/token-optim/briefs/delivery-truth/delivery-fixes/` |

## Baselines observed at `6fee4ecd` (host shell)

- `./scripts/check-hook-syntax.sh`: 160 shell scripts parse cleanly.
- `bash loom-hooks/tests/run-all.sh`: 102 passed, 0 failed. `LOOM_HOOK_TEST_BSD=1 bash loom-hooks/tests/run-all.sh`: 102 passed, 0 failed. After plan 3, 103.
- The cargo gate was not run while authoring (the authoring session may not write `loom/target`); the stage's first verifier round establishes it, and a failure it shows at the stage's base is reported before any edit is blamed.

## Briefs

- `doc/plans/token-optim/briefs/delivery-truth/delivery-fixes/common.md`
- `doc/plans/token-optim/briefs/delivery-truth/delivery-fixes/w1-rust-delivery.md`
- `doc/plans/token-optim/briefs/delivery-truth/delivery-fixes/w2-read-hooks.md`
- `doc/plans/token-optim/briefs/delivery-truth/delivery-fixes/w3-spawn-compaction.md`
- `doc/plans/token-optim/briefs/delivery-truth/delivery-fixes/c1-worker-brief.md`
- `doc/plans/token-optim/briefs/delivery-truth/delivery-fixes/c2-pre-compact.md`
- `doc/plans/token-optim/briefs/delivery-truth/delivery-fixes/replay-payload.json` (replay fixture)

YAML is authoritative where it and this prose differ.

---

<!-- loom METADATA -->

```yaml
loom:
  version: 2
  auto_merge: null
  sandbox:
    enabled: true
    auto_allow: true
    allow_unsandboxed_escape: false
    excluded_commands: []
    filesystem:
      deny_read:
      - ~/.ssh/**
      - ~/.aws/**
      - ~/.config/gcloud/**
      - ~/.gnupg/**
      deny_write:
      - ../../**
      allow_write: []
    network:
      allowed_domains:
      - crates.io
      - index.crates.io
      - static.crates.io
      - registry.npmjs.org
      additional_domains: []
      allow_local_binding: true
      allow_unix_sockets: []
      allow_all_unix_sockets: false
    linux:
      enable_weaker_nested: false
    command_confinement: confined
  ratchet_files:
  - loom/maintainability-baseline.txt
  - doc/loom/knowledge/check-baseline.txt
  provision:
  - working_dir: web
    command: >-
      test ! -e .npmrc && test ! -L .npmrc && bun install --frozen-lockfile
      --ignore-scripts --backend=copyfile --config=/dev/null
  stages:
  - id: delivery-fixes
    name: >-
      Remove read receipts, file worker records at issue, deliver worker briefs,
      reset compacted sessions
    description: |
      Implement doc/plans/token-optim/PLAN-02-delivery-truth.md (renamed IN_PROGRESS-PLAN-02-delivery-truth.md once loom run starts), metaplan plan 1.
      Use parallel subagents and skills to maximize performance.
      PATHS: working_dir is loom/ because loom runs each contract as `cargo test <name> -- --exact` from it. YAML paths and the worker table are package-relative (../ for files outside the crate); brief paths are repo-relative from the worktree root.
      Every worker reads doc/plans/token-optim/briefs/delivery-truth/delivery-fixes/common.md first, then its own brief. common.md section 2 holds the evidence for step 3; section 4 pins the interfaces workers share across both lanes.
      
      MAIN AGENT, IN ORDER:
      1. RE-READ the shared files plan 3 changed before this plan: ../loom-hooks/post-tool-use.sh (plan 3 renamed remind_knowledge_update to remind_memory_capture), ../loom-hooks/spawn-guard.sh (header item 5) and ../loom-hooks/tests/run-all.sh. If a block a brief names is gone or moved, tell the owning worker in its spawn prompt.
      2. REPLAY (metaplan step 3): from loom/, run once `bash -c 'TIMEFORMAT=%R; time (loom hook worker-brief < ../doc/plans/token-optim/briefs/delivery-truth/delivery-fixes/replay-payload.json > "$LOOM_SCRATCH_DIR/replay.out")'` (bash, because the session shell may be zsh, which ignores TIMEFORMAT). The installed loom is the pre-plan binary and this stage's own description stands in for token-core's. The sandbox denies the record write, so `{}` on stdout is expected; the elapsed time is the datum. Record it: loom memory note "cause: loom hook worker-brief took <s> s on the recorded token-core spawn against this stage (pre-plan binary); spawn-guard.sh killed it at loom_run_bounded 3; the worker query carries the whole stage description and retrieval time grows with query length (1.66 s at 2,048 bytes, 13 s at about 15.6 KB); planning replay 7.8-9.6 s with a 12.1 KB envelope under max_payload_bytes 16,384; fix: WORKER_QUERY_MAX_BYTES 2,048 and a 5 s bound". If the replay finishes under 3 s, record that too: the hook-duration evidence in the plan stands, and the fix stays.
      3. SPAWN all five in ONE message. W1, W2 and W3 BY AGENT TYPE loom-software-engineer in the BACKGROUND, each with a short fixed prompt plus "Your brief: <path>. Read it in full before anything else." C1 and C2 as loom-codex-forwarder subagents in the FOREGROUND, each with --model gpt-5.6-terra --effort xhigh and an explicit Bash timeout of 600000 ms; the forward task text is only "Your brief: <path>. Read it in full before anything else." (the forward is blocked when its text carries an apostrophe, the substring complete, or the CLI phrase for stage commands, so add nothing to that line). The codex units never run git and never touch .loom/; when the forwarders return, run git status --short and confirm no path outside the five territories below changed and nothing under .loom/. A forward that exits 124 (timed_out) is re-split against the partial tree (C1: steps 1-2, then step 3), never re-forwarded as is; work that still does not fit goes to a fresh loom-software-engineer. Then wait for W1-W3 with one background loom subagents watch. Territories are DISJOINT across both lanes; workers never spawn subagents and never run git.
      4. AFTER THE WORKERS: rustfmt --check --edition 2021 tests/read_intent_contracts.rs tests/spawn_compaction_contracts.rs tests/worker_brief_contracts.rs (a frozen file the formatter would rewrite fails completion; a failure is a dispute-contract, never an edit); cargo fmt --all; then cargo test --all-targets --no-run 2>&1 | tail -40, and route any compile error to a fresh loom-software-engineer for the owning territory (a codex unit's file included), quoting the error. Then one loom-verifier round briefed with the repository's full pre-push gate from loom/ (cargo build; cargo test --all-targets --no-fail-fast; cargo test --doc --workspace --all-features -- --test-threads=1; cargo clippy --all-targets -- -D warnings; cargo fmt --check; RUSTDOCFLAGS="-D warnings" cargo doc --workspace --all-features --no-deps), the hook steps (../scripts/check-hook-syntax.sh; bash ../loom-hooks/tests/run-all.sh; LOOM_HOOK_TEST_BSD=1 bash ../loom-hooks/tests/run-all.sh), every acceptance and after_stage command below verbatim, markdownlint from the repository root (cd .. && git ls-files '*.md' | rg -v '^doc/plans/|^loom/tests/fixtures/' | xargs bunx markdownlint-cli2, the pre-push step), and `wc -l ../loom-hooks/spawn-guard.sh ../loom-hooks/knowledge-orient.sh src/commands/hook/worker_brief.rs tests/integration/hooks_spawn_guard_gate.rs` (each at or under 400; spawn-guard.sh starts at 403). The unfiltered suite lives in that round and in integration-verify, not in this stage's acceptance, because loom plan verify warns on an unfiltered suite outside integration-verify. Each FAIL becomes a fix brief to a fresh worker of the owning territory; then a FRESH verifier.
      5. REVIEW rounds per the signal; fix every finding through a fresh worker.
      6. INTEGRITY: after the final review round, run loom stage review integrity delivery-fixes. Expected events (plan prose, "Expected test-integrity events"): TI-decl-rust, TI-assert-rust, and TI-edit events on src/context/read_receipts/tests.rs, src/context/read_receipts/tests_extra.rs, tests/integration/hooks_read_guard_receipts.rs, tests/integration/hooks_read_guard.rs, tests/integration/hooks_read_guard_repeat.rs, tests/integration/hooks_poll_guard_reads.rs, tests/integration/hooks_spawn_guard_gate.rs, src/commands/hook/tests_worker_brief.rs, src/commands/hook/tests_pre_compact.rs, src/context/tests/delivery/session.rs, and the six bash tests this stage edits. Copy each --event id verbatim from the integrity output: ids carry repo-relative paths (TI-edit-loom/src/..., TI-edit-loom-hooks/tests/...). File ONE loom stage dispute-integrity delivery-fixes naming every listed expected event (repeat --event) with the reason: "The plan removes the read-receipt path and the SubagentStart worker-brief bind (metaplan plan 1, steps 1-2); their tests and the assertions that pinned receipt calls, pending records, bind argv and the receipt epoch go with the code. The frozen contracts pin the replacement: no receipt intent, a record filed at issue, no brief for Codex-bound spawns, a compaction reset." An event outside the list is a defect to fix. No maintainability-baseline.txt row changes.
      7. Commit, then complete.
      
        | Worker | Role | Tier | Files owned | Shared context | Brief path |
        | W1 | Rust delivery | sonnet | src/context/read_receipts.rs; src/context/read_receipts/source.rs; src/context/read_receipts/storage.rs; src/context/read_receipts/transcript.rs; src/context/read_receipts/tests.rs; src/context/read_receipts/tests_extra.rs; src/commands/hook/read_receipt.rs; src/context/mod.rs; src/commands/hook/mod.rs; src/commands/hook/tests_cli_surface.rs; src/cli/types_ops.rs; src/cli/dispatch.rs; src/commands/hook/worker_brief/test_support.rs; src/commands/hook/tests_worker_brief.rs; src/context/delivery.rs; src/context/delivery/session.rs; src/context/tests/delivery/session.rs | common.md | doc/plans/token-optim/briefs/delivery-truth/delivery-fixes/w1-rust-delivery.md |
        | W2 | Read hooks | sonnet | ../loom-hooks/read-guard.sh; ../loom-hooks/_read_discipline.sh; ../loom-hooks/_read_ledger.sh; ../loom-hooks/_bounded.sh; ../loom-hooks/post-tool-use.sh; ../skills/loom-orchestration/SKILL.md; ../loom-hooks/tests/post-tool-use-tool-event.sh; ../loom-hooks/tests/post-tool-use-empty-output.sh; ../loom-hooks/tests/read-guard-sibling-receipts.sh; ../loom-hooks/tests/read-guard-session-ledgers.sh; tests/integration/hooks_read_guard.rs; tests/integration/hooks_read_guard_repeat.rs; tests/integration/hooks_read_guard_receipts.rs; tests/integration/hooks_poll_guard_reads.rs | common.md | doc/plans/token-optim/briefs/delivery-truth/delivery-fixes/w2-read-hooks.md |
        | W3 | Spawn and compaction hooks | sonnet | ../loom-hooks/spawn-guard.sh; ../loom-hooks/subagent-start.sh; ../loom-hooks/knowledge-orient.sh; ../loom-hooks/pre-compact.sh; ../loom-hooks/tests/subagent-start-ledger.sh; ../loom-hooks/tests/knowledge-orient-silent.sh; tests/integration/hooks_spawn_guard_gate.rs | common.md | doc/plans/token-optim/briefs/delivery-truth/delivery-fixes/w3-spawn-compaction.md |
        | C1 | Worker-brief delegate | codex terra | src/commands/hook/worker_brief.rs | common.md | doc/plans/token-optim/briefs/delivery-truth/delivery-fixes/c1-worker-brief.md |
        | C2 | Pre-compact delegate | codex terra | src/commands/hook/pre_compact.rs; src/commands/hook/tests_pre_compact.rs | common.md | doc/plans/token-optim/briefs/delivery-truth/delivery-fixes/c2-pre-compact.md |
      
      CONTRACT SURFACE (the contract session writes three files from this text before any code; each contract is a top-level #[test] fn named after its id with underscores, so `cargo test <name> -- --exact` selects it; every fn under 50 lines, every file under 400 lines and rustfmt-clean; each file declares `#[path = "integration/helpers.rs"] #[allow(dead_code)] mod helpers;` as tests/token_optimization_contracts.rs:3-6 does, spawns the loom binary only through helpers::loom_cmd() and hands it to hook scripts only as LOOM_BIN = helpers::loom_bin_path(), because tests/integration/binary_spawn_guard.rs fails any other spawn; hook scripts run through `helpers::clear_relay_env(&mut Command::new("bash"))` with LOOM_HOME set to a TempDir path):
      - Hook installation: write each script from loom::fs::permissions::constants into a TempDir with mode 0755, as setup_hook does in tests/integration/hooks_read_guard.rs:29-44 (read-guard.sh with HOOK_BOUNDED, HOOK_COMMON, HOOK_READ_DISCIPLINE, HOOK_READ_LEDGER, HOOK_READ_GUARD) and tests/integration/hooks_spawn_guard.rs:32-51 (spawn-guard.sh with the same helpers plus HOOK_SUBAGENT_PREAMBLE as _subagent-preamble.txt and HOOK_SPAWN_GUARD). knowledge-orient.sh needs HOOK_BOUNDED as _bounded.sh and HOOK_KNOWLEDGE_ORIENT.
      - Spawn-guard harness: a stub `loom` first on PATH that appends "$*" to $LOOM_CALLS and, for `hook worker-brief`, sleeps $WORKER_BRIEF_SLEEP seconds when set and prints {"nonce":"0123456789abcdef0123456789abcdef","brief":BRIEF}, where BRIEF contains the line `<!-- loom-worker-brief nonce=0123456789abcdef0123456789abcdef -->` (install_worker_brief_stub and run_with_worker_brief in tests/integration/hooks_spawn_guard_gate.rs:8-63: env LOOM_STAGE_ID, LOOM_MAIN_AGENT_PID = std::process::id(), LOOM_WORK_DIR, LOOM_SESSION_ID, HOME and TMPDIR a TempDir, PATH "<stub bin>:/usr/bin:/bin"; stdin {"tool_name":"Task","tool_input":{...}}). These spawn-guard contracts leave LOOM_BIN unset (clear_relay_env removes it; spawn-guard.sh calls "${LOOM_BIN:-loom}"), so the PATH stub answers hook worker-brief; LOOM_BIN = helpers::loom_bin_path() applies only to the read-guard and knowledge-orient contracts. The hook's stdout is one JSON object; the rewritten prompt is hookSpecificOutput.updatedInput.prompt. A first spawn of a session also carries the loom-orchestration advisory in additionalContext.
      - tests/read_intent_contracts.rs: read-guard-prepares-no-receipt-intent.
      - tests/spawn_compaction_contracts.rs: codex-bound-spawns-receive-no-worker-brief, slow-worker-brief-still-reaches-the-spawn, compacted-ordinary-session-loses-only-its-prompt-record.
      - tests/worker_brief_contracts.rs: issued-worker-brief-files-a-loadable-record, worker-query-stays-under-the-cap, worker-query-cuts-on-a-char-boundary. API: loom::commands::hook::worker_brief::{worker_query_text, WORKER_QUERY_MAX_BYTES} with worker_query_text(stage: &loom::models::stage::Stage, description: &str, prompt: &str) -> String, which joins description, prompt and the stage's own query text with "\n" in that order and cuts the result to at most WORKER_QUERY_MAX_BYTES (2_048) bytes at a character boundary. This file does not compile until C1 adds them (BuildFailed counts as red at freeze).
      - Stage literals: Stage's id, name and plan_id are String / Option<String> and skills is Vec<String> (models/stage/types.rs), so scenario literals such as id: "brief-stage" are written with .to_string() (skills: vec!["loom-rust".to_string()]).
      - Library API for fixtures: loom::verify::transitions::create_stage(&Stage, &work_dir); loom::context::delivery::{load_deliveries, record_delivery, delivery_dir, plan_key, hook_recipient_id, worker_recipient_id, DeliveryRecord, DeliveredNode}; loom::context::local_overlay::local_overlay_key(&root) -> (plan, stage). A record fixture follows `delivered` in src/commands/hook/tests_pre_compact.rs:21-35. A work dir is <root>/.loom/work, created with create_dir_all.
      - The contracts at freeze: the four in read_intent_contracts.rs and spawn_compaction_contracts.rs fail at runtime against 6fee4ecd behaviour; worker_brief_contracts.rs fails to build until worker_query_text exists, which counts as red for its three contracts.
      
      MEMORY: record mistakes, decisions and surprises via loom memory immediately (subagents too; record what the codex units report), including any knowledge file the tree contradicts as loom memory note "stale-knowledge: <file>#<heading> claims X; the tree does Y (file:line). Correction: <text>". NEVER loom knowledge (implementation stage); NEVER Claude Code auto-memory.
    summary: >-
      Deletes the read-receipt path, files each worker-brief record when the brief is
      issued, caps the worker query and raises the spawn guard's bound so loom's own
      stages get briefs again, stops briefing Codex forwarders, and resets an ordinary
      session's prompt record after compaction.
    dependencies: []
    parallel_group: null
    acceptance:
    - cargo build
    - "cargo test --lib commands::hook::"
    - "cargo test --lib context::"
    - cargo test --test integration hooks_read_guard
    - cargo test --test integration hooks_spawn_guard
    - cargo test --test integration hooks_poll_guard
    - cargo test --test maintainability
    - cargo test --doc --workspace --all-features -- --test-threads=1
    - cargo clippy --all-targets -- -D warnings
    - cargo fmt --check
    - RUSTDOCFLAGS='-D warnings' cargo doc --workspace --all-features --no-deps
    - cd .. && ./scripts/check-hook-syntax.sh
    - "cd .. && out=$(bash loom-hooks/tests/run-all.sh 2>&1); rc=$?; printf '%s\\n' \"$out\" | rg '^Results:|^  FAIL:'; printf '%s\\n' \"$out\" | rg -q '^Results: [0-9]+ passed, [0-9]+ failed$' && { [ \"$rc\" -eq 0 ] || ! printf '%s\\n' \"$out\" | rg '^  FAIL: ' | rg -q -v -x -F -e '  FAIL: hook stdin: held-open pipes without GNU timeout' -e '  FAIL: worktree-file-guard: the session scratchpad is open, its surroundings stay blocked'; }"
    - "cd .. && out=$(LOOM_HOOK_TEST_BSD=1 bash loom-hooks/tests/run-all.sh 2>&1); rc=$?; printf '%s\\n' \"$out\" | rg '^Results:|^  FAIL:'; printf '%s\\n' \"$out\" | rg -q '^Results: [0-9]+ passed, [0-9]+ failed$' && { [ \"$rc\" -eq 0 ] || ! printf '%s\\n' \"$out\" | rg '^  FAIL: ' | rg -q -v -x -F -e '  FAIL: hook stdin: held-open pipes without GNU timeout' -e '  FAIL: worktree-file-guard: the session scratchpad is open, its surroundings stay blocked'; }"
    - test ! -e src/context/read_receipts.rs
    - test ! -e src/context/read_receipts
    - test ! -e src/commands/hook/read_receipt.rs
    - "! rg -qF '_loom_read_receipt' ../loom-hooks"
    - "! rg -q -g '!tests/**' 'read-receipt' ../loom-hooks"
    - "! rg -q -g '!tests_cli_surface.rs' -e '--bind-agent' ../loom-hooks src tests"
    setup: []
    files:
    - src/context/read_receipts.rs
    - src/context/read_receipts/**
    - src/context/mod.rs
    - src/context/delivery.rs
    - src/context/delivery/session.rs
    - src/context/tests/delivery/session.rs
    - src/commands/hook/read_receipt.rs
    - src/commands/hook/mod.rs
    - src/commands/hook/tests_cli_surface.rs
    - src/commands/hook/pre_compact.rs
    - src/commands/hook/tests_pre_compact.rs
    - src/commands/hook/worker_brief.rs
    - src/commands/hook/worker_brief/**
    - src/commands/hook/tests_worker_brief.rs
    - src/cli/types_ops.rs
    - src/cli/dispatch.rs
    - tests/integration/hooks_read_guard.rs
    - tests/integration/hooks_read_guard_repeat.rs
    - tests/integration/hooks_read_guard_receipts.rs
    - tests/integration/hooks_poll_guard_reads.rs
    - tests/integration/hooks_spawn_guard_gate.rs
    - tests/read_intent_contracts.rs
    - tests/spawn_compaction_contracts.rs
    - tests/worker_brief_contracts.rs
    - ../loom-hooks/read-guard.sh
    - ../loom-hooks/_read_discipline.sh
    - ../loom-hooks/_read_ledger.sh
    - ../loom-hooks/_bounded.sh
    - ../loom-hooks/post-tool-use.sh
    - ../loom-hooks/spawn-guard.sh
    - ../loom-hooks/subagent-start.sh
    - ../loom-hooks/knowledge-orient.sh
    - ../loom-hooks/pre-compact.sh
    - ../loom-hooks/tests/post-tool-use-tool-event.sh
    - ../loom-hooks/tests/post-tool-use-empty-output.sh
    - ../loom-hooks/tests/read-guard-sibling-receipts.sh
    - ../loom-hooks/tests/read-guard-session-ledgers.sh
    - ../loom-hooks/tests/subagent-start-ledger.sh
    - ../loom-hooks/tests/knowledge-orient-silent.sh
    - ../skills/loom-orchestration/SKILL.md
    auto_merge: null
    working_dir: loom
    stage_type: standard
    artifacts:
    - src/commands/hook/tests_cli_surface.rs
    - src/commands/hook/worker_brief.rs
    - src/commands/hook/pre_compact.rs
    wiring:
    - source: src/commands/hook/worker_brief.rs
      pattern: let text = worker_query_text(&config.stage
      description: >-
        retrieve_pack builds the worker query through the capped builder (the call
        site, not the definition)
      literal: true
    before_stage:
    - command: cargo --version
      description: cargo on PATH
    - command: jq --version
      description: jq on PATH for the hook tests
    - command: rg --version
      description: rg on PATH for the hook tests
    - command: rg -qF 'remind_memory_capture' ../loom-hooks/post-tool-use.sh
      description: plan 3 (doctrine truth) has merged
    after_stage:
    - command: rg -q "loom_run_bounded 5 .*hook worker-brief" ../loom-hooks/spawn-guard.sh
      description: the spawn guard gives the worker-brief delegate 5 seconds
    - command: rg -qF "hook pre-compact >/dev/null" ../loom-hooks/knowledge-orient.sh
      description: >-
        SessionStart compact resets an ordinary session's prompt record (the call, not
        a comment)
    - command: "! rg -qF 'text.push_str(&payload.tool_input' src/commands/hook/worker_brief.rs"
      description: retrieve_pack no longer builds the uncapped query
    context_ceiling_tokens: null
    sandbox: {}
    ultracode: false
    implementers:
    - claude
    - codex
    subagent_timeout_secs: 900
    skills:
    - loom-rust
    contracts:
    - id: read-guard-prepares-no-receipt-intent
      file: tests/read_intent_contracts.rs
      test: read_guard_prepares_no_receipt_intent
      scenario: >-
        installs read-guard.sh and its helpers into a TempDir; writes a 50-line .rs
        file; runs the hook through bash with LOOM_BIN = helpers::loom_bin_path(),
        LOOM_STAGE_ID=stage-1, LOOM_SESSION_ID=sess-1, LOOM_WORK_DIR and TMPDIR a second TempDir
        <work>, LOOM_HOME a scratch dir, and stdin
        {"tool_name":"Read","tool_input":{"file_path":<file>},"agent_id":"agent-1","session_id":"sess-1","cwd":<work>}; asserts
        exit 0, that <work>/hooks/reads/sess-1/receipts does not exist and that
        <work>/loom-reads does not exist
      rejects: >-
        a read-guard.sh that still calls loom hook read-receipt --prepare for an
        eligible text file, which at 6fee4ecd writes an intent under
        <work>/hooks/reads/sess-1/receipts
    - id: codex-bound-spawns-receive-no-worker-brief
      file: tests/spawn_compaction_contracts.rs
      test: codex_bound_spawns_receive_no_worker_brief
      scenario: >-
        with the spawn-guard harness and a stub that answers at once, runs one Task
        spawn each for subagent_type loom-codex-forwarder (model sonnet, prompt starting
        LOOM-CODEX-FORWARD-ONLY), codex:codex-rescue (model sonnet) and
        loom-software-engineer (model sonnet), each with its own LOOM_CALLS file; asserts the two Codex-bound
        runs leave no line containing hook worker-brief in their calls file and no
        loom-worker-brief marker in stdout, and that the engineer run's updatedInput.prompt
        contains the marker line
      rejects: >-
        a spawn guard that still appends the worker brief to loom-codex-forwarder or
        codex:* spawns, as spawn-guard.sh does at 6fee4ecd, or one that stops briefing
        every typed spawn
    - id: slow-worker-brief-still-reaches-the-spawn
      file: tests/spawn_compaction_contracts.rs
      test: slow_worker_brief_still_reaches_the_spawn
      scenario: >-
        with the spawn-guard harness and WORKER_BRIEF_SLEEP=3.5, runs one Task spawn
        for subagent_type loom-software-engineer with model sonnet and prompt "slow brief
        task"; asserts exit 0 and that updatedInput.prompt ends with "\n\n" followed by
        BRIEF
      rejects: >-
        a spawn guard that keeps loom_run_bounded 3 around loom hook worker-brief,
        which kills a delegate slower than 3 s and appends nothing (every loom stage spawn
        since 2026-10-01)
    - id: compacted-ordinary-session-loses-only-its-prompt-record
      file: tests/spawn_compaction_contracts.rs
      test: compacted_ordinary_session_loses_only_its_prompt_record
      scenario: >-
        creates a TempDir checkout <root> with <root>/.loom/work; (plan, stage) =
        local_overlay_key(<root>); files records with record_delivery for
        hook_recipient_id(&stage, Some("session-a")) and hook_recipient_id(&stage, Some("session-b"));
        installs knowledge-orient.sh with _bounded.sh; runs it through bash with current_dir
        <root>, LOOM_WORK_DIR=<root>, LOOM_BIN = helpers::loom_bin_path(), LOOM_HOME a
        scratch dir, no LOOM_STAGE_ID, and stdin
        {"source":"compact","session_id":"session-a","cwd":<root>}; asserts exit 0, empty stdout, that
        load_deliveries(<root>/.loom/work, plan, stage) no longer holds session-a's record and still holds session-b's
      rejects: >-
        a knowledge-orient.sh that exits on compact without calling loom hook
        pre-compact (6fee4ecd), or a reset that clears every prompt record of the checkout
    - id: issued-worker-brief-files-a-loadable-record
      file: tests/worker_brief_contracts.rs
      test: issued_worker_brief_files_a_loadable_record
      scenario: "creates <root>/.loom/work and, with create_stage, Stage { id: \"brief-stage\", name: \"Brief stage\", plan_id: Some(\"PLAN-brief\"), skills: vec![\"loom-rust\"], ..Stage::default() }; runs helpers::loom_cmd() with args hook worker-brief, current_dir <root>, env LOOM_STAGE_ID=brief-stage, LOOM_WORK_DIR=<root>/.loom/work, LOOM_SESSION_ID=parent-1, LOOM_HOOK_CONTEXT=1, and stdin {\"tool_name\":\"Task\",\"tool_input\":{\"subagent_type\":\"loom-software-engineer\",\"prompt\":\"Implement a topic absent from all indexes\",\"description\":\"brief contract\"},\"session_id\":\"claude-parent\",\"cwd\":<root>,\"transcript_path\":<root>/t.jsonl}; parses stdout as {nonce, brief}; asserts load_deliveries(work, plan_key(&stage), \"brief-stage\") holds exactly one record whose recipient_id equals worker_recipient_id(\"brief-stage\", \"parent-1\", &nonce), and that delivery_dir(work, plan_key(&stage), \"brief-stage\").join(\"worker-brief\") holds no file ending .pending"
      rejects: >-
        a worker brief that still files <key>.pending (load_deliveries reads only
        *.json) and waits for a SubagentStart bind that never succeeds, or one that files the
        record under a key no reader derives
    - id: worker-query-stays-under-the-cap
      file: tests/worker_brief_contracts.rs
      test: worker_query_stays_under_the_cap
      scenario: "builds Stage { id: \"q-stage\", name: \"Query stage\", description: Some(\"stage words \".repeat(4_000)), ..Stage::default() } (48 KB); calls worker_query_text(&stage, \"worker task\", &\"prompt words \".repeat(400)); asserts WORKER_QUERY_MAX_BYTES == 2_048, the result's byte length is at most WORKER_QUERY_MAX_BYTES, and the result starts with \"worker task\\nprompt words \""
      rejects: >-
        a worker query that still appends the whole stage description, as retrieve_pack
        does at 6fee4ecd, which keeps retrieval at 8-10 s on loom's own stages
    - id: worker-query-cuts-on-a-char-boundary
      file: tests/worker_brief_contracts.rs
      test: worker_query_cuts_on_a_char_boundary
      scenario: >-
        builds Stage::default() with id "q-stage"; prompt = "a".repeat(2_045) followed
        by "ééé"; calls worker_query_text(&stage, "d", &prompt); asserts it returns
        without panicking, its byte length is at most 2_048, and format!("d\n{prompt}") starts
        with the result
      rejects: >-
        a cap that slices &text[..WORKER_QUERY_MAX_BYTES], which panics when the cut
        falls inside a multi-byte character of the worker's prompt
  - id: integration-verify
    name: Integration Verification
    description: |
      Final verification of doc/plans/token-optim/IN_PROGRESS-PLAN-02-delivery-truth.md. Verify FUNCTIONAL INTEGRATION, not only tests passing. NEVER Claude Code auto-memory.
      Use parallel subagents and skills to maximize performance.
      CONTEXT: read the plan, doc/plans/token-optim/briefs/delivery-truth/delivery-fixes/common.md, loom memory show --all, doc/loom/knowledge/INDEX.md and only the sections the plan cites.
      BUILD AND TEST (zero tolerance; nothing is pre-existing): every acceptance command below, through the canonical verifier the signal names.
      FUNCTIONAL (this session's own shell). Build first (cargo build --manifest-path loom/Cargo.toml) and use bin=$PWD/loom/target/debug/loom for these checks, NOT the loom on PATH: the installed binary predates this plan, so this is the sanctioned exception to "loom from PATH", as in PLAN-token-flow's integration-verify.
      - Replay: bash -c 'TIMEFORMAT=%R; time ("$0" hook worker-brief < doc/plans/token-optim/briefs/delivery-truth/delivery-fixes/replay-payload.json > "$LOOM_SCRATCH_DIR/replay.out")' "$bin" (bash: the session shell may be zsh, which ignores TIMEFORMAT). This replay is a timing diagnostic only and proves no delivery: the sandbox denies the record write (no state directory is writable, sandbox/settings.rs), so {} is expected, and every fail-open return in issue also prints {}. Delivery on populated inputs past the cap is proven by W1's unit test capped_query_still_issues_a_scoped_brief and the contract issued-worker-brief-files-a-loadable-record; delivery on the real repository is the operator check after merge (overview). the elapsed time must be under 5 s (planning measured 7.8-9.6 s before the cap). An early return (stage load failure, empty pack) also prints {}, faster: under 1 s the replay is inconclusive, so also time "$bin" knowledge context --budget-tokens 3000 --query "<the payload's tool_input.description, a newline, then tool_input.prompt, cut to 2,048 bytes>" and record both. Record it with loom memory note "replay after cap: <s> s". At 4 s or more, file a finding with the measurement.
      - "$bin" hook read-receipt --check < /dev/null and "$bin" hook worker-brief --bind-agent a < /dev/null each exit 2 (clap rejects them).
      - rg -n 'read_receipt|read-receipt|bind_pending|bind_nonce|store_pending|_loom_read_receipt' loom/src loom/tests loom-hooks finds nothing outside the three contract files, the two tests that assert absence (loom-hooks/tests/post-tool-use-tool-event.sh, loom/src/commands/hook/tests_cli_surface.rs) and the plan-verify fixture string "Terra read-receipts" in loom/src/plan/schema/structural_checks/worker_table_tests.rs.
      CODE REVIEW: parallel loom-code-reviewer subagents. Security: the worker query cut on untrusted prompt bytes; the record write path (hashed key, locked atomic write, no symlink follow); knowledge-orient.sh piping an untrusted payload to loom hook pre-compact. Correctness: rule 1 of _read_discipline.sh unchanged after rule 2's removal (deny path, outline, tier-1 hint, sibling advisory); the forwarder predicate; the 5 s bound; the compaction reset deleting only the payload session's record. Architecture: nothing orphaned (dead helpers, imports, comments naming removed code); spawn-guard.sh and every Rust file at or under 400 lines. Test coverage: each contract fails against the behaviour it rejects.
      SUGGESTIONS: weigh every pending reviewer suggestion the signal lists; resolve each one implemented with loom memory resolve <id> --outcome implemented --reason <what changed>.
      Record discoveries to loom memory for knowledge-distill, including any knowledge file the tree contradicts: loom memory note "stale-knowledge: ...".
    summary: >-
      Confirms on the merged tree that the full suite and the hook suites pass, that
      the recorded spawn now finishes inside the spawn guard's bound, and that the
      removed hook commands are gone.
    dependencies:
    - delivery-fixes
    parallel_group: null
    acceptance:
    - cargo build --manifest-path loom/Cargo.toml
    - cargo test --manifest-path loom/Cargo.toml --all-targets --no-fail-fast
    - >-
      cargo test --manifest-path loom/Cargo.toml --doc --workspace --all-features --
      --test-threads=1
    - cargo clippy --manifest-path loom/Cargo.toml --all-targets -- -D warnings
    - cargo fmt --manifest-path loom/Cargo.toml --check
    - >-
      RUSTDOCFLAGS='-D warnings' cargo doc --manifest-path loom/Cargo.toml
      --workspace --all-features --no-deps
    - ./scripts/check-hook-syntax.sh
    - bash loom-hooks/tests/run-all.sh
    - LOOM_HOOK_TEST_BSD=1 bash loom-hooks/tests/run-all.sh
    - >-
      git ls-files '*.md' | rg -v '^doc/plans/|^loom/tests/fixtures/' | xargs bunx
      markdownlint-cli2
    setup: []
    files: []
    auto_merge: null
    working_dir: '.'
    stage_type: integration-verify
    artifacts: []
    wiring:
    - source: loom/src/cli/dispatch.rs
      pattern: HookCommands::WorkerBrief => hook::worker_brief::worker_brief()
      description: the hook CLI dispatches the flag-free worker-brief delegate
      literal: true
    context_ceiling_tokens: null
    sandbox: {}
    ultracode: false
    implementers:
    - claude
    skills:
    - loom-rust
    - loom-security-audit
  - id: knowledge-distill
    name: Knowledge Distillation
    description: |
      Curate all stage memories into permanent knowledge. NEVER Claude Code auto-memory.
      SINGLE-AGENT: do NOT spawn subagents; memories are compact summaries; keep code spot-reads narrow.
      START with loom memory pending --group (corrections, mistakes, decisions, other); read the plan and the knowledge sections it touches.
      CORRECTIONS FIRST: apply every stale-knowledge memory in place with loom knowledge replace-section <file> "<heading>" "<body>", never loom knowledge update; read each replace-section result line (a non-matching heading appends and says so). Known sections, each rewritten to current truth (history goes only to mistakes topics):
      - architecture/token-accounting-and-receipts.md: "Read Receipt Lifecycle" (the path is removed; rereads are measured from transcripts by the reads section of loom usage; the advisory ledger records attempts; if plan 2's measurement shows same-agent rereads above 2% of Read characters, the revival design is completion at the agent's next Read from a transcript offset recorded at prepare time); "Worker Brief Lifecycle" (records filed at issue as <key>.json; no bind; Codex-bound types get no brief; WORKER_QUERY_MAX_BYTES 2,048; the delegate's 5 s bound; the cause found, citing the plan's evidence table); "Hook Phases" (the read-receipt, worker-brief --bind-agent and receipt-epoch rows go; knowledge-orient.sh's compact reset is added); "Open Runtime Uncertainties" (the receipt bullets go; the forward-receipt bullet stays for plan 2); "Scope and Evidence" and the page's frontmatter source list (drop the read_receipts files).
      - concerns/token-accounting-and-proof-defects.md: "Delivery-Proof and Quota-Continuity Defects" (the read-receipt, stranded-intent, worker-brief and ordinary-compaction bullets are resolved and go; the forward-receipt and quota bullets stay for plan 2; the stage spawn-credit bullet stays as an accepted limit); and the read-receipt bullet in "Open Follow-Ups After the Token-Optimization Plan (2026-09-13)".
      - architecture/context-retrieval-state.md: "Delivery Records and Epoch Suppression" (replace the clause that has loom hook pre-compact rotate the "read-receipt epoch" with: loom hook pre-compact runs from pre-compact.sh in stage sessions and from knowledge-orient.sh on SessionStart compact in ordinary sessions, and deletes just that session's prompt record). In "Base vs Overlay Ownership", delete the history sentence about "a known gap an earlier version of this section recorded".
      - architecture/skill-catalog.md, subsection "### Why Most Skills Live Outside `~/.claude/skills`": delete the sentence "Rule 2 advises on a repeated read only after a Rust read receipt proves the earlier result, and receipts do not form" and its link to the concerns bullet this stage deletes. Rule 1 is the only line-count rule (loom-hooks/_read_discipline.sh, loom_read_discipline_check). replace-section matches "## " headings; if it does not match this "### " subsection, edit it with the Edit tool and run loom knowledge sync.
      - mistakes/hooks-shell-portability.md cites read_receipts.rs:369, a path this plan deletes: keep the history and mark the citation "(file removed by PLAN-02-delivery-truth)".
      - architecture/hook-system.md (the "**The delegates**" paragraph under "Prompt-Submit Hooks and the `loom hook` Delegates": it names the read-receipt delegate, worker-brief --bind-agent, the pre-compact read-receipt epoch rotation, and "2 s for read-receipt, 3 s for ... both worker-brief calls", which becomes 5 s for the one worker-brief call), entry-points.md (the read_receipts.rs entry), concerns.md (the summary line naming read-receipt runtime uncertainties), architecture.md (the "Token Accounting and Receipts" summary naming forward/read/worker-brief receipt lifecycles).
      Then curate: the cause of the missing worker briefs as a mistakes entry (What happened, Why, Prevention, Fix: a best-effort hook delegate under a fixed bound fails silently when its cost grows with the repository; measure the delegate on the largest repository before choosing a bound), plus every mistake the stages recorded. TIER ROUTING: findings about 40 lines or fewer go inline in the tier-1 file; larger ones via loom knowledge update <category>/<slug> with a 2-4 line tier-1 summary and link. INDEX.md regenerates on every knowledge write.
      README/CONTRIBUTING: nothing user-facing changed (the removed commands were hidden); record that with loom memory note.
      SUGGESTIONS: record every unimplemented reviewer suggestion in concerns or the topic it belongs to, then resolve it promoted, merged or discarded.
      RECEIPTS: every memory taken into knowledge gets loom memory resolve <id> --outcome promoted|merged|discarded|deferred right after the write that used it; finish with loom memory pending --strict and resolve whatever it lists.
      LAST, if this stage removed structural issues, ratchet the baseline: loom knowledge check --write-baseline doc/loom/knowledge/check-baseline.txt
    summary: >-
      Rewrites the knowledge pages that described read receipts, worker-brief binding
      and compaction resets, and records why loom's stages issued no worker brief.
    dependencies:
    - integration-verify
    parallel_group: null
    acceptance:
    - loom knowledge check --strict --baseline doc/loom/knowledge/check-baseline.txt
    - loom memory pending --strict
    setup: []
    files:
    - doc/loom/knowledge/**
    auto_merge: null
    working_dir: '.'
    stage_type: knowledge-distill
    artifacts: []
    wiring: []
    context_ceiling_tokens: null
    sandbox: {}
    ultracode: false
    implementers:
    - claude
```

<!-- END loom METADATA -->
