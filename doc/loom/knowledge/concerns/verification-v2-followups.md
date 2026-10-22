# Verification V2 Followups

> v2 adapters, parsers, known gaps

## Nine Adapters Have Fixtures From Documented Formats, Not Captured Runs

`cargo-nextest`, `gradle`, `maven`, `sbt`, `rspec`, `phpunit`, `pest`, `swift-test` and `mix-test` have fixtures under
`loom/src/testrun/fixtures/<adapter>/` written from the runner's documented output format, because the runner was not
installed on the recording host. Each directory carries a `PROVENANCE.md`; the other 14 adapters were captured from real
runs. Their parsers are unproven against real output until someone records a live run and replaces the fixture.

Open parser and command gaps found while writing the language skills, checked against the tree on 2026-09-25:

- **swift-test** reads only XCTest's `Executed N tests`. Swift Testing (`@Test`) prints `Test run with N tests`, so a
  filter selecting only Swift Testing tests parses `executed 0` and reads as `NotSelected` at freeze and completion.
- **maven** runs `mvn -q test -Dtest=...`; quiet mode suppresses Surefire's `Tests run:` line on a passing run, so a
  passing contract likely parses `Unparsed` (exit 0 passes with a warning) instead of `Passed`.
- **pest** `--filter <name>`: the mechanism by which a multi-word description reaches PHPUnit's name filter was not
  found in Pest's source. Run one through the real runner when Pest is available.
- **ctest** runs `cmake --build build && ctest --test-dir build ...` relative to the package dir. `scan.rs` makes every
  directory with a `CMakeLists.txt` a package, so a contract under a `tests/CMakeLists.txt` runs where no `build/`
  exists and completion can never pass. Nothing configures `build/`. Fix by walking up to the configured CMake root.
- **minitest** runs `ruby -Itest <file>` without `bundle exec`, unlike rspec with a `Gemfile`.
- **JVM**: `mvn` is taken from `PATH` even when `./mvnw` exists; every Gradle subproject with its own `build.gradle*`
  is a separate package and falls back to `gradle` on `PATH`; Android (`test` aggregate) and Kotlin Multiplatform
  (`jvmTest`) modules have no plain `test` task for `--tests`.
- **csharp** detection needs a `*.csproj` or `*.sln`; a directory holding only `.slnx` is not detected (project
  directories beneath it still are).
- **dart** `--plain-name` is a substring match, so a description contained in another test's name selects both.
- The test-file globs for swift, dart, csharp, cpp, java, kotlin, scala, ruby, php and elixir were not pinned by the
  design; `testrun/languages.rs` is the authority and the language skills describe conventions only.

## Verification v2 Known Gaps

- **Contract outcomes are self-attested.** The daemon re-derives changed paths from git but does not re-run agent-written
  contract tests (untrusted code outside the sandbox). A raw-socket client could report `failed` for an
  already-passing contract; completion re-runs every frozen contract and requires a pass. Security review accepted it.
- **Tracking-key collision.** `loom-contract-<id>` (Contract session of stage `<id>`) equals the Stage key of a stage named
  `contract-<id>`; `orphan_candidates.rs:21-34` matches pid-file stems by prefix, so after a daemon restart the scanner
  can adopt one stage's process into the other. Same class as the `merge-`, `knowledge-` and `adjudication-` prefixes;
  `validation.rs` `RESERVED_NAMES` reserves none of them.
- **Escalated contract stage recovery.** `loom stage retry` refuses `NeedsHumanReview`; `human-review --approve` resets
  the contract budget inside the locked stage update that queues the stage (`loom/src/commands/stage/human_review.rs:95-111`).
  Approval does not retain the exhausted contract budget.
- **Continuation writer.** After a ceiling handoff the replacement contract writer gets a fresh contract signal without
  its predecessor's handoff (`generate_contract_signal` takes no handoff file).
