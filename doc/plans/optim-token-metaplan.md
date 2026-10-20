# Token Optimization Metaplan

This metaplan orders the next round of loom efficiency work into six plans and a set of policy notes. It was assembled on 2026-10-18 against HEAD `6fee4ecd` (branch `token-optimization`) and revised the same day after a pressure test against the code, the installed Claude Code and Codex CLIs, and this machine's transcripts. It starts from the review in [optimizations-suggestions-fable-astra-2026-10-18.md](../optimizations-suggestions-fable-astra-2026-10-18.md). Every claim in that review was re-verified against the tree, along with every recommendation in five earlier reports. Those reports were deleted after review, and their complete disposition is recorded at the end of this file, so no suggestion from them is lost or silently dropped.

This file is not a loom plan. Each numbered section describes one plan to be authored later with `/loom-plan-writer` as `doc/plans/PLAN-<name>.md`, checked with `loom plan verify`, committed together with its briefs (`loom run` refuses uncommitted plan inputs, `commands/run/plan_inputs.rs:37-43`), and run with `loom run`. This metaplan is committed too, because knowledge pages cite it. Until plan 3 fixes the bootstrap heuristic, `loom plan verify --strict` fails on any plan without a bootstrap stage (`plan/schema/validation.rs:1109-1137`); such a plan records that one warning as accepted in its overview. The sections give each plan author the problem, the evidence, the settled design decisions, the owning files, the traps and the acceptance bar. File and line references are to `6fee4ecd`. A plan author re-reads the named ranges before writing stage briefs, and anchors every brief by symbol as well as line.

## Objective and binding constraints

The objective is fewer provider tokens per accepted run, with no loss of correctness, no weaker verification or review, and no slower critical path. Claude and Codex consumption stay in separate columns: no plan sums them, and no plan claims a saving that moved work from one provider to the other. No plan carries a percentage target: the August study's 35% goal was never demonstrated, and this metaplan sets no replacement number.

The review separates three costs, and every plan names which one it targets:

| Cost | Observable event | Plans |
| --- | --- | --- |
| Explicit rereading | The same recipient requests unchanged bytes again within a valid context epoch | 1 (measured at about 0.4% of Read characters) |
| Repeated investigation | A successor reconstructs a conclusion an accepted dependency already established | 4 |
| Resident context replay | Content already in a conversation is re-read as input on every later turn | 1, 3, 4, 5 |

### How a saving is shown

A whole accepted run is too expensive to repeat for evidence. The last completed plan, PLAN-token-flow (six stages, 2026-10-14, about 3.8 hours), used 1,522 Claude requests, 195.2M resident input tokens and 0.84M output tokens, and moved the seven-day Claude quota from 83% to 87%. At an assumed 20% run-to-run variation, a 10% saving needs about 63 paired runs to resolve: about five weeks of the whole subscription for one claim. A subscription verdict is out of reach for any plausible saving, because quota is reported in whole percents: two endpoints carry 2 points of uncertainty against 4 points of consumption per run. Evidence therefore follows the kind of change:

1. **A byte change that leaves agent behaviour alone** (false doctrine removed, a template body cut, a brief no longer appended to a spawn that never reads it) is accepted on a deterministic replay with unchanged gate and review outcomes. The replay regenerates the admitted text at the base and candidate revisions against one frozen fixture and reports the exact byte difference per surface. It spends no provider tokens. Plan 2 builds it.
2. **A change in what agents are given that can change what they do** (plan 4) adds frozen single-stage replays: recorded worker prompts or a recorded stage, rerun three times per arm with the same model and effort, judged on the unit's own tests, an independent review, and per-provider totals with mean and standard deviation.
3. **A policy default** (effort, ceilings, budgets, lifecycle) changes only when the existing `loom usage` sections show an effect large enough to resolve with at most 16 pairs. Section 6 records each such question and the cheap evidence it waits on.

A smaller first prompt or fewer reads alone never counts as a saving. The replay reports resident bytes times the recorded requests that follow the admission point, labelled an estimate.

### Runtime facts that bound every plan

