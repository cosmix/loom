# Contract Autonomy Followups

> Chain, filing, test gaps

## Contract Runs the Chain Check Cannot See

The chain check proves which file rustc compiles under a contract's test name, not what else runs in that binary.

- A `macro_rules!` defined before a chain link in a file the implementer owns can shadow `assert!` for every descendant module, including the frozen file.
- A constructor (`#[ctor]`-style) in the same test binary can exit the process early while a shadow test passes.
- `.cargo/config.toml` runner overrides and `build.rs` can fake or alter a `cargo test` run.
- Revisit condition: pin each Rust contract run to the target the chain check resolved (run `cargo test --test <name>` or `--lib`/`--bin` for that root, with the config and build script frozen or refused).
- Inner attributes on a chain file are deny-listed (`cfg*`, unreadable path) while outer ones are allow-listed. On a nightly toolchain with `custom_inner_attributes`, a non-builtin inner attribute could rewrite the module (`collection/rust/chain.rs:244`). An allow-list would fail closed on both sides.
- `prefixes()` (`chain.rs:350`) lets the contract's test path extend past the frozen file's module path: a frozen file declaring `mod inner;` with test `inner::t` puts the test in an unfrozen file; a build failure at freeze counts as red, so it survives the freeze.
- A symlink at the second `CreateRoot` alternative (`scaffold.rs:203`) is invisible to the strict reader but its path is allowed, so rustc sees both files (E0761); the build failure counts as red at freeze.
- `run_dir_problems` runs at freeze only for contracts that set `cwd` (`collection/mod.rs:219`); a workspace-only `Cargo.toml` nested inside the package directory is the nearest marker but not the package manifest, and only completion catches it.
- A `cfg_attr` in any chain file, crate root included, is refused (`#![cfg_attr(not(test), no_std)]`, `#![cfg_attr(docsrs, ..)]`); the plan-writer skill documents the `tests/` workaround. Consider allowing a predicate that does not mention `test`.
- `harness.rs:23` `is_production_source` calls the test_support.rs and conftest.py helpers production source, so the harness warning is wrong for them (warn-only).
- `collection/mod.rs` `within()` silently drops scaffold edits outside `working_dir`, so the writer cannot satisfy the chain check and the refusal does not say why; `read_base` returns a base symlink's target text as file content while the worktree reader refuses symlinks.
- `changes.rs:78` `changed_paths` applies `listed_name` before dropping worktree-scaffold paths, while `harness_files` drops them first (line 202): an odd name under scaffolding would block a freeze; nothing pins the scaffold-first order of `listed_unless_scaffold`.
- `completion.rs:102` `run_contract` re-detects the adapter with `resolve_adapter` for `resolve_run` and `judge`, while `is_rust` uses the recorded one. Resolve once, recorded first, and share it.

## Freeze-Refusal Filing Gaps

- `stage_service/dispute_kinds.rs:197`: any `confirm()` error, I/O from `load_session_exact`/`load_freeze` included, becomes `Response::Error`, which `file_freeze_refusal` maps to `Declined`, and the tick marks the record declined for good. The pinned API reserves `Declined` for admission refusals and `Err` for I/O failure.
- `event_handler/freeze_refusal.rs:77`: a persistently failing filing is retried every tick with one warn per key and no counter; only the hung-session watchdog ends it. Consider a bounded attempt count.
- `freeze_refusal.rs:143`: a record marked declined is returned early for the same session, so a later local refusal by that writer is never recorded or filed; only a stderr line reports the decline.
- `freeze_refusal.rs:262`: `escalate_refused_freezes` prints its own REVIEW NEEDED line, and in daemon mode the monitor's `StageNeedsHumanReview` event prints it again and sends the desktop notification.
- `freeze_refusal.rs:228`: the `freeze-refusal-children` key of a writer whose children stay Unknown is never removed once the writer is replaced (one string per such writer; clear by stage prefix if it ever matters).
- `commands/stage/human_review.rs:99`: `handle_approve` resets the contract respawn budget but not `tally.freeze_refusal_disputes`, so after the fourth-refusal escalation an approved stage's fresh writer escalates on its first refusal.
- `host_verify/freeze.rs:317`: a capture failure after a passing run (a harness-matching file with a control character created after the run) is filed as `infrastructure:`; the rerun hits the CLI pre-check with the real reason, at the cost of one budget slot.
- `orchestrator/signals/contract.rs:97`: the contract signal embeds the adjudication feedback file whenever it exists and only the next adjudication apply clears it, so after `loom stage reset` a re-entered contract phase shows the earlier run's notice.
- `verify/contracts/refusal.rs:138` `Standing` has two variants (`Other`, `Frozen`) and works as a bool; `REFUSAL_FILE` is `pub` but used only inside the module (the hook mirrors the name in shell). `loom-hooks/commit-guard.sh:300` hard-codes "quiet for 5 minutes" while the rule is `REFUSAL_QUIET_SECS = 300`; nothing keeps them in step.
- `stage_service/dispute_kinds.rs:22`: module doc item 4 says an exhausted budget escalates the stage; for a freeze refusal `exhausted_response` only refuses and the tick step escalates after retiring the writer. The file is at 398 of 400 lines, so the next addition needs a split.
- `inbox_drain/freeze.rs:42`: a failed refusal-record write settles the request refused and nothing files it; the design relies on the writer retrying or hung detection respawning it.

## Freeze-Refusal Verdict and Briefing Gaps

