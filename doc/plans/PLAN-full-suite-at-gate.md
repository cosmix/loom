# PLAN: Full Suite Only at the Gate

Stop full test-suite runs outside the stage gate, and make repeated gate runs cheap through caching.

## Decision

- **The waste is the full suite.** The number of targeted runs does not matter.
- **Subagents** run the tests for the code they wrote, and its module, when they are confident the code is ready or the fix is correct. They never run the whole suite.
- **Orchestrators** run the full suite only through `loom check`.
- **Integration-verify is exempt from the one rule below.** Both its orchestrator and its reviewers may run the full suite. Every full run goes through `loom check integration-verify [--criterion <n>]`, so the pass cache and job joining dedupe it: on one tree, the first request runs and the rest join it or hit the cache.
- **A repeated full check is never refused.** Caching makes it cheap.

Rejected designs and the reasons for rejecting them are in `mistakes/test-sweep-and-fixture-discipline.md#duplicate-test-runs-were-framed-as-a-run-count-problem`.

## Evidence

Two scripts in `doc/plans/briefs/full-suite-at-gate/` scanned stage transcripts under `~/.claude/projects/*worktrees*`. Run either as `python3 -I <script> <days>`.

- **`gate_audit.py`**: 91 standard-stage sessions over 14 days.
  - Subagents ran about 870 test commands, mostly targeted.
  - Main agents ran 215 direct test commands and 663 `loom check` calls. Most of those calls were follows of one host job.
- **`full_runs.py`**: bare runner calls over 30 days.
  - About 60 came from subagents. Most of them are `troofoo` integration-verify reviewers running `bun test` again and again, which the guard's integration-verify carve-out allows. Others are a `tui-tokens` worker running `cargo test --all-targets` and some `collaterai` runs.
  - About 71 came from main agents. Examples are `vitest run` over everything, `bun run test`, and `cargo test --no-fail-fast` together with clippy and fmt, typically right beside a `loom check`.

## Changes

Each change has an implementation brief in `doc/plans/briefs/full-suite-at-gate/`. A brief quotes the current text at `path:line`, gives the exact replacement, and lists ordered steps with an acceptance command after each. **The brief is binding. Where this summary and a brief differ, the brief wins.**

### 1. Doctrine text: `w1-doctrine.md`

- **Standard-stage rule.** BLOCK-A's run bullets become one rule: run the tests for the code you wrote, and its module, when you are confident the code is ready or the fix is correct; never the whole suite. BLOCK-A is the subagent rule block, pinned byte for byte in four copies: the preamble, `blocks.rs`, `tests_doctrine_blocks.rs`, and the guard's `block()` message.
- **Orchestrator gate.** `ORCHESTRATOR_GATE` says the orchestrator never runs the full suite outside `loom check`.
- **Integration-verify.** The integration-verify prefix stops carrying BLOCK-A. `INTEGRATION_VERIFY_OVERRIDE`, the preamble's EXCEPTION bullet and the "never runs the gate" bullet now say: full runs go through `loom check integration-verify [--criterion <n>]`.
- **Review dimension 3** ("Build/test/sandbox") is assigned to a `loom-software-engineer`, because `loom-code-reviewer` has no Bash.
- **Other copies of the rule** are updated too: `CLAUDE.md.template`, `AGENTS.md.template`, `codex-forward.sh`, `commit-filter.sh`, README, CONTRIBUTING, the agent definitions, `skills/loom-orchestration`, and `skills/loom-plan-writer/references/{parallelization,codex-implementers}.md`.
- **Plan-writer overlap.** w1 also owns `parallelization.md:52-53` ("optional, ONE scoped check, skip if unsure"), which the skills sweep flagged but left out because it covers no plan-writer files.
- **Line count.** `cache.rs` is recorded in `loom/maintainability-baseline.txt`; update its count.

### 2. Skills: `w2-skills.md`

There are 49 line-neutral edits across 20 skills. Each whole-suite command or "rerun until green" line is labelled as the gate's and points to the targeted form. `loom-rust` and `loom-typescript` have no such lines and get no edit. Skill bodies are neither embedded in the binary nor pinned by tests; `dev-install.sh` refreshes the installed copies.

### 3. Parallel `loom check` in one session: `w3-host-jobs.md`

Integration-verify's orchestrator and reviewers share one `LOOM_SESSION_ID` and run full suites through `loom check`. A subagent's `loom check` already relays, with evidence in w3 §1.1; w3 adds a test for it. Three defects are fixed:

- **Cancel-and-replace.** An uncovered check on the same head, tree and session no longer cancels the live check, whether it is running or queued. It joins a new waiting job (`pending.json` beside `current.json`, `host_verify/pending.rs`, `store/slots.rs`), and that job runs the union of the waiting selections. A different tree, a different session, or a completion, freeze or verdict-completion request still supersedes, and cancels the waiting job too. Every cancel path clears the waiting slot. The runner promotes the waiting job even when every runner slot is busy.
- **Wait state.** Wait state is keyed by selection, so two reviewers in one session follow their own jobs.
- **Criterion order.** `--criterion` is sorted and deduplicated once.

Known limits, to record in knowledge:
- A waiting job's verdict covers the whole union, so a reviewer can see failures of criteria it did not select.
- Two followers of the same selection share one state file.

### 4. Finer caching: later

Two designs, with a choice between them deferred (one of them, not both):

- **Per-criterion `inputs:` globs:** narrow the content manifest in a criterion's key to the files it reads. Simple, but a missing input gives a false pass.
- **Incremental full suite:** reuse the last full pass and rerun only the tests the source graph and adapters select for the changed files, falling back to the full run when a changed file can't be traced.

### 5. Stale installed agents: `w4-install-and-guard.md` Part A

`install.sh` copies no agents; it calls `loom install-assets`. A new `prune_stale_agents` in `loom/src/assets/install/claude.rs` removes every `$CLAUDE_DIR/agents/loom-*.md` that has no source in `agents/`. That covers `loom-verifier.md`, which is retired, and `loom-mechanical-engineer.md`, which was renamed to `loom-software-developer`. A regular file is backed up with `back_up_asset` before removal, and a symlink is unlinked without being followed. `install.sh` itself is not edited: its delegation line is pinned by `constants_tests.rs`.

### 6. Guard split: `w4-install-and-guard.md` Part B

Guard lines 200-541 (the runner matchers) move verbatim into `loom-hooks/_verify_matchers.sh`, which is registered in `loom/src/fs/permissions/constants.rs` and in the two test fixtures that copy hook helpers. The guard drops to about 268 lines and the new file is about 356. `block()` stays in the guard. Guard behaviour is unchanged: integration-verify subagents keep full-suite access.

## Execution

| Order | Brief | Files owned | Suggested agent |
| --- | --- | --- | --- |
| 1 | w4 (Part B, then Part A) | `loom-hooks/subagent-verify-guard.sh`, `loom-hooks/_verify_matchers.sh`, `fs/permissions/constants.rs`, the two hook test fixtures, `assets/install/*`, `commands/install_assets.rs`, `assets/tests/write_assets.rs` | `loom-software-engineer` |
| 2 | w1 | every doctrine site in w1's sites table; edits the guard's `block()` text only after step 1 | `loom-software-engineer` |
| 2 | w2 | `skills/**` except `loom-orchestration` and `loom-plan-writer` | `loom-software-developer` |
| 2 | w3 | `loom/src/host_verify/**`, `commands/verify.rs`, `inbox_drain/check.rs`, `event_handler/host_job_cancel*`, `commands/hook/relay/tests.rs` | `loom-senior-software-engineer` |

- **Parallelism.** The three order-2 briefs write disjoint files and can run in parallel. w1's integration-verify wording describes w3's joining behaviour, so run the final verification after both.
- **Gates.** Each worker runs the narrow acceptance commands its brief lists. The executing agent then runs the full gate once on the final tree: `cargo test`, `cargo clippy -- -D warnings`, `cargo fmt --check`, the maintainability test, and the shell hook suites w4 names.
- **Knowledge.** Afterwards, update:
  - `conventions/guidance-channels-and-plugin-scope.md`
  - the host-job and integration-verify sections of `architecture/verification-v2-gates.md`
  - `entry-points/hooks.md` and `architecture/hook-system.md` (the new `_verify_matchers.sh`)
  - the `verification-v2-followups.md` bullet that w3 §8 deletes

## Out of scope

- **Follow calls of a running job.** The repeated `loom check` lines are one host job being followed. `host_verify/wait.rs::budget_for` caps a call at 100 s to stay inside Claude Code's 120 s Bash limit.
- **Any count of targeted runs**, and any refusal of a full `loom check`.

## Acceptance

- Every brief's own acceptance section passes.
- `rg -n "AT MOST ONE narrowly-scoped" --glob '!doc/**' --glob '!target'` finds nothing.
- The full gate passes on the final tree.
- **Field check:** after the next plan's stages, rerun `full_runs.py`. It should find no bare runner call by a standard-stage subagent or main agent, and no bare full-suite runner call in integration-verify: every full run there goes through `loom check`.
