# Verification V2 Followups

>

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
- **ctest** builds and tests a `build/` directory relative to its package root. `skills/project/scan.rs::resolve_nested_cmake` folds nested CMakeLists directories into their ancestor CMake package. The command does not configure a build tree; the package must provide one.
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

- **Contract red runs are host-owned.** The freeze RPC accepts head/tree identity and queues a Freeze host job; `host_verify/freeze.rs` prepares and runs contracts through the confined host runner. `stage_service/contracts.rs::freeze_with_reports` applies host reports only after checking captured file hashes. Session-supplied red reports are not the freeze admission interface.
- **Tracking-key collision.** `loom-contract-<id>` (Contract session of stage `<id>`) equals the Stage key of a stage named
  `contract-<id>`; `orphan_candidates.rs::orphan_candidate` matches pid-file stems by prefix, so after a daemon restart the scanner
  can adopt one stage's process into the other. Same class as the `merge-`, `knowledge-` and `adjudication-` prefixes;
  `validation.rs` `RESERVED_NAMES` reserves none of them.
- **Escalated contract stage recovery.** `loom stage retry` refuses `NeedsHumanReview`; `human-review --approve` resets
  the contract budget inside the locked stage update that queues the stage (`loom/src/commands/stage/human_review.rs::handle_approve`).
  Approval does not retain the exhausted contract budget.
- **Continuation writer.** After a ceiling handoff the replacement contract writer gets a fresh contract signal without
  its predecessor's handoff (`generate_contract_signal` takes no handoff file).
- **Stale wiring pin from another stage.** Integration-verify has no self-service channel for it: `dispute-criteria` takes an
  acceptance index and the daemon admits a dispute only for the caller's own stage (`daemon/server/self_service.rs::ownership_to_enforce`).
  The repair is an operator `loom stage amend --field wiring`. Consider a dispute kind for aggregated wiring gaps, or
  have the aggregated check name the amend command.
- **Target-branch resolution differs.** `commands/hook/review_harvest.rs::review_harvest` resolves from the stage worktree, completion
  and both `loom stage review` commands from the CWD repo root. They agree while the CWD is inside the repo; share
  `commands/stage/review_status.rs::stage_worktree_and_target`.
- **Journal splitting.** `fs/memory/parser.rs::parse_journal` treats any line starting `###` as an entry header and `validate_content`
  checks length only, so a note or suggestion whose text starts with `###` splits the journal. Review harvest flattens
  suggestions to one line but does not escape a leading `###`.
- **Impact selection misses macro-only use** (`context/extract/rust.rs`) and skips every test in a contract file.
- **Reachable by bare name** passes when any symbol of that name is reached from any `from` node; common names (`run`,
  `handle`) can false-pass. `Partial` coverage counts as checked in definition-site exclusion.
- **`loom plan verify` does not check where contracts run.** A stage with `contracts` whose `working_dir` has no manifest
  for the contract's adapter (`Cargo.toml` for cargo) verifies clean, although every contract command then exits
  non-zero: `cargo test <name> -- --exact` takes no manifest path. See
  [verification-v2-delivery.md](../mistakes/verification-v2-delivery.md). A preflight check could resolve each
  contract's adapter and require its manifest in `working_dir`.
- **Integrity blind spot.** A file marked skip-worktree or assume-unchanged hides its edits from the `git diff` the
  fingerprint and the count totals trust. `verify::integrity::current_events` is the
  evidence source for `loom stage dispute-integrity` (`commands/stage/dispute_kinds.rs:73`), so the blind spot reaches
  the judge's evidence too.
- **Duplicated helpers** to unify: `verify/review/store.rs::canonical_work_dir` and the private one in
  `verify/contracts/store.rs`; the journal walk in `signals/v2_section_review.rs` and `commands/memory/handlers/pending.rs::staged_entries`;
  `CwdGuard` copied into four test files; `verify/utils.rs` `.size_limit(1 << 20)` versus `wiring.rs` `PATTERN_SIZE_LIMIT`.
- **Slow hook tests** (`loom-hooks/tests/run-all.sh` is 162 s after the PATH-fixture fix): `prefer-modern-tools-missing-rg-fd.sh`
  78 s, `poll-guard-subagent-waits.sh` 63 s, and four others near 40 s.

