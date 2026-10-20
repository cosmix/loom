# Plan: Stage exits and the stage environment

## Overview

Stage agents in PLAN-source-graph-mechanism avoided disputes, filed them late with work
uncommitted, and stopped to ask the operator for things a plan could have granted up front.
Four causes sit in the code:

- Completion's impact-selected tests pick the wrong tests: they diff against the nearest
  published base layer, which pulled `web/` vitest files into Rust-only stages. A runner
  that cannot start (no `node_modules`, a 403 from the npm registry) then reads as a failing
  test.
- Only acceptance criteria can be disputed. A wrong wiring check or wiring test has no
  route, and the doctrine frames a dispute as a cost.
- `loom stage block` by a stage agent leaves the agent's session alive. Its later exit is
  filed as a crash, which overwrites the block reason, charges the retry budget and
  auto-retries the stage into the same wall.
- Plans learn about network and install needs at run time. No lint knows which registry
  `bunx`, `uv` or `go get` reach, nothing reads the repository's git hooks, and a worktree
  has no way to install JS dependencies before a session starts.

This plan fixes each cause and states one doctrine for how a stage ends when it cannot
finish (BLOCK-F): a wrong check gets a dispute, a need only a person can meet gets a block,
and neither ends in a turn that asks the operator to act.

## Goals and non-goals

- Impact-selected tests run on the stage's own diff, and a runner that cannot start is a
  note.
- `loom stage dispute-criteria --field acceptance|wiring|wiring-tests`, end to end.
- A contract freeze refuses files that fail the stage's formatter check.
- A refused dispute leaves no open request; `loom stage reset` closes open disputes.
- A stage agent's `loom stage block` retires its session, the exit is not a crash, and
  `loom status` shows the reason; a subagent never blocks or disputes.
- Plan-level `provision` commands install worktree dependencies each time a stage leaves the
  queue for a worktree session (first spawn, retry, handoff successor, requeue after a
  verdict), run by the daemon from a `loom init` snapshot that no stage can edit. They may
  write only files git ignores. Sessions the contract phase starts on the same worktree reuse
  that install (Choice 8).
- `loom plan verify` errors on missing registry domains, on repository hooks the sandbox
  cannot serve, and on JS packages no `provision` entry covers.
- The plan-writer skill carries an environment inventory, so a person is rarely needed.
- Non-goal: impact-selected tests get no dispute route (settled). Once environment failures
  are notes, a failure there is a regression the stage fixes.
- Non-goal: no backward-compatibility code. The dispute `field` is required on the wire and
  on disk (see Choices).

## Preconditions

Run `loom init` on this plan only after all of these hold, in order:

1. PLAN-source-graph-mechanism has completed and merged to main.
2. A loom built from that main has been installed with `bash dev-install.sh`.
   `./finish-eval-fix.sh` refuses a binary older than the last `loom/src` commit
   (`check_binary`), so the install comes first.
3. `./finish-eval-fix.sh` has been worked through. The source-graph plan's integration-verify
   may already carry the eval refresh; the script then drops its stashed copy and says so.
4. If step 3 committed anything under `loom/src`, `loom/Cargo.toml` or `loom/Cargo.lock`,
   reinstall with `bash dev-install.sh`.
5. PLAN-web-host-graft-followthrough is not running. It edits `signals/cache.rs`,
   `signals/tests_doctrine.rs`, `commands/status/web/**`, `cli/**` and the maintainability
   ledger. Re-ground it after this plan merges: BLOCK-F sits in `append_completion_rules`,
   `STABLE_PREFIX_MAX_BYTES` is 8,192, and `status/web/model.rs` strips `close_reason`.

That fresh binary still predates this plan's impact-selection fix and its `provision` support,
so the installed-binary hazard below applies to every standard stage. It bites
`stage-exits` hardest: that stage branches only after `completion-gates` merges, so the
nearest published base layer can predate its branch point by a whole stage's work.

The baselines, byte counts and line numbers in this plan were measured at `b9aaea21`, before
these steps. Workers anchor every edit by symbol. On the new main, before `loom init`,
re-measure these and update the YAML and briefs wherever a number moved:

- `wc -l` of `loom/src/plan/schema/types.rs` (416), `orchestrator/core/stage_executor.rs`
  (597), `commands/plan/verify.rs` (555), `orchestrator/signals/cache.rs` (524),
  `daemon/server/dispute.rs` (384), `daemon/wire_tests.rs` (372),
  `commands/stage/acceptance_runner.rs` (366), `commands/init/plan_setup.rs` (396),
  `fs/work_dir.rs` (433) and `loom/tests/adjudication_e2e.rs` (525);
- the ledger lines for `handle_dispute_criteria` (91), `handle_one_event` (116), `start_stage`
  (149), `before_stage_gate_passed` (62) and `initialize_with_plan` (206);
- `wc -c CLAUDE.md.template` (18,734), `generate_stable_prefix()` (6,720) and the standard
  floor (8,513);
- `rg -n '^## '` on the knowledge sections knowledge-distill rewrites by exact heading (the
  source-graph plan's knowledge-distill may have moved them; `replace-section` appends when a
  heading does not match).

Then, from the repository root, run
`loom plan verify --strict "$PWD/doc/plans/PLAN-stage-exits-and-environment.md"`. The path
must be absolute: with a relative path the installed binary hands its lints an empty
repository root and skips the Rust test-filter lint ("the repository has no HEAD commit"), the
defect `plan-environment` fixes.

## Before `loom run`