- **The prompt cache is not shared between loom sessions.** Claude Code shares cache entries only between sessions running in parallel in the same directory (<https://code.claude.com/docs/en/prompt-caching#cache-scope>), and every stage runs in its own worktree. Codex keys its prompt cache per session or thread (`prompt_cache_key`). No plan may rest a saving on cache reuse across sessions: a session saves only content it never receives or never generates.
- **Claude Code writes a call's `tool_result` row only after that call's PostToolUse hooks finish.** In 99 of 99 sampled PostToolUse hook rows in this repository's stage transcripts, and 828 of 831 across all projects, the hook row precedes its result row. A PostToolUse hook that reads the transcript for the current call's result finds nothing. The payload's own `tool_response` carries the result (`loom-hooks/codex-forward-result.sh:32-40` reads it).
- **The stage sandbox denies writes to `.loom` and `~/.claude/projects`** (`sandbox/control_surfaces/session_denies.rs:71,97-99`; `sandbox/settings.rs:267-269`). Bash in a stage session, and in every subagent it spawns, can write the worktree and `$LOOM_SCRATCH_DIR` (in the capsule's `allowWrite`). Hooks run outside the sandbox and write state. The CLI and the Codex wrapper cannot, from inside a stage. `loom stage output set` writes the stage file directly and has no relay kind (`commands/stage/output.rs:52`; `relay/kind.rs:12-23`), so stage sessions cannot set outputs, and no recorded transcript calls it.
- **Bash output is already bounded per call** at 12,000 characters in stage sessions (`orchestrator/terminal/native/wrapper.rs:286`, since `b686b61b`). Since `cf9341e5` a standard stage's gate runs in a fresh `loom-verifier`, outside the main session.
- **Spawns cost.** A subagent's first request carried a median 19,146 resident tokens in PLAN-token-flow. Subagents reason best when they finish early, so content added to every spawn is paid on every spawn, and content a parent can put in its own brief belongs there.
- **No session in PLAN-token-flow passed 400,000 resident tokens.** The main maximum was 386,980 and the subagent maximum 299,506.
- **One checkout runs one plan.** `loom init` refuses while `.loom/work` holds state (`commands/init/execute.rs:124-132`), so plans run in sequence and a shared file only needs an order.
- **Installed assets are shared.** Stage sessions run the installed hooks, agents, skills and template under `~/.claude`. After a plan that edits them merges, the operator runs `./dev-install.sh`.

### Rules for every plan

- **Necessity.** Every new command, flag, hook, telemetry event, config key, state file or protocol names the problem it solves and why the closest existing block cannot carry it. When a mechanism cannot work and its measured saving is small, it is removed.
- **The gate.** Every code stage runs, from `loom/`, the pre-push gate (`loom/.githooks/pre-push:28,97,107,126`): `cargo build`, `cargo test --all-targets --no-fail-fast`, `cargo test --doc --workspace --all-features -- --test-threads=1` (target selection excludes doctests), `cargo clippy --all-targets -- -D warnings`, `cargo fmt --check` and `RUSTDOCFLAGS="-D warnings" cargo doc --workspace --all-features --no-deps`. Plain `cargo test` and unit filters do not stand in for it (`conventions/git-and-build-workflow.md`, "`cargo test` Is Not This Repo's Test Gate"). A stage that edits `loom-hooks/` also runs `./scripts/check-hook-syntax.sh`, `bash loom-hooks/tests/run-all.sh` and `LOOM_HOOK_TEST_BSD=1 bash loom-hooks/tests/run-all.sh` (CI: `.github/workflows/ci.yml:280,290,295`); `cargo test` runs none of these. A new hook test gets its `run_test` line in `run-all.sh`.
- **Size limits.** `cargo test` runs the maintainability scanner (`loom/tests/maintainability/scanner.rs:6`, 400 lines per file) and the exact ledger `loom/maintainability-baseline.txt`, which fails on growth and on shrinkage. Files at or near the limit: `commands/hook/worker_brief.rs` (400), `orchestrator/terminal/native/tests_confinement_e2e.rs` (400), `commands/knowledge/bootstrap/mod.rs` (400), `fs/memory/mod.rs` (399), `orchestrator/signals/tests_doctrine.rs` (399), `loom-hooks/spawn-guard.sh` (398), `orchestrator/signals/retrieval.rs` (397), `commands/hook/user_prompt_compose.rs` (396), `loom-hooks/codex-forward.sh` (396), `commands/usage/mod.rs` (392). New code goes in new modules, and tests move out first.
- **Edited copies.** Plans edit the repository copies of skills, agents, hooks and `CLAUDE.md.template`. The installed copies are regenerated by `./dev-install.sh` and never edited.

Operator decisions recorded on 2026-09-18 bind every plan. Opus stays the default main-session tier for standard stages. There is no working budget on a main session, and subagent tasks are sized to finish under about 400,000 tokens. Delegation is a cost decision, tokens times model tier. The installed template also serves sessions outside loom plans, so guidance is relocated and never deleted. The 800,000-token ceilings stay; 150,000/120,000 must not return. Distill runs on sonnet, and that tier is settled. Lowering model capability, effort or verification coverage to hit a token number is excluded.

## State of the tree at 6fee4ecd

The review was written at `2bd9f736`. None of the memory, delivery, receipt or usage code changed between that revision and HEAD, but re-verification found errors in the review and defects it did not see:

| Finding | Evidence | Consequence |
| --- | --- | --- |
| Read receipts cannot form. `result_body` requires an `is_error` field that successful `Read` results omit (1,114 of 1,128 stage Read results lack it; the 14 present are `true`), and completion runs at PostToolUse, before the result row exists. The fixtures write the pair before calling complete, so tests pass. | `context/read_receipts/transcript.rs:142`; `loom-hooks/post-tool-use.sh:257`; `read_receipts/tests.rs:78,143` | Rule 2 of `_read_discipline.sh` never escalates. Identical same-agent rereads are 29 of 7,612 stage Read results, about 0.4% of Read characters. Plan 1 removes the path. |
| Receipt intents are prepared in every session and completed only in stages, and stage sessions strand them too (18 in one session). | `loom-hooks/read-guard.sh:107-111`; `post-tool-use.sh:229` | Removed with the receipt path in plan 1. |
| Hard denial of a reread needs `[hooks] deny_enabled = true` (off by default) and a live main-agent ancestor. | `loom-hooks/_read_discipline.sh:230`; `_common.sh:1604` | The review's "deny a second" was opt-in. |
| Worker-brief delivery records never bind: 201 `worker-brief/*.pending` files and 0 bound records exist across this machine's projects, and the record loader reads only `*.json`. In PLAN-token-flow and guard-integration no subagent received a brief at all (0 of 44 carry the marker). | `commands/hook/worker_brief.rs:297-348`; `loom-hooks/subagent-start.sh:132-137`; `context/delivery.rs:273-277` | Successor ranking never sees worker deliveries. Plan 1. |
| Forward receipts never form: `loom hook forward-receipt` reads the transcript at PostToolUse and skips calls with no result. | `commands/hook/forward_receipt.rs:62-70`; `post-tool-use.sh:251` | `forward_join.rs` has no Codex thread linkage. Plan 2. |
| PreCompact is registered only for stage sessions, and stage sessions do not compact in practice: 0 compaction boundaries in 282 stage transcripts, while 11 ordinary-session transcripts compacted. A compacted stage session re-reads its signal. | `fs/permissions/hooks/config.rs` `build()`; `loom-hooks/session-start.sh:147-155` | The spawn-credit trace (`commands/hook/user_prompt.rs:239-248`, `context/delivery/session.rs:116-123`) needs no marker. Plan 1 resets ordinary sessions after compaction. |
| Codex units already receive a Knowledge Brief from loom's Codex `UserPromptSubmit` hook (7 of 7 recent unit rollouts carry one). The spawn guard also appends a worker brief of about 10,000 characters to `loom-codex-forwarder` prompts; 18 of the 94 forwarders that received one relayed it. | `fs/permissions/codex_hooks.rs:114-121`; `loom-hooks/spawn-guard.sh:298` | Plan 1 stops appending to forwarders. |
| Receipts do NOT consume own-stage memory slots, contrary to the review; they are filtered before the last-10 cut. Standard stages wrote 5 receipts in 50 journals; IntegrationVerify and KnowledgeDistill write theirs after every standard stage finishes. | `fs/memory/query.rs:12` | Plan 4 filters with `settled_ids` and keeps Decisions. |
| Memory is plentiful: 50 standard-stage journals hold 1,011 unique entries (median 19.5 per stage), and 82% carry `--evidence`. | `.loom/memory/archive/*/memory/*.md` | The gap is selection. Plan 4. |
| Memory entries carry no stage id, commit or evidence hash, and their `session` field is agent-supplied. `parse_journal` overwrites the journal's stage from any `**Stage**:` line, entry bodies included, and a body line `---` or `###` splits the entry. | `fs/memory/types.rs:145-166`; `fs/memory/parser.rs:17-31`; `orchestrator/core/inbox_drain/apply.rs:122-130` | The producer is the journal a reader opened (`fs/memory/staged.rs:17-24`), never a field. Plan 4 escapes body lines at write. |
| No merge-commit SHA is stored, but every stage lands through a two-parent merge whose second parent is `Stage.completed_commit` (`e133f68c` has parents `5f489d2c` and guard-core's `d4cd3148`). `merge_assumed` marks an unverified operator assertion. | `models/stage/types.rs:396-398,425-426`; `git/merge/tree.rs:1-7`; `git/merge/mod.rs:85-90` | Plan 4 derives each dependency's change from git. |
| The dependency boost in retrieval never fires in this repository. It compares declared `files`/`artifacts` to node paths by exact string, so a glob matches nothing and a `working_dir`-relative path never matches. | `orchestrator/signals/retrieval.rs:100-123`; `context/rank_source/paths.rs:45-60` | Plan 4. |
| The IntegrationVerify/KnowledgeDistill keyword checklist scans raw journal lines, including headings and `*Receipt:*` lines, with no cap, no receipt handling and zero tests. | `orchestrator/signals/generate/enrichment.rs:87-124,157-180` | Replaced by plan 4; the cross-stage summary (`:17-63`) stays. |
| A failed journal append becomes a refusal; the last run applied 259 memory relays and refused none. | `inbox_drain/apply.rs:126-128`; `inbox_drain/session_pass.rs:150-172` | No plan owns it until a refusal is observed. |
| Predecessor outputs and the embedded handoff render unbounded, but no recorded transcript sets an output, and a handoff the successor needs is read whole either way. | `orchestrator/signals/format/helpers.rs:53-85`; `format/sections.rs:445-458` | No cap is added. |
| Quota continuity treats any `resets_at` change as a reset, the comparison compares `resets_at` exactly across arms, and the Claude windows alternate by one second between polls. | `quota/history.rs:211-212`; `commands/usage/comparison_quota.rs:88,121-128`; `.loom/work/quota/history/claude.jsonl` | Plan 2 applies a tolerance. Subscription claims stay out of acceptance. |
| The `--compare` token verdict is per-dimension dominance with worst-outcome aggregation and no variance, and its latency rule rejects a candidate one millisecond slower in one pair. | `commands/usage/comparison_token.rs:85-101`; `comparison_eval.rs:177-179,228-245` | Plan 2. |
| Nothing writes the run evidence that `--compare` consumes, and `--project <dir>` reads only the directory's own transcript slug, missing every stage worktree slug. | `models/execution_receipt.rs` (consumers only); `commands/usage/discovery.rs:105-106` | Plan 2. |
| The stable-prefix split behind `prompt_cache_split` is inert: its probe looks for `--append-system-prompt-file`, which the installed `claude --help` does not print, and once enabled the signal still carries the stable prefix. | `orchestrator/terminal/native/capsule.rs:71`; `orchestrator/signals/format/mod.rs:100` | Section 6 drops the cache experiment. |
| The worker brief is stage-only: `spawn-guard.sh` gates it on `GATE_PASSED`, and `worker_brief.rs` returns nothing without `LOOM_STAGE_ID`. The script header (`:19-20`) says otherwise. | `loom-hooks/spawn-guard.sh:108-111,298`; `commands/hook/worker_brief.rs:56-80` | Plan 3 corrects the header; section 6 records the interactive question. |
| The orchestration skill is 24,573 bytes (23,998 at the review), with no size test. `maxTurns: 1000` is set on four of six agent definitions. | `skills/loom-orchestration/SKILL.md`; `agents/*.md` | Plan 5 caps and slims the skill. |

## Plan map

| # | Plan file (to author) | Order | Depends on | Targets |
| --- | --- | --- | --- | --- |
| 3 | `PLAN-01-doctrine-truth-and-plan-guards.md` | 1st | none | wasted correction turns; wasted runs |
| 1 | `PLAN-02-delivery-truth.md` | 2nd | 3 | per-spawn resident bytes; proofs no later plan may count while broken |
| 2 | `PLAN-03-usage-evidence.md` | 2nd or 3rd | none | the replay and usage evidence later plans are judged on |
| 4 | `PLAN-04-dependency-findings.md` | 4th | 1, 2, 3 | repeated investigation by successors, contract writers and workers |
| 5 | `PLAN-05-guidance-and-gate-output.md` | 5th | 2, 3 | resident replay from instructions and gate output |
| 6 | Policy notes (no plan file) | — | 2 | defaults: effort, ceilings, lifecycle, budgets |
| 7 | `PLAN-06-sandbox-route-closure.md` | separate track | 3 | security remainder of the retired proposal |

```text
3 doctrine truth + plan guards
  -> 1 delivery truth          (after 3: post-tool-use.sh, run-all.sh)
  -> 2 usage evidence          (independent of 1; either order)
  -> 4 dependency findings     (needs 1's worker briefs, 2's replay, 3's sections.rs)
  -> 5 guidance + gate output  (needs 3's template and hook edits, 2's replay)
7 sandbox route closure: after 3; not beside 5 (agents/*.md)
6 policy notes: questions answered from plan 2's evidence; no code
```

Files more than one plan edits, and their order:

| Plans | Shared file | Order |
| --- | --- | --- |
| 3, 1, 2 | `loom-hooks/post-tool-use.sh` (3: reminder `:269-298`; 1: receipt completion `:229-259`; 2: the forward-receipt call `:251`), `loom-hooks/tests/run-all.sh` | 3 first; 1 and 2 rebase on each other |
| 3, 4 | `orchestrator/signals/format/sections.rs` and its ledger rows | 3 first |
| 3, 5 | `CLAUDE.md.template` (3: `:75`; 5: Rules 3/3b, Rule 14), `loom-hooks/session-start.sh:153`, `post-tool-use.sh` | 3 first |
| 3, 5 | `skills/loom-plan-writer/SKILL.md`, `references/bookend-stages.md` | 3 first; plan 5 adds no plan-writer cap |
| 1, 4 | none: plan 4 does not edit `worker_brief.rs` | — |
| 5, 7 | `agents/*.md` (7: tool-route text) | 5 first |

## 1. Delivery truth (`PLAN-02-delivery-truth.md`)

Three delivery proofs loom relies on do not form, and one delivery path pays for text nobody reads:

- **Read receipts cannot form** (state table). Even with `is_error` treated as optional, completion at PostToolUse (`post-tool-use.sh:257`) runs before Claude Code writes the call's result row. The saving ceiling is small: 29 identical same-agent rereads in 7,612 stage Read results, about 55,000 tokens over 545 transcripts, before excluding legitimate rereads after edits.
- **Worker-brief delivery records never bind.** `store_pending` (`commands/hook/worker_brief.rs:297-315`) writes `<key>.pending`. `subagent-start.sh:132-137` calls `loom hook worker-brief --bind-agent`, which looks for the nonce in the child transcript (`bind_pending`, `bind_nonce`, `marker_nonce`, `:317-396`), and the record loader reads only `*.json` (`context/delivery.rs:273-277`). Across this machine there are 201 pending files and 0 bound records.
- **No worker brief reached a subagent in loom's own runs.** 0 of 44 subagents in PLAN-token-flow and guard-integration carry a `loom-worker-brief nonce=` marker, and `.loom/work/context/PLAN-token-flow/` has no `worker-brief/` directory, while 541 transcripts in other projects carry one. `issue` (`worker_brief.rs:127-148`) returns nothing when the pack is empty, when the encoded envelope exceeds `max_payload_bytes` (16,384, `:143-145`), or when `store_pending` fails. The cause in this repository is not yet known.
- **Codex forwarders carry a brief they rarely relay.** The spawn guard appends the worker brief to `loom-codex-forwarder` prompts (`spawn-guard.sh:298-320`). 94 of 187 forwarder transcripts carried one (median about 10,000 characters), 18 relayed it, and a forwarder makes a median of 3 model calls. Codex units get their own brief from loom's Codex `UserPromptSubmit` hook (`fs/permissions/codex_hooks.rs:114-121`, `user-prompt-context.sh`).
- **Ordinary sessions keep their prompt delivery record across compaction.** No PreCompact hook is registered outside stages, so knowledge suppressed as already delivered stays suppressed after the session lost it.

The plan:

1. **Remove the read-receipt path.** Delete the intent prepare call (`read-guard.sh:107-111`), the completion branch in `post-tool-use.sh:229-259` (the forward-receipt call at `:251` stays for plan 2), rule 2's receipt check in `_read_discipline.sh` (`_loom_read_receipt_proven`, around `:277-290`), `loom hook read-receipt` (`cli/dispatch.rs:146-153` and its clap definition) and `context/read_receipts*` with their tests. Before deleting, `rg -n 'read_receipt|read-receipt|receipt_eligible'` across `loom/`, `loom-hooks/` and `doc/` lists every consumer. The `reads` section of `loom usage` measures rereads from transcripts and stays. The sibling ledger (`_read_ledger.sh`) stays. Its advisory at `:305` says a file "was read" and is reworded to "requested whole by N agents (an attempt, not proof of delivery)", together with its pin at `loom-hooks/tests/read-guard-sibling-receipts.sh:68`. If plan 2's reread measurement later shows same-agent rereads above 2% of Read characters, the revival design is completion at the agent's next Read, from a transcript offset recorded at prepare time.
2. **File worker-brief records at issue.** The brief rides the spawn's own `updatedInput`, so `store_pending` writes the bound `<key>.json` when the spawn guard appends it. `bind_pending`, `bind_nonce`, `marker_nonce`, the `--bind-agent` flags and the SubagentStart call (`subagent-start.sh:132-137`) are deleted, which takes `worker_brief.rs` below 400 lines.
3. **Find why loom's own stages issued no brief.** The first stage replays `loom hook worker-brief` on a recorded spawn payload from a PLAN-token-flow stage (transcripts under `~/.claude/projects/-home-dkaponis-src-loom--worktrees-token-core/`) and records which return in `issue` fired. If the envelope exceeds `max_payload_bytes`, the fix drops pack items from the tail until it fits, so a brief is never abandoned whole. The stage records the cause with `loom memory note`.
4. **Stop appending briefs to forwarders.** The condition at `spawn-guard.sh:298` gains `[[ "$AGENT_TYPE_REQ" != loom-codex-forwarder ]]`, with no new lines (the file is at 398).
5. **Reset ordinary sessions after compaction.** `knowledge-orient.sh`, registered globally on SessionStart (`fs/permissions/hooks/config.rs:16`), exits on `source == "compact"` at `:49-52`. Before that exit it pipes the payload to `loom hook pre-compact`, which deletes the prompt recipient's record (`commands/hook/pre_compact.rs:61-96`). The stage confirms that `loom hook pre-compact` finds the work directory from the payload's `cwd` outside a stage. A SessionStart with that source fires once, after a compaction happened. Stage sessions keep `pre-compact.sh` as it is.
6. **Correct the text.** The doc comments at `pre_compact.rs:15-17`, `context/delivery/session.rs:13-16` and `loom-hooks/pre-compact.sh:35-37` state the current behaviour. The knowledge-distill stage rewrites the read-receipt sections of `doc/loom/knowledge/architecture/token-accounting-and-receipts.md`, the delivery-proof section of `concerns/token-accounting-and-proof-defects.md`, and "Delivery Records and Epoch Suppression" in `architecture/context-retrieval-state.md` (removing its history sentence at `:99-100`).

Dropped, with the reason:

- **Repairing receipt completion.** Under 0.5% of Read input, and transcripts prove delivery after the fact.
- **A compaction marker for stage sessions.** The case has not occurred, and re-anchoring re-reads the signal.
- **A new `compaction-reset.sh` and its registration.** `knowledge-orient.sh` is already registered in every loom-initialized repository.
- **Recording explicit `loom knowledge context` output.** At most 25 overlapping prompt briefs (each at most 1,500 tokens) in 102 ordinary sessions, and in a stage the CLI cannot write delivery state.
- **Wrapper-side retrieval for Codex, nonce binding to rollouts, and a credentialed cross-OS test.** Codex already receives a brief, the wrapper cannot write `.loom` in a stage, and nothing would consume the binding.
- **Codex read suppression and a Codex compaction reset.** Codex CLI 0.163.0 emits PreToolUse, PermissionRequest, PostToolUse, PreCompact, PostCompact, SessionStart, SessionEnd, SubagentStart, SubagentStop, UserPromptSubmit, Stop and Interrupt, but has no Read tool (its reads are shell commands), and a unit is one prompt bounded at 540 seconds.

Acceptance:

- A hook test runs `read-guard.sh` on a Read payload and asserts that no intent file appears.
- A hook test runs `spawn-guard.sh` with a stub `loom hook worker-brief` on a `loom-codex-forwarder` payload and on a `loom-software-engineer` payload. Only the second prompt gains a `loom-worker-brief` marker.
- A unit test issues a brief and asserts that `load_deliveries` returns its record.
- A hook test feeds `knowledge-orient.sh` a `source: "compact"` payload against a fixture work directory and asserts that the prompt record is gone.
- The cause found in step 3 has a regression test. After merge, one stage run of this repository shows a worker transcript whose first user message carries the brief marker and the brief's first pack item verbatim (a marker alone does not prove the body arrived; `worker_brief.rs:215-224` renders them separately), and a bound record; this operator check is recorded in the plan's overview.
- The gate, with the hook steps. The change is a byte change in the evidence classes above, so no paired run is needed.

## 2. Usage evidence (`PLAN-03-usage-evidence.md`)

Plans 4 and 5 are judged on a deterministic replay, and plan 4 also on frozen single-stage replays (see "How a saving is shown"). `loom usage` and `loom usage --compare` exist, but they cannot yet produce that evidence honestly:

- `--project <dir>` selects only the directory's own transcript slug (`commands/usage/discovery.rs:105-106`). Stage sessions write under `<slug>--worktrees-<stage-id>`, and loom removes the worktree after merge; the default path (`:108-119`) adds only worktrees that still exist. This repository has 38 such slugs and 0 live worktrees, and the totals still read `Measured`.
- Codex rows are not scoped to a project (`usage/mod.rs:287-288`), and forward receipts never form (state table), so `forward_join.rs` joins nothing.
- `--since` and `--until` accept a date or a relative duration, never an instant (`usage/time_range.rs:27`).
- The token verdict is per-dimension dominance (`comparison_token.rs:85-101`), so an intervention that cuts the total while one dimension rises reads as inconclusive. Aggregation takes the worst pair and computes no variance (`comparison_eval.rs:228-245`). A candidate one millisecond slower in one pair is rejected (`:177-179`), and notification latency has no producer.
- Nothing writes the `RunEvidence` that `--compare` consumes (`comparison_schema.rs:46-72`, with `deny_unknown_fields`).
- Quota continuity reads the one-second `resets_at` jitter as a reset (`quota/history.rs:211-212`), and the comparison compares `resets_at` and interval length exactly across arms (`comparison_quota.rs:88,121-128`).
- No section reports Bash output volume per command (`sections/tools.rs` reports request shapes). Observation-only requests carry no tokens (`polling.rs:25-98`). Orientation covers subagents only and counts requests (`lifecycle.rs:168-175`). `agents.by_agent_model` reports `agent_type: unknown` for every row without a spawn-ledger join (2,006 of 2,006 sonnet subagent requests in PLAN-token-flow).

The plan:

1. **Discovery.** `--project <dir>` also selects every `~/.claude/projects` entry named `<slug>--worktrees-*`, whether or not the worktree still exists. A provider reads `Partial` when a session record in `.loom/work/sessions/` names a `transcript_path` the selection did not read. `--since` and `--until` accept RFC3339 instants.
2. **Forward receipts.** `loom hook forward-receipt` takes the forward's result from the PostToolUse payload's `tool_response`, as `codex-forward-result.sh:32-40` already does, instead of the transcript (`forward_receipt.rs:62-70`). The hook call at `post-tool-use.sh:251` pipes the payload. Today Codex discovery receives only `codex_root` (`usage/mod.rs:287-290`), every Codex row in the window enters normalization (`usage/provider.rs:44-49`), and `forward_join.rs:99-115` only attaches labels. Membership therefore becomes a filter applied before aggregation: Codex rows of a project are the threads its forward receipts name (retries included, since each forward call writes its own receipt), and every other Codex row in the window is reported as foreign and excluded from the provider totals. A receipt whose thread has no rollout in the window makes the Codex provider `Partial`, never a smaller `Measured` total. A fresh clone isolates Claude slugs only; it never isolates Codex rollouts, which share `~/.codex/sessions`. Codex rollouts are copied out of `~/.codex/sessions` at the end of a measured run, because they are not retained.
3. **Verdict.** Beside dominance, a declared per-provider net-total rule sums `resident_input_tokens + output_tokens`. A candidate is supported when no provider's total rises and each provider's mean paired reduction exceeds twice its standard error. A reduction on one provider with a rise on the other reads `cross-provider-transfer`. Variance is the sample variance (divisor n−1); below three pairs the summary reads `insufficient-pairs`. Latency uses a declared tolerance judged across pairs. Notification latency is reported missing until a producer exists, and the overall verdict says so. Providers are never summed and cache weights are never invented.
4. **Producer.** A `loom usage` output mode writes the existing `RunEvidence` (it gains `Serialize`) for one project and time window: per-provider totals marked `Measured` only when no unknown, ambiguous or fallback rows remain, retry and fix counts (`Stage.retry_count`, `fix_attempts`, `models/stage/types.rs:372,465`), the stage critical path, review rounds and unresolved findings from `.loom/work/reviews/<stage>/round-N.json`, and disputes. Finding ids (`F-<round>-<k>`, `verify/review/store.rs:148`) are local to one run, and review severities (`critical|major|minor`, `verify/review/report.rs:166`) differ from `FindingEvidence`'s (`comparison_schema.rs:107-112`). The independent reviewer therefore assigns cross-arm ids and applies a severity mapping the README states. `UsageArgs` and `ProviderSelection` move from `usage/mod.rs` to `usage/args.rs` first.
5. **Sections.** `tools` gains Bash output bytes by command. `polling.rs` attaches resident tokens to observation-only requests (`loom subagents watch`, Monitor, wakeups, repeated status commands). `lifecycle.rs` reports orientation for main sessions too, in requests and tokens, to the first edit, commit, recorded review round or verdict, and skips read-only roles whose recorded round is their last act. `agents.rs` joins `spawns.jsonl` to `starts.jsonl` in order per loom session and agent type, and reports two parallel same-type spawns with different requested models as ambiguous.
6. **Quota.** Continuity and `reset_incomparable` apply the 300-second tolerance of `tokens/quota_points.rs:12`, and interval lengths match within one poll interval (`quota/poller.rs:29`, 180 s). This corrects the display; no acceptance in this metaplan uses a subscription verdict.
7. **Replay.** A test-only harness (an `#[ignore]` integration test) renders, for a frozen `.loom/work` fixture, every stage signal through `format_signal_with_metrics` (`orchestrator/signals/format/mod.rs:98`) and every recorded worker query through the worker-brief retrieval, and prints bytes per section from `SignalMetrics`. Running it at two revisions and diffing the output is the byte evidence for plans 4 and 5. Template, skill and agent bytes are file sizes. The resident estimate multiplies changed bytes by the requests that follow the admission point in recorded transcripts of the same stage type.

The live protocol, for the change that needs it (plan 4's frozen replays, and a section 6 question that qualifies). Live trials consume subscription quota, so each needs operator scheduling and authority, and runs from an ordinary shell with no other Claude or Codex session open: a stage session cannot run `loom run`, and its own consumption would be concurrent use (`comparison_quota.rs:130-131`).

1. Each run gets a new clone at `<experiment-root>/<intervention>-<arm>-<run>`, outside this repository, so no two runs share a `~/.claude/projects` slug. The clone's checked-out branch is reset to the pinned revision and used as the plan's base branch, so no branch is created. The tooling writes the arm's `.loom/config.toml` into the clone, because `.loom/` is gitignored (`.gitignore:61`).
2. A clone isolates only `.loom/work` and `.loom/config.toml`. The `loom` binary, the hooks under `~/.claude/hooks/loom/`, the agents, the skills and `~/.claude/CLAUDE.md` are shared, so arms run one at a time, and before each arm `./dev-install.sh` installs that arm's revision from its clone. The environment identity records the installed binary's revision and digests of the installed assets and of `~/.loom/config.toml`.
3. Freeze the workload fixtures, the source-input revision and digest, the acceptance contract, the required commands, the review dimensions, and the providers, models and effort. Name the one intervention. The manifest lists one changed dimension, because an empty `changed_dimensions` is invalid.
4. Include successful work, expected failures and review-heavy work, with the same checks and independent review in both arms. A clone at a new path starts cold; a warm start is a second run in the same clone inside the first run's cache TTL.
5. Interleave baseline and candidate order across repetitions, and retain every outcome, including failures, retries, fixes and incomplete runs. A missing measurement is unknown, never zero.
6. Only `measured-canonical` rows from `loom usage --provider all --since <start> --until <end> --json` are measurements. `duplicate-exact`, `ambiguous-conflict`, `fallback-coverage`, `synthetic` and `unknown-usage` rows, and every absent dimension, go into the artifact as missing. Stage ledgers written before 2026-09-13 (`.loom/work/subagents/<stage>/codex.jsonl`) contain fake forward records that hook tests appended, and are never provenance.
7. Fix the pair count and the stopping rule before the first pair. Run every retained artifact through `loom usage --compare <artifact> --json`. Any new defect, omitted requirement, weakened check or review, failed semantic completion, or latency regression beyond the tolerance rejects the candidate, whatever the average. A rejected candidate is reverted through the operator's ordinary deployment procedure, and its evidence is kept.

Dropped, with the reason:

- **The `[experiment]` config table and `[plan] base_revision`.** The ids are artifact fields (`comparison_schema.rs:49-56`), and the tooling records `git rev-parse HEAD`.
- **An `Admitted` telemetry event.** Transcripts and the replay already hold the admitted bytes.
- **A run-metrics sidecar file and a new `run_metrics.rs`.** The existing sections are extended.
- **A policy `InterventionDimension`.** `comparable` compares no configuration (`comparison_eval.rs:79-114`).
- **`exp/<id>/<arm>` branches.** The clone's own branch serves.
- **Stage attribution for main-agent rows, and the `--stage` slug match.** Both sit outside the evidence path.
- **The live tracker's defects in `tokens/`** (Codex fork replay, double-counted threads, the 8 MiB checkpoint). `loom usage` does not read the tracker; they stay in `concerns/token-accounting-and-proof-defects.md`.

Acceptance: a checked-in fixture corpus drives the tests. It holds transcripts (one of them from a removed worktree), a stage directory, review rounds, a quota-history excerpt with the one-second jitter, and a forward call with its PostToolUse payload. The tests assert that the removed worktree's transcript is counted, that a missed transcript reads `Partial`, that the jitter reads as one interval, that a forward receipt forms from the payload, that with arm A, arm B and unrelated Codex activity in one rollout store with overlapping timestamps each project's Codex provider totals include only its own threads, that a receipt naming a missing thread reads `Partial`, and that the producer's `RunEvidence` round-trips through `--compare`. Three synthetic artifacts pin the verdict: a known reduction is supported, an A/A set is inconclusive, and a single pair is `insufficient-pairs`. The replay harness runs on the fixture and prints per-section bytes. The gate, with the hook steps. Files near their limits: `forward_join.rs` (351), `sections/agents.rs` (335), `comparison_validation.rs` (313) and `config_sections.rs` (358). `models/stage/types.rs` (696) is in the ledger at an exact length, so any new Stage field is extracted.

## 3. Doctrine truth and plan guards (`PLAN-01-doctrine-truth-and-plan-guards.md`)

Agents spend turns, and sometimes whole corrections, on instructions that are false or contradict each other, and plans lose runs and lessons through gaps in verification and cleanup. Each contradiction below costs a decision turn at best, and at worst a wrong write followed by a review finding. The plan goes first because it is low risk, and because plans 1, 4 and 5 edit the same files after it.

Doctrine and signal text:

- **IntegrationVerify curates no knowledge.** IV signals print the "KNOWLEDGE UPDATES REQUIRED" box (`format/sections.rs:36-46`, `format/section_boxes.rs:6-21`) and take the Knowledge Management arm (`sections.rs:49-50`, including `loom knowledge update entry-points` at `:140-146`). The IV prefix (`orchestrator/signals/cache.rs:163`) and the plan-writer (`skills/loom-plan-writer/SKILL.md:134`) say IV never curates knowledge. IV leaves that arm and the box and joins the `Standard` arm, so it gets `## Stage Memory` and the "NEVER use 'loom knowledge'" line (`:199-231`). The pins at `tests_brief_rendering.rs:115-160` (`a_populated_tree_never_claims_the_knowledge_base_is_empty` and the empty-tree case) are rewritten in the same stage. `format_semi_stable_section` (recorded at 278) and `sections.rs` (569) shrink, and their ledger entries are lowered to the exact new counts in the same commit; `format_dynamic_section` (205) is untouched.
- **Post-commit reminder.** `post-tool-use.sh:269-298` tells every stage type to run `loom knowledge update mistakes`, which implementation signals forbid (`sections.rs:227`). That item and the stale item 3 are deleted. The text stays type-neutral, because hooks have no stage-type variable, and the `POST-COMMIT REMINDER` heading stays (pinned by `loom-hooks/tests/post-tool-use-commit-reminder-tokenized.sh`).
- **Dead and mis-scoped hook checks.** `commit-guard.sh:337` checks `$WORK_DIR/knowledge`, which never exists, so `remind_knowledge_capture` (`:333-353`, called at `:451`) is dead and goes. `remind_memory_usage` (`:357-393`) reads `"$memory_dir/$stage_id.md"` in place of the `*.md` loop at `:369`; its `stage_id` argument at `:359` is unused today. `loom-hooks/tests/commit-guard-memory-reminder.sh` writes `build-api.md` instead of `session.md`. `learning-validator.sh` (`:56-88`) counts `^###` entry headings in `"$memory_dir/${LOOM_STAGE_ID}.md"`. Today it tests `^##` (`:68`), which only the summary writer produced, so once that writer goes the old check would warn in every session. A new `learning-validator` hook test covers a stage journal with entries (quiet), an empty or missing stage journal (warns) and a journal of another stage only (warns). Both checks stay advisory.
- **Note prefixes.** The stage signal teaches `found:` and `gotcha:` (`sections.rs:218-219`), the subagent preamble teaches `found/gotcha:` (`loom-hooks/_subagent-preamble.txt:34`), and `NotePrefix::parse` recognises only `found/gotcha:` (`commands/memory/handlers/prefix.rs:37-48`). Journals hold 129 `gotcha:`, 128 `found:` and 28 `found/gotcha:` notes. `NotePrefix::parse` accepts all three as one class, and a test pins them.
- **Recovery text.** `CLAUDE.md.template:75`, `loom-hooks/session-start.sh:153` and `sections.rs:562` run `loom memory list`, which lists every journal in the plan. Each becomes `loom memory list --stage <stage-id>`.
- **`orchestrator/signals/format/codex.rs:90`** says "YOU (opus)". The section is cache-stable and carries no model, so it becomes "YOU, the stage's main agent".
- **`loom-hooks/spawn-guard.sh:19-20`** says the brief is appended in every session. It is stage-only (`:108-111`).
- **`orchestrator/signals/tests_size.rs:36`** says the template is about 17,000 bytes. It is 19,916.

Documentation and skills (the repository copies):

- **`README.md`.** `:941` and `:1310` name journals `<session>.md`; they are `<stage>.md` (`fs/memory/storage.rs:19-21`). `:948` says later stages inherit memory; it states the current behaviour (`generate/context.rs:68` reads only the stage's own journal), and plan 4 rewrites it. `:1054` replaces the Fable "major bugs" case with the plan-writer's three cases (`SKILL.md:174-175`). `:1115` describes the OS boundary plan-authored commands run under (bubblewrap or sandbox-exec, network denied, `verify/criteria/confine_host.rs`) and the `command_confinement: inherit` exception.
- **`skills/loom-usage/SKILL.md`.** `:401` calls `update` "the only write path"; `replace-section` and `delete-section` exist. `:464` claims that `loom review` prunes knowledge; `loom review` generates review documents from stage memories (`cli/types.rs:196`). The same false claim is in `references/plan-template.md:157` and `references/bookend-stages.md:36`.
- **Plan-writer.** `references/authoring-detail.md:110` calls knowledge "append-only". `references/bookend-stages.md:36` ("Skip ONLY if … no new knowledge") disagrees with `SKILL.md:127` ("ALWAYS"). Settle on always: a distill stage with no pending memory passes `loom memory pending --strict` and records nothing, and otherwise resolves each entry with `loom memory resolve <id> --outcome discarded --reason ...` or a curation. `SKILL.md:251` and `references/authoring-detail.md:48` describe a main-agent "FOUNDATION step" that BLOCK-B forbids; it becomes a delegated assignment. `SKILL.md:343` says only knowledge, IV and distill stages get the `doc/loom/knowledge/**` write grant, but `sandbox/config.rs:120-148` applies it to every stage type. `SKILL.md:137` and `bookend-stages.md:36` say the tier-1 scaffold carries `##` headings; it does not (`fs/knowledge/templates.rs`).
- **Append-only scaffold.** The tier-1 scaffold stamps "This file is append-only - agents add discoveries, never delete." into all seven files (`fs/knowledge/templates.rs`), and `doc/loom/knowledge/stack.md:4` repeats it. The sentence changes to name `update`, `replace-section` and `delete-section`. `KnowledgeDir::has_content` (`fs/knowledge/dir.rs:52-77`) detects the scaffold by `##` lines that do not contain "append-only", so the scaffold text and the detector change together, with a test that a fresh scaffold still reads as empty.

Dead path:

- **The memory summary.** `handle_context_warning` (`orchestrator/monitor/handlers.rs:100-103`, called from `monitor/detection.rs:194-199` on the Yellow transition) writes a `## Summary` through `write_summary` and `generate_summary`, and the parser drops it (`fs/memory/parser.rs:26-29`). The handler and its call go (the `SessionContextWarning` event stays), with `write_summary`, `generate_summary`, their `fs/memory/mod.rs` re-exports, the unread `MemoryJournal.summary` field (`types.rs:215`), `test_generate_summary`, and the `generate_summary` rendering in the `export_suggestions.rs` test.

Knowledge freshness:

- `architecture/owned-waits.md` gains `sources`/`verified` frontmatter. The five other high-impact pages (`context-ceiling`, `hook-system`, `memory-spool`, `signal-generation`, `token-accounting-and-receipts`) already carry it, and `context-retrieval-state.md` and `memory-spool.md` are re-annotated by plans 1 and 4 when they rewrite them.
- `loom knowledge check --strict --baseline doc/loom/knowledge/check-baseline.txt` exits 1 at HEAD: the baseline holds only comments, `conventions.md` is 251 lines and `mistakes/ci-toolchain-and-cargo.md` is 404. The plan fixes those structural issues, or ratchets the baseline with `--write-baseline`, so the check is green before any acceptance depends on it. `--strict-evidence` is not added to distill acceptance: it is red at HEAD (41 changed sources) and would make every distill re-verify pages for unrelated changes.
- One retrieval fixture test confirms that a section annotated with a non-current `--state` (`commands/knowledge/annotate.rs:64-77`) leaves retrieval. Cross-file duplicate headings are already detected (`fs/knowledge/catalog/headings.rs:68`).

Plan guards:

- **Bootstrap warning.** `loom plan verify --strict` fails plans that correctly omit a bootstrap stage, because the id/name heuristic at `plan/schema/validation.rs:1109-1137` warns and `commands/plan/verify.rs:64-66,406` counts the warning. The warning fires only when `!KnowledgeDir::has_content()`. `check_knowledge_recommendations` takes the repository root, and both callers (`commands/plan/verify.rs:406`, `commands/init/plan_setup.rs:133`) and its seven tests (`plan/schema/tests/knowledge_recommendations_tests.rs`) are updated.
- **Bookend lints.** One new module under `plan/schema/validation/v2_lints/`, registered in `v2_lints/mod.rs`, adds two findings with `error_in_v2` (a warning on version-1 plans). The first requires exactly one `knowledge-distill` stage, with no dependents and every other stage among its ancestors. The second requires every `standard` stage to be an ancestor of an `integration-verify` stage. Stage types come from `detect_stage_type`, because `stage_type` may be omitted, and the walk reuses `transitive_dependencies`. Every v2 plan and fixture in the repository conforms; only archived v1 plans violate, and they get warnings. `v2_lints::run` is called only from `commands/plan/verify.rs:400`, so `loom init` and `loom run` are unaffected. The goal-backward exemption for KnowledgeDistill at `validation.rs:478` stays.
- **Stage `model`.** The field is interpolated raw into the `claude` command line (`orchestrator/terminal/native/mod.rs:174`), and only effort is validated (`plan/schema/types.rs:296-302`). The field is validated against `CLAUDE_MODELS` (`loom/src/claude.rs:18`: haiku, sonnet, opus, fable) in its deserializer, the same set `[models]` accepts (`fs/work_dir/config_sections/models_config.rs:61`). No path accepts a full model id today. The stage first checks `doc/plans/` and `loom/tests/fixtures/plans/` for any.
- **Archive failure.** `archive_run_state` (`fs/memory/archive.rs:16-27`) returns `Option<PathBuf>`, where None means both "nothing to archive" and "failed", and three callers delete the work directory after it: `fs/plan_lifecycle.rs:263`, `commands/init/cleanup.rs:193` and `commands/clean/mod.rs:230`. It returns `Result<Option<PathBuf>>`, and a failure while the memory directory holds journals aborts each caller, leaving the plan IN_PROGRESS and retryable. The error names the archive path.
- **Failed-run distill.** One README paragraph beside `README.md:557` documents distilling a failed run: copy `<archive>/memory/*.md` into the new plan's `.loom/work/memory/`. The stage runs the procedure once.
- `SKILL.md:294` and the README's `plan verify` text list the new lints.

Tests:

- **Subcommand resolution.** `plan/schema/validation/v2_lints/loom_subcommands.rs` gains a `pub(crate)` function over `unknown_subcommand` (`:60`, private today) that takes text and returns unresolved `loom` paths. Markdown surfaces (`CLAUDE.md.template`, `AGENTS.md.template`, `commands/*.md`, `skills/loom-*`) are read through fenced code lines and inline code spans. Hook scripts are read through lines whose command word is `loom`, `"${LOOM_BIN:-loom}"`, `$LOOM_BIN` or a leading `loom_run_bounded <n>`, normalised to `loom`. Words containing `*` or `<`, and flags, end the walk. The test lives in a new `orchestrator/signals/tests_doctrine_commands.rs` (`tests_doctrine.rs` is at 399). It asserts a minimum extracted count per surface group, and self-checks that `loom knowledg update` and `loom hook nope` are rejected, so an extractor that matches nothing fails.
- **No knowledge writes.** The existing tests gain assertions: the standard-signal test in `tests_cache.rs`, which already asserts `!content.contains("loom knowledge update entry-points")` at `:203`, and the IV helper `integration_verify_signal_without_a_pack`, for both `knowledge_tree_empty` values, assert that `loom knowledge update` is absent. A test over `tests_commit_timing.rs::all_generators()` does not qualify: those four stable prefixes never contained the phrase.
- **Lints.** Unit tests in `plan/schema/tests/v2_lint_tests.rs` cover each new lint in its error form (`split_lint_findings(2, ...)`), its warning form (`split_lint_findings(1, ...)`) and a conforming plan, plus one end-to-end case in `loom/tests/plan_environment_contracts.rs`. Nothing reads `loom/tests/fixtures/plans/*.md` in tests; `v2-valid.md` is checked by running `loom plan verify --strict` on it from a scratch git repository.
- The archive-abort test, the hook tests above, and the gate with the hook steps.

No guidance is lost: each removed sentence is false or has the replacement named above.

Dropped, with the reason:

- **Removing `dirty-paths.json` and `loom context record-edit`.** Its output goes to `/dev/null`, so it has no token effect, and the removal touches about 20 files.
- **A `command_confinement: inherit` warning.** It is a documented opt-in that no plan uses.
- **A discard flag and telemetry event for skipping distill.** Skipping loses curation, not data: memory is archived, and the archive abort guards the data.
- **A catalog check on every knowledge-completion branch.** Those branches are operator overrides behind `admin_proof`, and the baseline is red.
- **A bootstrap receipt gated on the check.** The receipt is a freshness marker for `--refresh`.
- **Confirming audit item 1g.** It has no testable statement.

## 4. Dependency findings (`PLAN-04-dependency-findings.md`)

A successor stage needs two things from an accepted dependency: the interface it changed and the constraints its sessions discovered. Both exist the moment the dependency merges: the first in git and the source graph, the second in the dependency's memory journal. This plan delivers each through a channel loom already has:

- The change set feeds the existing dependency boost of the Knowledge Brief, so the stage signal, the contract signal and every worker brief list the dependency's changed symbols that match their own task. A short-lived worker therefore orients on a dependency without re-reading it, and the parent can keep its workers small.
- The journal feeds one capped section of the stage signal, in the slot the keyword checklist occupies today.

Not everything a stage learned reaches its successors this way, and nothing tries to carry all of it. A recipient that needs more reads the code it edits, asks `loom map`, runs the headline's `git diff`, or lists the dependency's journal (below). The plan adds no command, hook, telemetry event, config key, stored state or completion step. Every added byte is paid again on each later turn of its recipient, so the saving can come only from reads and searches a recipient skips.

**Evidence** (2026-10-18; journals under `.loom/memory/archive/` and `.loom/work/memory/`; transcripts under `~/.claude/projects/-home-dkaponis-src-loom--worktrees-<stage>/`; tokens are tool-result characters divided by 4; "replay-weighted" multiplies each result by the API calls that follow it in its transcript):

- **Memory is plentiful.** 50 standard-stage journals hold 1,011 unique entries, median 19.5 per stage, and 82% carry `--evidence`. The gap is selection. Of guard-core's 26 entries, about 7 constrain guard-integration. Of token-core's 45, 21 are `mutation:` notes and 8 are duplicated review suggestions.
- **Receipts are rare while successors run.** Standard stages wrote 5 receipts in those 50 journals; IntegrationVerify (127) and KnowledgeDistill (1,472) write after every standard stage has finished.
- **Successors re-read their dependency's changed files, and workers and contract writers do most of it:**

  | Pair | Successor transcripts | First arrival on predecessor-changed files | Replay-weighted |
  | --- | --- | --- | --- |
  | guard-core → guard-integration | 17 | 65,500 | 2,140,000 |
  | token-core → tui-tokens | 9 | 5,900 | 80,000 |
  | token-core → web-tokens | 11 | 2,500 | 19,000 |

  In guard-integration, 11 transcripts read `git/target_guard/mod.rs` (76 calls: 7 full Reads, 25 range Reads, 35 `rg`, 1 `loom map --outline`), although the plan named its API. token-core's successors needed little because their stage descriptions pinned its API. Main sessions read 1,100, 0 and 0 tokens of dependency code.
- **Session sizes.** Main sessions made 45–98 API calls and ended at 166,000–211,000 resident tokens.
- **Worker briefs did not arrive** in these runs (0 of 44 subagents carry the marker). Plan 1 fixes that, and Part 1 reaches workers only after it.
- **The dependency boost never fires in this repository** (state table).

**Part 1: the accepted change set, through the existing boost.**

- **Source of the paths.** `StageQuery::dependency_paths` (`orchestrator/signals/retrieval.rs:100-123`) returns, for each dependency that is Completed with a `completed_commit`, the paths its merge brought to the target. Today it returns the declared `files` and `artifacts`.
  - loom lands every stage through a two-parent merge commit whose second parent is the verified commit (`git/merge/tree.rs:1-7`, `git/merge/mod.rs:85-90`). In history, `e133f68c` has parents `5f489d2c` and guard-core's `d4cd3148`.
  - The merge M is found with `git rev-list --first-parent --merges --parents --ancestry-path <completed>..<target>`, choosing the commit whose parents include `completed_commit`. The target comes from `fs::resolve_target_branch_from_config` (`fs/mod.rs:75`).
  - base = `git merge-base <M>^1 <completed>`, and the paths are `changed_paths(repo, base, completed)` (`git/merge/control_paths.rs:95-112`). The merge-base drops target commits merged into the stage branch during conflict resolution.
  - A dependency used as the base branch before its merge (`git/worktree/base.rs` `resolve_base_branch`) has no M; its base is `git merge-base <completed> <target>`.
  - With no `completed_commit`, with `merge_assumed` and no M, or for a Knowledge-type stage, the paths are the declared files joined to the dependency's `working_dir`, with globs skipped. That fallback also fixes today's `working_dir` mismatch.
- **What the boost does.** The ranker already gives every node in those files the StageDependency rung (`context/rank_source.rs:263-265`) and orders them by the query's own terms. The Knowledge Brief renders the selected nodes in its `### Source (signature index)` section: path, signature and line span, sanitized and fenced (`orchestrator/signals/format/brief.rs:10-45`). The brief budget stays `stage_brief_budget_tokens` (3,000, `context/config.rs:165`). The change alters which items fill it and adds no tokens.
- **Where it reaches.** The stage signal and the contract signal, both through `retrieve_stage_pack` (`retrieval.rs:58-84`; `orchestrator/signals/contract.rs:66-80`). Every worker brief, through the hook's query, which already calls `StageQuery::dependency_paths` and adds the worker's description, prompt and declared paths (`commands/hook/worker_brief.rs:160-182`), so each worker sees the dependency symbols that match its own task with no edit to `worker_brief.rs`.
- **Records and cost.** Delivery records already list each source node delivered (`persist_delivery`, `retrieval.rs:193-208`; `store_pending`, `worker_brief.rs:297-315`). The cost is three read-only git calls per dependency per signal or brief.
- **Risk.** A large change set can push knowledge items out of the brief. The offline check below measures pack composition before the plan is accepted.

**Part 2: findings, in one capped section of the stage signal.**

- **Inputs.** `read_staged_entries` over the direct dependencies (`fs/memory/staged.rs:18-35`), which pairs each entry with the journal it came from; the producer is that journal, never the untrusted `session` field or a `**Stage**:` line inside a body. `settled_ids` over every journal, as `pending_suggestions` does (`orchestrator/signals/v2_section_review.rs:170-181`). The dependencies' stage files (`verify::transitions::load_stage`). The recipient's declared `files` and `artifacts`. The paths of the pack `build_signal_context` has just retrieved.
- **Eligible dependencies.** Direct, Completed, and not of Knowledge type. Ancestors are excluded: their code is already in the base, and concatenation multiplies the size.
- **Excluded entries.** Receipt and Change entries; `mutation:` notes; non-Decision entries that any receipt settles. A Decision stays even when settled, because it still constrains the code. For a version-2 IntegrationVerify recipient, Suggestion entries are left out too, because its "Reviewer Suggestions" block already lists every pending one (`orchestrator/signals/v2_section.rs:17-22`).
- **Relevance for an ordinary stage.** An entry is kept when an evidence path, or a path token in its text, matches the recipient's declared files (joined to its `working_dir` and matched as globs); when that path matches a file the recipient's brief selected; or when its text names the recipient's stage id. Before matching, evidence paths drop an absolute `.worktrees/<id>/` prefix, are joined to the producer's `working_dir` when relative, and pass through `normalize_dependency_path` (`context/rank_source/paths.rs:75`).
- **Other recipients.** IntegrationVerify keeps every non-excluded entry, because its assignment covers all its dependencies. KnowledgeDistill gets no section, because its prefix already processes every journal.
- **Order.** Entries naming the recipient first, then notes starting `mistake:`, `gotcha:`, `found:` or `found/gotcha:` (checked locally in the selector), then Decisions with rationale, then the rest. Ties break by dependency order, timestamp and id; timestamps have one-second granularity.
- **Rendering.** One fence from `fence_for` (`context/render.rs:114`), opened by the brief's reference-data sentence (`format/brief.rs:58`, widened to `pub(super)`). Per dependency, a headline: stage, accepted commit with "assumed" when `merge_assumed` is set, shown and total counts, the `git diff --stat` command for its change, and `loom memory list --stage <dependency>`. One line per entry: type, short id, claim (at most 200 characters), rationale (at most 100) and evidence, all through `single_line` (`verify/review/report.rs:28-31`), so a body holding a fence or a heading cannot break the section.
- **Journal round trip.** Flattening at render time cannot repair a body the parser already split. `format_entry` (`fs/memory/parser.rs:150-158`) writes the body verbatim, and `parse_journal` (`:17-40,91-112`) ends an entry at a body line `---`, starts a new one at a `###` heading, overwrites the journal's stage at `**Stage**:` and switches to metadata at `*Context:*`, `*Evidence:*`, `*Session:*` or `*Receipt:*`. A legitimate note holding a Markdown separator loses everything after it, and a body heading becomes a second entry. `format_entry` therefore prefixes every body line that starts with `\`, `---`, `#`, `<!--`, `**Stage**:` or `*` with one `\`, and `parse_entry_line` strips one leading `\` from a content line. The encoding lives in `parser.rs` (191 lines), which every writer and reader of journals already shares. No migration is added (the project is unreleased).
- **Cap.** `MAX_DEPENDENCY_FINDINGS_BYTES = 1600` (about 400 tokens by `context/schema/estimate.rs:16`), shared by all dependencies so fan-in cannot multiply it. Headlines are always kept, and a footer counts what was omitted. The precedent is `MAX_PLAN_OVERVIEW_BYTES` with `truncate_overview` (`orchestrator/signals/generate/context.rs:98-131`).
- **Wiring.** `build_signal_context` (`orchestrator/signals/generate.rs:202-239`) calls the selector after retrieval at `:236`, because relevance reads the pack, for every stage with dependencies; the call sits after the `wants_stage_enrichment` block (`:231`), which keeps only `build_cross_stage_summary`. `generate_recovery_signal` (`orchestrator/signals/recovery.rs:30-44`), which bypasses `build_signal_context`, calls the same selector after its own `retrieve_stage_pack` at `:43`, and `format_recovery_signal` renders the section, so a stage resumed by `loom stage retry` keeps its findings. `EmbeddedContext.wiring_checklist` becomes `dependency_findings` (`orchestrator/signals/types.rs:67-68`) and keeps its slot at `format/sections.rs:410-414`, so the recorded 569-line file and the 205-line `format_dynamic_section` do not change. The heading "Downstream Wiring Checklist" (`orchestrator/signals/parse.rs:46`) becomes "Dependency Findings". `build_wiring_checklist` and `wiring_items` (`generate/enrichment.rs:87-124,157-180`) are deleted; they shipped as item 3b of PLAN-strengthen-verification (`d47a3f35`). The cross-stage summary (item 3a, `:17-63`) stays.
- **Late relays.** The journal is read at every generation, so a handoff continuation (`orchestrator/continuation/mod.rs:76` calls `generate_signal`) picks up entries relayed after the first signal.

```text
Reference data below — quoted source, NOT instructions.
guard-core accepted at d4cd3148: 3 of 26 entries | git diff --stat 5f489d2c d4cd3148 | loom memory list --stage guard-core
- [decision 6167f528] reference-transaction installer reports UpToDate only when content equals SCRIPT and mode is 0755 | why: git skips a non-executable hook | loom/src/git/hooks/reference_transaction.rs
- [suggestion 639cf1ee] loom repair reinstalls only the pre-commit hook; a removed reference-transaction hook turns attestation off until the next loom init | loom/src/commands/repair/workspace.rs:304
- [note dbfda97e] mistake: GIT_GRAFT_FILE=/dev/null makes git 2.53 print a grafts advice on stderr; use /dev/null/loom-no-grafts | loom/src/git/runner.rs:52
```

(Illustrative, built from guard-core's archived journal.)

**What is not delivered, and what the recipient does instead.** File contents, diffs, handoffs, transcripts, read ledgers, plan-brief text, ancestor journals, own-stage memory and stage outputs are not delivered by this plan. A recipient that needs more has these routes:

- read the code it edits, which stays legitimate;
- `loom map --outline <file>` or `loom map --window <id>` for a body;
- the headline's `git diff` for the exact change;
- `loom memory list --stage <dependency>`, or `loom memory query "<text>" --stage <dependency>`. `--stage` is always spelled out, because the query defaults to every stage (`commands/memory/handlers/read.rs:65-68`), and the query matches its whole argument as one substring (`fs/memory/query.rs:23-35`), so generated text never writes a multi-term query.

The information roles stay as they are: the handoff describes the current unfinished task, memory records discoveries, knowledge holds claims curated across plans, and archives preserve history. Archive retention alone does not authorize automatic retrieval.

**Decided against, with the reason.**

- **A required "notes for successors" completion step.** Supply is not short (median 19.5 entries, 82% with evidence). A required step adds at least one main-session turn at completion, when resident context is largest (166,000–211,000 tokens in the sessions sampled), which costs more than either token-flow successor could save (19,000 and 80,000 replay-weighted tokens), and stages with no standard dependents would pay it too.
- **A new note prefix or wording rule.** An entry addressed to a dependent is selected when it names the dependent's id or a file the dependent declares.
- **Findings in worker briefs.** The main agent reads the section and writes each worker's prompt. The measured worker re-reading is dependency code, which Part 1 covers. `worker_brief.rs` is at the 400-line limit.
- **A stored record, a base file or a symbol diff between revisions.** Part 1 derives the change from git at each use, and the brief already renders signatures.
- **A changed-evidence "check" label.** Successors start right after the merge and read the code they edit; the headline names the accepted commit, and no case of a changed evidence file was measured.
- **Availability reporting and retrying a failed journal append.** The run loop drains before it spawns (`orchestrator/core/run.rs:93-99`), the last run applied 259 memory relays and refused none, and a retry would need a new ledger state, because the drain records `Applying` before the handler so that a crash is never retried (`inbox_drain/session_pass.rs:150-172`).
- **Capture-time rejection of journal-structure lines.** Any session can already resolve any event id (`commands/memory/handlers/resolve.rs:19`), so a forged receipt gains no new authority; the write-side escaping above keeps such lines as content.
- **Capping the v2 "Reviewer Suggestions" block.** It renders only for IntegrationVerify (`v2_section_review.rs:123-126`) and lists every pending suggestion because IV must dispose of each one; a cap would silently waive review. Plan 4 leaves Suggestions out of an IV recipient's findings, so no suggestion appears twice.
- **`memory:<id>` delivery nodes.** Nothing reads them, `DeliveryRecord` needs a pack for its epoch (`context/delivery.rs:96-101`), and spawn telemetry would count them as knowledge items (`orchestrator/core/stage_telemetry.rs:35`).
- **Moving `NotePrefix` into `fs::memory`.** A local prefix check suffices, and the move would write inside PLAN-web-host-graft-followthrough's `loom/src/commands/memory/**` glob.
- **A recipient-level `SignalBudget`, per-stage-type `[retrieval]` overrides, caps on dependency outputs and the embedded handoff, and a rebuilt own-stage memory export** (the former signal-budget plan). Each capped section keeps its own constant. No recorded transcript sets an output, and stage sessions cannot. A handoff the successor needs is read whole either way. Own-stage memory reaches only later sessions of the same stage, whose handoff carries the task state, and standard stages settle almost none of their own entries (5 receipts in 50 journals). Per-stage-type budgets wait for section 6's evidence.

**Cost and proof.**

- **Added resident tokens.** Part 1 adds none. Part 2 adds at most about 400 tokens per call of the stage main session: at 45–98 calls, at most 18,000–39,000 replay tokens per session. A dependency with no relevant entry costs its headline, about 30 tokens.
- **Removable orientation.** guard-integration 65,500 first-arrival and 2.14 million replay-weighted tokens; tui-tokens 5,900 and 80,000; web-tokens 2,500 and 19,000. The saving can only come where successors work on their dependency's interfaces, and mostly through workers.
- **Tests, in new files** (`relay_e2e.rs`, `spool_drain_tests.rs` and `tests_drain.rs` are each over 340 lines):
  1. Change set on a git fixture merged the way `merge_stage` merges: the paths equal the stage's own change; a target merged into the stage branch is excluded; an unmerged dependency used as base uses the merge-base with the target; `merge_assumed` or a missing commit falls back to declared files joined to `working_dir`, with globs skipped.
  2. On a fixture following `orchestrator/signals/tests_brief_e2e.rs:57-107`, the signal's brief lists a symbol from the dependency's changed file with the StageDependency reason. Through `commands/hook/worker_brief/test_support.rs`, a worker prompt that names that symbol receives it in its brief.
  3. Selector unit tests: relevance by declared file, by brief path and by stage id; each exclusion, with a settled Decision kept; IntegrationVerify without the filter; the shared cap with headlines and footer; stable order under fan-in; path normalization for absolute worktree, `working_dir`-relative and `../` evidence; a body containing triple backticks or a heading renders as one escaped line; all four note prefixes rank together.
  3a. Journal round trip, through `append_entry` and `read_journal` (an in-memory `MemoryEntry` does not prove it): notes whose bodies hold `---`, a `### … id=` line, a `**Stage**:` line, `*Receipt:*`/`*Evidence:*` lines and a leading `\` read back as one entry each, with identical content, the journal's stage unchanged and no fabricated receipt or entry; the findings section renders each as one line.
  4. Signal end to end, reading the written signal file (the test it follows asserts only `signal_path.exists()` at `tests_brief_e2e.rs:88`, which a section that never renders would pass): B, which depends on A, has a "Dependency Findings" section with A's commit and the relevant decision id; unrelated C shows A's headline only; KnowledgeDistill shows no section; the renamed heading parses; a recovery signal for B carries the same section.
  5. An entry appended to A's journal after B's first signal appears in B's continuation signal.
- **Measurement, cheapest first** (evidence class 2):
  1. **Offline, with no model calls.** A test-only harness runs over fixtures of the three recorded pairs: a scratch clone checked out at the successor's base, the dependency's stage file and its archived journal. It builds the stage pack and each recorded worker query with the old and the new `dependency_paths`, renders the section, and reports items gained and lost, section tokens times recorded API calls, and the recorded reads and searches of dependency files whose symbols the new packs carry, both first-arrival and replay-weighted. If the cost exceeds the covered reads for guard-integration, the plan stops.
  2. **Frozen-successor replay.** After plan 1, six recorded worker prompts (the four largest guard-integration orientations and the two token-flow workers that read dependency files) are rerun with the old and the new boost, three times each, with the same model and effort, through plan 2's live protocol. Each run records input (cache read and write separately), output, API calls, tokens before the first edit, and the unit's targeted tests. The new arm must lower the median per-provider total with no more test failures.

**Files and stages.**

- **New:** `orchestrator/signals/generate/findings.rs` (a pure selector and renderer of about 170 lines, plus a loader of about 30 lines that calls `load_stage` and `read_staged_entries`) with `generate/findings_tests.rs`; `accepted_change_base` (about 25 lines) beside `changed_paths` in `git/merge/control_paths.rs` (185 lines); the offline harness as an ignored test file.
- **Edited:** `orchestrator/signals/retrieval.rs` (397 lines; its tests `:211-397` move to `retrieval_tests.rs` first, then `dependency_paths` changes); `orchestrator/signals/generate.rs` (356); `orchestrator/signals/recovery.rs`, `recovery_format.rs`; `generate/enrichment.rs` (180, losing about 60); `orchestrator/signals/types.rs`; `format/sections.rs` (field rename, counts unchanged); `orchestrator/signals/parse.rs`; `format/brief.rs` (one visibility change); `fs/memory/parser.rs` (the round-trip encoding); `README.md:948`, which becomes "a stage's signal lists the relevant findings of its accepted dependencies, and its briefs boost the files they changed".
- **Stages:** (1) knowledge-bootstrap; (2) two stages in parallel with disjoint files: `dependency-change-set` (`control_paths.rs`, `retrieval.rs` and its test extraction, tests 1–2) and `dependency-findings` (`findings.rs`, `generate.rs`, `recovery.rs`, `recovery_format.rs`, `types.rs`, `sections.rs`, `parse.rs`, `enrichment.rs`, `brief.rs`, `fs/memory/parser.rs`, `README.md`, tests 3, 3a, 4 and 5); (3) integration-verify, which runs the offline harness; (4) knowledge-distill, which rewrites `architecture/memory-spool.md:32` (it describes the wiring checklist), the enrichment part of `architecture/signal-generation.md` including the `wiring_checklist` field at `:160`, and re-annotates `memory-spool.md`.
- **Plan dependencies:** plan 1, because worker briefs must reach workers in loom's own runs before Part 1 can reach them; plan 2, for the replay and the live protocol; plan 3, which edits `sections.rs` first.

## 5. Guidance and gate output (`PLAN-05-guidance-and-gate-output.md`)

Resident instructions and gate output are paid on every later turn of every session that receives them. The items below are byte changes (evidence class 1) and need no paired runs.

- **Verifier gate output.** A gate step whose output passes the 12,000-character cap (`wrapper.rs:286`) is truncated, so the failure list may never reach the verifier, and a misread round costs a fresh verifier. `agents/loom-verifier.md` and `VERIFIER_GATE` (`orchestrator/signals/cache/blocks.rs:94`) tell the verifier to run each step as `<command> > "$LOOM_SCRATCH_DIR/gate-<step>.log" 2>&1; echo "exit=$?"` and then print the runner's failure lines from the log with `rg -n` and a few lines of context, never `<command> | tail`, which reports the pipe's status. The log sits outside the tree, so the definition's "no redirection into the tree" (`agents/loom-verifier.md:32`) holds; the definition and `VERIFIER_GATE` name the scratch log as the verifier's one write. The verifier keeps its report (command, exit code, failing excerpt per step). Rule 14 (`CLAUDE.md.template:195`) points at the same recipe; Rule 8's row (`:132`) is already scoped to file readers and stays.
- **IntegrationVerify fence wording.** `loom-hooks/_subagent-preamble.txt:18-20` tells every IV subagent to run the full build, suite and linter, while the IV prefix (`orchestrator/signals/cache.rs:115-126`) assigns one canonical verifier and has other reviewers run targeted checks. The preamble's exception is reworded to the single owner. BLOCK_A (`:11-17`) is not edited, because `tests_doctrine.rs:113-132` requires it byte-identical across surfaces. The pins are in `tests_doctrine_prefixes.rs:168,216-245`, and a new needle covers the reworded exception.
- **Rules 3 and 3b.** Both procedures already live in hook output: the ceiling message (`post-tool-use.sh:132`) and the compact/resume re-anchor (`session-start.sh:147-155`). The template bodies (`CLAUDE.md.template:69-75`) are cut to one line each; hard stop 4 at `:12` stays. No test pins either hook string today, so the plan adds both pins. A manifest test lists each relocated item with its new home and a literal needle, loading each home with `include_str!`. It also freezes the template's `###` headings and rule bullets at `6fee4ecd`: each remains in the template or appears in the manifest, so a deletion fails. `CLAUDE_MD_TEMPLATE_MAX_BYTES` (`tests_size.rs:38`, 20,480 against 19,916 today) is lowered to the new actual.
- **Orchestration skill.** Every stage main session loads `skills/loom-orchestration/SKILL.md` (24,573 bytes, about 6,000 tokens on every later request). The plan removes paragraphs that duplicate `CLAUDE.md.template` or the stage signal's stable prefix, naming each removed paragraph's surviving copy in the brief, and adds a size ceiling in the `tests_size.rs` pattern set at the slimmed size.
- **Generic skill triggers.** `skills/loom-golang/SKILL.md:5-16` lists `go`, `map`, `pointer`, `slice`, `channel`, `interface`, `struct` and `defer`, and `_rank` qualifies a skill on two single-word hits (`loom-hooks/skill-trigger.sh:22,265-279`), so "struct" and "map" qualify loom-golang on a Rust task. Those words become Go-qualified phrases (`go map`, `go interface`, …); a phrase scores 2 and qualifies alone. `STOPWORDS` is untouched (its shell and Rust copies are pinned equal by `hooks_skill_trigger_evidence.rs:152`). One Go-on-Rust negative case joins `loom/tests/integration/hooks_skill_trigger_evidence.rs`.
- **Size advisory** (operator decision of 2026-09-18). The Write/Edit branch of `post-tool-use.sh` tallies, for a stage main session (a payload with no `agent_id` and the stage environment present), the lines and files the main agent changed, in a per-session file under `.loom/work/hooks/` (hooks run outside the sandbox). Past 20 changed lines or 2 files (BLOCK_B point 1) it warns once through `additionalContext`. It never denies, and it adds no hook script or registration.
- **Heredoc block message.** `loom-hooks/loom-control-complete.sh:154-170` refuses unquoted heredoc openers, because an unquoted body expands `$(...)`. The refusal stays, and its message says to quote the delimiter. The heredoc test (`loom-hooks/tests/loom-control-complete-heredoc.sh`) asserts the message.
- **Enforcement matrix** (graft guidance #4). The knowledge-distill stage adds each hard stop missing from `doc/loom/knowledge/concerns/agent-rule-bending-hardening.md:15-77`, with its pinning test or "none".

Dropped, with the reason:

- **A bounded gate command and `.loom/work/gate-logs/`.** The verifier recipe covers it, the sandbox denies `.loom` (`session_denies.rs:71`) and grants no read of `gate-logs` (`sandbox/settings.rs:39`), and the testrun adapters expose counts only (`testrun/mod.rs:39-47`).
- **Residual poll classification.** Identical repeats of `ls`, `wc`, `stat` and `git status` are already counted (`_progress-classification.sh:197-210`).
- **The `@` worker-id check.** Already handled: named-spawn ids are refused with a specific message (`commands/subagents/wait/model.rs:44-48`).
- **Slimming other catalogued skills and adding "Not for" text to them.** They load rarely, and load counts are unmeasured (section 6).
- **A replay corpus of real prompts.** One negative case covers the observed false positive.
- **Carrying recommended skills into spawns.** It adds per-spawn bytes and invites skill loads by workers that should finish early.
- **Project-local skill resolution.** Verify and the runtime (`orchestrator/core/orchestrator.rs:263-288`) would have to change together, and no plan is blocked by it.
- **Explicit `maxTurns` and `effort` on every agent.** Pinning effort on an agent that inherits it changes its behaviour by stage type.
- **A new `hard-stop-enforcement.md` page.** The existing page above carries the classification.
- **A KNOWLEDGE-FIRST sentence on ranked retrieval.** The contract already routes literal questions to `rg`, and the sentence would be paid in every stable prefix (`cache/blocks.rs:25`).
- **The `git show -s` false positive.** Already handled (`poll-guard.sh:231-246`).

Acceptance: plan 2's replay shows the template and orchestration-skill bytes falling, with the manifest test green; the new pins; hook tests for the size advisory and the heredoc message; a verifier dry run on a fixture with a failing test shows the exit code, the log path and the failure excerpt while the log holds the full output, and a masked status (`| tail`) fails the dry run; the gate, with the hook steps.

## 6. Policy notes (no plan file)

Each default below stays until the cheap evidence named here shows an effect large enough to resolve with at most 16 pairs (evidence class 3). None needs a plan file now.

- **Prompt-cache split (the former E1): dropped.** The split is inert on the installed `claude` (`capsule.rs:71`), and once enabled it would duplicate the 8.1 KB stable prefix (`format/mod.rs:100`). Across sessions there is no shared cache. Removing `prompt_cache_split` and its probe is a cleanup for any later plan that edits `capsule.rs`.
- **Standard-stage effort (E2).** Medium (`models/stage/defaults.rs:127-145`; Knowledge medium, Distill high, IntegrationVerify xhigh). The last run had no retries or fix attempts in any stage. Revisit when a quality defect is traced to medium effort.
- **Ceilings (E3).** 800,000 for main sessions and subagents (`models/constants.rs:44,55`), with a 1.25× daemon backstop clamped to 95% of the window. No session passed 400,000 in the last run, so a lower subagent ceiling would never trigger. 150,000/120,000 stays outside the space.
- **Worker lifecycle (E4).** Strict one-shot workers. Boot is about 19,000 tokens per subagent, about 0.5% of the last run (48 subagents), and 45.8% of subagents peaked below twice their boot (`commands/usage/sections/peaks.rs:110-121`). Duplicate discovery is measured offline with the `reads` section before any arm, with plan 4 in place, because its boost changes what a fresh spawn costs to orient. `maxTurns: 1000` stays: no runaway is recorded, and the largest transcripts made 101–200 requests.
- **Retrieval budgets (E5).** Prompt 1,500 and stage brief 3,000 (`context/config.rs:165`), clamped to 256–100,000. They are judged by plan 2's replay plus `loom knowledge eval`, and per-stage-type overrides wait for a type-dependent optimum. Source windows (two 12-line windows on the CLI and stage surfaces, none on hooks) are judged the same way.
- **Interactive worker briefs (E6).** Out-of-stage spawns get none (`spawn-guard.sh:108-111`; `worker_brief.rs:56-80`). Existing interactive transcripts show whether retrieval would have hit before anything is added. Out-of-stage spawns have no dependencies, so plan 4's boost gives them nothing.
- **Instruction distribution (E7).** Skill load counts times bytes come from the `skills` section of `loom usage`, deterministically.
- **Semantic lane (E8).** Nothing is built unless `loom knowledge eval` on held-out cases shows a recall gap the lexical and graph lanes cannot close.

Policy candidates examined and dismissed:

- **A Sonnet verifier.** It weakens the gate the whole Pareto constraint depends on.
- **Closed Sonnet orchestrator profiles with a `NeedsOrchestratorUpgrade` handoff.** A per-stage `model` override is already sanctioned, and there is no evidence of misuse.
- **150,000/120,000 ceilings.**
- **Universal effort lowering.**
- **A forced fresh worker at 40 requests or 120,000 tokens.**
- **A cumulative per-session retrieval budget (secure-v2 Decision 6).** It contradicts the no-working-budget decision and the per-delivery budget design.

Each would trade quality or a settled operator decision for a token number.

## 7. Sandbox route closure (`PLAN-06-sandbox-route-closure.md`)

This plan does not target tokens. It carries the valid, unshipped security remainder of the retired overhead-reduction proposal and of `PLAN-secure-distilled-loom-v2.md`, so that those documents can be retired without losing it. The stage capsule already passes `--settings`, setting sources, strict MCP and disabled auto-memory (`orchestrator/terminal/native/capsule.rs:30-120`). Several tool routes run outside the Bash sandbox and stay open (`doc/loom/knowledge/concerns/sandbox-and-confinement-gaps.md:253-265`). The design source is the withdrawn sandbox-escape-hardening plan, items D1–D7 (`git show e6f00616^:doc/plans/PLAN-sandbox-escape-hardening.md`).

- **Tool routes.** Tool-name entries are added to `permissions.deny` in the capsule settings (`orchestrator/terminal/native/session_settings/contents.rs:105`) for LSP, RemoteTrigger, CronCreate, PushNotification and Workflow. The `--disallowedTools` flag is not used: it is variadic (`claude --help`), and the command line puts the positional prompt right after the capsule flags (`orchestrator/terminal/native/mod.rs:174`), so it would consume the prompt; `tests_capsule.rs:83-103` checks only string order and would not catch that. WebFetch and WebSearch stay allowed: the gaps file marks their sandbox bypass unverified, and `agents/loom-software-engineer.md:4` lists them. `SendMessage` and `ListAgents` stay, because agent teams need them (`orchestrator/terminal/native/wrapper.rs:363`). No `Read(` deny is emitted: only `Edit(path)` rules are enforced (`doc/loom/knowledge/mistakes/sandbox-write-rules-inert.md`). The stage brief greps the plan-writer and the agent definitions for LSP and changes the text that names it.
- **Credentials.** New credential paths (the gh token file, `.netrc`, `.npmrc`, `.git-credentials`, docker and kube configuration, `.pypirc`, cargo credentials) go in a second const consumed by `MANDATORY_DENY_READ` in `sandbox/settings/policy.rs` and by `default_deny_read` in `sandbox_config.rs`. `CREDENTIAL_DENY_READ_PATHS` (`fs/permissions/state_root.rs:51`) stays as it is, because `is_loom_written_read_deny` (`:125`) uses it to strip mirrors older loom versions wrote and must not claim operator-authored rules. `~/.codex/auth.json` is added by the capsule builder only when the stage is not licensed for Codex. Entries are plain paths with no globs, since a `denyRead` glob expands to one mount per match. A `denyRead` path that does not exist mounts nothing (`architecture/execution-containment.md:335-339`).
- **Sibling worktree metadata.** Directory denies on `R/.git/worktrees/<id>` for each sibling present at spawn reuse the `worktree_admin_dirs` loop in `sandbox/control_surfaces/session_denies.rs:79-82`. The stage's own entry, `refs/` and the object store stay writable: the merge resolver runs `git merge --no-commit --no-ff` inside its stage worktree (`orchestrator/signals/merge.rs:220-223`). Ref and common-dir denies are out of scope, because no deny list can narrow them while stages commit in-session (`concerns/agent-rule-bending-hardening.md` G2, `:118-120`); they need the withdrawn D1 (daemon-owned commits). A `denyWrite` on an absent path leaves an empty host placeholder, and an empty loose ref shadows `packed-refs`, so every `denyWrite` this plan adds names a path that exists at spawn.
- **Manual mode.** `print_manual_start` (`orchestrator/core/stage_spawn.rs:377-399`) and `stage_knowledge.rs:57-66` print a bare `claude` command. Each print site adds one line saying that the session runs without the wrapper's confinement.
- **Optional tail: sibling worktree reads.** An absolute `denyRead` of `<R>/.worktrees`, resolved at spawn, with the session's own worktree re-allowed through `allowRead`. Relative `../` forms are filtered out (`sandbox/settings/policy.rs:46`) and cannot be used.

Acceptance: a unit test per new deny on the emitted capsule, in the style of `sandbox/settings/tests_read_denies.rs`, including each disallowed tool name; a bubblewrap-only probe in a new test file (CI installs bubblewrap at `.github/workflows/ci.yml:118-122`; `tests_confinement_e2e.rs` is at 400 lines and skips without `srt` and `socat`); a test for the manual-mode line; the gate. A live, credentialed in-session canary (Bash, an interpreter, a build script, a symlink swap, the file tools and git refs) becomes a manual release checklist built from the withdrawn `sandbox-canary` stage text.

Dismissed:

- **The capability compiler and per-principal grants.** Too large for the residual problem, which the targeted denies above address.
- **An outer whole-process runtime.** Its residual routes are the tool routes above.
- **An ephemeral HOME or `CLAUDE_CONFIG_DIR`.** It breaks authentication and plugins.
- **Disabling agent teams and Remote Control.** Both are sanctioned features.
- **A secure planning session.** Planning happens outside loom's launch path.
- **A `CapabilityDenied` amendment flow.** Superseded by `loom stage block`, retry and amend.
- **Per-session package caches.** A full copy per session is infeasible at the recorded sizes (cargo registry 1.75 GB, bun 30 GB, npm 7.7 GB, uv 62 GB, no reflinks; `concerns/state-confinement-gaps.md:12-17`), and a cold cache would slow every stage's first build.
- **Knowledge stages in a worktree.** It touches merge flows that already shipped; it can return as its own plan.

## Existing open plans

| Plan | Recommendation |
| --- | --- |
| `PLAN-secure-distilled-loom-v2.md` | Do not run it; archive it. Its stages 12, 14, 16 and 17 shipped natively (retrieval, briefs, source graph, overlays), as did the plan-writer half of 18, most of stages 1–3, stage 10 (`orchestrator/merge_lifecycle.rs`) and stage 19 (the IV prefix's single gate owner). Stage 4's schema collides with the shipped `version: 2` plans. Stages 5 and 7–9 (outer runtime, run state machine, WAL/CAS, amendments) are too large for their problems, and stage 6's outer runtime and ephemeral home are dismissed in plan 7. Stage 11 survives as plan 7's capsule tests and release checklist, and stage 15 (the cache split) is dropped in section 6. Stage 13's main-edit ban and orchestrator profiles contradict operator decisions, and stage 20's Opus curator contradicts the settled distill tier. Decision 6's cumulative retrieval budget is invalid. Its valid remainder is in plans 2, 3 and 7. Its readiness revision `6158348d` is stale, and its two source documents were retired with this metaplan. |
| `PLAN-web-host-graft-followthrough.md` | Re-author before running. It was written against `85bac617` (736 commits behind HEAD). Its web `--host` half shipped in `89db4e69` and `444143fb`, its sibling notes name archived or absent plans (`IN_PROGRESS-PLAN-loop-recovery.md`, `PLAN-loom-state-confinement.md`), and its audit brief cites the deleted graft report. The remainder is `loom memory prepare` wired into both distill surfaces (`orchestrator/signals/cache.rs:187,217,219`, `commands/distill.md`); `commands/memory/handlers/prepare.rs` does not exist yet. Its owned globs cover `loom/src/commands/memory/**`, `orchestrator/signals/cache.rs`, `cache/**`, `tests_doctrine.rs`, `cli/**` and the ledger, which plans 2, 3, 4 and 5 also edit, so it runs between them, never interleaved. Until it is re-authored and merged, the distill prepass is an open dependency. |
| `PLAN-model-router-hooks.md` | Keep parked. The September audit found typed spawns clean, with no evidence of tier misuse. Revisit only if plan 2's agent-by-model report shows misuse. If revived, it owns `fs/permissions/constants.rs`, `fs/permissions/hooks.rs` and `cli/dispatch.rs`, which plans 1 and 5 edit. |
| `PLAN-adopt-ecc-best-practices.md` | Its hook profile (`LOOM_HOOK_PROFILE` minimal/standard/strict) is dismissed for this track: a minimal profile drops guards that prevent real violations, and their prevented-violation rate is unmeasured. Its post-edit rustfmt/clippy hooks add per-edit output to every session, so they need plan 2's replay before they run. |
| `PLAN-strengthen-verification.md` | Shipped (`7fe7736c`, `d47a3f35`); archive it. Plan 4 retires its item 3b, the keyword wiring checklist; item 3a, Cross-Stage Changes, stays. |
| `PLAN-merge-off-main-checkout.md` | Shipped (`git/merge/tree.rs`, `checkout_state.rs`, `loom/tests/merge_off_checkout.rs`); archive it. Plan 7's denies keep the shipped worktree resolver working. |
| `PLAN-loom-ui-mods.md` | Unshipped. Its YAML owns `src/commands/hook/**`, `src/cli/types_ops.rs` and the maintainability ledger, which plans 1, 2, 4 and 5 edit, so it runs before plan 3 or after plan 5. Its gauge reads Claude Code usage by design (its Decision 5) and takes nothing from plan 2. |
| `PLAN-codex-model-fallback.md` | Shipped (`e8796a83`; `loom/src/commands/pressure/fallback.rs`); archive it. |

## Disposition of the retired source documents

The five reports below were deleted on 2026-10-18, after each was reviewed in full against `6fee4ecd`; they were never committed, so these tables are their only record. `doc/token-optimization-evaluation.md` was retired the same day. It held no findings, and its contents moved: the artifact contract and verdict rules to the "Offline Comparison" section of `doc/loom/knowledge/architecture/token-accounting-and-receipts.md`, and the paired canary protocol to plan 2. "Shipped" means the mechanism exists in the tree, not that its saving was measured. Several reports included supporting data directories; `doc/loom-improvement-2026-09-18-data/` remains on disk and was not part of the deletion. "§6" below is section 6, the policy notes.

### `doc/token-optimization-2026-08-29.md` (30-day transcript census)

| Item | Disposition | Where |
| --- | --- | --- |
| A. Batch independent tool calls | Shipped: `CLAUDE.md.template:193`, orchestration skill, plan-writer stage sizing | — |
| A. Signal says "read these in one request" | Dismissed: signals list files to modify, not a read set; the batching rule is already resident | — |
| B. Make the context governor act | Shipped: absolute ceilings, hook handoff, daemon backstop (`models/constants.rs`, `post-tool-use.sh:132`) | — |
| B. 150k/100-request cap; split IV and distill | Dismissed: contradicts the 800k decision; splitting only to reset context was rejected | §6 (E3) records why a lower ceiling stays untested |
| C. Shrink the template | Shipped: 43.5 KB → 19,916 B, capped at 20,480 by test | Plan 5 continues by relocation |
| C. Uninstall plugin skills | Dismissed: user tooling outside loom; loom's own catalog is core-only (`skills/index_catalog.rs`) | — |
| D. Reread guard, ranged reads, shared-context ownership column | Shipped mechanism; receipts cannot form, and the path is removed | Plan 1 |
| E. Untyped-spawn detection, typed model pinning | Shipped (`spawn-guard.sh:209-252`; TYPE/MODEL columns in `loom subagents`) | — |
| E. Move subagents down a tier on price alone | Dismissed: capability is not traded for cost | — |
| F. Orchestrator effort | Shipped (standard stages at medium) | §6 (E2) |
| G. Make `loom map` reachable | Shipped at read time (`_read_discipline.sh` outline redirect); embedding outlines in signals dismissed, since every stage would pay for them | — |
| H. Plan re-embed, prefix size | Shipped (overview cap 4,096, `plan_overview: false`, prefix cap 8,192) | — |
| H. Cross-stage prefix file via `stable_prefix_hash` | Dismissed: each session needs its rules in its own context, and the prompt cache is not shared across stage sessions | — |
| I. Hand-rolled poll loops | Shipped (`poll-guard.sh`, `_progress-classification.sh`); residual forms dropped, since identical repeats are already counted | — |
| §1 Unregistered SessionStart hook | Shipped (`hooks/config.rs:16`; global `knowledge-orient.sh`) | — |
| Tier-1 knowledge oversize | Shipped (tier-1 files 8–21 KB, INDEX under 16 KB) | — |
| A weekly rollup subcommand | Shipped as on-demand `loom usage --since/--until` | — |
| "What not to do": keep the Knowledge Brief and subagent preambles, do not spawn fewer reviewers because they return little, do not reduce parallelism, do not chase output tokens or one-off large results | Adopted as constraints; section 6's dismissals follow them | — |
| Misleading figures | Constants it cites (`DEFAULT_CONTEXT_LIMIT=200_000`, 65/75%) no longer exist; output used first-fragment accounting (≈25% undercount); dollars were list-price notional; its batching bound counted dependent calls | Not copied forward |

### `doc/token-optimization-2026-09-12-codex.md` (Claude and Codex ledger)

| Item | Disposition | Where |
| --- | --- | --- |
| #1 Accounting repair: final-stream merge, `--until`, Codex coverage, forward join, quota history | Shipped (`transcript_merge.rs:18-21`, `codex_provider.rs`, `forward_join.rs`, `quota/history.rs`); residual: quota jitter, no record producers, dominance-only verdict, forward receipts that never form | Plan 2 |
| #2 Job-owned wait; wrapper-done/job-running race | Shipped (`subagents/wait/engine.rs`; `codex-forward.sh:330-340` blocks on the exact job) | — |
| Wrapper reporting newest jobs; guard recording no job id | Shipped (`codex-forward.sh:158,330-395`; `codex-forward-guard.sh:264-337`) | — |
| Poll-guard `loom` classification | Shipped (`_progress-classification.sh:88-95,118-129`) | — |
| #3 Mandatory/advisory "Load all" merge in signals | Shipped (`format/skills.rs:36-40`) | — |
| #3 Generic skill-trigger collisions; real-prompt replay suite | Partial ("context" is a stopword; golang generics remain); the phrase fix and one negative case are kept, the replay suite is dropped | Plan 5 |
| #3 P8 scoped worker brief | Shipped in stages; its records never bound and loom's own stages issued none; Codex units get their own brief from the Codex prompt hook | Plan 1; §6 (E6) |
| #4 Content-aware read receipts | Built; cannot form; removed | Plan 1 |
| #4 Batching doctrine, worker-table diagnostics | Shipped (`structural_checks/worker_table.rs`) | — |
| #5 Criterion cache false pass, environment/confinement fingerprint, files over 8 MiB, post-execution tree check, cached duration | Shipped (`cache_contract.rs:174-221`, `cache_fingerprint.rs`, `runner.rs:196-255`) | — |
| #5 One canonical verification owner | Shipped in the IV prefix; preamble wording still conflicts | Plan 5 |
| #6 Knowledge semantic freshness | Mechanism shipped (`catalog/evidence.rs`, `--strict-evidence`); 18 of 149 pages annotated; a page still quoted 150,000 (corrected during this review); `--strict-evidence` stays out of distill acceptance | Plan 3 |
| §7 Subscription weights, wake-up reliability, successor rereads | Subscription claims are unreachable at whole-percent precision; idle turns get tokens in `polling.rs`; successor rereads are measured by plan 4's offline harness | Plan 2; plan 4 |
| §7 Task model inheritance, SubagentStart additionalContext, PostToolUse additionalContext, skill-index input, Bash output limit | Superseded or settled (`updatedInput` delivery; the 12,000-character Bash cap) | — |
| Fable §8 defects, including the read-discipline hint trusting `LOOM_STAGE_ID` | Shipped (`_read_discipline.sh:352`) | — |
| "Policies to defer" | Adopted as binding constraints | §6 |
| Misleading claims | Its present-tense defects above are fixed; its links point at `hooks/` (now `loom-hooks/`), an archived plan and a data directory that no longer exists; the 434M polling figure is an exposure bound, not a saving | Not copied forward |

### `doc/REPORT-loom-improvement-findings-2026-09-18.md` (three-week transcript audit)

`DONE-PLAN-loom-efficiency-and-acceptance.md` implemented most of its sections 4.1–4.10. The residue:

| Item | Disposition | Where |
| --- | --- | --- |
| 1a–1f criterion lints, base-tree evaluation, knowledge baseline ratchet, plan-writer split | Shipped (`criterion_hazards.rs`, `base_tree_tests.rs`, `knowledge check --baseline`, plan-writer at 385 lines plus references) | — |
| 1g Wiring rejection names file and pattern; red pins repairable without dispute | Partial (`loom check --suggest` exists); no testable statement remains | — |
| 2a Bounded gate output | A verifier recipe with a scratch log; no new command | Plan 5 |
| 2b Subagent sizing; 2c role-scoped guidance; 2d preamble prepended by the guard; 2e `Task` removed from engineers | Shipped | — |
| 2f Rules 3/3b moved to the ceiling hook | The hooks carry them; the template bodies are cut | Plan 5 |
| 2g By-command Bash volume | Open | Plan 2 |
| 3a stopwords; 3b abstention and dropped omitted line in the prompt hook; 3c machine-generated gate; 3d precision floor; 3e ranking and stub suppression | Shipped | — |
| 3f Brief budget by session type | Waits for a type-dependent optimum | §6 (E5) |
| 4a resolvable skill names; 4b matcher stopwords and evidence | Shipped (golang residue in plan 5) | Plan 5 |
| 4c Skill recommendation carried into spawns | Partial (declared skills only); the rest dropped, since it adds per-spawn bytes | — |
| 4d `skills:` field | Shipped; the project-local resolution gap is dropped from this programme | — |
| 4e Slim oversized skills | The orchestration skill only | Plan 5 |
| 4.5 Tree-wide read receipts | Built; cannot form; removed | Plan 1 |
| 6a Advisory warning on a stage main's own edits | Open (decided 2026-09-18) | Plan 5 |
| 6b Widen the subagent verify guard | Declined (`DONE-PLAN-loom-efficiency-and-acceptance.md:35`) | — |
| 6c Residual `ls`/`wc`/`git status` polling | Dropped: identical repeats are already counted (`_progress-classification.sh:197-210`) | — |
| 6d Rule 9 names the harness reminder | Shipped | — |
| 6e Heredoc blocks | Shipped for quoted openers; unquoted openers stay refused, with a clearer message | Plan 5 |
| 6f Scratchpad allowed | Shipped | — |
| 6g Hook false positives | `git show -s` closed (`poll-guard.sh:231-246`); `no-preexisting-failures.sh` only warns | — |
| 6h Rule 8 versus Rule 14 | Rule 8 is already scoped to file readers; Rule 14 points at the verifier recipe | Plan 5 |
| 6i Skill-trigger text pinned | Shipped | — |
| 7a Delegation as a cost rule; 7c Codex unit sizing | Shipped | — |
| 7d Worker brief for ad hoc spawns | Open (stage-gated) | §6 (E6) |
| 8a Grouped pending worklist | Shipped (`pending --group`); full prepass owned by `PLAN-web-host-graft-followthrough.md` | — |
| 8b note shape; 8d tier-2 limits; 8f distill proposes hooks; 8g worker retrieval; 8h `delete-section` | Shipped | — |
| 8c Section state leaves retrieval; 8e cross-file duplicate headings | 8c: one fixture test; 8e: shipped (`catalog/headings.rs:68`) | Plan 3 |
| 9a window filter; 9b peaks | Shipped | — |
| 9c Before/after artifact per token change | The replay and the `RunEvidence` producer | Plan 2 |
| 4.10 `subagents watch` with `@` ids | Shipped (`wait/model.rs:44-48`) | — |
| 4.10 Untyped-warning wording | Shipped | — |
| 4.10 Fork detection | Dismissed until a hook event exposes forks | — |
| 6 Model router | Parked | Existing open plans |

### `doc/PROPOSAL-loom-overhead-reduction.md` (overhead-reduction proposal)

| Item | Disposition | Where |
| --- | --- | --- |
| §1.4, 9.1, 2.2 Opus/xhigh orchestrators, distill and bootstrap | Dismissed: contradicts the settled distill tier and effort defaults | — |
| §3.1, 7.1 Typed knowledge-readiness decision | Narrowed: the bootstrap warning fires only for a missing or scaffold-only tree; the digest-bound typed field is dismissed | Plan 3 |
| §3.2, 6.1, 6.3 Exactly one distill finalizer that cannot be skipped | A v2 lint requires it; skipping stays an operator choice, since memory is archived | Plan 3 |
| §6.2 Run state machine | Dismissed: too large; the memory archive already keeps lessons | — |
| §6.4 Sealed retention set; cleanup gated on a receipt | Narrowed: archive failure blocks cleanup | Plan 3 |
| §6.5 WAL/CAS two-phase apply | Dismissed beyond existing resolve receipts and `pending --strict` | — |
| §6.6 `replace-section`/`delete-section`, strict audit, baseline debt | Shipped | — |
| §6.6 "`loom review` prunes knowledge" claims | Open (false doctrine in three files) | Plan 3 |
| §6.6, 8.4 Opus curator with cheap gatherers | Dismissed (tier settled); deterministic prepass owned by the web-host-graft plan | — |
| §7.2 Knowledge stage in an isolated workspace | Dropped from this programme | Plan 7 (dismissed) |
| §3.3, 9.5 Closed allowlist for the stage `model` field | Open | Plan 3 |
| §3.3 Signal names the resolved orchestrator | The section names no model | Plan 3 |
| §9.1 Sonnet orchestrator profiles, `NeedsOrchestratorUpgrade` | Dismissed | §6 |
| §9.2 Luna/Terra routing; require `claude` beside `codex` | Superseded (haiku default; runtime fallback) | — |
| §9.2 Fable doctrine drift | README fix open; a typed `loom-fable-implementer` agent dismissed | Plan 3 |
| §9.3 Lifecycle ledger; block untyped spawns | Shipped | — |
| §9.3 Fatal no-main-agent-edits | Dismissed (operator decision 3); an advisory warning is kept | Plan 5 |
| §9.3 Block completion while workers are outstanding | Dismissed until evidence of the failure | — |
| §9.4 Sonnet routine reviewer | Dismissed: weakens the verifier | §6 |
| §10 `affects_final_snapshot`; §10.2 `final_verification_owner` field and tree assembly | Dismissed (v2 contracts and `reachable` cover them); the ancestry lint is kept | Plan 3 |
| §10.1 Stage Necessity Test; §10.3–10.4 stage-local checks plus one gate | Shipped | — |
| §10.1 Foundation as a worker assignment | Wording open | Plan 3 |
| §3.4 Sandbox policy booleans, strict allowlist, fail-if-unavailable; hook setup failure blocks spawn | Shipped (`sandbox/settings/policy.rs`, `stage_executor.rs:81-101`) | — |
| §3.5 Confine plan-authored commands | Shipped (bubblewrap / sandbox-exec); README stale | Plan 3 |
| §5.8 Capsule settings, setting sources, strict MCP, auto-memory off | Shipped | — |
| §5.8, 5.10 Tool routes outside the Bash sandbox | Open: capsule `permissions.deny` entries | Plan 7 |
| §5.8 Ephemeral HOME; disable teams and Remote Control | Dismissed | — |
| §5 Raw manual launch strings | A notice at each print site | Plan 7 |
| §3.4, 5.7 Read confinement beyond a denylist | Narrowed to an extended credential denylist; sibling worktree reads as an optional tail | Plan 7 |
| §5.9 Shared git control plane | Sibling worktree admin dirs denied; ref denies need daemon-owned commits; broker dismissed | Plan 7 |
| §5.1–5.6 Capability compiler, per-principal grants | Dismissed | — |
| §5.7 Outer whole-process runtime | Dismissed | — |
| §5.11 `CapabilityDenied` amendment | Superseded by `loom stage block/retry/amend` | — |
| §5.12 Adversarial canary matrix | Capsule unit tests, a bubblewrap probe and a manual release checklist | Plan 7 |
| §5.13 Secure planning session | Dismissed (outside loom's launch path) | — |
| §8, 8.1, 8.5 Catalog, briefs, budgets, dedupe, untrusted quoting, eval, tree-sitter graph, overlays | Shipped | — |
| §8.3 Retrieval filtered by principal grant | Dismissed (no principal grants exist) | — |
| §8.5 Semantic summaries and embeddings | Experiment only | §6 (E8) |
| §3.6, 13 Provider usage and cache counters, gates | Shipped; the `SignalMetrics` bytes/4 estimate is unused by any verdict, and the replay reports bytes | — |
| §11.2 Stable-prefix split; exclude-dynamic arm | Dropped: the probe is inert, the prefix would be sent twice, and the cache is not shared across sessions | §6 |
| §11.2 Persist doctrine/plan/policy/signal hashes for cache claims | Dropped with the cache experiment | — |
| §12 Plan-writer modularization | Shipped (385 lines plus 10 references) | — |
| §15 Phase 1 `loom plan migrate --check` | Dismissed (name collides with shipped `version: 2`) | — |
| §15 Phase 0.4 Locked memory append, idempotent index | Shipped | — |
| §15 Phase 6.5 Scheduler polling delays | Dismissed (unmeasured) | — |
| Stale claims | Metrics, source graph and context packs it called missing now exist; plan-writer size and template size figures are obsolete | Not copied forward |

### `doc/REPORT-graft-inspired-loom-distillation.md` (graft-inspired distillation report)

| Item | Disposition | Where |
| --- | --- | --- |
| Native tree-sitter source graph; Graft comparator spike | Shipped (12 grammars, `loom/Cargo.toml:39-53`); spike dismissed | — |
| Gap 1 / Phase 1 chunk catalog, ranking, fusion, budgets, omissions, `--explain` | Shipped (`fs/knowledge/chunker.rs`, `context/`) | — |
| Gap 2 map as a graph view | Shipped | — |
| Gap 3 INDEX full rescan per write | Dismissed (no measured cost at 149 files) | — |
| Gap 3 Block direct knowledge writes in worktrees | Shipped (`worktree-file-guard.sh:318-333`) | — |
| Gap 4 Locked append, full timestamps | Shipped | — |
| Gap 4 Memory summary round-trip | Writer deleted | Plan 3 |
| Gap 4 Structured event schema (confidence, fingerprint, cursor) | Partial; plan 4's one-line rendering contract covers what successors need; the rest dismissed with WAL/CAS | Plan 4 |
| Gap 5 Deterministic distill prepass | Owned by `PLAN-web-host-graft-followthrough.md`, which must be re-authored | — |
| Gap 6 Typed context pack; stage brief | Shipped | — |
| Gap 7 Lifecycle state and evidence | Shipped mechanism; stable-ID supersede/retire verbs dismissed with WAL/CAS | Plan 3 (freshness) |
| Gap 8 Remove `memory promote`; stop calling knowledge append-only | Shipped; the scaffold and plan-writer residue remain | Plan 3 |
| Gap 8 IntegrationVerify must not curate knowledge | Open | Plan 3 |
| Gap 8 Compact recovery | `--stage` scoping kept; "do not reread the signal" dismissed (weakens recovery) | Plan 3 |
| Base/overlay store; reconcile→merge→verify→publish→cleanup order | Shipped (`orchestrator/merge_lifecycle.rs`) | — |
| Phase 4 semantic summaries, cruxes, clusters; embeddings | Experiment only | §6 (E8) |
| Phase 5 LSP edges, SQLite, MCP transport | Dismissed until measured | — |
| Committed `.receipts/`, WAL, cursor advance | Dismissed beyond existing receipts | — |
| Stable frontmatter ids, generated/human fences | Dismissed with WAL/CAS | — |
| Phase 0 Mandatory terminal distill | A v2 lint | Plan 3 |
| Host finalizer for knowledge stages; bootstrap blocks invalid deliverables | Dropped: completion overrides stay operator overrides, the baseline is red at HEAD, and the bootstrap receipt is a freshness marker; plan 3 fixes the bootstrap warning only | — |
| Single writer: host rejects agent knowledge diffs | Narrowed to doctrine and the existing file-tool guard | Plan 3 |
| Graph result shapes with coverage and confidence | Shipped | — |
| Push small, pull large; delivery records; PreCompact epoch | Shipped; ordinary sessions reset after compaction; stage sessions do not compact | Plan 1 |
| Per-child pack CLI flags; shadow mode; interactive Stop sync | Superseded | — |
| One prompt-submit dispatcher | Dismissed: independent hooks, no ordering dependency | — |
| Path-only post-edit recorder | Shipped; its `dirty-paths.json` has no consumer; removal dropped, since its output goes to `/dev/null` | — |
| Fail-open, model-free hooks; remove CONTEXT DUMP | Shipped | — |
| Commit-guard knowledge path and stage scoping; post-commit reminder | Open | Plan 3 |
| Guidance #1–3, #5–7 | Shipped | — |
| Guidance #4 enforcement matrix | Rows added to `concerns/agent-rule-bending-hardening.md` | Plan 5 |
| Guidance #8 top-N incomplete; #9 edges not authoritative | Dropped: the knowledge contract already routes literal questions to `rg`, and a sentence would be paid in every stable prefix | — |
| `/loom-knowledge` skill | Dismissed (covered by `loom-usage`; its stale lines fixed in plan 3) | Plan 3 |
| Doctrine tests that named commands exist | Partial (plans only) | Plan 3 |
| README memory location and inheritance claims | Open | Plans 3 and 4 |
| Parallel distill gatherers | Dismissed (single-agent distill doctrine) | — |
| Untrusted rendering; tracked-only symlink-free enumeration | Shipped | — |
| Outcome A/B evaluation of retrieval | Kept as the admission gate for the semantic lane | §6 (E8) |
| Stale baseline | Template, `hooks/` and `.work` paths, removed `knowledge` subcommands, tier-1 sizes | Not copied forward |

### `doc/optimizations-suggestions-fable-astra-2026-10-18.md` (kept)

Every section of the review is owned by a plan here or dismissed:

| Review section | Where |
| --- | --- |
| What should change next; the three costs | Objective; plan 4 |
| P0 subscription interpretation and intervention attribution | Objective ("How a saving is shown"); plan 2 |
| P1 universal 150,000-token ceiling | Dismissed; §6 (E3) |
| P2 template and orchestration skill size | Plan 5; §6 (E7) |
| P3 measured effect of typed delegation | Plan 2 (agent-by-model join) |
| P4 runtime correlation and provider coverage | Plan 1 |
| P5 size caps are ceilings, not measurements | Plan 2 (replay bytes per section) |
| P6 effort defaults | §6 (E2) |
| P7 knowledge size enforcement | Shipped |
| P8 interactive work outside stages | §6 (E6) |
| P9 lifecycle proxies and deferred TTL | Plan 2 (orientation); §6 (E4) |
| P10 `maxTurns` | §6 (E4); `maxTurns: 1000` stays |
| P11 idle turns under owned waits | Plan 2 (`polling.rs` tokens) |
| Incidental August defects | Shipped; not reopened |
| The current memory path (capture, ordinary context, specialized view, scheduling races) | Plan 4; races dropped (drain before spawn; 0 refusals in 259 relays); own-stage export and predecessor outputs left as they are |
| A minimal dependency-findings change (capture, assembler, rendering contract, allowance, escape route, current-checkout validation, direct-only scope, information roles, exclusions, six acceptance cases) | Plan 4. Workers receive dependency symbols through the existing boost; a constant cap of about 400 tokens replaces the 800–1,500 allowance; the changed-evidence label and availability reporting are dropped with their reasons; only the keyword checklist is replaced |
| Delivery gap 1, compaction | Plan 1 (ordinary sessions; stage sessions do not compact) |
| Delivery gap 2, exact Read proof versus advisory ledger | Plan 1 (receipts cannot form; the advisory is reworded) |
| Delivery gap 3, worker and provider paths; source windows | Plan 1; §6 (E5, E6) |
| Delivery gap 4, explicit knowledge delivery | Dropped: at most 25 overlapping briefs in 102 sessions, and the CLI cannot write delivery state in a stage |
| Gap 5, optional follow-ups | Plan 4 (dropped items with reasons); §6 |
| What the existing measurements establish | Plan 2; no percentage target |
| Evaluation and order of work; subscription-claim conditions | Objective ("How a saving is shown"); plan 2's protocol and quota tolerance |
| Limits and maintenance | Its sentence that historical census files remain evidence is stale: the census was deleted |
| Claims corrected by re-verification | Receipts do not consume selection slots; deny is opt-in; PreCompact is stage-only; the orchestration skill grew; its knowledge anchor moved |

Its links to the retired August and September studies and to the evaluation document now point at this metaplan and the knowledge page.

## Knowledge corrected during this review

Per the stale-knowledge rule, these corrections were written to `doc/loom/knowledge/` on 2026-10-18:

- **`architecture/token-accounting-and-receipts.md`:**
  - "Read Receipt Lifecycle" states the `is_error` requirement, stage-only completion and the opt-in deny gate.
  - "Open Runtime Uncertainties" states that receipts do not form, and that Claude Code writes a call's tool_result row after its PostToolUse hooks finish.
  - "Offline Comparison" holds the full `--compare` artifact contract and verdict rules from the retired evaluation document.
- **`concerns/token-accounting-and-proof-defects.md`:**
  - "Delivery-Proof and Quota-Continuity Defects" covers receipt formation, the PostToolUse ordering, out-of-stage intents, compaction credit, missing global PreCompact, worker-brief records that never bind, forward receipts that never form, and quota jitter.
  - The resolved poll-guard size and model-palette concerns were removed.
- **`patterns/orchestrator-daemon-loop.md`:** the ceiling default is 800,000, not 150,000, and the stable prefix cap is 8,192 bytes, not "~1000 bytes".
- **`concerns/agent-rule-bending-hardening.md`:** G6 rewritten. Plan-authored commands run under an OS boundary by default, and the residual exposure is the `inherit` opt-out.
- **`mistakes/doctrine-and-acceptance.md`:** new section "A metaplan specified mechanisms without a need, a cost or a runtime sample".

Owned by a plan: `architecture/context-retrieval-state.md` (compaction, worker-brief binding and the history sentence at `:99-100`, plan 1); `architecture/memory-spool.md:32` and `architecture/signal-generation.md` (the wiring checklist that plan 4 deletes; accurate until then, plan 4).