- **Waiting check verdicts cover the union.** Followers waiting for different criterion subsets share the verdict for the combined waiting selection. Revisit if reviewers misread failures from criteria they did not request.
- **Followers of one selection share wait state.** The first follower to see the job end removes the state; a later follower can relay again. An unchanged passing tree hits the cache, while a failing criterion runs again. Revisit if field data shows repeated failing integration-verify runs.
- **A promoted waiting job's hold counts its wait.** `hung_latch::job_holds` (`orchestrator/monitor/hung_latch.rs`) times an unstarted job from `queued_at`, and a waiting job keeps the `queued_at` of its first request. Following sessions heartbeat on every `loom check`. Revisit if a waiting job's session is flagged hung while the check ahead of it ran.
- **The `--criterion` normaliser call is unpinned.** Deleting `normalized_criteria` from `commands/verify.rs::execute` passes every test, because relay mode exits the process. Revisit when the relay path gains an in-process test seam.
- **Integration-verify full runs are routed by doctrine only.** `subagent-verify-guard.sh` exits 0 for every subagent and the main agent of an integration-verify stage. `scripts/full-runs-field-check.py` measures bare runs there until the field check is done. Revisit if they stay common next to `loom check integration-verify` calls.
- **A package-scoped run passes the guard as scoped.** `_verify_matchers.sh` reads a package selection (`cargo test -p <crate>`, `--package`, and each runner's equivalent) as scoped; in a project with one package that is the whole suite. Revisit if field data shows package scope used as a full run.

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
- `status/data/collector.rs::build_stage_summary` stays under the 50-line ledger through `let (facts, now) = (...)`; moving the ledger entry or extracting a helper is the cleaner fix.

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

## Authoritative contract run-directory resolution

The freeze CLI preflights site changes and submits the head and tree identity. The host freeze job performs the authoritative recheck, resolves contract run directories, executes the contract checks and records their freeze evidence. The CLI does not supply self-attested red-run results for a later daemon directory resolution.

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

- **Accept re-freezes the contract and harness set.** `orchestrator/adjudication/apply_contract.rs::accept` combines the old freeze paths, current contract paths and current harness matches before `read_frozen_set` and `refreeze_set`. Changed harness files receive current hashes and deleted optional recorded/harness paths leave the freeze. Each generation is complete and synced before its record is published ([Accepted Dispute Re-Freeze](../architecture/contract-phase.md#accepted-dispute-re-freeze)); generations are kept indefinitely, and power loss has not been exercised.
- **Recovery signals groups of dead children.** `host_verify/signals.rs::kill_led_groups` runs for `Interrupted` records whose child is long gone (the daemon was down for hours, or the host rebooted). The leader check cannot tell a recycled pid that leads its own group from the old criterion leader, so the SIGKILL can reach an unrelated service group. Gating the call on a live `verify-host` child breaks `recovery_interrupts_running_jobs_and_leaves_queued_ones_alone` (`host_verify/tests_runner_recovery.rs`), which asserts that a led group is killed while the record's pid is a stranger. Revisit: record a start time or command line per group, then rewrite that test.

## Gate-Dedup and Integration-Verify Review Suggestions

Reviewer suggestions from the gate-dedup and integration-verify rounds that were not implemented. Each is open unless marked otherwise. Locations are at the review's tree.

- `loom/src/plan/schema/validation_suite.rs`: lint messages use 1-based criterion numbers, while `loom check --criterion` and the printed labels are 0-based. Align with the other lints in that file.
- `loom/src/plan/schema/validation_suite.rs::build_config`: a value-less flag followed by a positional (`--release x`) takes `x` as its value. Only missed warnings result; accepted.
- `loom/src/plan/schema/validation_suite.rs::scopes_package` recognizes separate and attached `-p` forms, including `-pfoo` and `-p=foo`, plus `--package`.
- `loom/src/plan/schema/validation_suite.rs::classify`: `classify` resolves `npm test` and `package.json` scripts against the lint process's cwd, not the stage's `working_dir`.
- `loom/src/plan/schema/validation_suite.rs::NOT_COVERED_FLAGS`: `NOT_COVERED_FLAGS` lacks `--benches` and `--examples`, so `cargo test --benches` is judged covered by `cargo test`.
- `loom/src/commands/stage/acceptance_runner.rs::run_selected_acceptance`: a `--criterion` subset run prints "All acceptance criteria passed!" when only the selected criteria ran. Print "Selected criteria passed" when the selection is set.
- `loom/src/orchestrator/adjudication/apply_contract.rs::accept`: `recorded` is fed from three sources, but `record.files` already holds the pre-amendment file. The other sources serve only the no-such-contract bail; collapse to `record.files` plus one existence check.
- `loom/src/orchestrator/adjudication/apply_contract.rs::accept_notice`: `accept_notice` lists every re-frozen path (up to 500) in the feedback body. Cap it at 20 paths and a count.
- `loom/src/verify/contracts/frozen_set.rs::read_frozen_set`: re-freeze is all-or-nothing. One harness file that is a symlink or over 4 MiB, or the 501st match, fails the accept; it retries up to `MAX_APPLY_ATTEMPTS`, then escalates to a human with the accept never applied. A deleted contract file also errors the accept instead of being dropped. The judge prompt should say so.
- `loom/src/orchestrator/adjudication/prompt/contract.rs::push_content`: the diff fence is a fixed three backticks. A bare three-backtick line in a harness file closes the fence under CommonMark. Use a fence longer than any backtick run in the diff; this also fixes the main diff fence.
- `loom/src/orchestrator/core/inbox_drain/check.rs::validate`: `validate` builds a whole `CriteriaConfig::default()` (which reads `LOOM_ACCEPTANCE_CACHE`) only to call `check_selection`; a free function over the set and stage would do. The line is probably over the rustfmt chain width of 60.
- `loom/src/plan/schema/validation_suite.rs` (the `-p` and `--benches` cases above) and `loom/tests/gate_dedup_contracts.rs::plan_verify_flags_full_vitest_suite_in_standard_stage`: only vitest runs through the binary. A loop over `registry::all()` would pin every adapter.
- `loom-hooks/subagent-verify-guard.sh::scan_args`: `scan_args` treats `-p`/`--package` as scoped, but `cargo_test.rs::is_full_run` treats `cargo test -p <crate>` as a full run, and `pytest -p plugin` reads as a scope. The main agent can run a whole crate with `-p`.
- `loom-hooks/subagent-verify-guard.sh::check_command`: `timeout 300 cargo test` bypasses the hook because `timeout` is not in the prefix list. Add `timeout` and `bundle exec`/`php` prefixes. `make test` stays allowed for the main agent by design.
- `loom-hooks/subagent-verify-guard.sh`: the main-agent branch ignores `LOOM_SESSION_TYPE` (exported by `orchestrator/terminal/native/wrapper.rs::build_wrapper_script`), so contract, adjudication and base-conflict sessions of a standard stage get the main-agent block. Low impact; unverified.
- `loom/src/orchestrator/signals/tests_doctrine_gate.rs::the_retired_agent_definition_does_not_exist`: the retired-agent test covers five surfaces. README, CONTRIBUTING, `spawn-guard.sh` `TYPED_AGENTS` and the plan-writer SKILL and references are not asserted. `code_reviewer_judges_criteria_intent` matches two loose words anywhere in the file.
- `AGENTS.md.template:57`: "The orchestrator performs integration verification" should name the stage's gate (`loom check`) for standard stages.
- `loom-hooks/tests/subagent-gate-payload.sh`: the header comment splits `LOOM_MAIN_AGENT_PID` across two lines. Case (j) sits before (f) and there is no (k).
- `loom/tests/integration/hooks_subagent_verify_guard_cases.rs::BLOCK_CASES`: main-agent cases do not cover `bun test`, `cargo nextest` or bare `jest` as the main agent.
- `loom/tests/integration/hooks_subagent_verify_guard_main.rs::write_standard_stage`: `write_standard_stage` duplicates `write_stage_file` in the carve-out module and in `gate_dedup_contracts/support.rs::write_standard_stage`.

## Subcommand Lint Path Ownership

The subcommand lint resolves stage file and artifact entries against `working_dir` before deciding whether a stage or a dependency may add a CLI subcommand. Partial-component glob prefixes are conservatively treated as possible ownership; this can produce warnings for patterns whose later tokens would not match.

## Merge Resolver and Relay Gaps

- **A relay ticket is stranded when its Bash call exits non-zero.** The relay hook is registered on `PostToolUse` only, so a command that prints its `LOOM_RELAY_V1` line and then exits non-zero is not relayed; a resolver's commit ticket stayed `pending relay` this way. The trigger then was the `TMPDIR` deny ([the mistake](../mistakes/sandbox-write-rules-inert.md)), and anything else failing a Bash call can repeat it.
- **`--resolved` ignores the requesting session.** `inbox_drain/merge_resolved.rs::resolve_merge_from_inbox` takes `_session`, so a dead resolver's queued request can cancel a newer resolver's check. Requests drained after a resolver exits are deliberate, so a fix has to keep that case.
- **Plan locks outside unit tests.** `fs/locking.rs::plan_lock_dir` uses `temp_dir()` only under `cfg(test)`. Integration tests that reach `lock_plan` (`plan_amendment_integration`, `adjudication_e2e`, `pressure_pipeline_contracts`, `refusal_adjudication_contracts`) still need a writable `/tmp` and fail inside a session sandbox, and `loom pressure` run by hand in a session fails the same way.
- **Unproven end to end.** No real resolver has run `loom check` through a daemon from its capsule, and the kill-ordering test in `inbox_drain/tests_merge_check.rs` has not been run against the mutations it is meant to catch.

## Fewer-Disputes Follow-Ups: Integrity, Reviewer Brief, Dry Run, Verdict Completion

Open items from the reviews of the fewer-disputes plan, each unimplemented at the time of writing. Mechanisms: [verification-v2-gates](../architecture/verification-v2-gates.md), [hook-system](../architecture/hook-system.md), [plan-lifecycle-and-fields](../architecture/plan-lifecycle-and-fields.md), [adjudication-lifecycle](../architecture/adjudication-lifecycle.md).

**Integrity detector** (`verify/integrity/`)

- `moves_collect.rs::collected` checks collection per file: a destination function inside `#[cfg(any())] mod dead {}` or a Python non-`Test*` class still cancels; Go returns `true` unconditionally, so a destination with a `//go:build <tag>` or `// +build` header (or under `testdata`, `_`, `.` directories) cancels although `go test` skips it. Revisit: check enclosing-scope attributes and class names, and treat a build-constraint header as uncollected.
- `moves.rs::normalize` drops indentation, so for Python a moved function whose assert was re-indented under an empty loop keeps the same identity. Revisit: keep relative indentation in Python identity.
- `moves_tests.rs::tree` duplicates `verify/contracts/collection/rust/mod.rs:127`, and `moves_collect.rs::parent` duplicates `rust/mod.rs:86`; share them once file ownership allows.
- Scan cost on a large stage: the Rust collection cache is keyed by (path, scope) and each distinct moved function repeats about six `chain_problems` calls (key it by destination file); `extract_functions` runs for a delta even when no other delta of the language added or lost statements; each changed base test file is fetched by two `git cat-file blob` processes (`edits.rs`, `count.rs`) and head files are read twice (share one read or use `git cat-file --batch`). The reviewer brief bounds the scan at 5 s.
- Integrity prompt text: none open; the judge prompt passes event id, path and language through `single_line`.

**Reviewer brief** (`commands/hook/reviewer_brief*.rs`, `loom-hooks/_spawn_policy.sh`)

- One lost statement whose cleaned text starts with `<!-- loom-` (possible for Kotlin and Scala, whose `statement_start` begins at the line's first non-whitespace character) suppresses the whole brief silently. Revisit: prefix or indent each fence line.
- A reviewer-brief timeout, failure or bad envelope is reported only through `loom_debug`, while the signal tells the orchestrator not to paste the brief. Add an `additionalContext` warning as `loom_reviewer_model_override` does with `WARN_REVIEWER`.
- `loom_reviewer_brief` appends even when the prompt already holds a `<!-- loom-reviewer-brief` marker, so a pasted brief doubles the prompt. Skip when the marker is present, or have the guard drop caller-supplied marker lines; today the agent file only says the last section is loom's.
- The sentence "For every lost assertion statement below, report a finding" also covers accepted events, while the next sentence says they are context only; the wording is pinned by the plan, so change it there.
- `NO_RECORD` (`reviewer_brief.rs`) must equal the literal `not accepted` in `verify/integrity/gate.rs::shortfall`. Export one constant or return a distinct shortfall variant.
- `render` reserves but does not cap the integration section; its byte budgets sum to about 16 KiB, so a pathological plan can exceed `MAX_BRIEF_BYTES`. The 10 KiB path budget lists about 190 of this plan's 223 files and summarises the rest by directory; the "typical path is 44 bytes" comment in `reviewer_brief_plan.rs` holds only for short early paths.
- `plan_changed_files` spawns three git processes per completed stage; one `git rev-list --first-parent --merges --parents <target>` would locate every stage's merge. `cut()` (`dry_run.rs`) and `clean()` (`reviewer_brief_plan.rs`) are near-identical one-line-then-truncate helpers.

**`loom plan verify --dry-run`** (`commands/plan/verify/`)

- `dry_run_cleanup.rs::kill_processes_in` SIGKILLs every process whose cwd is inside a stale `loom-dry-run-*` directory, an operator shell included, with a pid-reuse gap between `read_link` and `kill`; the kill path and the signal handler are untested. Revisit: kill only the run's recorded process group, or use pidfd.
- `dry_run_exec.rs` reads, parses and builds the plan into stages twice (also `dry_run.rs:73-82`); pass the parsed metadata and stages through.

**Verdict completion** (`orchestrator/core/verdict_completion.rs`, `host_verify/verdict_complete.rs`)

- `start_verdict_completions`: one unreadable stage file aborts the whole sweep every tick, stranding every held stage until it is fixed; tolerate it per stage. Operator-held `NeedsAdjudication` stages with open disputes take the bounded dispute lock every tick only to return `Ok(false)`; check for unapplied disputes before locking.
- A `submit_verdict_completion` error (for example an unreadable `current.json`) hands the stage back at once even when a job may be running, so a fresh session can start beside a running host child. The doc comment makes this deliberate. Revisit if a fresh session is ever seen running beside a host child.
- `readable_record` is the third copy of the job-record identity check (`apply_retry.rs:49`, `store/order.rs:32`); move one helper into `host_verify/store`. The Completed-resume strings repeat `apply.rs::resume_completed`, and a crash-resumed application writes the marker `already completed` instead of the completion note (cosmetic).
- `latest_completion_head` is a `#[cfg(test)]`-only wrapper; the child test module can call the private `latest_completion` directly.
- A `VerdictComplete` that finishes or hands back the stage is logged only through tracing; the operator sees no line for the outcome.
- Test gaps: the reverse ordering in `verdict_hold_tests.rs` (a requeue-kind sibling verdict applied first, then a holding last verdict; safety relies on the gates refusing at application), and an error from the later eligibility checks (`uncounted_run`, `effective_policy_hash`), since the injected failure in `core/verdict_completion/tests.rs` hits `deciding_job`'s `load_current` first.
- Doctrine text in `orchestrator/signals/helpers.rs` `STAGE_EXIT_RULES`: "Before a dispute, run `loom stage complete` on that commit" has no antecedent in the paragraph now that "Commit before filing either" is dropped (consider "on your committed tree"), and "your last-completed tree" can read as the last tree that passed (the unambiguous form is "the tree your last `loom stage complete` ran on"). `BLOCK_F` in `tests_doctrine_v2.rs` changes in step.

**Rule 17 debt in the plan's changed files**, all recorded in `loom/maintainability-baseline.txt` at unchanged values: `plan/schema/validation.rs` 1063 lines (`validate` 357, `validate_structural_preflight` 196, `check_cross_stage_wiring_coverage` 66), `models/stage/types.rs` 591, `orchestrator/signals/cache.rs` 511 (`generate_knowledge_distill_stable_prefix` 74), `models/stage/methods.rs` 492, `models/stage/defaults.rs` `default` 76, `cli/dispatch.rs` `dispatch` 57.

## Full-Suite-at-Gate Review Leftovers

Reviewer suggestions nobody implemented; none is required for correctness.

- **Doctrine pins can be stricter.** `orchestrator/signals/tests_doctrine_prefixes.rs` asserts only that the integration-verify prefix lacks one phrase; `!prefix.contains("cargo")` would also reject a regression to a `cargo test`-only parenthesis. `tests_doctrine_blocks.rs` `RETIRED_PHRASES` lacks the old BLOCK-A rustfmt wording (`concat!("rustfmt --edition ", "2021 <files>")`), and no stage criterion greps it outside the knowledge tree.
- **The criteria sentence reads narrower than the gate.** `orchestrator/signals/cache.rs` introduces the criteria with a colon that lists build, test, lint and format; a gate also carries flake-check and markdownlint, so "such as the project's own ..." is accurate.
- **The refuse path is not shown to leave the waiting job alone.** `commands/stage/merge/live_job_tests.rs` could assert, between the daemon-running and no-daemon calls, that `load_pending` still returns the waiting job as `Queued`.
- **`merge.rs` reads both host-job slots without the stage lock.** `observed_host_jobs` (`commands/stage/state/loop_recovery/mod.rs`) reads them under `store::lock_stage` and drops the lock before `submit::cancel`; doing the same in `commands/stage/merge.rs` would remove the duplicated `live` closure (also in `submit.rs`).
- **The verify guard warns on every Bash call of a jq-less host.** `subagent-verify-guard.sh` runs `loom_warn_no_jq` before the cheap `LOOM_MAIN_AGENT_PID` gate, so every session on that host, loom or not, sees the warning. An early `exit 0` when the variable is empty would scope it to loom sessions.
- **The field check echoes matched segments.** `scripts/full-runs-field-check.py` prints the first 160 characters of a matched segment, and its `env <word>` prefix matches `env TOKEN=... cargo test`, so an inline secret reaches the terminal. Low risk: the input is the operator's own transcripts.