- Committed on main: this plan and its briefs (`fedd3285`) and the four knowledge files
  (`3fc28031`), including `concerns/agent-rule-bending-hardening.md#Stage Agents Stop and
  Report Instead of Disputing or Blocking`, which knowledge-distill rewrites. Commit every
  later edit to this plan or a brief (the pressure-test revisions included) before
  `loom init`: a stage worktree is cut from `HEAD`, so an uncommitted brief is missing or stale
  there and its worker starts blind (`mistakes/verification-harness.md`, "Untracked Plan and
  Worker Briefs Leave a Worktree Stage Blind").
- `~/.bun/install/cache` exists on this host (checked 2026-09-30), so a first `bunx` or
  `bun install` in a worktree does not fail with `EROFS`.
- The operator runs `cargo audit` once in `loom/` on the host. That refreshes the RustSec
  database in `~/.cargo/advisory-db` (present on 2026-10-01), which the sandbox cannot fetch
  (github.com is not allowed); `completion-gates` then runs `cargo audit --no-fetch` against
  that copy, with a write grant for the database's lock file only.

## HAZARD: this plan runs on the installed binary

Every stage runs under the loom binary installed per the Preconditions, built from main
after PLAN-source-graph-mechanism. That binary still diffs impact selection against the
nearest published base and has no `provision` support; a binary built from this plan is
installed only after the plan completes (`mistakes/verification-harness.md`,
"The PATH Binary Can Lag `main` MID-PLAN, Not Just Behind Your Build"). If `loom stage
complete`'s impact-selected step picks `web/` vitest files, the stage agent runs
`cd "$(git rev-parse --show-toplevel)/web" && bun install --frozen-lockfile` in its worktree
and completes again (a stage's shell starts in `loom/`, where a bare `cd web` fails; the
lockfile is `web/bun.lock`): `registry.npmjs.org` is allowed, `web/node_modules/**` is in
`allow_write`, and every stage's sandbox grants the bun cache
(`sandbox/package_caches.rs`). Every standard stage's description repeats this.
Integration-verify runs no impact-selected tests, so it needs no such step.

This plan declares no `provision` entry: the installed `loom plan verify` and `loom init`
reject the unknown field.

## Knowledge bootstrap: skipped

The tier-1 files already describe this codebase, and `loom knowledge check` reported 0
issues on 2026-09-30. The seams this plan changes are covered by
`architecture/adjudication-lifecycle.md`, `concerns/runtime-and-session-safety.md`,
`concerns/agent-rule-bending-hardening.md` and
`mistakes/computed-values-and-hidden-couplings.md` ("StageSummary Is a Wire Type"), which
each stage's Knowledge Brief quotes.

## Execution diagram

```mermaid
graph LR
    completion-gates --> stage-exits
    completion-gates & plan-environment & stage-exits --> integration-verify
    integration-verify --> knowledge-distill
```

`completion-gates` and `plan-environment` start together. `stage-exits` starts once
`completion-gates` has merged and runs beside `plan-environment` if that is still going.

## Stage necessity

- **completion-gates.**
  - Q1: `stage-exits` teaches `dispute-criteria --field` in its doctrine, its failure
    guidance and the usage skill, so that syntax must be merged first.
  - Q2: `stage-exits` also edits `dispute_transport.rs`, `dispute_criteria_tests.rs` and
    `daemon/wire_tests.rs` after this stage.
  - Q4: merged with `plan-environment` (seven workers, fourteen contracts, about 65 files)
    one session passes 500,000 tokens.
- **plan-environment.** Q4, as above. It shares no file with the other two stages, the
  maintainability ledger included: its edits to ledgered files are net-zero, so only
  `completion-gates` edits `loom/maintainability-baseline.txt`.
- **stage-exits.** Q1 and Q2 on `completion-gates`, as above.

## Gate conventions (every code stage)

- `working_dir: "loom"`. Loom runs each contract as `cargo test <name> -- --exact` from the
  stage's `working_dir`, and the repository root has no `Cargo.toml`. YAML paths in `files`,
  `acceptance`, `artifacts`, `wiring`, contract `file` and the worker tables are
  package-relative; paths outside the package are written `../skills/...`, `../loom-hooks/...`
  or `../CLAUDE.md.template`. Prose paths are repository-relative (`loom/src/...`).
- Warm-up: `cargo build --all-targets` is the first acceptance entry. The main agent starts it
  in the background before briefing wave 1 and runs every acceptance command once in-session
  before `loom stage complete` (each simple acceptance command has a 300 s cap).
- The canonical gate. `CONTRIBUTING.md` names `loom/.githooks/pre-push` authoritative; this
  plan runs every item of it, each where it can observe the change:
  - The full suite (`cargo test --all-targets --no-fail-fast`, 1 min 44 s on the host at
    `b9aaea21`) runs once, in integration-verify. A standard stage runs module filters and
    named `--exact` tests, plus the build, clippy, fmt and rustdoc with warnings denied. This
    is the project's settled rule (`mistakes/ci-toolchain-and-cargo.md`, "The Same Suite Ran
    Once Per Stage, Per Check, Per Judge"), and `loom plan verify --strict` fails a plan whose
    standard stage runs the unfiltered suite (`plan/schema/validation_suite.rs`). The filters
    therefore cover every module and test target a stage's change reaches, not only the files
    it edits: project detection reaches `skills::`, `commands::project` and
    `commands::hook::`; the self-service dispute client's tests run under
    `daemon::server::client::`; `loom plan verify` has its own integration target; and
    `plan-environment`'s new binary-spawning contract file is checked by the spawn guard
    (`tests/integration/binary_spawn_guard.rs`, which fails any test file that spawns the
    binary without `helpers::loom_cmd()`), named in that stage's acceptance.
  - The markdown lint, read-only (`bunx markdownlint-cli2` over the tracked `*.md` files the
    pre-push hook lints, without `--fix`), in every stage that edits Markdown:
    `plan-environment`, `stage-exits`, integration-verify and knowledge-distill. It reported 0
    issues in 265 files on 2026-10-01.
  - `scripts/flake-check.sh` with the filter of each stage's own timing-sensitive tests:
    `verdict_apply_tests::` in `completion-gates` (G2 edits `verdict_apply_tests.rs`),
    `verdict_retirement_tests::` in `stage-exits` (S1's retirement tests). Its default filters
    run in integration-verify. Each filter is its own acceptance entry, with its own 300 s cap.
  - `cargo audit --no-fetch` runs once, in `completion-gates`: no stage changes `Cargo.lock`,
    and integration-verify's `working_dir: "."` has no `Cargo.toml`, so `plan verify` would
    warn on it there (cargo has no stable `-C`, and `cargo audit` takes no
    `--manifest-path`). It exits 0 at `3fc28031` against the host's database.
- Workers do not run `cargo fmt` (they share one crate mid-wave). After the last wave the main
  agent runs `cargo fmt --all` once, then the acceptance commands.
- Contracts live in one integration-test file per stage (`tests/<stage>_contracts.rs`,
  discovered by cargo, so no harness file is needed). Several name symbols the stage has not
  written, so they freeze as `build_failed`. Before the final review round the main agent
  takes each contract, applies the implementation its `rejects:` names (or reverts the key
  line), confirms the contract fails, restores the tree, and records
  `loom memory note "mutation: <contract-id> red under <mutation>"`.
- The maintainability ledger (`loom/maintainability-baseline.txt`, a ratchet file): a stage
  that removes or lowers a ledger line files ONE `dispute-integrity` for
  `TI-ratchet-loom/maintainability-baseline.txt` after its final review round, with the reason
  "tightening only: <entry> removed after the refactor this plan prescribes". The briefs mark
  each ledgered item "net-zero" or "remove".
- Existing assertion lines in test files are never edited (a `TI-edit` event); new
  assertions go in new lines or new tests. `common.md` in the briefs directory states this
  and the other worker rules.
- Anchors: edits are anchored by symbol; line numbers in this plan were read at `b9aaea21`.

## Baseline evidence

Measured on the host at `b9aaea21`, not under a stage sandbox:

- `cargo test --all-targets --no-fail-fast` in the main checkout (`loom/`): exit 0, 28
  targets, 6,936 tests passed, 1 min 44 s.
- In a clean `git archive HEAD` copy: `cargo build --all-targets` (37 s warm),
  `cargo clippy --all-targets -- -D warnings`, `cargo fmt --all -- --check`,
  `RUSTDOCFLAGS='-D warnings' cargo doc --workspace --all-features --no-deps`,
  `cargo test --test maintainability` (8 passed), `cargo test --test adjudication_e2e` (13
  passed), the three `loom-hooks/tests/commit-guard-*.sh` scripts, and every `--lib` module
  filter this plan's acceptance names that exists at HEAD all exit 0 with at least one
  test selected (`verify::impact_tests::` 2, `verify::review::fingerprint` 7,
  `skills::project::` 21, `orchestrator::adjudication::` 105, `daemon::server::dispute` 16,
  `daemon::server::self_service` 8, `daemon::wire::` 13, `verify::goal_backward::` 51,
  `verify::contracts::` 29, `commands::stage::` 185, `completions::` 103,
  `fs::stage_request::` 18, `orchestrator::core::inbox_drain::` 20, `relay::` 137,
  `orchestrator::signals::` 222, `plan::schema::` 249, `commands::plan::` 5,
  `orchestrator::core::stage_executor` 7, `orchestrator::core::event_handler::` 61,
  `orchestrator::monitor::` 99, `commands::status::` 422, `fs::permissions::` 119,
  `models::` 254). The full suite in that copy failed 10 tests for reasons of the copy's
  location only (a path containing `claude`, a scratch root under `/tmp`, and a build commit
  of `unknown`); the main-checkout run above is the baseline.
- `daemon::server::stage_control` selects zero tests at HEAD, so no acceptance entry filters
  on it.
- `generate_stable_prefix()` is 6,720 bytes against a 7,168-byte ceiling
  (`orchestrator/signals/tests_size.rs`), the standard signal's stable plus semi-stable floor
  is 8,513 of 10,240, and `CLAUDE.md.template` is 18,734 of 20,480 bytes. BLOCK-F is 850 bytes.

## Choices this plan settles

Where the brief left a detail open, the plan takes one value:

1. The dispute `field` is required in `DisputeKind::Criterion`, `Request::DisputeCriteria` and
   `StageRequest::Dispute`, with no serde default; the CLI flag defaults to `acceptance`.
   `DisputeKind::criterion(field, index)` is the constructor every call site uses.
2. An accepted verdict on an acceptance dispute may still amend `acceptance` or `wiring`
   (unchanged). A wiring dispute's verdict must amend `wiring`, and a wiring-tests dispute's
   `wiring-tests`; any other field is a `needs-more-evidence` question.
3. Goal-backward gaps print `[wiring n]` and `[wiring_tests n]`, 0-based, the index
   `--criterion-index` takes with `--field wiring` or `--field wiring-tests`.
4. The impact selection set is every path changed since `git merge-base HEAD <target>`, the
   set the test-integrity fingerprint uses, listed by ONE helper both share
   (`fingerprint.rs::changes_since_merge_base`); the graph is still built on the nearest
   published base. When that listing fails (a missing target ref, or pinned git that cannot
   resolve inside a stage sandbox), selection falls back to the graph's own changed set and
   says so in a note: a test that cannot be selected is a note, never a failure.
5. A JS adapter is one whose `language()` is `javascript` (vitest, jest, mocha, bun-test,
   node-test). Its package needs an install when its `package.json` declares at least one
   `dependencies` or `devDependencies` entry (an unreadable manifest counts as declaring
   one); such a package is ready when a `node_modules` directory exists in its package
   directory or an ancestor up to and including the checkout root. A dependency-free
   `node --test` or `bun test` package always runs. Exit status 127 is a note for every runner.
6. Fixture directories: detection skips every directory named `fixtures` below the scan root.
   A stage whose `working_dir` lies inside such a directory loses its detected runner; that is
   accepted.
7. The freeze runs each formatter-check criterion with the stage's `setup` prefix, from the
   stage's working directory, under the stage's command confinement, with the acceptance
   runner's own timeouts (a Simple criterion 300 s, an Extended one 30 s). A criterion is a
   formatter check only when every simple command it expands to is one: a compound
   `cargo fmt --check && cargo clippy` is not run, because at freeze time the contracts are red
   and uncompilable by design. `gofmt -l` is not a check (it exits 0 on unformatted files).
8. `provision` is a `version: 2` field, like `ratchet_files`. `loom init` copies the entries
   into the work directory's `config.toml` (`[plan_provision]`, beside the `[plan_sandbox]`
   snapshot), and the spawn gate runs only that copy, never the live plan file: a stage's
   sandbox may write `doc/**`, plan files are not merge-gate control paths, and provision runs
   on the host, so reading the live plan would let a merged stage edit run host commands. To
   change entries during a run, the operator edits `[plan_provision]` and runs
   `loom stage retry <id>`. The gate runs the `before_stage` checks on the pristine worktree
   FIRST, then provisions (`find_prior_stage_work` skips the before-stage delta proof when it
   sees any uncommitted, non-ignored file, which a provision could write). It runs
   synchronously in the daemon's scheduling pass, like `before_stage`: a slow install delays
   the daemon's tick by up to 600 s per entry, and a helper thread stamps the tick every 10 s
   meanwhile so `loom status` does not report a stalled loop. The gate sits on `start_stage`,
   the one path by which a worktree stage leaves the queue: a first spawn, a retry, a handoff
   successor and a requeue after a verdict all pass it, for standard stages (before the
   contract session), integration-verify and knowledge-distill. Two things never reach it.
   The first is knowledge-bootstrap, adjudication and merge sessions, and an adopted live
   session. The second is the contract phase's own spawns on the same worktree
   (`event_handler/contract_phase.rs::spawn_on_stage_worktree`: the contract-to-implementation
   handoff and a replacement contract session). Those reuse the install that the same
   `start_stage` made minutes earlier. That is safe because a provision may write only files
   git ignores, and no session is expected to delete them: a session that breaks its own
   dependencies is recovered by `loom stage retry`, which requeues the stage through the gate.
   The gate enforces the ignored-only rule. It lists `git status --porcelain -z
   --untracked-files=all` before and after provisioning, and any entry that appears only
   after blocks the stage with the paths named and "a provision command may write only files
   git ignores". Such a file would otherwise count as the contract session's own non-contract
   edit (`contracts/freeze.rs::checked_reports` refuses it) and, on a retry, as prior stage
   work (`find_prior_stage_work`). When either listing fails (a worktree git cannot read),
   the gate skips the comparison with a warning, as `find_prior_stage_work` does. A
   `loom stop` mid-install leaves the command to finish or time out; the stage is still
   Queued, and the next spawn provisions again.
9. A failed provision (a failing command, or one that leaves a file git does not ignore) sets
   `failure_info` (`InfrastructureError`, the reason's lines as evidence) as well as
   `close_reason`, so attention never labels it an agent block. The
   reason keeps the last 10 non-blank lines of stderr, each cut to 300 characters.
10. The registry lint scans acceptance, setup, wiring tests, after-stage checks and the
    dead-code check. It skips `before_stage`, which runs on the host.
11. `*.x` matches any host ending in `.x`, `*` matches every host, anything else matches
    exactly.
12. The JS provision lint runs on `version: 2` plans only (`provision` is v2-only); the
    registry and hook lints report warnings on v1 plans.
13. BLOCK-F is emitted by `append_completion_rules`, which serves the standard,
    integration-verify and knowledge-distill prefixes: `cache.rs` is ledgered at 524 lines,
    so no new call line can go there, and the rule holds for knowledge-distill too.
    `STABLE_PREFIX_MAX_BYTES` rises from 7,168 to 8,192 (about 7,575 bytes after BLOCK-F),
    documented in `tests_size.rs` as the BLOCK-D raise was. The agreement test reads all
    three prefixes and the orchestration skill.
14. A relayed dispute's notice tells the agent to end its turn, as a relayed block's does.
15. Attention for an agent block keeps `retry_guidance`'s command (`loom stage retry <id>`,
    or `--force` at the retry limit).
16. The sandbox keeps the source-graph plan's `~/.cargo/advisory-db..lock` grant
    (`completion-gates` runs `cargo audit --no-fetch`) and adds
    `loom/maintainability-baseline.txt` (`completion-gates` removes one ledger line).
17. `plan/schema/types.rs` (ledgered at 416 lines) stays net-zero: the three lines of the
    `provision` field replace the four-line `PermissionMode` re-export comment, cut to one.
    `commands/init/plan_setup.rs::initialize_with_plan` (ledgered at 206) stays net-zero: its
    two-line `write_plan_sandbox` call becomes a two-line `persist_plan_snapshots` call.
18. Every block writer clears `failure_info`: `handle_block_stage` (relayed, spooled and
    socket blocks), the daemon-down direct write in `state_relay.rs`, and
    `human-review --reject`. Nothing else clears it after a crash (auto-retry only requeues;
    `loom stage retry` clears it only with `--force`), so without this a stage that crashed
    once would read as crash-blocked after its agent's block: no retirement, the exit filed as
    a crash, and an auto-retry of the stage the agent blocked. "Blocked with no
    `failure_info`" then reliably means an agent's or operator's block or a rejected review.
19. The daemon retires the live agent on every such block, the operator's included (an
    operator blocking a stage mid-work stops its agent, with a handoff). It never retires an
    adjudication or a merge-resolution session (`MergeConflict -> Blocked` is legal, and a
    killed resolver would never report `MergeSessionCompleted`), and a block with nothing to
    retire writes nothing (a daemon restart re-emits `StageBlocked` for every Blocked stage).
20. The JS-provision and pre-commit-hook lints judge the repository, not the stages. After
    this plan, every v2 plan in this repository needs
    `provision: [{ working_dir: web, command: "bun install --frozen-lockfile" }]` and, in every
    sandboxed stage, `registry.npmjs.org`; the plan-writer skill says so. For the same reason
    `loom/tests/fixtures/plans/v2-valid.md` passes `loom plan verify --strict` only from a
    scratch repository; its header and `CONTRIBUTING.md` are corrected.
21. BLOCK-F binds a stage's main agent. `CLAUDE.md.template` and the subagent preamble, which
    subagents read, say a subagent reports a need or a wrong check to its orchestrator and
    never runs `loom stage block` or `loom stage dispute-*`: a subagent's relayed block would
    retire the main session mid-orchestration.

## Stages

### 1. completion-gates

Waves (worker table in YAML):

1. **Wave 1, three workers in one message.**
   - G1 (sonnet): impact selection on the stage's own diff, JS runner readiness, exit 127,
     fixture directories.
   - G2 (sonnet): `CriterionField` and `DisputeKind::Criterion.field` first, then the CLI,
     transport, wire, daemon handlers, spool and relay drains, dynamic completion, dispute
     bookkeeping (check the transition before writing `request.md`), `reset` closing open
     disputes, the `handle_dispute_criteria` refactor, and every `DisputeKind::Criterion`
     literal outside `src/orchestrator/adjudication/`. `dispute.rs` is 384 lines with inline
     tests; G2 first moves them verbatim to `daemon/server/dispute_tests.rs` (a `#[path]`
     module, same test path), or the widened test calls push the file past 400.
   - G4 (sonnet): the formatter gate of `loom stage contracts freeze` and the contract-session
     rule.
2. **Wave 2: G3 (opus).** The adjudication side of `--field`: the prompt shows the right entry,
   the verdict validator checks the amended field, `plan_patch` accepts `wiring-tests`, the
   apply path resyncs `wiring_tests`, gap labels, the rejection wording (`apply_reject` keeps
   "upheld the disputed acceptance criterion" for acceptance disputes, which two assertions
   pin; `feedback.rs`), the `close_open_disputes` doc now that reset calls it, and the literals
   under `src/orchestrator/adjudication/`. The crate does not compile between the waves; G3's
   one check builds wave 1's files too, and an error outside G3's files goes back to the main
   agent, never into G3's hands.