- **Stale wiring pin from another stage.** Integration-verify has no self-service channel for it: `dispute-criteria` takes an
  acceptance index and the daemon admits a dispute only for the caller's own stage (`daemon/server/self_service.rs:86-94`).
  The repair is an operator `loom stage amend --field wiring`. Consider a dispute kind for aggregated wiring gaps, or
  have the aggregated check name the amend command.
- **Target-branch resolution differs.** `commands/hook/review_harvest.rs:119` resolves from the stage worktree, completion
  and both `loom stage review` commands from the CWD repo root. They agree while the CWD is inside the repo; share
  `commands/stage/review_status.rs::stage_worktree_and_target`.
- **Journal splitting.** `fs/memory/parser.rs:30` treats any line starting `###` as an entry header and `validate_content`
  checks length only, so a note or suggestion whose text starts with `###` splits the journal. Review harvest flattens
  suggestions to one line but does not escape a leading `###`.
- **Impact selection misses macro-only use** (`context/extract/rust.rs:19`) and skips every test in a contract file.
- **Reachable by bare name** passes when any symbol of that name is reached from any `from` node; common names (`run`,
  `handle`) can false-pass. `Partial` coverage counts as checked in definition-site exclusion.
- **`loom plan verify` does not check where contracts run.** A stage with `contracts` whose `working_dir` has no manifest
  for the contract's adapter (`Cargo.toml` for cargo) verifies clean, although every contract command then exits
  non-zero: `cargo test <name> -- --exact` takes no manifest path. See
  [verification-v2-delivery.md](../mistakes/verification-v2-delivery.md). A preflight check could resolve each
  contract's adapter and require its manifest in `working_dir`.
- **Integrity blind spot.** A file marked skip-worktree or assume-unchanged hides its edits from the `git diff` the
  fingerprint and the count totals trust. `verify::integrity::current_events` has no production caller (kept for the
  pinned dispute-kinds API).
- **Duplicated helpers** to unify: `verify/review/store.rs::canonical_work_dir` and the private one in
  `verify/contracts/store.rs`; the journal walk in `signals/v2_section_review.rs` and `commands/memory/handlers/pending.rs::staged_entries`;
  `CwdGuard` copied into four test files; `verify/utils.rs` `.size_limit(1 << 20)` versus `wiring.rs` `PATTERN_SIZE_LIMIT`.
- **Slow hook tests** (`loom-hooks/tests/run-all.sh` is 162 s after the PATH-fixture fix): `prefer-modern-tools-missing-rg-fd.sh`
  78 s, `poll-guard-subagent-waits.sh` 63 s, and four others near 40 s.

## Provision Install Validation Gaps

