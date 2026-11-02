# Contract Autonomy Followups

>

## Contract Runs the Chain Check Cannot See

The chain check proves which file rustc compiles under a contract's test name, not what else runs in that binary.

- A `macro_rules!` defined before a chain link in a file the implementer owns can shadow `assert!` for every descendant module, including the frozen file.
- A constructor (`#[ctor]`-style) in the same test binary can exit the process early while a shadow test passes.
- `.cargo/config.toml` runner overrides and `build.rs` can fake or alter a `cargo test` run.
- Revisit condition: pin each Rust contract run to the target the chain check resolved (run `cargo test --test <name>` or `--lib`/`--bin` for that root, with the config and build script frozen or refused).
- Inner attributes on a chain file are deny-listed (`cfg*`, unreadable path) while outer ones are allow-listed. On a nightly toolchain with `custom_inner_attributes`, a non-builtin inner attribute could rewrite the module (`collection/rust/chain.rs::file_problems`). An allow-list would fail closed on both sides.
- `prefixes()` (`chain.rs::is_auto_target`) lets the contract's test path extend past the frozen file's module path: a frozen file declaring `mod inner;` with test `inner::t` puts the test in an unfrozen file; a build failure at freeze counts as red, so it survives the freeze.
- A symlink at the second `CreateRoot` alternative (`verify/contracts/collection/rust/scaffold.rs::created_problems`) is invisible to the strict reader but its path is allowed, so rustc sees both files (E0761); the build failure counts as red at freeze.
- `run_dir_problems` runs at freeze only for contracts that set `cwd` (`collection/mod.rs::chain_problems`); a workspace-only `Cargo.toml` nested inside the package directory is the nearest marker but not the package manifest, and only completion catches it.
- A `cfg_attr` in any chain file, crate root included, is refused (`#![cfg_attr(not(test), no_std)]`, `#![cfg_attr(docsrs, ..)]`); the plan-writer skill documents the `tests/` workaround. Consider allowing a predicate that does not mention `test`.
- `harness.rs::is_production_source` `is_production_source` calls the test_support.rs and conftest.py helpers production source, so the harness warning is wrong for them (warn-only).
- `collection/mod.rs` `within()` silently drops scaffold edits outside `working_dir`, so the writer cannot satisfy the chain check and the refusal does not say why; `read_base` returns a base symlink's target text as file content while the worktree reader refuses symlinks.
- `changes.rs::changed_paths` uses `listed_unless_scaffold`, which removes worktree scaffolding before `listed_name` validates the remaining paths. Odd scaffold names do not block a freeze.
- `verify/contracts/completion.rs::run_contract` re-detects the adapter with `resolve_adapter` for `resolve_run` and `judge`, while `is_rust` uses the recorded one. Resolve once, recorded first, and share it.

## Freeze-Refusal Filing Gaps

- `orchestrator/core/event_handler/freeze_refusal.rs::dispute_refused_freezes_at`: a persistently failing filing is retried every tick with one warn per key and no counter; only the hung-session watchdog ends it. Consider a bounded attempt count.
- `orchestrator/core/event_handler/freeze_refusal.rs::standing_refusal`: a record marked declined is returned early for the same session, so a later local refusal by that writer is never recorded or filed; only a stderr line reports the decline.
- `orchestrator/core/event_handler/freeze_refusal.rs::file_refusal`: `escalate_refused_freezes` prints its own REVIEW NEEDED line, and in daemon mode the monitor's `StageNeedsHumanReview` event prints it again and sends the desktop notification.
- `orchestrator/core/event_handler/freeze_refusal.rs::note_child_disposition`: the `freeze-refusal-children` key of a writer whose children stay Unknown is never removed once the writer is replaced (one string per such writer; clear by stage prefix if it ever matters).
- `host_verify/freeze.rs::apply_passed`: a capture failure after a passing run (a harness-matching file with a control character created after the run) is filed as `infrastructure:`; the rerun hits the CLI pre-check with the real reason, at the cost of one budget slot.
- `orchestrator/signals/contract.rs::generate_contract_signal`: the contract signal embeds the adjudication feedback file whenever it exists and only the next adjudication apply clears it, so after `loom stage reset` a re-entered contract phase shows the earlier run's notice.
- `verify/contracts/refusal.rs` `Standing` has two variants (`Other`, `Frozen`) and works as a bool; `REFUSAL_FILE` is `pub` but used only inside the module (the hook mirrors the name in shell). `loom-hooks/commit-guard.sh::remind_contract_freeze` hard-codes "quiet for 5 minutes" while the rule is `REFUSAL_QUIET_SECS = 300`; nothing keeps them in step.
- `stage_service/dispute_kinds.rs`: module doc item 4 says an exhausted budget escalates the stage; for a freeze refusal `exhausted_response` only refuses and the tick step escalates after retiring the writer. The file is at 398 of 400 lines, so the next addition needs a split.
- `orchestrator/core/inbox_drain/freeze.rs::settle`: a failed refusal-record write settles the request refused and nothing files it; the design relies on the writer retrying or hung detection respawning it.

## Freeze-Refusal Verdict and Briefing Gaps