3. **Main agent:** `cargo fmt --all`, then the acceptance commands.

Risk walk:

| Area | Contract or check |
| --- | --- |
| Untrusted input (index and field from the wire) | `wiring-dispute-indexes-the-wiring-list` |
| Lifecycle (a refused filing, a reset) | `refused-dispute-writes-no-request`; `dispute_kinds::tests::a_refused_file_dispute_writes_no_request`, `state_tests::reset_closes_open_disputes` by name |
| Configuration (what detection counts as a package) | `fixture-directories-hold-no-packages` |
| External data (a runner's environment, a formatter's verdict) | `js-runner-without-node-modules-is-a-note`, `freeze-refuses-unformatted-contract-files` |
| Reachability (`--field` reaches the amendment; `run` uses the new set) | `wiring-tests-verdict-must-amend-wiring-tests`; wiring test on `--help`; `impact_tests::tests::run_selects_the_committed_stage_diff` and `field_values_complete_for_dispute_criteria` by name |
| The behaviour the stage exists for | `impact-selection-uses-the-stage-merge-base` |

Filesystem paths and process scale do not apply beyond the unit tests the briefs name: the
readiness lookup never walks above the checkout root, and the change listing is NUL-separated
git output.

Callers checked:

- `WorktreeGraph.changed`: read outside its own module's tests only by `impact_tests.rs:78`
  (which keeps it as the fallback when the merge-base listing fails) and one test assertion
  (`impact_tests_tests.rs:110`). `build_for_worktree` also serves reachable checks
  (`verify/goal_backward/mod.rs:95`) and integration-verify re-verification
  (`complete_verification_v2.rs:97`); neither reads `changed`, and `worktree_graph.rs` is not
  edited.