Plan authors are trusted, so these are author-mistake gaps, not stage-agent bypasses. The validator is `plan/schema/validation/v2_fields/provision_installs.rs`; the required forms are in [plan-environment](../architecture/plan-lifecycle-and-fields.md#plan-provision-snapshot-spawn-gate-hardening).

- **Indirection is not seen through.** Install detection takes the first non-flag word as the subcommand, so `npm --prefix web ci`, `bun --cwd web install`, `pnpm -C web install`, `env -C web bun install` (`command_start` stops at the `-C` value) and `corepack pnpm install` are not checked. `changes_directory` does not see through `eval`: `refusal && eval 'cd web' && bun install ...` moves the install unflagged, and `eval "bun install"` escapes install detection. Fix: parse `eval`'s argument as a nested script in `nested_scripts`, or reject the wrappers.
- **The `.npmrc` refusal tests only the entry's directory.** For a `working_dir` nested in an npm or pnpm workspace the root `.npmrc` may still be read. Document `working_dir` as the workspace root or check the ancestors.
- **Yarn.** Yarn classic reads `.yarnrc` `yarn-path` and yarn berry reads the yarn berry config `yarnPath`; both run a repository JS file on the host even with `--ignore-scripts`. Berry also rejects `--frozen-lockfile` and `--ignore-scripts` (it wants `--immutable`), and `YARN_INSTALL` uses classic syntax. Validation checks only `--ignore-scripts` and the `.npmrc` refusal.
- **uv.** `uv sync --frozen --no-install-project` still builds sdist dependencies through their build backends on the host and reads agent-writable the uv config and `[tool.uv]` project index settings (a registry redirect like `.npmrc`). Validation requires only `--no-install-project`.
- **Host caches.** The stage sandbox can write `~/.bun/install/cache`, `~/.npm` and the pnpm store (`sandbox/package_caches.rs`), which the host provision reads with the operator's `HOME`. `--backend=copyfile` stops writes back into the bun cache but not a poisoned extracted entry, and a committed `web/node_modules` symlink is an untested channel. Fix: seed or isolate the cache for provision (see [state-confinement-gaps](state-confinement-gaps.md)).
- **Module cycle.** `visit_argvs` (`v2_lints/mod.rs`) is used by `v2_fields::provision_installs`, while `v2_lints::js_provision` imports the hardened constants from `v2_fields`. Moving `visit_argvs` next to `command_start` and `nested_scripts` in `criterion_hazards.rs` removes it.

## Provision Gate and Environment Lint Gaps

- **Before/after comparison by entry string.** `provision_gate.rs` matches `git status` entry strings, so a provision that rewrites a file already listed before it ran passes the ignored-files-only rule. Compare content (a hash of the diff and untracked files) or document the limit.
- **`loom init` prints no provision entries.** The operator cannot see which host commands the plan will run unsandboxed; listing them at init would show it.
- **Duplicated code.** `block_for_provision` repeats `persist_blocked_stage` (`stage_executor.rs`) plus `close_reason` and the graph mark; `write/read_provision_snapshot` re-implement `config_sections.rs` `write_section/read_section` including a toml to `toml_edit` round trip; `merge_gate.rs::hooks_dir_prefix` reads `--local/--global/--system` itself while `git/hooks.rs::configured_hooks_path` implements the same precedence.
- **Hook lint false negatives.** `repo_hooks.rs` skips a symlinked hook (`ln -s ../../scripts/pre-commit .git/hooks/pre-commit`) and does not expand a `~/` `core.hooksPath`. A multi-link file is not refused (no impact: hook content is never echoed and `.git` is write-denied).
- **Registry lint.** `registry_domains.rs` ignores the stage's `excluded_commands` (which run outside the sandbox), so it errors for a command that can already reach the network; `npm --prefix web install` and bare `yarn` are missed; `--offline` installs are over-reported. `sandboxed_commands` filters by the `BEFORE_STAGE_LABEL` label prefix; a `sandboxed` flag on `StageCommand` would not depend on label text.
- **JS lint advice.** `js_provision.rs` tells a package with no lockfile to install with `npm ci` after committing a lockfile, even when its runner is bun.

## Completion-Gate and Dispute Backlog

- `format_gate.rs` builds a `Stage::default()` whose empty id makes `${STAGE_ID}` expand to an empty string, contradicting the module doc; `biome check` also lints, so a lint error in a contract file is reported with "run the formatter" advice; the formatter-gate tests (`freeze_tests.rs`) need a host `rustfmt` binary.
- Three enums mirror the dispute field (`DisputeField` in `cli/types_stage_disputes.rs`, `AmendField`, `CriterionField`), and `dispute_criteria_with_mode` takes nine arguments under `allow(too_many_arguments)`.
- Other `std::env::set_current_dir` tests (`commands/knowledge/tests*.rs`, `telemetry/tests.rs`) restore the cwd by hand and are not panic-safe; the guard in `commands/stage/state_relay.rs` tests could become a shared `CwdGuard`.
- `commands/status/data/collector.rs::build_stage_summary` stays under the 50-line ledger through `let (facts, now) = (...)`; moving the ledger entry or extracting a helper is the cleaner fix.

## Test Gaps Recorded by Reviewers

- No test drives `loom init` with a provision plan and reads the snapshot back; passing `LoomConfig::default()` instead of the parsed plan's loom config survives the wiring regex (`commands/init/plan_setup.rs`).
- `start_stage` reaching `pre_spawn_gates_passed` is checked by a source regex only; a runtime test that a failing provision stops the spawn would pin it.
- Git-status tolerance of a failing listing is covered only through a non-git `TempDir`, which depends on `TMPDIR` not sitting inside a repository.
- An unreadable or unparsable manifest counting as declaring dependencies (`impact_tests/runs.rs`) and a manifest over 256 KiB (`skills/project/tests.rs`) have no test.
- `joined_by_and`'s `|`, `&` and parenthesis arms have no test (`plan/schema/tests/v2_tests.rs`); the `pushd web && refusal && install` case fails the `opens` check first, so it does not guard `changes_directory`.
- `crash-blocked-stage-is-not-an-agent-block` covers a crash with retries left only; a `(_, Some(reason))` arm placed after the auto-retry arm is caught only by `attention_model_guidance_tests.rs`. `pre-commit-hook-registry-need-is-an-error` has no negative control (unit tests cover it). The `close_reason` sanitizer test asserts only the absence of ESC and bidi characters, not the flattened text.

## Contract Freeze Format Gate Recognises a Fixed Formatter List

`verify/contracts/format_gate.rs` treats an acceptance criterion as a formatter check only when its command text matches `FLAG_CHECKS`, the biome rule or `SCRIPT_CHECKS`: cargo fmt, rustfmt, prettier, oxfmt, ruff format, black, biome, and `format:check` scripts. A plan with no match gets no gate, and other languages (Go, C/C++, Elixir, .NET, Dart, Swift, Ruby, Kotlin) are not covered. `gofmt -l` is excluded because it prints unformatted files and exits 0.

**Options, none implemented:**

- A `format_check: bool` field on `TruthCheck` (`models/stage/checks.rs`, `deny_unknown_fields`, serde default and skip-if-false like `WiringCheck.literal`), honoured by `is_format_check`. A Simple (string) criterion cannot carry it. The field must be threaded through plan amendment (`plan/amendment.rs`, `plan/amendment_fields.rs`), `acceptance_command.rs` validation, the adjudication prompt and `verify/criteria/cache_contract.rs` (pass-cache keys); check whether it needs the `version: 2` gate.
- A `stdout_empty` field beside `stderr_empty`, so `gofmt -l .` fails on any output without a `test -z` wrapper.
- A narrower rule recognising `test -z "$(gofmt -l ...)"` and more formatter commands in `format_gate.rs`, one file, no schema; `invocations()` splits subshells and pipelines, so each accepted shape needs a test, with near-miss tests as the module already has.

## Contract Run Dir Detected Twice at Freeze

The freeze CLI resolves each contract's run dir for its red run, and the daemon resolves it again from the worktree when it writes `freeze.json` (`daemon/server/contracts.rs`); `ContractRunReport` carries no cwd. A run-root marker matching a `harness` glob that the writer adds or removes between the two would record a dir the red run never used. Fix when it matters: send the CLI's cwd in the report and refuse a freeze whose daemon-side resolution differs.

`loom stage amend --field contracts` re-freezes nothing. An amended explicit `cwd` still takes effect, because the plan's `cwd` beats the recorded one, and a changed `file` drops the recorded dir. But the new file's content is not hashed until a dispute re-freezes it.

## In-Session Completion Gaps

Open after host jobs moved the run out of the session: `loom check` and `loom stage complete` in a session relay the run to the daemon ([Stage-to-Daemon Channels](../patterns/stage-daemon-channels.md)).

- **The gate and completion share one runner.** `loom check` runs criteria through the confined runner that `loom stage complete` uses, so the orchestrator's gate and completion agree. Where they can differ is the session's own shell: an unfiltered `cargo test --all-targets` there fails 26 tests that pass under `loom check` ([Sandbox And Confinement Gaps](sandbox-and-confinement-gaps.md)).
- **Suites that fail in the session shell.** These fail there at base, not under the host job:
  - a TMPDIR that contains "claude" (`hooks_subagent_verify_guard`);
  - the bounded-stdin hook test reads `LOOM_*` from an ancestor process;
  - a worktree-file-guard scratchpad test;
  - `plan_environment_contracts` (see [Tests and Criteria That Pass in a Session Fail Under the Confined Runner](sandbox-and-confinement-gaps.md)).

## Dispute Accept and Host-Job Recovery Gaps

- **Accept re-freezes only the disputed contract's files.** `orchestrator/adjudication/apply_contract.rs` `accept()` collects the contract's file, the recorded file for that contract and the refrozen contracts. Any other file in the freeze record, such as a harness-glob file the disputed diff touched (`tests/host_verification_contracts/support.rs` in the host-verification stage), keeps its old hash. `loom stage complete` then fails with `frozen file ... changed after the freeze` (`verify/contracts/completion.rs`) while every criterion is green. Fix direction: re-freeze every frozen file the disputed diff touched, or every harness-glob file, on accept.
- **Recovery signals groups of dead children.** `host_verify/signals.rs::kill_led_groups` runs for `Interrupted` records whose child is long gone (the daemon was down for hours, or the host rebooted). The leader check cannot tell a recycled pid that leads its own group from the old criterion leader, so the SIGKILL can reach an unrelated service group. Gating the call on a live `verify-host` child breaks `recovery_interrupts_running_jobs_and_leaves_queued_ones_alone` (`host_verify/tests_runner_recovery.rs`), which asserts that a led group is killed while the record's pid is a stranger. Revisit: record a start time or command line per group, then rewrite that test.

## Gate-Dedup and Integration-Verify Review Suggestions

Reviewer suggestions from the gate-dedup and integration-verify rounds that were not implemented. Each is open unless marked otherwise. Locations are at the review's tree.

- `loom/src/plan/schema/validation_suite.rs:134`: lint messages use 1-based criterion numbers, while `loom check --criterion` and the printed labels are 0-based. Align with the other lints in that file.
- `loom/src/plan/schema/validation_suite.rs:129`: a value-less flag followed by a positional (`--release x`) takes `x` as its value. Only missed warnings result; accepted.
- `loom/src/plan/schema/validation_suite.rs:57`: `has_flag(options, "-p")` misses the attached form `-pfoo`, which `build_config` handles, so `cargo test -pfoo` is classed as a full run.
- `loom/src/plan/schema/validation_suite.rs:43`: `classify` resolves `npm test` and `package.json` scripts against the lint process's cwd, not the stage's `working_dir`.
- `loom/src/plan/schema/validation_suite.rs:21`: `NOT_COVERED_FLAGS` lacks `--benches` and `--examples`, so `cargo test --benches` is judged covered by `cargo test`.
- `loom/src/commands/stage/acceptance_runner.rs:211`: a `--criterion` subset run prints "All acceptance criteria passed!" when only the selected criteria ran. Print "Selected criteria passed" when the selection is set.
- `loom/src/orchestrator/adjudication/apply_contract.rs:46`: `recorded` is fed from three sources, but `record.files` (line 63) already holds the pre-amendment file. Lines 46, 55 and 56 serve only the no-such-contract bail; collapse to `record.files` plus one existence check.
- `loom/src/orchestrator/adjudication/apply_contract.rs:84`: `accept_notice` lists every re-frozen path (up to 500) in the feedback body. Cap it at 20 paths and a count.
- `loom/src/verify/contracts/frozen_set.rs:38`: re-freeze is all-or-nothing. One harness file that is a symlink or over 4 MiB, or the 501st match, fails the accept; it retries up to `MAX_APPLY_ATTEMPTS`, then escalates to a human with the accept never applied. A deleted contract file also errors the accept instead of being dropped. The judge prompt should say so.
- `loom/src/orchestrator/adjudication/prompt/contract.rs:281`: the diff fence is a fixed three backticks. A bare three-backtick line in a harness file closes the fence under CommonMark. Use a fence longer than any backtick run in the diff; this also fixes the main diff fence near line 198.
- `loom/src/commands/verify_relay.rs:115`: the `--criterion` selection is not normalised. Wait state compares the raw Vec (`wait.rs:153`), so the same set in another order requests anew. Sort and dedup once in `verify::execute`.
- `loom/src/orchestrator/core/inbox_drain/check.rs:100`: `validate` builds a whole `CriteriaConfig::default()` (which reads `LOOM_ACCEPTANCE_CACHE`) only to call `check_selection`; a free function over the set and stage would do. The line is probably over the rustfmt chain width of 60.
- `loom/src/plan/schema/validation_suite.rs` (the `-p` and `--benches` cases above) and `loom/tests/gate_dedup_contracts.rs:116`: only vitest runs through the binary. A loop over `registry::all()` would pin every adapter.
- `loom-hooks/subagent-verify-guard.sh:245`: `scan_args` treats `-p`/`--package` as scoped, but `cargo_test.rs:108-118` treats `cargo test -p <crate>` as a full run, and `pytest -p plugin` reads as a scope. The main agent can run a whole crate with `-p`.
- `loom-hooks/subagent-verify-guard.sh:522`: `timeout 300 cargo test` bypasses the hook because `timeout` is not in the prefix list. Add `timeout` and `bundle exec`/`php` prefixes. `make test` stays allowed for the main agent by design (`:379`).
- `loom-hooks/subagent-verify-guard.sh:128`: the main-agent branch ignores `LOOM_SESSION_TYPE` (`wrapper.rs:340`), so contract, adjudication and base-conflict sessions of a standard stage get the main-agent block. Low impact; unverified.
- `loom-hooks/subagent-verify-guard.sh:166`: the comment lists `CLAUDE.md.template` as a byte-identical BLOCK-A copy. `block_a_agrees_across_every_surface` (`tests_doctrine.rs:113-133`) pins only the two prefixes, the preamble and the guard. The same claim sits in `orchestrator/signals/cache/blocks.rs:7-9`.
- `loom/src/orchestrator/signals/cache.rs:119`: "Rule 5's complete-suite instruction" is a dangling reference, since template Rule 5 now says only that integration-verify keeps its own gate. Pinned by `tests_doctrine_prefixes.rs:222`.
- `skills/loom-orchestration/SKILL.md:75`: "host runner that completes the stage" misleads for `loom check`. Say that `loom stage complete` reuses the check's cached passes on an unchanged tree.
- `loom/src/orchestrator/signals/tests_doctrine_gate.rs:26`: the retired-agent test covers five surfaces. README, CONTRIBUTING, `spawn-guard.sh` `TYPED_AGENTS` and the plan-writer SKILL and references are not asserted. `code_reviewer_judges_criteria_intent` matches two loose words anywhere in the file.
- `AGENTS.md.template:57`: "The orchestrator performs integration verification" should name the stage's gate (`loom check`) for standard stages.
- `loom-hooks/tests/subagent-gate-payload.sh:36`: the header comment splits `LOOM_MAIN_AGENT_PID` across two lines. Case (j) sits before (f) and there is no (k).
- `loom/tests/integration/hooks_subagent_verify_guard_cases.rs:37`: a stale comment says "expected to still fail until the concurrent hook fix lands". No cases cover `bun test`, `cargo nextest` or bare `jest` as the main agent.
- `loom/tests/integration/hooks_subagent_verify_guard_main.rs:19`: `write_standard_stage` duplicates `write_stage_file` in the carve-out module and in `gate_dedup_contracts/support.rs:305`.
- `agents/loom-code-reviewer.md:33`: a stray blank line sits between review-method items 5 and 6.
- `loom/src/assets/install/claude.rs:60`: the installer never prunes `~/.claude/agents`, and `install.rs` has no pruning of retired agent files either. A retired verifier agent file stays installed on upgraded machines until removed by hand.