- `orchestrator/adjudication/verdict_freeze_plan.rs::check`: the lint diff compares message strings, so a patch that rewords an existing lint error counts as added (conservative).
- `verdict_freeze_refusal.rs::accept`: an accept patching only `contracts` never derives the new contracts' scaffold at recording (the lint run has `repo_root` `None`), so an unreachable re-pointed contract is recorded and applied, then costs the next freeze a refusal.
- `orchestrator/adjudication/verdict_freeze_refusal.rs::check_globs` inspects only files that exist at recording time: a glob over an empty directory is accepted and production files the writer creates there later are frozen. Consider re-running `harness_source_matches` at freeze.
- `orchestrator/adjudication/prompt/freeze_refusal_evidence.rs::fenced` re-implements `context::render::fence_for` and can emit a 4+ backtick fence that `prompt/truncate.rs::halve_section` (lines 62-76) does not handle; `halve_section` also finds its heading with `evidence.find(header)`, so a heading string inside an earlier writer-controlled fence makes an over-budget prompt shrink the wrong section (evidence loss, not label forgery).
- `apply_freeze_refusal.rs::amendment_line` repeats `apply_accept::field_name`; `scaffold_line` reaches the file through a three-hop re-export (`signals/mod.rs`, `prompt/freeze_refusal.rs`, `orchestrator/adjudication/prompt.rs`).
- Duplicated predicates that can drift: `stage_service/contracts.rs::check_changes` allowed-path rule (`is_contract_or_harness || scaffold.paths.contains`) repeated in `commands/stage/contracts/freeze.rs::check_changes` and `prompt/freeze_refusal_evidence.rs::recheck_section` (a `ScaffoldCheck::allows` would end it); `verify/contracts/completion.rs::collection_problems` `collection_problems` duplicates `StageTree::chain_problems`, and the prefix join and "adapter language is rust" test are repeated in `v2_lints/contracts.rs` (`join()` duplicates `collection::rust::join`) and `collection/mod.rs`; `commands/subagents/wait/codex_binding.rs::receipt_matches` repeats the receipt-to-`CodexExecution` mapping of `codex_lifecycle/direct.rs::direct_observation`.
- Newline splitting of error text: `fs/safe_fs.rs::portable_open_walk` escapes component bytes with `escape_ascii`, so that branch does not inject filename newlines into refusal problems. `validate_relpath` still interpolates the displayed path in validation errors; assess its callers separately before treating all safe-fs diagnostics as escaped.

## Contract Autonomy Test Gaps

- Untested: the lib-then-`src/main.rs`-then-path tie-break of `verify/contracts/collection/rust/scaffold.rs::rank`; a strict chain where the first candidate root fails and a later one is clean (`chain.rs::problems`); the `check_filer` arm for a stage with no current session; `daemon_record`'s drop of an unreadable `refused.json`; `keep_outcome` (`commands/stage/contracts/freeze.rs::keep_outcome`), the only producer of the local refusal file; the `ForwardState::TimedOut` arm of `codex_lifecycle/direct.rs::direct_observation` (only `Canceled` is covered); a direct codex record with a malformed turn id (`subagent_lifecycle/codex_evidence.rs::validate_terminal_fields`); an `apply_amendment` case that reloads a plan after a `harness` glob such as `**/fixtures/*.json` or `tests/x #1` to prove the YAML quoting (`plan/amendment/tests.rs::harness_globs_round_trip_verbatim`).
- `freeze_refusal_tests.rs::the_fourth_refusal_retires_the_writer_and_escalates` returns early as root, so it asserts nothing there. Seven `host_verify` tests and the `host_verification_e2e` deadline test skip silently when bubblewrap cannot start (`tests_freeze.rs::confinement_unavailable`): run the gate once with `LOOM_TEST_REQUIRE_SANDBOX_FREE=1` or outside a sandbox.
- Several tests outside `collection/` need the source graph but are ungated (`completion_tests`, `freeze_tests`, daemon `contracts_tests`, `signals/contract_tests`, `v2_lints/contracts_tests`, `apply_freeze_refusal_tests`), so `cargo test --no-default-features --lib` would fail them.
- `loom-hooks/tests/commit-guard-contract-session.sh`: the call-recording stub sees only invocations through `$LOOM_BIN` and `commit-guard.sh` references none; the output assertions at lines 100-105 are the effective guard.
- `format_gate::tests::a_formatted_file_gives_no_problem` fails inside the agent Bash sandbox (it runs `cargo fmt --check` confined, a nested sandbox); `orchestrator/core/run_tests.rs::a_manual_tick_files_a_refused_contract_freeze` is the first test to drive `run_tick` end to end, in a non-git temp dir under manual mode.

## Codex Direct-Mode Follow-Ups

- Token discovery (`codex_lifecycle/token_threads.rs::authorized_threads`) obtains authorization-correlated threads through companion jobs. `tokens/discover.rs::stage_threads` separately reads retained forward receipts, which can name direct-mode threads. A direct forward without a retained thread-bearing receipt has no companion discovery fallback.
- Inside the 1560 s receipt window, a `locate_job` error (ambiguous job, binding mismatch) reports Active instead of Unknown at once; telling them apart needs a typed error from `codex_lifecycle/jobs.rs`.
- A damaged receipt journal marks every authorization in the stage Unknown, companion rows whose job is healthy included (consistent with `commands/subagents/wait/codex_binding.rs::receipt_matches`); a companion row could still be classified from its job.

## Relayed Commands Run in the Background Again

`loom check`, `loom memory ...` and `loom stage commit` are relayed only when the Bash call is foreground, unredirected and bare. The mistake recurred in two stages of one plan (twice by the integration-verify main agent after the refusal-adjudication stage recorded it), on top of the earlier incident in [mistakes/memory-relay-drain-gap.md](../mistakes/memory-relay-drain-gap.md): each ended `refused: the request never reached the daemon` and cost a gate cycle. Prose has failed three times; proposal: a PreToolUse hook that denies `run_in_background: true` (and stdout redirection or a pipe) on a Bash command whose first loom subcommand is relay-eligible, naming the foreground rerun.