- Project detection: `verify/contracts/mod.rs` (a contract's runner), `verify/impact_tests.rs`,
  `skills/recommend.rs`, `commands/hook/project_types.rs` and `commands/project.rs`
  (`loom project detect`). All five want fixture manifests left out. `loom map --eval-edges`
  and the labelled corpora never call detection.
- SILENT sites: `verdict.rs:58` `Criterion { .. }` must pass the field (G3). `verdict_kinds.rs`
  `(_, other)` is never reached by a criterion dispute (`parse_and_validate_for` routes it to
  `classify_and_validate`), so it is a NO-OP. The `apply_kinds.rs` `_ =>` arms send every
  criterion verdict to `apply_accept`/`apply_reject`, which read the field from the amendment:
  NO-OP. `loom-hooks/loom-relay.sh:64-65` maps the subcommand to the `dispute` kind and relays
  the serialized `StageRequest::Dispute`, which now carries the field: NO-OP.
  `completions/dynamic/mod.rs:281` completes stage ids; the flag list comes from clap, and G2
  adds value completion for `--field`.
- `persist_verdict_result` and `resync_after_amendment` (`apply.rs:140-257`) copy
  `acceptance` and `wiring` but not `wiring_tests`; G3 adds it to both.

Expected integrity events: `TI-ratchet-loom/maintainability-baseline.txt` (G2 removes
`function src/daemon/server/dispute.rs handle_dispute_criteria 91` after refactoring the
function under 50 lines). `tests/adjudication_e2e.rs` stays at exactly 525 lines. No existing
assertion line changes: the assertions moved with `dispute.rs`'s tests leave a production
file (no test-file glob matches `dispute.rs`, so the integrity gate never counted them there)
and only add to the totals in `dispute_tests.rs`.

### 2. plan-environment

1. **Wave 1, two workers in one message.**
   - P1 (sonnet): the `provision` schema and v2 rules, the executor and the init-time
     snapshot (`orchestrator/provision.rs`, called from `loom init`), and the spawn gate
     (`orchestrator/core/provision_gate.rs`: before-stage checks, then provisioning, then the
     refusal of any file the install left that git does not ignore).
   - P3 (sonnet): the plan-writer skill: the environment inventory, `provision`, the
     `loom repair` fix, and the Section 9 and reference wording that agrees with BLOCK-F.
2. **Wave 2: P2 (sonnet).** The registry, hook and JS-provision lints, the fixture plan, the
   `v2-valid.md` header, and the one-line `plan verify` fix that gives the lints an absolute
   repository root. The hook lint finds the hooks directory with scoped `git config` reads:
   loom's git runner forces `-c core.hooksPath=/dev/null` on every command, so
   `rev-parse --git-path hooks` always answers `/dev/null`.
3. **Main agent:** `cargo fmt --all`, then the acceptance commands.

Risk walk:

| Area | Contract or check |
| --- | --- |
| Untrusted input / filesystem paths (a `working_dir` that escapes) | `provision-working-dir-cannot-escape`, `provision-refuses-a-symlinked-escape` |
| Process I/O (a failing install's output) | `failed-provision-names-command-and-dir` |
| Configuration (the entry reaches the right directory) | `provision-runs-in-its-working-dir` |
| Lifecycle (retry re-runs provisioning; the gate order) | commands are idempotent by rule; `provision_gate::tests::a_failing_provision_blocks_the_stage_with_its_reason` and `a_failing_before_stage_check_blocks_before_provisioning` by name |
| Filesystem (provision output the contract freeze and a retry would read as stage work) | `provision_gate::tests::a_provision_that_leaves_an_unignored_file_blocks_the_stage` and `a_provision_that_writes_only_ignored_files_spawns` by name |
| Test harness (a contract that spawns the binary) | `binary_spawn_guard::only_sanctioned_spawners_spawn_the_loom_binary_directly` by name |
| Trust boundary (who writes the commands the host runs) | `provision_gate::tests::the_gate_reads_the_snapshot_not_the_plan` by name; wiring on `persist_plan_snapshots(` in `plan_setup.rs` |
| Reachability (lints registered in `loom plan verify`) | `js-package-without-provision-is-an-error`, `registry-tool-without-registry-domain-is-an-error`, `pre-commit-hook-registry-need-is-an-error` (all drive the binary) |
| External data (the repository's hook path and script) | `pre-commit-hook-registry-need-is-an-error` |

Found while grounding: `loom plan verify <relative path>` passes an empty repository root to
the lints (`find_repo_root` walks the parent of a relative path, and `"".join(".git")` exists
at the checkout root), so `git` runs with `current_dir("")` and fails. Today that is why
`loom plan verify doc/plans/X.md` notes "the repository has no HEAD commit". P2 passes an
absolute path (net-zero line change in the ledgered `verify.rs`), which the new lints need.

Expected integrity events: none. `plan/schema/types.rs` (416 lines), `stage_executor.rs`
(597 lines, `start_stage` 149, `before_stage_gate_passed` 62), `plan/schema/validation.rs`
(`validate` 357, untouched), `commands/plan/verify.rs` (555) and
`commands/init/plan_setup.rs::initialize_with_plan` (206) stay net-zero; `fs/work_dir.rs`
(433) is not edited.

### 3. stage-exits

All three workers in one message:

- S1 (opus): block lifecycle. Every block writer clears `failure_info` first (Choice 18).
  On `StageBlocked` with no `failure_info` (any `loom stage block`, the agent's or the
  operator's, or a rejected review) the daemon retires the stage's live agent, never an
  adjudication or merge-resolution session, through the disputing-agent takedown (handoff,
  kill, confirm, clear `stage.session` while still Blocked), and
  `exited_after_stage_finished` forgives an exit on such a stage.
- S2 (sonnet): `StageSummary.close_reason` on the wire, flattened, stripped before the browser
  snapshot, and the attention note.
- S3 (sonnet): BLOCK-F and every surface that must agree with it, and the subagent surfaces
  (`CLAUDE.md.template`, `_subagent-preamble.txt`) that keep subagents from blocking or
  disputing (Choice 21).

Then the main agent runs `cargo fmt --all` and the acceptance commands.

Where 0499aa3d's retire-before-status ordering applies: the block status is written before
the daemon learns of it, so the handler retires while the stage is Blocked and clears
`stage.session` only if the stage is still Blocked; a stage retried in between keeps its new
session. `loom stage retry` already refuses a live `stage.session` without `--force`
(`commands/stage/skip_retry.rs:54-69`).

Risk walk:

| Area | Contract or check |
| --- | --- |
| Untrusted input (an agent-written reason in terminal output) | `sanitize::tests::a_control_sequence_in_the_close_reason_is_flattened` by name |
| Lifecycle (the retired session's exit, a crash on a blocked stage, a stale `failure_info`) | `session_events::tests::an_agent_blocked_stage_session_exit_is_not_a_crash`, `verdict_retirement_tests::an_agent_block_retires_the_live_session` (driven through `handle_events` with a stale crash `failure_info`), `control_block::tests::a_block_clears_a_prior_attempts_failure_info` by name |
| Reachability (the reason reaches a TUI over the wire; a relayed dispute ends the turn; the guidance names every route) | `agent-block-reason-reaches-attention-over-the-wire`; `dispute_criteria::tests::relay_mode_dispute_ends_the_turn`, `acceptance_runner::tests::failure_guidance_names_every_route` by name |
| The behaviour the stage exists for | `crash-blocked-stage-is-not-an-agent-block` |

Attention wording follows the brief. It also shows for an operator's `loom stage block` and a
`human-review --reject`, which are Blocked with no `failure_info` too (every block writer
clears it, Choice 18).

Expected integrity events: none. `handle_one_event` (116 lines) stays net-zero; `cache.rs`
is not edited.

### Integration verification

- The rest of the canonical gate of `loom/.githooks/pre-push`: the full suite, clippy with
  warnings denied, fmt, rustdoc with warnings denied, the read-only markdown lint, and
  `scripts/flake-check.sh` with its four default filters, each its own acceptance entry
  (`cargo audit --no-fetch` ran in `completion-gates`).
- Review subagents: security (the provision executor runs snapshot commands on the host, and
  nothing a stage can write reaches that snapshot; the hook reader; `close_reason` in terminal
  output; subagents cannot block), architecture (the block lifecycle against the takedown
  invariants, the selection set against the fingerprint), and test coverage (every contract's
  mutation memory spot-checked).
- Functional smoke (wiring tests): `loom stage dispute-criteria --help` lists `--field` and
  `wiring-tests`; `loom plan verify --json` on `loom/tests/fixtures/plans/v2-environment-lints.md`
  exits 1 naming `registry.npmjs.org`, the `pre-commit hook` and `package`web`` (one needle
  per lint: the registry lint's own text says "provision" too); `loom project detect` lists no
  `tests/fixtures` package.

### Knowledge distillation

Standard, plus these rewrites to current truth:

- `concerns/runtime-and-session-safety.md#Dispute Bookkeeping Gaps Around Escalation`: both
  gaps are closed; rewrite or remove the section.
- `concerns/agent-rule-bending-hardening.md#Stage Agents Stop and Report Instead of Disputing
  or Blocking`: the doctrine is BLOCK-F and the block lifecycle is enforced; keep only what
  remains open.
- `skills/loom-plan-writer/references/v2-contracts.md`, the gate table row for impact-selected
  tests and its bullet: the selection is the stage's diff since its merge base; a runner that
  cannot start is a note.
- README: the `dispute-criteria` syntax with `--field`, `provision` in the Plan Version 2
  table (run from the `loom init` snapshot, after `before_stage`), and that `loom stage block`,
  the agent's or the operator's, retires the stage's live session and shows the reason in
  `loom status`.
- `CONTRIBUTING.md`'s line that `v2-valid.md` exercises every v2 change under
  `loom plan verify --strict`: say to verify it from a scratch git repository, because the
  repository-level lints judge the checkout the plan sits in (Choice 20).

---

<!-- loom METADATA -->

```yaml
loom:
  version: 2
  ratchet_files:
    - loom/maintainability-baseline.txt
    - doc/loom/knowledge/check-baseline.txt
  sandbox:
    enabled: true
    auto_allow: true
    filesystem:
      deny_read: ["~/.ssh/**", "~/.aws/**", "~/.config/gcloud/**", "~/.gnupg/**"]
      allow_write:
        - "loom/src/**"
        - "loom/tests/**"
        - "loom/eval/**"
        - "loom/target/**"
        - "loom/Cargo.toml"
        - "loom/Cargo.lock"
        - "loom/maintainability-baseline.txt"
        - "~/.cargo/advisory-db..lock"
        - "loom-hooks/**"
        - "CLAUDE.md.template"
        - "web/node_modules/**"
        - "doc/**"
        - "skills/**"
    network:
      allowed_domains: ["crates.io", "index.crates.io", "static.crates.io", "registry.npmjs.org"]
      allow_local_binding: false
      allow_unix_sockets: []
  stages:
    - id: completion-gates
      name: "Completion gates: impact selection, criterion disputes, contract freeze"
      summary: "Impact-selected tests run on the stage's own diff and treat a runner that cannot start as a note; fixture manifests stop counting as packages; wiring checks and wiring tests become disputable; a contract freeze refuses unformatted files; a refused dispute leaves no open request."
      stage_type: standard
      skills: ["loom-rust"]
      subagent_timeout_secs: 900
      working_dir: "loom"
      dependencies: []
      description: |
        Implement stage 1 of doc/plans/PLAN-stage-exits-and-environment.md ("completion-gates", and "Choices this plan settles" 1-7).
        Use parallel subagents and skills to maximize performance.
        Every worker first reads doc/plans/briefs/stage-exits-and-environment/common.md, then its own brief.
        Spawn every worker BY AGENT TYPE with the fixed prompt plus "Your brief: <path>. Read it in full before anything else."
        Territories are DISJOINT. Workers NEVER spawn subagents.
        Waves: G1, G2 and G4 in ONE message; then G3. The crate does not compile between the waves (G2 changes DisputeKind; G3 migrates src/orchestrator/adjudication/), so no wave-1 worker runs a check; G3 runs its one check and reports (never fixes) errors in files it does not own. After G3 the main agent runs cargo fmt --all once, routes any remaining compile error to a fresh worker for the file's owner territory, then builds and tests.

        | Worker | Role | Tier | Files owned | Shared context | Brief path |
        | ------ | ---- | ---- | ----------- | -------------- | ---------- |
        | G1 | Impact selection and fixtures | sonnet | src/verify/impact_tests.rs; src/verify/impact_tests/runs.rs; src/verify/impact_tests_tests.rs; src/verify/review/fingerprint.rs; src/commands/stage/complete_verification_v2.rs; src/skills/project/scan.rs; src/skills/project/tests.rs | common.md | doc/plans/briefs/stage-exits-and-environment/completion-gates/g1-impact-selection.md |
        | G2 | Dispute field plumbing and bookkeeping | sonnet | src/models/dispute.rs; src/models/dispute_tests.rs; src/models/stage/dispute_budgets.rs; src/cli/types_stage.rs; src/cli/types_stage_disputes.rs; src/cli/dispatch_stage.rs; src/commands/stage/dispute_criteria.rs; src/commands/stage/dispute_criteria_tests.rs; src/commands/stage/dispute_transport.rs; src/commands/stage/state/loop_recovery/mod.rs; src/commands/stage/state_tests.rs; src/daemon/protocol.rs; src/daemon/protocol_debug.rs; src/daemon/wire_tests.rs; src/daemon/server/dispute.rs; src/daemon/server/dispute_tests.rs; src/daemon/server/dispute_kinds.rs; src/daemon/server/dispute_kinds_tests.rs; src/daemon/server/stage_control.rs; src/daemon/server/self_service_tests.rs; src/daemon/server/tests/self_service_client.rs; src/fs/stage_request/types.rs; src/fs/stage_request/apply.rs; src/fs/stage_request/tests/apply.rs; src/fs/stage_request/tests/spool.rs; src/orchestrator/core/inbox_drain/apply.rs; src/orchestrator/core/inbox_drain/test_support.rs; src/orchestrator/core/verdict_apply_tests.rs; src/orchestrator/signals/adjudication.rs; src/relay/payload.rs; src/completions/dynamic/mod.rs; src/completions/dynamic/commands.rs; src/completions/dynamic/tests/tests_stage.rs; tests/adjudication_e2e.rs; maintainability-baseline.txt | common.md | doc/plans/briefs/stage-exits-and-environment/completion-gates/g2-dispute-field-plumbing.md |
        | G3 | Dispute field adjudication and gap labels | opus | src/orchestrator/adjudication/prompt.rs; src/orchestrator/adjudication/prompt/criterion.rs; src/orchestrator/adjudication/prompt/criterion_entry.rs; src/orchestrator/adjudication/prompt/tests.rs; src/orchestrator/adjudication/prompt/tests_kinds.rs; src/orchestrator/adjudication/prompt/tests_golden.rs; src/orchestrator/adjudication/verdict.rs; src/orchestrator/adjudication/verdict_tests.rs; src/orchestrator/adjudication/plan_patch.rs; src/orchestrator/adjudication/apply.rs; src/orchestrator/adjudication/tests.rs; src/orchestrator/adjudication/record_tests.rs; src/orchestrator/adjudication/mod.rs; src/orchestrator/adjudication/tests_criterion_field.rs; src/orchestrator/adjudication/closed_disputes.rs; src/orchestrator/adjudication/feedback.rs; src/verify/goal_backward/mod.rs | common.md; G2's CriterionField | doc/plans/briefs/stage-exits-and-environment/completion-gates/g3-dispute-field-adjudication.md |
        | G4 | Contract freeze formatter gate | sonnet | src/verify/contracts/format_gate.rs; src/verify/contracts/mod.rs; src/commands/stage/contracts/freeze.rs; src/orchestrator/signals/contract.rs; src/orchestrator/signals/contract_tests.rs | common.md | doc/plans/briefs/stage-exits-and-environment/completion-gates/g4-freeze-format-gate.md |

        CONTRACT SURFACE (the contract session writes tests/completion_gates_contracts.rs from this, before any code; top-level #[test] fns, so each contract's test value is its fn name):
        - loom::verify::impact_tests::stage_changes(worktree: &Path, target_branch: &str) -> anyhow::Result<Vec<PathBuf>>: worktree-relative paths changed since git merge-base HEAD <target_branch>, committed on the branch, staged, unstaged or untracked and not ignored; sorted, no duplicates. worktree is the worktree root.
        - loom::verify::impact_tests::run(stage: &Stage, working_dir: &Path, criteria_config: &loom::verify::criteria::CriteriaConfig, target_branch: &str) -> anyhow::Result<ImpactOutcome>; ImpactOutcome { ran: Vec<String>, notes: Vec<String> }. A JS package with no node_modules gives the note "`<package>` has no node_modules in this worktree (a plan `provision` entry installs it); the full suite runs in integration-verify" and runs nothing. Stage is loom::models::stage::Stage (Stage::default() works).
        - loom::skills::project::ProjectProfile::discover(dir).package_details() -> Vec<PackageDetail { path: PathBuf (relative to the checkout root), kinds, runner, skills }>. No package has a path component named `fixtures`.
        - loom::models::dispute::{CriterionField, DisputeKind, DisputeRequest, DisputeVerdict, request_file}: CriterionField::{Acceptance, Wiring, WiringTests} (serde "acceptance", "wiring", "wiring-tests"); DisputeKind::criterion(field: CriterionField, criterion_index: usize) -> DisputeKind builds DisputeKind::Criterion { criterion_index, field }. request.md is YAML frontmatter between the first two "---" lines, parsed into DisputeRequest with serde_yaml.
        - loom::daemon::handle_dispute_criteria(work_dir: &Path, stage_id: &str, field: CriterionField, criterion_index: usize, reason: String, evidence_commit: Option<String>, failure_output: Option<String>) -> anyhow::Result<loom::daemon::Response>. A filed dispute answers Response::DisputeCreated { id }; a refusal answers Response::Error { message } and writes no request.md (an out-of-range index's message contains "out of range"). Stage setup mirrors daemon/server/dispute.rs tests::setup: loom::fs::work_dir::WorkDir::new(tmp) then .initialize(), then loom::verify::transitions::save_stage(&stage, wd.root()); request.md is loom::models::dispute::request_file(&wd.root().join("disputes"), stage_id, id). Stage fields: id, name, status (loom::models::stage::StageStatus), acceptance (Vec<loom::plan::schema::AcceptanceCriterion>, Simple(String)), wiring (Vec<loom::plan::schema::WiringCheck { source, pattern, description, literal }>), wiring_tests, dispute_count; ..Stage::default().
        - loom::orchestrator::adjudication::verdict::{parse_and_validate_for, ValidationOutcome}: parse_and_validate_for(raw: &str, kind: &DisputeKind) -> ValidationOutcome; ValidationOutcome::Verdict(DisputeVerdict::Accept { plan_patch, citations, reasoning }) with plan_patch.inner a serde_json::Value holding "field"; an accept needs "reasoning" and one citation {"file","line","excerpt","claim"}; plan_patch is {"field": ..., "patch": {"op": "replace", "index": 0, "value": "<YAML>"}, "reason": "..."}.
        - loom::verify::contracts::format_gate::format_problems(acceptance: &[loom::plan::schema::AcceptanceCriterion], setup: &[String], working_dir: &Path, confinement: loom::models::stage::CommandConfinement) -> anyhow::Result<Vec<String>>: runs only the criteria that are formatter checks (every simple command the criterion expands to is one of: cargo fmt with --check, rustfmt --check, prettier --check, oxfmt --check, biome format or biome check without --write/--fix/--apply, ruff format --check, black --check, a format:check script; gofmt -l is not one, and a compound such as cargo fmt --check && cargo clippy is not run) and returns one problem per failing one, naming its command.
        - Git in tests runs with GIT_CONFIG_GLOBAL and GIT_CONFIG_SYSTEM pointed at missing files and GIT_CONFIG_NOSYSTEM=1, user.name/user.email set locally, as src/verify/impact_tests_tests.rs does.

        HAZARD (installed binary): this plan runs on the loom binary installed before it (built from main after PLAN-source-graph-mechanism), which still selects impact tests from the nearest published base layer. If loom stage complete's impact-selected step picks web/ vitest files, run cd "$(git rev-parse --show-toplevel)/web" && bun install --frozen-lockfile in this worktree (a bare cd web fails from loom/; registry.npmjs.org is allowed; the bun cache is pre-granted), then run loom stage complete again.
        EXPECTED INTEGRITY EVENTS: TI-ratchet-loom/maintainability-baseline.txt only (G2 removes the handle_dispute_criteria line). File ONE dispute-integrity for it after the final review round, reason "tightening only: handle_dispute_criteria refactored under 50 lines as the plan prescribes". No other ledger line and no existing assertion line changes; tests/adjudication_e2e.rs stays at 525 lines.
        CONTRACTS: before the final review round, prove each contract red by mutation (Gate conventions) and record loom memory note "mutation: <id> red under <mutation>".
        MEMORY: record mistakes, decisions and surprises via loom memory immediately (subagents too); never loom knowledge in this stage; never Claude Code auto-memory.
      before_stage:
        - command: "rg -q -F 'stage_changes' src/verify/impact_tests.rs"
          exit_code: 1
          description: "BEFORE: impact selection has no stage-diff selection set"
      after_stage:
        - command: "cargo test --test completion_gates_contracts impact_selection_uses_the_stage_merge_base -- --exact"
          exit_code: 0
          stdout_contains: ["1 passed"]
          description: "AFTER: impact selection uses the stage's diff since its merge base"
      acceptance:
        - "cargo build --all-targets"
        - "cargo clippy --all-targets -- -D warnings"
        - "cargo fmt --all -- --check"
        - "../scripts/flake-check.sh verdict_apply_tests::"
        - "cargo audit --no-fetch"
        - "cargo test --lib verify::impact_tests::"
        - "cargo test --lib verify::review::fingerprint"
        - "cargo test --lib verify::goal_backward::"
        - "RUSTDOCFLAGS='-D warnings' cargo doc --workspace --all-features --no-deps"
        - "cargo test --lib verify::contracts::"
        - "cargo test --lib skills::"
        - "cargo test --lib commands::project"
        - "cargo test --lib commands::hook::"
        - "cargo test --lib models::"
        - "cargo test --lib daemon::server::dispute"
        - "cargo test --lib daemon::server::self_service"
        - "cargo test --lib daemon::server::client::"
        - "cargo test --lib daemon::wire::"
        - "cargo test --lib commands::stage::"
        - "cargo test --lib fs::stage_request::"
        - "cargo test --lib orchestrator::core::inbox_drain::"
        - "cargo test --lib orchestrator::core::verdict_apply"
        - "cargo test --lib orchestrator::adjudication::"
        - "cargo test --lib orchestrator::signals::"
        - "cargo test --lib relay::"
        - "cargo test --lib completions::"
        - "cargo test --lib verify::impact_tests::tests::exit_127_is_a_note -- --exact"
        - "cargo test --lib verify::impact_tests::tests::run_selects_the_committed_stage_diff -- --exact"
        - "cargo test --lib verify::goal_backward::tests::gaps_name_their_wiring_and_wiring_tests_index -- --exact"
        - "cargo test --lib commands::stage::state::state_tests::reset_closes_open_disputes -- --exact"
        - "cargo test --lib daemon::server::dispute_kinds::tests::a_refused_file_dispute_writes_no_request -- --exact"
        - "cargo test --lib completions::dynamic::tests::tests_stage::field_values_complete_for_dispute_criteria -- --exact"
        - "cargo test --test adjudication_e2e"
        - "cargo test --test completion_gates_contracts"
        - "cargo test --test maintainability"
      files:
        - "src/verify/impact_tests.rs"
        - "src/verify/impact_tests/**"
        - "src/verify/impact_tests_tests.rs"
        - "src/verify/review/fingerprint.rs"
        - "src/verify/goal_backward/mod.rs"
        - "src/verify/contracts/mod.rs"
        - "src/verify/contracts/format_gate.rs"
        - "src/commands/stage/complete_verification_v2.rs"
        - "src/commands/stage/dispute_criteria.rs"
        - "src/commands/stage/dispute_criteria_tests.rs"
        - "src/commands/stage/dispute_transport.rs"
        - "src/commands/stage/state/**"
        - "src/commands/stage/state_tests.rs"
        - "src/commands/stage/contracts/freeze.rs"
        - "src/skills/project/**"
        - "src/models/dispute.rs"
        - "src/models/dispute_tests.rs"
        - "src/models/stage/dispute_budgets.rs"
        - "src/cli/types_stage.rs"
        - "src/cli/types_stage_disputes.rs"
        - "src/cli/dispatch_stage.rs"
        - "src/daemon/protocol.rs"
        - "src/daemon/protocol_debug.rs"
        - "src/daemon/wire_tests.rs"
        - "src/daemon/server/dispute.rs"
        - "src/daemon/server/dispute_tests.rs"
        - "src/daemon/server/dispute_kinds.rs"
        - "src/daemon/server/dispute_kinds_tests.rs"
        - "src/daemon/server/stage_control.rs"
        - "src/daemon/server/self_service_tests.rs"
        - "src/daemon/server/tests/self_service_client.rs"
        - "src/fs/stage_request/**"
        - "src/orchestrator/core/inbox_drain/**"
        - "src/orchestrator/core/verdict_apply_tests.rs"
        - "src/orchestrator/adjudication/**"
        - "src/orchestrator/signals/adjudication.rs"
        - "src/orchestrator/signals/contract.rs"
        - "src/orchestrator/signals/contract_tests.rs"
        - "src/relay/payload.rs"
        - "src/completions/dynamic/**"
        - "tests/adjudication_e2e.rs"
        - "tests/completion_gates_contracts.rs"
        - "maintainability-baseline.txt"
      artifacts:
        - "src/verify/contracts/format_gate.rs"
        - "src/orchestrator/adjudication/prompt/criterion_entry.rs"
      wiring:
        - source: "src/verify/impact_tests.rs"
          pattern: "stage_changes\\(&root, target_branch\\)"
          description: "run selects tests from the stage's diff since its merge base"
        - source: "src/verify/impact_tests.rs"
          pattern: "changes_since_merge_base\\("
          description: "stage_changes lists paths through the fingerprint's merge-base helper"
        - source: "src/verify/review/fingerprint.rs"
          pattern: "changes_since_merge_base\\("
          description: "compute_local uses the same merge-base listing as impact selection"
        - source: "src/verify/impact_tests/runs.rs"
          pattern: "missing_node_modules\\("
          description: "run_group checks a JS runner's node_modules before it runs a selection"
        - source: "src/skills/project/scan.rs"
          pattern: "\"fixtures\""
          literal: true
          description: "project detection skips fixture directories"
        - source: "src/commands/stage/contracts/freeze.rs"
          pattern: "format_problems\\("
          description: "the freeze runs the stage's formatter checks before running contracts"
        - source: "src/commands/stage/contracts/freeze.rs"
          pattern: "must pass the stage's formatter checks before"
          literal: true
          description: "the freeze refuses on the formatter gate's problems"
        - source: "src/completions/dynamic/mod.rs"
          pattern: "\"--field\" =>"
          literal: true
          description: "--field values complete for dispute-criteria"
        - source: "src/commands/stage/state/loop_recovery/mod.rs"
          pattern: "close_open_disputes\\("
          description: "loom stage reset closes the stage's open disputes"
      wiring_tests:
        - name: "dispute-criteria takes --field"
          command: "cargo run --quiet -- stage dispute-criteria --help"
          success_criteria:
            exit_code: 0
            stdout_contains: ["--field", "wiring-tests"]
        - name: "project detect lists no fixture package"
          command: "cargo run --quiet -- project detect"
          success_criteria:
            exit_code: 0
            stdout_contains: ["runner=cargo-test"]
            stdout_not_contains: ["tests/fixtures"]
      contracts:
        - id: impact-selection-uses-the-stage-merge-base
          file: tests/completion_gates_contracts.rs
          test: impact_selection_uses_the_stage_merge_base
          scenario: "in a TempDir git repo on branch main, commits A (src/lib.rs, src/other.rs, web/a.test.ts), then B editing web/a.test.ts on main; checks out branch loom/s from B, commits C editing src/lib.rs, edits src/other.rs without committing and writes an untracked src/new.rs; calls stage_changes(root, \"main\")"
          rejects: "a selection that diffs from an older base than the stage's merge base (it lists web/a.test.ts, changed by work merged before the stage branched) or from HEAD (it misses the committed src/lib.rs); expected exactly [src/lib.rs, src/new.rs, src/other.rs]"
        - id: js-runner-without-node-modules-is-a-note
          file: tests/completion_gates_contracts.rs
          test: js_runner_without_node_modules_is_a_note
          scenario: "in a TempDir git repo on main with one committed README.md, writes untracked web/package.json {\"devDependencies\":{\"vitest\":\"^3.2.0\"}} and web/src/a.test.ts (import { test } from \"vitest\"; function boom() { throw new Error(\"boom\") } test(\"x\", boom)), with no node_modules anywhere; calls run(&Stage::default(), root, &CriteriaConfig::default(), \"main\")"
          rejects: "a selection that runs npx vitest run with no node_modules, so a missing install, a refused registry or a missing npx reads as a failing test (run returns Err), instead of Ok with a note containing \"has no node_modules\" and an empty ran list"
        - id: fixture-directories-hold-no-packages
          file: tests/completion_gates_contracts.rs
          test: fixture_directories_hold_no_packages
          scenario: "in a TempDir git repo writes Cargo.toml at the root, tests/fixtures/labeled/rust/Cargo.toml, tests/fixtures/labeled/go/go.mod, fixtures/app/package.json and tests/fixtures-extra/pkg/Cargo.toml, each a minimal manifest; calls ProjectProfile::discover(root).package_details()"
          rejects: "a detection that still lists a package below a directory named fixtures, or one that matches the substring and also drops tests/fixtures-extra/pkg"
        - id: wiring-dispute-indexes-the-wiring-list
          file: tests/completion_gates_contracts.rs
          test: wiring_dispute_indexes_the_wiring_list
          scenario: "saves stage s1 (Executing, one acceptance criterion, two wiring checks, no wiring tests) and stage s2 (Executing, one acceptance criterion, no wiring tests); files handle_dispute_criteria for s1 with CriterionField::Wiring index 1, then for s2 with CriterionField::WiringTests index 0"
          rejects: "a daemon that still range-checks every field against the acceptance list (it refuses wiring index 1 on s1, or files wiring-tests index 0 on s2), or that drops the field so s1's request.md does not read back as DisputeKind::criterion(CriterionField::Wiring, 1); s2 must answer an Error containing \"out of range\" and leave no request.md"
        - id: wiring-tests-verdict-must-amend-wiring-tests
          file: tests/completion_gates_contracts.rs
          test: wiring_tests_verdict_must_amend_wiring_tests
          scenario: "for kind DisputeKind::criterion(CriterionField::WiringTests, 0), validates an accept verdict whose plan_patch is {\"field\": \"wiring-tests\", \"patch\": {\"op\": \"replace\", \"index\": 0, \"value\": \"name: t\\ncommand: \\\"true\\\"\\nsuccess_criteria:\\n  exit_code: 0\\n\"}, \"reason\": \"r\"}, then the same verdict with \"field\": \"acceptance\""
          rejects: "a validator that still admits only acceptance|wiring (the wiring-tests accept becomes needs-more-evidence), or that lets a wiring-tests dispute's verdict amend the acceptance list (the second verdict must be DisputeVerdict::NeedsMoreEvidence)"
        - id: refused-dispute-writes-no-request
          file: tests/completion_gates_contracts.rs
          test: refused_dispute_writes_no_request
          scenario: "saves stage s3 with status Completed and one acceptance criterion; calls handle_dispute_criteria(wd, \"s3\", CriterionField::Acceptance, 0, ...)"
          rejects: "a handler that writes request.md before it learns the stage cannot move to NeedsAdjudication, leaving disputes/s3/1/request.md as an open dispute with no verdict, or that counts the refused filing in dispute_count; the answer must not be DisputeCreated"
        - id: freeze-refuses-unformatted-contract-files
          file: tests/completion_gates_contracts.rs
          test: freeze_refuses_unformatted_contract_files
          scenario: "in a TempDir crate (Cargo.toml for package probe, src/lib.rs containing \"pub fn  add(a:u32,b:u32)->u32{a+b}\\n\"), calls format_problems with acceptance [Simple(\"cargo fmt --check\"), Simple(\"false\")], no setup, the crate dir and CommandConfinement::Confined"
          rejects: "a freeze that runs no formatter check (no problem) or runs every acceptance command (the non-formatter false also becomes a problem); expected exactly one problem, containing \"cargo fmt --check\""

    - id: plan-environment
      name: "Plan environment: provision and plan-time checks"
      summary: "Plans can declare provision commands the daemon runs in a stage's worktree each time the stage leaves the queue for a session, writing only files git ignores; loom plan verify errors on registry domains a stage's sandbox lacks, on a git hook the sandbox cannot serve, and on JS packages nothing provisions; the plan-writer skill gains an environment inventory."
      stage_type: standard
      skills: ["loom-rust"]
      subagent_timeout_secs: 900
      working_dir: "loom"
      dependencies: []
      description: |
        Implement stage 2 of doc/plans/PLAN-stage-exits-and-environment.md ("plan-environment", and "Choices this plan settles" 8-12, 17 and 20).
        Use parallel subagents and skills to maximize performance.
        Every worker first reads doc/plans/briefs/stage-exits-and-environment/common.md, then its own brief.
        Spawn every worker BY AGENT TYPE with the fixed prompt plus "Your brief: <path>. Read it in full before anything else."
        Territories are DISJOINT. Workers NEVER spawn subagents.
        Waves: P1 and P3 in ONE message; then P2 (its JS lint reads the provision field P1 adds); then the main agent runs cargo fmt --all once and the acceptance commands.

        | Worker | Role | Tier | Files owned | Shared context | Brief path |
        | ------ | ---- | ---- | ----------- | -------------- | ---------- |
        | P1 | Provision schema, executor and spawn gate | sonnet | src/plan/schema/types.rs; src/plan/schema/types_v2.rs; src/plan/schema/mod.rs; src/plan/schema/validation/v2_fields.rs; src/plan/schema/tests/v2_tests.rs; src/orchestrator/provision.rs; src/orchestrator/provision_tests.rs; src/orchestrator/mod.rs; src/orchestrator/core/provision_gate.rs; src/orchestrator/core/provision_gate_tests.rs; src/orchestrator/core/mod.rs; src/orchestrator/core/stage_executor.rs; src/commands/init/plan_setup.rs | common.md | doc/plans/briefs/stage-exits-and-environment/plan-environment/p1-provision.md |
        | P2 | Environment lints | sonnet | src/plan/schema/validation/v2_lints/mod.rs; src/plan/schema/validation/v2_lints/registry_domains.rs; src/plan/schema/validation/v2_lints/repo_hooks.rs; src/plan/schema/validation/v2_lints/js_provision.rs; src/plan/schema/validation/v2_lints/sandbox_capability.rs; src/plan/schema/tests/mod.rs; src/plan/schema/tests/v2_lint_environment_tests.rs; src/commands/plan/verify.rs; tests/fixtures/plans/v2-environment-lints.md; tests/fixtures/plans/v2-valid.md (header paragraph only) | common.md; P1's ProvisionEntry | doc/plans/briefs/stage-exits-and-environment/plan-environment/p2-environment-lints.md |
        | P3 | Plan-writer skill | sonnet | ../skills/loom-plan-writer/SKILL.md; ../skills/loom-plan-writer/references/sandbox.md; ../skills/loom-plan-writer/references/v2-contracts.md; ../skills/loom-plan-writer/references/authoring-detail.md; ../skills/loom-plan-writer/references/grounding-protocols.md | common.md; BLOCK-F text | doc/plans/briefs/stage-exits-and-environment/plan-environment/p3-plan-writer-skill.md |

        CONTRACT SURFACE (the contract session writes tests/plan_environment_contracts.rs from this, before any code; top-level #[test] fns):
        - loom::plan::schema::ProvisionEntry { working_dir: String, command: String } (serde, deny_unknown_fields); loom::plan::schema::LoomConfig gains provision: Vec<ProvisionEntry> (YAML key loom.provision).
        - loom::orchestrator::provision::run_provision(entries: &[ProvisionEntry], worktree: &Path) -> Result<(), String>: runs each command with sh -c in worktree.join(working_dir), in order, stopping at the first failure. Err is the stage's block reason: "provision `<command>` in `<working_dir>` failed: <detail>", where detail is the last 10 non-blank stderr lines joined by newlines (or "exit <code>" when stderr is empty). A working_dir that resolves outside the worktree (a symlink) is an Err and runs nothing.
        - loom::plan::schema::validate(&LoomMetadata) -> Result<(), Vec<ValidationError>> (ValidationError has message: String); LoomMetadata parses from YAML text with serde_yaml. A provision working_dir that is absolute or contains `..` is an error whose message contains "provision".
        - loom plan verify --json <plan>: spawn the binary ONLY through helpers::loom_cmd(), declared as #[path = "integration/helpers.rs"] #[allow(dead_code)] mod helpers; exactly as tests/map_cli.rs does, with args ["plan", "verify", "--json", "PLAN.md"] and current_dir set to the TempDir git repo that holds PLAN.md (a RELATIVE plan path). tests/integration/binary_spawn_guard.rs fails any other test file that writes Command::new(env!("CARGO_BIN_EXE_loom")), and an unscrubbed spawn inherits the running stage's LOOM_* identity and the real ~/.loom update state; stdout is one JSON object whose "errors" is an array of {"stage_id": string|null, "message": string}; the exit status is 1 when errors is non-empty. For the plan text, copy the YAML of tests/fixtures/plans/v2-valid.md and add a summary: line to every stage (the fixture has none), wrapped in the <!-- loom METADATA --> markers and a yaml code fence as that file is. A sandbox block goes under loom: as sandbox: { network: { allowed_domains: [...] } }; provision goes under loom: as provision: [{ working_dir: web, command: "bun install --frozen-lockfile" }].
        - Lint message fragments the contracts may match: a JS package lint names the package in backticks and the word "provision" (for web: "`web`"); a registry lint names the missing domain ("registry.npmjs.org"); a hook lint names "pre-commit hook" and the missing domain. The registry lint's message also contains the word "provision", so a check for the JS lint matches "package `web`", never "provision" alone.
        - Git in tests, and the loom binary the tests run, get GIT_CONFIG_GLOBAL and GIT_CONFIG_SYSTEM pointed at missing files and GIT_CONFIG_NOSYSTEM=1, as src/verify/impact_tests_tests.rs does, so a user's global core.hooksPath never leaks in; a repo-local core.hooksPath is set with git config inside the TempDir repo. The hook lint reads core.hooksPath with scoped git config reads (--local, --global, --system), never rev-parse --git-path hooks: loom's git runner forces -c core.hooksPath=/dev/null on every command.
        - loom::orchestrator::provision::{persist_plan_snapshots, write_provision_snapshot, read_provision_snapshot}: loom init persists the plan's provision entries into the work directory's config.toml ([plan_provision]); the spawn gate reads only that snapshot, never the plan file.

        HAZARD (installed binary): this plan runs on the loom binary installed before it (built from main after PLAN-source-graph-mechanism), which still selects impact tests from the nearest published base layer and rejects a plan-level provision key. If loom stage complete's impact-selected step picks web/ vitest files, run cd "$(git rev-parse --show-toplevel)/web" && bun install --frozen-lockfile in this worktree (a bare cd web fails from loom/; registry.npmjs.org is allowed; the bun cache is pre-granted), then run loom stage complete again.
        EXPECTED INTEGRITY EVENTS: none. Never edit loom/maintainability-baseline.txt in this stage: plan/schema/types.rs (416 lines), stage_executor.rs (597; start_stage 149, before_stage_gate_passed 62), commands/plan/verify.rs (555) and commands/init/plan_setup.rs initialize_with_plan (206) stay net-zero, fs/work_dir.rs (433) is not edited, and a new file or function stays under its limit.
        CONTRACTS: before the final review round, prove each contract red by mutation (Gate conventions) and record loom memory note "mutation: <id> red under <mutation>".
        MEMORY: record mistakes, decisions and surprises via loom memory immediately (subagents too); never loom knowledge in this stage; never Claude Code auto-memory.
      before_stage:
        - command: "rg -q -F 'provision' src/plan/schema/types.rs"
          exit_code: 1
          description: "BEFORE: the plan schema has no provision field"
      after_stage:
        - command: "cargo test --test plan_environment_contracts provision_runs_in_its_working_dir -- --exact"
          exit_code: 0
          stdout_contains: ["1 passed"]
          description: "AFTER: a provision entry runs in its working directory"
      acceptance:
        - "cargo build --all-targets"
        - "cargo clippy --all-targets -- -D warnings"
        - "cargo fmt --all -- --check"
        - "cd .. && git ls-files '*.md' | rg -v '^(doc/plans/|loom/tests/fixtures/)' | xargs bunx markdownlint-cli2"
        - "cargo test --test integration binary_spawn_guard::only_sanctioned_spawners_spawn_the_loom_binary_directly -- --exact"
        - "cargo test --lib plan::schema::"
        - "cargo test --lib orchestrator::provision::"
        - "cargo test --lib orchestrator::core::provision_gate"
        - "cargo test --lib orchestrator::core::stage_executor"
        - "RUSTDOCFLAGS='-D warnings' cargo doc --workspace --all-features --no-deps"
        - "cargo test --lib commands::plan::"
        - "cargo test --lib commands::init::"
        - "cargo test --lib orchestrator::signals::tests_doctrine"
        - "cargo test --lib orchestrator::core::provision_gate::tests::a_failing_provision_blocks_the_stage_with_its_reason -- --exact"
        - "cargo test --lib orchestrator::core::provision_gate::tests::a_failing_before_stage_check_blocks_before_provisioning -- --exact"
        - "cargo test --lib orchestrator::core::provision_gate::tests::the_gate_reads_the_snapshot_not_the_plan -- --exact"
        - "cargo test --lib orchestrator::core::provision_gate::tests::a_provision_that_leaves_an_unignored_file_blocks_the_stage -- --exact"
        - "cargo test --lib orchestrator::core::provision_gate::tests::a_provision_that_writes_only_ignored_files_spawns -- --exact"
        - "cargo test --test plan_environment_contracts"
        - "cargo test --test integration plan_verify"
        - "cargo test --test maintainability"
        - 'rg -q -F "loom.provision" ../skills/loom-plan-writer/SKILL.md'
        - 'rg -q -F "loom stage block" ../skills/loom-plan-writer/SKILL.md'
        - 'rg -q -F "provision" ../skills/loom-plan-writer/references/sandbox.md'
        - command: "rg -q -F 'run `loom repair`, merge with suggestions' ../skills/loom-plan-writer/SKILL.md"
          exit_code: 1
        - command: "rg -q -F -e 'report a needed sandbox block as a blocker' -e 'STOP and report it as a blocker' -e 'ends your turn and sends the stage to adjudication; loom retires' ../skills/loom-plan-writer"
          exit_code: 1
      files:
        - "src/plan/schema/types.rs"
        - "src/plan/schema/types_v2.rs"
        - "src/plan/schema/mod.rs"
        - "src/plan/schema/validation/v2_fields.rs"
        - "src/plan/schema/validation/v2_lints/**"
        - "src/plan/schema/tests/**"
        - "src/orchestrator/provision.rs"
        - "src/orchestrator/provision_tests.rs"
        - "src/orchestrator/mod.rs"
        - "src/orchestrator/core/provision_gate.rs"
        - "src/orchestrator/core/provision_gate_tests.rs"
        - "src/orchestrator/core/mod.rs"
        - "src/orchestrator/core/stage_executor.rs"
        - "src/commands/plan/verify.rs"
        - "src/commands/init/plan_setup.rs"
        - "tests/fixtures/plans/v2-environment-lints.md"
        - "tests/fixtures/plans/v2-valid.md"
        - "tests/plan_environment_contracts.rs"
        - "../skills/loom-plan-writer/**"
      artifacts:
        - "src/orchestrator/provision.rs"
        - "src/orchestrator/core/provision_gate.rs"
        - "src/plan/schema/validation/v2_lints/registry_domains.rs"
        - "src/plan/schema/validation/v2_lints/repo_hooks.rs"
        - "src/plan/schema/validation/v2_lints/js_provision.rs"
        - "tests/fixtures/plans/v2-environment-lints.md"
      wiring:
        - source: "src/orchestrator/core/stage_executor.rs"
          pattern: "if !self\\.pre_spawn_gates_passed\\("
          description: "start_stage gates the spawn on the before-stage checks and provisioning"
        - source: "src/orchestrator/core/provision_gate.rs"
          pattern: "self\\.before_stage_gate_passed\\("
          description: "the spawn gate still runs the before-stage checks"
        - source: "src/orchestrator/core/provision_gate.rs"
          pattern: "read_provision_snapshot\\("
          description: "the spawn gate reads the init-time snapshot, not the plan file"
        - source: "src/orchestrator/core/provision_gate.rs"
          pattern: "run_provision\\("
          description: "the spawn gate runs the snapshot's provision entries"
        - source: "src/orchestrator/core/provision_gate.rs"
          pattern: "may write only files git ignores"
          literal: true
          description: "the spawn gate refuses a provision that leaves a file git does not ignore"
        - source: "src/orchestrator/provision.rs"
          pattern: "PROVISION_TIMEOUT"
          description: "run_provision passes the 600 s timeout to the command runner"
        - source: "src/orchestrator/provision.rs"
          pattern: "Duration::from_secs(600)"
          literal: true
          description: "one provision command may run 600 s"
        - source: "src/commands/init/plan_setup.rs"
          pattern: "persist_plan_snapshots\\("
          description: "loom init snapshots the plan's provision entries"
        - source: "src/plan/schema/validation/v2_lints/mod.rs"
          pattern: "registry_domains::check\\("
          description: "plan verify runs the registry-domain lint"
        - source: "src/plan/schema/validation/v2_lints/mod.rs"
          pattern: "repo_hooks::check\\("
          description: "plan verify runs the repository-hook lint"
        - source: "src/plan/schema/validation/v2_lints/mod.rs"
          pattern: "js_provision::check\\("
          description: "plan verify runs the JS provision lint"
      wiring_tests:
        - name: "plan verify reports the environment lints"
          command: "cargo run --quiet -- plan verify --json tests/fixtures/plans/v2-environment-lints.md"
          success_criteria:
            exit_code: 1
            stdout_contains: ["registry.npmjs.org", "pre-commit hook", "package `web`"]
      contracts:
        - id: provision-runs-in-its-working-dir
          file: tests/plan_environment_contracts.rs
          test: provision_runs_in_its_working_dir
          scenario: "creates a TempDir worktree with a web/ directory and calls run_provision(&[ProvisionEntry { working_dir: \"web\", command: \"pwd > provisioned.txt\" }], worktree)"
          rejects: "an executor that runs the command in the worktree root or the daemon's own cwd; expected Ok and worktree/web/provisioned.txt holding the canonical path of worktree/web"
        - id: failed-provision-names-command-and-dir
          file: tests/plan_environment_contracts.rs
          test: failed_provision_names_command_and_dir
          scenario: "creates a TempDir worktree with web/ and calls run_provision with [{web, \"echo registry refused >&2; exit 7\"}, {web, \"touch ran-after\"}]"
          rejects: "an executor that ignores the exit status, drops stderr from the reason, or keeps running later entries; expected Err starting \"provision `echo registry refused >&2; exit 7` in `web` failed: \" and containing \"registry refused\", and no web/ran-after"
        - id: provision-refuses-a-symlinked-escape
          file: tests/plan_environment_contracts.rs
          test: provision_refuses_a_symlinked_escape
          scenario: "creates a TempDir worktree whose web is a symlink to a second TempDir outside it, and calls run_provision with [{web, \"touch escaped\"}]"
          rejects: "an executor that follows the symlinked working_dir out of the worktree and runs the host command there; expected Err and no escaped file in the outside directory"
        - id: provision-working-dir-cannot-escape
          file: tests/plan_environment_contracts.rs
          test: provision_working_dir_cannot_escape
          scenario: "parses a version 2 plan YAML (one standard stage with one contract, as src/plan/schema/tests/mod.rs create_valid_metadata_v2 builds) whose loom.provision is [{working_dir: \"../elsewhere\", command: \"true\"}], calls validate; then the same with working_dir \"/abs/web\""
          rejects: "validation that accepts a working_dir with a .. component or an absolute path, which would run a host command outside the worktree; each case must give an error whose message contains \"provision\""
        - id: js-package-without-provision-is-an-error
          file: tests/plan_environment_contracts.rs
          test: js_package_without_provision_is_an_error
          scenario: "in a TempDir git repo commits web/package.json {\"devDependencies\":{\"vitest\":\"^3.2.0\"}} and PLAN.md (the v2-valid plan, no provision), runs loom plan verify --json PLAN.md from the repo; then rewrites PLAN.md with provision [{working_dir: web, command: \"bun install --frozen-lockfile\"}] and runs it again"
          rejects: "a lint never registered in loom plan verify, one that reads the repository through the empty root a relative plan path gives (it reports nothing), or one that still errors when an entry covers web; the first run must list an error containing \"`web`\" and \"provision\", the second none"
        - id: registry-tool-without-registry-domain-is-an-error
          file: tests/plan_environment_contracts.rs
          test: registry_tool_without_registry_domain_is_an_error
          scenario: "in a TempDir git repo, PLAN.md is the v2-valid plan with \"bunx tsc --noEmit\" added to the standard stage's acceptance and sandbox network allowed_domains [crates.io]; runs loom plan verify --json PLAN.md; then with allowed_domains [crates.io, \"*.npmjs.org\"]"
          rejects: "a lint that only knows the old curl/wget/gh/install list (bunx passes silently) or that ignores wildcard patterns; the first run must list an error containing \"registry.npmjs.org\", the second none"
        - id: pre-commit-hook-registry-need-is-an-error
          file: tests/plan_environment_contracts.rs
          test: pre_commit_hook_registry_need_is_an_error
          scenario: "in a TempDir git repo runs git config core.hooksPath hooks and writes an executable hooks/pre-commit containing the line git ls-files -z -- '*.md' | xargs -0 bunx markdownlint-cli2 --fix || true; PLAN.md is the v2-valid plan with sandbox network allowed_domains [crates.io]; runs loom plan verify --json PLAN.md"
          rejects: "a lint that reads only .git/hooks and ignores core.hooksPath, or that does not see bunx behind xargs; expected an error whose message contains \"pre-commit\" and \"registry.npmjs.org\""

    - id: stage-exits
      name: "Stage exits: block lifecycle, attention and doctrine"
      summary: "A stage agent's loom stage block retires its session without a crash, loom status shows the block reason, and every doctrine surface tells a stage to dispute a wrong check, block for a need only a person can meet, and never stop to ask the operator."
      stage_type: standard
      skills: ["loom-rust"]
      subagent_timeout_secs: 900
      working_dir: "loom"
      dependencies: ["completion-gates"]
      description: |
        Implement stage 3 of doc/plans/PLAN-stage-exits-and-environment.md ("stage-exits", and "Choices this plan settles" 13-15, 18, 19 and 21).
        Use parallel subagents and skills to maximize performance.
        Every worker first reads doc/plans/briefs/stage-exits-and-environment/common.md, then its own brief.
        Spawn every worker BY AGENT TYPE with the fixed prompt plus "Your brief: <path>. Read it in full before anything else."
        Territories are DISJOINT. Workers NEVER spawn subagents. S1, S2 and S3 go in ONE message; then the main agent runs cargo fmt --all once and the acceptance commands. The three share one crate mid-wave: a worker's check can fail on another's unfinished file, and each worker reports such errors instead of fixing them.

        | Worker | Role | Tier | Files owned | Shared context | Brief path |
        | ------ | ---- | ---- | ----------- | -------------- | ---------- |
        | S1 | Block lifecycle | opus | src/orchestrator/core/event_handler.rs; src/orchestrator/core/event_handler/stage_takedown.rs; src/orchestrator/core/event_handler/blocked.rs; src/orchestrator/core/event_handler/verdict_retirement_tests.rs; src/orchestrator/monitor/session_events.rs; src/orchestrator/monitor/session_events/tests.rs; src/daemon/server/control_block.rs; src/commands/stage/state_relay.rs; src/commands/stage/human_review.rs | common.md | doc/plans/briefs/stage-exits-and-environment/stage-exits/s1-block-lifecycle.md |
        | S2 | Block reason in status | sonnet | src/commands/status/data/mod.rs; src/commands/status/data/collector.rs; src/commands/status/data/sanitize.rs; src/commands/status/web/model.rs; src/commands/status/web/model_tests_stages.rs; src/commands/status/render/attention_model.rs; src/commands/status/render/attention_model_guidance_tests.rs; src/commands/status/render/attention_model_tests.rs; src/commands/status/render/attention_tests.rs; src/commands/status/render/graph_tests.rs; src/commands/status/ui/tui/ledger/tests.rs; src/commands/status/ui/tui/ledger/rows.rs; src/commands/status/ui/tui/state_tests.rs; src/daemon/wire_tests.rs | common.md | doc/plans/briefs/stage-exits-and-environment/stage-exits/s2-block-attention.md |
        | S3 | BLOCK-F and agreeing surfaces | sonnet | src/orchestrator/signals/helpers.rs; src/orchestrator/signals/tests_doctrine_v2.rs; src/orchestrator/signals/tests_doctrine_blocks.rs; src/orchestrator/signals/tests_size.rs; src/orchestrator/signals/v2_section_review.rs; src/orchestrator/signals/format/sandbox_section.rs; src/orchestrator/signals/format/helpers.rs; src/commands/stage/acceptance_runner.rs; src/commands/stage/complete_verification.rs; src/commands/stage/dispute_transport.rs; src/commands/stage/dispute_criteria_tests.rs; ../CLAUDE.md.template; ../loom-hooks/commit-guard.sh; ../loom-hooks/_subagent-preamble.txt; ../skills/loom-orchestration/SKILL.md; ../skills/loom-usage/SKILL.md | common.md; BLOCK-F text | doc/plans/briefs/stage-exits-and-environment/stage-exits/s3-stage-exit-doctrine.md |

        BLOCK-F, verbatim (S3 places it; the plan's prose and every surface use these exact bytes):
        **When the stage cannot finish.** Fix what the stage can fix. A criterion, wiring check, contract, review finding or test-integrity event that is wrong gets a dispute: `loom stage dispute-criteria` (with `--field` for wiring and wiring-tests entries), `dispute-contract`, `dispute-findings` or `dispute-integrity`. Filing ends this session by design: the daemon starts a fresh session with the verdict, so waiting gains nothing. Never revert, weaken or postpone correct work to avoid a dispute. A need only a person can meet (a credential, a host install, a network domain or path the plan does not grant) gets `loom stage block <stage-id> "<what is needed and why>"`: the daemon retires the session and shows the reason to the operator. Commit your work before filing either. Never end a turn asking the operator to act while the stage is executing.

        CONTRACT SURFACE (the contract session writes tests/stage_exits_contracts.rs from this, before any code; top-level #[test] fns):
        - loom::commands::status::data::StageSummary (every field pub; build it as src/commands/status/render/attention_model_tests.rs make_stage_summary does) gains close_reason: Option<String>, serialized under the key "close_reason" when Some.
        - loom::commands::status::render::attention_entries(&[StageSummary]) -> Vec<AttentionEntry { id, label, command: Option<String>, note: Option<String>, automatic: bool, .. }>.
        - A Blocked stage with failure_info None and close_reason Some(r) has one entry: label "BLOCKED", note Some("blocked: " + r) (the same note whether the stage agent, the operator's loom stage block or human-review --reject set the reason), command Some("loom stage retry <id>") while retry_count < max_retries (default 3), automatic false.
        - loom::models::failure::{FailureInfo { failure_type, detected_at, evidence }, FailureType::SessionCrash}.

        HAZARD (installed binary): this plan runs on the loom binary installed before it (built from main after PLAN-source-graph-mechanism), which still selects impact tests from the nearest published base layer. This stage branches after completion-gates merges, so that base can predate its branch point by a whole stage's work, and completion-gates' fix is not in the running binary. If loom stage complete's impact-selected step picks web/ vitest files, run cd "$(git rev-parse --show-toplevel)/web" && bun install --frozen-lockfile in this worktree (a bare cd web fails from loom/; registry.npmjs.org is allowed; the bun cache is pre-granted), then run loom stage complete again.
        EXPECTED INTEGRITY EVENTS: none. handle_one_event stays at 116 lines; cache.rs is not edited; no existing assertion line in a test file changes (the sandbox_section.rs assertion S3 rewrites sits in a production file's inline test module, which the integrity profile does not count; so do S1's new control_block.rs test and S3's format/helpers.rs and acceptance_runner.rs tests).
        CONTRACTS: before the final review round, prove each contract red by mutation (Gate conventions) and record loom memory note "mutation: <id> red under <mutation>".
        MEMORY: record mistakes, decisions and surprises via loom memory immediately (subagents too); never loom knowledge in this stage; never Claude Code auto-memory.
      before_stage:
        - command: "rg -q -F 'close_reason' src/commands/status/data/mod.rs"
          exit_code: 1
          description: "BEFORE: status summaries do not carry the block reason"
      after_stage:
        - command: "cargo test --test stage_exits_contracts agent_block_reason_reaches_attention_over_the_wire -- --exact"
          exit_code: 0
          stdout_contains: ["1 passed"]
          description: "AFTER: an agent block's reason reaches the attention note over the wire"
      acceptance:
        - "cargo build --all-targets"
        - "cargo clippy --all-targets -- -D warnings"
        - "cargo fmt --all -- --check"
        - "RUSTDOCFLAGS='-D warnings' cargo doc --workspace --all-features --no-deps"
        - "cd .. && git ls-files '*.md' | rg -v '^(doc/plans/|loom/tests/fixtures/)' | xargs bunx markdownlint-cli2"
        - "../scripts/flake-check.sh verdict_retirement_tests::"
        - "cargo test --lib orchestrator::signals::"
        - "cargo test --lib orchestrator::core::event_handler::"
        - "cargo test --lib orchestrator::monitor::"
        - "cargo test --lib commands::status::"
        - "cargo test --lib commands::stage::"
        - "cargo test --lib daemon::wire::"
        - "cargo test --lib fs::permissions::"
        - "cargo test --lib daemon::server::control_block"
        - "cargo test --lib orchestrator::monitor::session_events::tests::an_agent_blocked_stage_session_exit_is_not_a_crash -- --exact"
        - "cargo test --lib orchestrator::core::event_handler::verdict_retirement_tests::an_agent_block_retires_the_live_session -- --exact"
        - "cargo test --lib orchestrator::signals::tests_doctrine_v2::block_f_agrees_across_every_surface -- --exact"
        - "cargo test --lib daemon::server::control_block::tests::a_block_clears_a_prior_attempts_failure_info -- --exact"
        - "cargo test --lib commands::stage::dispute_criteria::tests::relay_mode_dispute_ends_the_turn -- --exact"
        - "cargo test --lib commands::stage::acceptance_runner::tests::failure_guidance_names_every_route -- --exact"
        - "cargo test --lib commands::status::data::sanitize::tests::a_control_sequence_in_the_close_reason_is_flattened -- --exact"
        - "cargo test --test stage_exits_contracts"
        - "cargo test --test maintainability"
        - "bash ../scripts/check-hook-syntax.sh"
        - "bash ../loom-hooks/tests/commit-guard-contract-session.sh"
        - "bash ../loom-hooks/tests/commit-guard-nested-worktree.sh"
        - "bash ../loom-hooks/tests/commit-guard-sigpipe-many-dirty-files.sh"
        - 'rg -q -F "loom stage block" ../CLAUDE.md.template'
        - 'rg -q -F "or a dispute or block is filed" ../loom-hooks/commit-guard.sh'
        - 'rg -q -F -- "--field wiring" ../skills/loom-usage/SKILL.md'
        - 'rg -q -F "loom stage block" ../loom-hooks/_subagent-preamble.txt'
        - command: "rg -q -F 'dispute-criteria <stage-id> \"criteria X' ../skills/loom-usage/SKILL.md"
          exit_code: 1
      files:
        - "src/orchestrator/core/event_handler.rs"
        - "src/orchestrator/core/event_handler/**"
        - "src/orchestrator/monitor/session_events.rs"
        - "src/orchestrator/monitor/session_events/**"
        - "src/commands/status/**"
        - "src/daemon/wire_tests.rs"
        - "src/orchestrator/signals/**"
        - "src/commands/stage/acceptance_runner.rs"
        - "src/commands/stage/complete_verification.rs"
        - "src/commands/stage/dispute_transport.rs"
        - "src/commands/stage/dispute_criteria_tests.rs"
        - "src/commands/stage/state_relay.rs"
        - "src/commands/stage/human_review.rs"
        - "src/daemon/server/control_block.rs"
        - "tests/stage_exits_contracts.rs"
        - "../CLAUDE.md.template"
        - "../loom-hooks/commit-guard.sh"
        - "../loom-hooks/_subagent-preamble.txt"
        - "../skills/loom-orchestration/SKILL.md"
        - "../skills/loom-usage/SKILL.md"
      artifacts:
        - "src/orchestrator/core/event_handler/blocked.rs"
      wiring:
        - source: "src/orchestrator/core/event_handler.rs"
          pattern: "on_stage_blocked\\("
          description: "the StageBlocked event retires an agent-blocked stage's session"
        - source: "src/orchestrator/signals/helpers.rs"
          pattern: "STAGE_EXIT_RULES"
          description: "the completion rules emit BLOCK-F"
        - source: "src/commands/status/web/model.rs"
          pattern: "close_reason = None"
          literal: true
          description: "the browser snapshot drops close_reason, which the strict web schema does not know"
        - source: "src/commands/stage/complete_verification.rs"
          pattern: "print_acceptance_failure_guidance\\("
          description: "a goal-backward failure prints the dispute and block guidance"
      contracts:
        - id: agent-block-reason-reaches-attention-over-the-wire
          file: tests/stage_exits_contracts.rs
          test: agent_block_reason_reaches_attention_over_the_wire
          scenario: "builds a StageSummary for stage s1 with status Blocked, failure_info None, retry_count 0, max_retries None and close_reason Some(\"needs registry.npmjs.org for bunx vitest\"), serializes it with serde_json and deserializes it back (what a TUI does with a daemon frame), then calls attention_entries on the result"
          rejects: "a close_reason that is not serialized, so the deserialized frame loses it, or an attention rule that still shows only loom stage retry with no note; expected one entry with label BLOCKED, note Some(\"blocked: needs registry.npmjs.org for bunx vitest\"), command Some(\"loom stage retry s1\") and automatic false"
        - id: crash-blocked-stage-is-not-an-agent-block
          file: tests/stage_exits_contracts.rs
          test: crash_blocked_stage_is_not_an_agent_block
          scenario: "builds a StageSummary for stage s2 with status Blocked, failure_info Some(FailureInfo { failure_type: SessionCrash, evidence: [\"boom\"] }), retry_count 0 and close_reason Some(\"Session crashed\"), round-trips it through serde_json and calls attention_entries"
          rejects: "an attention rule that labels every Blocked stage with a close_reason as an agent block; expected automatic true and a note that does not start with \"blocked: \""

    - id: integration-verify
      name: "Integration verification"
      summary: "Confirms the new dispute field, block lifecycle, provisioning and plan-time lints are wired into the running program, and that the full suite, lints and docs build pass."
      stage_type: integration-verify
      skills: ["loom-rust", "loom-security-audit"]
      working_dir: "."
      dependencies: ["completion-gates", "plan-environment", "stage-exits"]
      description: |
        Final verification of doc/plans/PLAN-stage-exits-and-environment.md. Verify FUNCTIONAL INTEGRATION, not just tests passing. NEVER Claude Code auto-memory.
        Use parallel subagents and skills to maximize performance.
        CONTEXT: read the plan, loom memory show --all, and the knowledge sections the brief quotes.
        HAZARD (installed binary): this stage runs on the loom binary installed before the plan. Integration-verify runs no impact-selected tests, and the full Rust suite needs no web/node_modules (web/dist is tracked and embedded), so no install step is needed here.
        BUILD AND TEST (zero tolerance; fix every warning and failure through an engineer subagent, sonnet or opus by the rubric): the canonical gate of loom/.githooks/pre-push: the full suite, clippy with warnings denied, fmt, rustdoc with warnings denied, the markdown lint (read-only; fix what it reports, bunx markdownlint-cli2 --fix repairs most of it), and scripts/flake-check.sh with its four default filters (cargo audit --no-fetch runs in completion-gates, whose working_dir holds Cargo.toml; no stage changes Cargo.lock).
        CODE REVIEW: spawn parallel loom-code-reviewer subagents, each recording a loom-review block:
        - security: loom/src/orchestrator/provision.rs runs plan-authored commands on the host (confinement, working_dir canonicalisation, symlink refusal, the reason's size, and that the spawn gate reads only the loom init snapshot in the work directory, never the plan file a stage can edit); the pre-commit hook reader (bounded read, the scoped git config reads); close_reason flattening before terminal output; the dispute field and index from the wire; that no subagent surface tells a subagent to run loom stage block or a dispute.
        - architecture: the block retirement against the takedown invariants (no second writer, session cleared only while Blocked, adjudication and merge-resolution sessions untouched, every block writer clearing failure_info); the gate order (before-stage checks, then provisioning) and the tick stamped while provisioning; the impact selection set against the fingerprint helper both now share; the lints' repository root.
        - test coverage: every contract, and two stages' "mutation: <id> red" memories spot-checked by re-applying a rejects implementation and confirming the contract fails.
        Fix or dispute every finding; never defer one.
        SUGGESTIONS: weigh every pending reviewer suggestion the signal lists; resolve each implemented one with loom memory resolve <id> --outcome implemented --reason <what changed>.
        FUNCTIONAL: every stage's wiring checks re-run on the merged tree; the wiring tests below drive the built binary: dispute-criteria --help lists --field, plan verify reports all three environment lints on the fixture plan, and project detect lists no fixture package.
        Run every acceptance command once in-session before loom stage complete (each has a 300 s cap).
        Record discoveries to loom memory for knowledge-distill, including every knowledge file the tree now contradicts: loom memory note "stale-knowledge: <file>#<heading> claims X; the tree does Y".
      acceptance:
        - "cargo build --manifest-path loom/Cargo.toml --all-targets"
        - "cargo test --manifest-path loom/Cargo.toml --all-targets --no-fail-fast"
        - "cargo clippy --manifest-path loom/Cargo.toml --all-targets -- -D warnings"
        - "cargo fmt --manifest-path loom/Cargo.toml --all -- --check"
        - "RUSTDOCFLAGS='-D warnings' cargo doc --manifest-path loom/Cargo.toml --workspace --all-features --no-deps"
        - "git ls-files '*.md' | rg -v '^(doc/plans/|loom/tests/fixtures/)' | xargs bunx markdownlint-cli2"
        - "scripts/flake-check.sh quota::"
        - "scripts/flake-check.sh process::"
        - "scripts/flake-check.sh verdict_apply_tests::"
        - "scripts/flake-check.sh stalled_judge_tests::"
      wiring_tests:
        - name: "dispute-criteria lists --field"
          command: "cargo run --manifest-path loom/Cargo.toml --quiet -- stage dispute-criteria --help"
          success_criteria:
            exit_code: 0
            stdout_contains: ["--field", "wiring-tests"]
        - name: "plan verify reports the environment lints"
          command: "cargo run --manifest-path loom/Cargo.toml --quiet -- plan verify --json loom/tests/fixtures/plans/v2-environment-lints.md"
          success_criteria:
            exit_code: 1
            stdout_contains: ["registry.npmjs.org", "pre-commit hook", "package `web`"]
        - name: "project detect lists no fixture package"
          command: "cargo run --manifest-path loom/Cargo.toml --quiet -- project detect"
          success_criteria:
            exit_code: 0
            stdout_contains: ["runner=cargo-test"]
            stdout_not_contains: ["tests/fixtures"]
      files:
        - "loom/**"
        - "loom-hooks/**"
        - "skills/**"
        - "CLAUDE.md.template"
        - "doc/**"

    - id: knowledge-distill
      name: "Distill knowledge"
      summary: "Records what this plan changed and learned in the knowledge base (stage exits, the block lifecycle, disputable wiring, provisioning, plan-time environment checks) and updates the README's dispute, block and plan-format passages."
      stage_type: knowledge-distill
      working_dir: "."
      dependencies: ["integration-verify"]
      description: |
        Curate all stage memories into permanent knowledge, and update user docs. NEVER Claude Code auto-memory.
        SINGLE-AGENT: do NOT spawn subagents. Memories are compact summaries: lean on them and keep code spot-reads narrow.
        START with loom memory pending --group (corrections, mistakes, decisions, other, suggestions). Read this plan and the knowledge sections it touches.
        CORRECTIONS FIRST: apply every stale-knowledge: memory in place with loom knowledge replace-section <file> "<heading>" "<body>", never with loom knowledge update. Read each command's output: a non-matching heading appends and says so.
        CURRENT TRUTH: rewrite these to the merged tree:
        - concerns/runtime-and-session-safety.md#Dispute Bookkeeping Gaps Around Escalation: a filing checks the transition before request.md is written and a reset closes open disputes; rewrite the section to what stays open, or remove it if nothing does.
        - concerns/agent-rule-bending-hardening.md#Stage Agents Stop and Report Instead of Disputing or Blocking: BLOCK-F is the doctrine, a stage agent's block retires its session, and plan verify checks the environment; keep only what remains open.
        - architecture/adjudication-lifecycle.md ("Dispute Kinds: Criterion, Findings, Contract, Integrity") and entry-points/orchestrator-daemon-and-sessions.md (the Request::DisputeCriteria shape): DisputeKind::Criterion and the request carry field; the verdict must amend that field (an acceptance dispute's may still amend wiring).
        - the section on impact-selected tests wherever knowledge describes them: the selection set is the stage's diff since its merge base; a JS runner without node_modules and exit 127 are notes.
        Tier-route by size; INDEX.md regenerates on every write.
        SKILL TABLE: in skills/loom-plan-writer/references/v2-contracts.md Section 5, rewrite the gate-table row and the bullet for impact-selected tests to the same truth.
        MISTAKES: every mistake memory becomes a prevention rule in the right mistakes topic.
        README: in the Stage Commands block, the dispute-criteria line becomes loom stage dispute-criteria <stage-id> [--field acceptance|wiring|wiring-tests] --criterion-index N --reason <text> [--evidence-commit <sha>] [--failure-output <path>]; add a provision row to the Plan Version 2 table (loom init snapshots the entries; the daemon runs them on the host in each worktree after the before-stage checks), and say in the Disputes section that loom stage block, the agent's or the operator's, retires the stage's live session and shows the reason in loom status.
        CONTRIBUTING: the line saying a plan-version-2 change is exercised by loom/tests/fixtures/plans/v2-valid.md under loom plan verify --strict becomes: verify it from a scratch git repository holding only that file (use the words "scratch git repository"), because the repository-level lints (the pre-commit hook, JS packages without provision) judge the checkout the plan sits in. Change nothing else in CONTRIBUTING unless a memory says it changed.
        SUGGESTIONS: record every unimplemented reviewer suggestion in concerns or its topic, then resolve it promoted, merged or discarded.
        RECEIPTS: every entry taken into knowledge gets loom memory resolve <id> --outcome promoted|merged|discarded|deferred right after the write that used it. Finish with loom memory pending --strict.
        MARKDOWN: acceptance runs the pre-push hook's markdown lint read-only; run bunx markdownlint-cli2 --fix on the Markdown files you changed first, and fix by hand what it cannot.
        LAST, if this stage removed structural issues: loom knowledge check --write-baseline doc/loom/knowledge/check-baseline.txt
      acceptance:
        - "loom knowledge check --strict --baseline doc/loom/knowledge/check-baseline.txt"
        - "loom memory pending --strict"
        - "git ls-files '*.md' | rg -v '^(doc/plans/|loom/tests/fixtures/)' | xargs bunx markdownlint-cli2"
        - "rg -q -F 'dispute-criteria <stage-id> [--field acceptance|wiring|wiring-tests]' README.md"
        - "rg -q -F '`provision`' README.md"
        - "rg -q -F 'retires' README.md"
        - "rg -q -F 'node_modules' skills/loom-plan-writer/references/v2-contracts.md"
        - "rg -q -F 'scratch git repository' CONTRIBUTING.md"
        - command: "rg -q -F 'Refused filings leave an open dispute' doc/loom/knowledge/concerns/runtime-and-session-safety.md"
          exit_code: 1
      files:
        - "doc/loom/knowledge/**"
        - "README.md"
        - "CONTRIBUTING.md"
        - "skills/loom-plan-writer/references/v2-contracts.md"
```

<!-- END loom METADATA -->