- `verdict_freeze_plan.rs:54`: the lint diff compares message strings, so a patch that rewords an existing lint error counts as added (conservative).
- `verdict_freeze_refusal.rs:149`: an accept patching only `contracts` never derives the new contracts' scaffold at recording (the lint run has `repo_root` `None`), so an unreachable re-pointed contract is recorded and applied, then costs the next freeze a refusal.
- `verdict_freeze_refusal.rs:279` `check_globs` inspects only files that exist at recording time: a glob over an empty directory is accepted and production files the writer creates there later are frozen. Consider re-running `harness_source_matches` at freeze.
- `prompt/freeze_refusal_evidence.rs:310` `fenced()` re-implements `context::render::fence_for` and can emit a 4+ backtick fence that `prompt/truncate.rs::halve_section` (lines 62-76) does not handle; `halve_section` also finds its heading with `evidence.find(header)`, so a heading string inside an earlier writer-controlled fence makes an over-budget prompt shrink the wrong section (evidence loss, not label forgery).
- `apply_freeze_refusal.rs:134` `amendment_line` repeats `apply_accept::field_name`; `:36` `scaffold_line` reaches the file through a three-hop re-export (`signals/mod.rs`, `prompt/freeze_refusal.rs`, `prompt.rs:36`).
- Duplicated predicates that can drift: `stage_service/contracts.rs:326` allowed-path rule (`is_contract_or_harness || scaffold.paths.contains`) repeated in `commands/stage/contracts/freeze.rs:59` and `prompt/freeze_refusal_evidence.rs:259-270` (a `ScaffoldCheck::allows` would end it); `completion.rs:148` `collection_problems` duplicates `StageTree::chain_problems`, and the prefix join and "adapter language is rust" test are repeated in `v2_lints/contracts.rs` (`join()` duplicates `collection::rust::join`) and `collection/mod.rs`; `commands/subagents/wait/codex_binding.rs:257` repeats the receipt-to-`CodexExecution` mapping of `codex_lifecycle/direct.rs:127-130`.
- Newline splitting of error text: `fs/safe_fs.rs:223` `portable_open_walk` and `:77` `validate_relpath` interpolate raw or lossy components into errors that `problem_lines` splits on newlines (needs macOS or an old kernel plus a mid-walk swap; same class as accepted git stderr in daemon `Response::Error` text; escaping with `special_walk::escaped` would close it).

## Contract Autonomy Test Gaps

- Untested: the lib-then-`src/main.rs`-then-path tie-break of `scaffold.rs:63`; a strict chain where the first candidate root fails and a later one is clean (`chain.rs:62`); the `check_filer` arm for a stage with no current session; `daemon_record`'s drop of an unreadable `refused.json`; `keep_outcome` (`commands/stage/contracts/freeze.rs:254`), the only producer of the local refusal file; the `ForwardState::TimedOut` arm of `codex_lifecycle/direct.rs:114` (only `Canceled` is covered); a direct codex record with a malformed turn id (`codex_evidence.rs:176`); an `apply_amendment` case that reloads a plan after a `harness` glob such as `**/fixtures/*.json` or `tests/x #1` to prove the YAML quoting (`plan/amendment/tests.rs:159`).
- `freeze_refusal_tests.rs:264` returns early as root, so it asserts nothing there. Seven `host_verify` tests and the `host_verification_e2e` deadline test skip silently when bubblewrap cannot start (`tests_freeze.rs:34`): run the gate once with `LOOM_TEST_REQUIRE_SANDBOX_FREE=1` or outside a sandbox.
- Several tests outside `collection/` need the source graph but are ungated (`completion_tests`, `freeze_tests`, daemon `contracts_tests`, `signals/contract_tests`, `v2_lints/contracts_tests`, `apply_freeze_refusal_tests`), so `cargo test --no-default-features --lib` would fail them.
- `loom-hooks/tests/commit-guard-contract-session.sh:35`: the call-recording stub sees only invocations through `$LOOM_BIN` and `commit-guard.sh` references none; the output assertions at lines 100-105 are the effective guard.
- `format_gate::tests::a_formatted_file_gives_no_problem` fails inside the agent Bash sandbox (it runs `cargo fmt --check` confined, a nested sandbox); `orchestrator/core/run_tests.rs:85` is the first test to drive `run_tick` end to end, in a non-git temp dir under manual mode.

## Codex Direct-Mode Follow-Ups

- Token discovery (`codex_lifecycle/token_threads.rs::authorized_threads`, used by `tokens/discover.rs`) finds codex threads only through companion jobs, so direct forwards contribute no token threads.
- Inside the 600 s receipt window, a `locate_job` error (ambiguous job, binding mismatch) reports Active instead of Unknown at once; telling them apart needs a typed error from `codex_lifecycle/jobs.rs`.
- A damaged receipt journal marks every authorization in the stage Unknown, companion rows whose job is healthy included (consistent with `codex_binding.rs:244`); a companion row could still be classified from its job.

## Relayed Commands Run in the Background Again

`loom check`, `loom memory ...` and `loom stage commit` are relayed only when the Bash call is foreground, unredirected and bare. The mistake recurred in two stages of one plan (twice by the integration-verify main agent after the refusal-adjudication stage recorded it), on top of the earlier incident in [mistakes/memory-relay-drain-gap.md](../mistakes/memory-relay-drain-gap.md): each ended `refused: the request never reached the daemon` and cost a gate cycle. Prose has failed three times; proposal: a PreToolUse hook that denies `run_in_background: true` (and stdout redirection or a pipe) on a Bash command whose first loom subcommand is relay-eligible, naming the foreground rerun.
