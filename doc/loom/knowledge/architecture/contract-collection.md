# Contract Collection

>

## Contract Collection: Scaffold, Chain Check and Target Model

Completion runs a Rust contract by test name only (`cargo test <name> -- --exact`), so a frozen file counts
only if rustc compiles it under that name. `verify/contracts/collection/` (`mod.rs`, `harness.rs`, `rust/`)
proves that from the Cargo target model and the `mod` chain, reading the stage base through `git cat-file
blob` (never `git show`) and the worktree through bounded reads that follow no symlink. Without the
`source-graph` feature every Rust contract is refused (`NO_SOURCE_GRAPH`): the chain cannot be proven without
parsing Rust.

**Scaffold.** `derive_scaffold(base, [(file, test)])` computes, from the stage base alone, the edits that put
each contract file into a test target: `ScaffoldEdit::Declare` (one out-of-line `mod x;` added to an existing
file) and `ScaffoldEdit::CreateRoot` (exactly one of `<dir>/mod.rs`, `<dir>.rs`, holding only declarations).
The root needing the fewest edits wins; ties go to the lib, then `src/main.rs`, then path order
(`scaffold.rs::rank`). The contract's own module declaration is `test_only` (the signal suggests
`#[cfg(test)]`); a module some contract needs outside tests is not. An edit wholly outside the stage
`working_dir` is dropped (`within`), so the writer cannot satisfy the check for it and the refusal does not
name the dropped path. `stage_scaffold` is the one entry the signal, the freeze and the adjudication prompt
share; a contract whose chain cannot be derived is an error, and `plan verify` lints it over HEAD
(`v2_lints/contracts.rs`). The derivation walks leniently: it tolerates only a missing declaration or module
file, so any other broken link (a base root starting `#![cfg(not(test))]`) makes the scaffold an `Err`, never a
clean diff. A test of "the diff accepts but the chain check refuses" therefore puts the offending line in a
non-scaffold, harness-allowed chain file.

**Strict diff** (`scaffold_problems(edits, base, current)`, run first by `freeze_scaffold`): the worktree may
differ from the base only by those edits. Any other changed line is reported (20 lines named per file, the rest
counted), a reordered declaration or a declaration inserted directly under a kept `#[..]` line of the base is
refused (otherwise `#[cfg(test)]` or `#[allow]` moves from a base item onto the new `mod` line while the
line-for-line comparison still matches), and a created root must be exactly one of its alternatives. Only when
the diff is clean does the chain check run.

**Chain check** (`chain_problems(source, file, test)`): some libtest test target of the owning package reaches
the file through plain `mod` links, and the test path starts with the file's module path under that root.
Passing requires ONE clean candidate root; otherwise the problems of the chain that got furthest are reported.

- A link is one out-of-line `mod name;`. Its outer attributes may be `#[cfg(test)]`, `#[allow(..)]`,
  `#[expect(..)]` and doc comments; `#[path]`, `#[cfg_attr]`, any other `cfg` and any attribute with an
  unreadable path are refused.
- A chain file may carry no inner attribute that cfg's it out: of the cfg attributes only `#![cfg(test)]`
  passes, any `#![cfg_attr(..)]` is refused (crate roots included; a contract behind one belongs in a `tests/`
  file, see `skills/loom-plan-writer/references/v2-contracts.md`), and an inner attribute whose path cannot be
  read, or a shebang, is refused. A file that does not parse is refused.
- A module needs exactly one file (the flat file XOR the directory's `mod.rs`); a root's children and a `mod.rs` file's live in its
  own directory, any other module file keeps them in a directory named after it (the rustc rule, not the simplified one in the plan
  prose).
- Test target model (`targets.rs`, `Cargo.toml` read with the `toml` crate, no `cargo metadata`, which spawns
  a process and can write `Cargo.lock`): lib, `src/main.rs`, `src/bin/*`, `tests/*`, `[[bin]]` and `[[test]]`
  entries, `autobins`/`autotests`. Auto roots under `tests/` and `src/bin/` are only those the contract file's
  own path names (`tests/<n>.rs`, `tests/<n>/main.rs`), because a `FileSource` cannot list directories, so a
  contract in `tests/support/util.rs` reached only from an auto root `tests/it.rs` is refused unless that root is
  a `[[test]]` entry. A model miss can only refuse, never pass. A root with `test = false`, `harness = false`
  or non-empty `required-features` is not a libtest target and is named as the reason.
- `run_dir_problems(source, file, run_dir)`: the nearest `Cargo.toml` at or above the run directory must be the
  owning package's manifest and must select it (a workspace root whose `default-members` leave the package out
  does not). The freeze checks it only for contracts that set `cwd`; completion checks it for the directory
  the contract runs from (`collection_problems`).

**Parsing gotchas** (`rust/items.rs`): tree-sitter-rust 0.24 lexes `#!/**/[..]` and `#! [..]` at a file's start
as a shebang node while rustc reads both as inner attributes, and comments are named-child extras that shift
positional `named_child(0)` reads. Read nodes by kind or field, never by position; normalise `r#` per path
segment; treat an unreadable path or a shebang as refused. See
[mistakes/verification-v2-delivery.md](../mistakes/verification-v2-delivery.md) for the positional-read bypass.

**Harness hygiene.** `harness_source_matches(harness, files)` finds existing files a harness glob matches that
are production source (a parsed language, no test-file convention). `plan verify` warns over HEAD; the
freeze-refusal verdict refuses such a glob at recording. A harness is for test-only files; module wiring comes
from the scaffold. The test_support.rs and conftest.py helpers count as production source for this warning.

## Contract Trial

`loom check <stage> --contracts [--replacement <frozen-file>=<path> ...]` runs a stage's frozen contracts where completion runs: a Check host job, confined, calling `verify::contracts::completion::check`. A full `loom check <stage>` of a standard stage with a freeze runs them after the goal-backward checks. The selection rides as `JobRecord.contracts` plus a `replacements.json` job file, and `host_verify::submit::submit_contracts(work_dir, request, replacements, now)` is the entry point; `SubmitRequest` gained no field. The point is that a contract unable to run under the host job (a confined command launched from inside it) is found by the agent before completion, not after six disputes ([the incident](../mistakes/completion-runtime.md#confined-contract-tests-looped-through-six-accepted-disputes-2026-11-20)).

### Replacement trial

With `--replacement`, `verify/contracts/trial.rs::with_admitted_overlay` lays each replacement over its frozen file in the worktree, under the host-job stage lock taken by `child_contracts::admit_overlay` (the snapshot of the original bytes is taken after admission). The job runs the freeze's format and static gates on those files, then the contracts, with `CachePolicy::Bypass` (`completion::run`), and puts the frozen files back.

- **Receipt.** A pass writes `trial.json` (`TrialReceipt`: HEAD, plus a `TrialFile` of path and sha256 per replacement). `dispute_admission::check_trial` refuses a filing whose replacements are not covered by a receipt at the worktree's current HEAD (`trial::receipt_covers`); the adjudicator prompt reuses `receipt_files_cover` (`prompt/contract_replacement.rs`). A receipt is not tied to the freeze generation or to uncommitted implementation edits, and is lost once newer applied jobs push its job directory out of `KEPT_JOB_DIRS` (3).
- **Restore.** The final restore takes the stage lock and writes a file back only while it still holds the overlay bytes and the freeze does not record those bytes: an accepted ruling usually writes the very bytes the trial ran, so "the file still holds the overlay" alone would revert a legitimate accept. `frozen_restore.rs::undo_trial_overlay` writes the frozen copy, not the pre-trial bytes, which is safe because every gate refuses drift.
- **A killed trial.** A SIGKILLed trial child is undone daemon-side in `host_verify/signals.rs::kill_groups` through `child_contracts::undo_stopped_overlay`, and `recover_after_restart` undoes the overlay of a trial that was Running at restart. The restore is not spared from the watchdog.
- **Known leaks.** A SIGKILL mid-publish leaves a `.<name>.<rand>.tmp` beside the file (`fs/safe_replace.rs`); a superseded trial killed before its first confined spawn leaves its overlay until completion's drift check names it.

### Environment-red refusal

`verify/contracts/freeze.rs::judge_run` refuses a red whose output carries an environment marker (`verify/environment.rs::environment_marker`: `LAUNCHER_MARKERS` `bwrap:` and `operation not permitted`, then `HOST_MARKERS`: name-resolution and network failures, `connection refused`, `read-only file system`, `no space left on device`; the denial markers `eperm`, `sandbox_apply`, `deny(1)` and `couldntreadcurrentdirectory` are read only by `sandbox_denial` and `run_denial`, so the freeze and `dry_run_classify.rs` miss them), because such a red proves nothing about the implementation. `BuildFailed` stays accepted. `judge_output` (beside `judge_run`) carries the output text to the check. The marker applies even when the runner's summary parsed, so a contract whose expected failure message contains `connection refused` cannot be frozen; `plan verify --dry-run` (`dry_run_classify.rs`) treats the same markers as assertion text once a summary parsed. The writer's rules state the run environment: no network, no nested OS sandbox, writes only in the tree and TMPDIR. A contract about confinement asserts on the generated policy (`verify::criteria::confined_policy`) and launches nothing.

A trial with `--replacement` re-queues a new host job on every call and each submit cancels the previous one, so a second trial within 120 s returns a result only while its payload stays identical.

Limit: when the red is a build failure the freeze cannot tell the contract cannot run where completion runs; the recourse is the trial and `--replacement`.

## Criterion Narrowing, Base Evidence and Hand-Offs

An adjudicator may delete or narrow an acceptance criterion that cannot pass whatever the implementation, but only with evidence loom produced, and what it leaves out is handed to a later check. The stage agent never narrows its own checks.

### Base run

`loom check <stage> --criterion <n> --at-base` (a session relays it; `commands/verify.rs` refuses it without a session, `AT_BASE_NEEDS_SESSION`) runs the criterion confined in a checkout of the stage's base commit, with the stage's build cache (`host_verify/child_base.rs`). The result is a `BaseEvidence` in `reviews/<stage>/base-evidence.json` (`verify/review/handoffs.rs`): `index`, `command`, `base` (the commit it ran at), `passed`, `executed`, `failed`, `build_failed`, `timed_out`. A later run of the same index and command replaces the earlier one.

The checkout lives in the stage cache (`stage_cache/base_checkout.rs`), so a second base run is warm:

- Path `<cache base>/loom/stage-base/<stage cache name>/<base>`, mode 0700, opened no-follow; the build cache is its sibling `cache-<base 12 hex>`, not nested under `<base>/`.
- Filled and judged clean with `GIT_DIR=<repo common dir>`, `GIT_WORK_TREE=<checkout>` and a private `GIT_INDEX_FILE`; git is never discovered inside the checkout, whose `.git` an agent could plant.
- Ignored build output is kept on reuse (no `git clean -fdx`); `ensure_base_dirs` prunes the checkouts and caches of every other base when one is made; `stage_cache/cleanup.rs::remove_stage_cache` runs both removals and reports the first error.
- Impact-test selections run with per-command scratch caches, not the stage cache (`verify/impact_tests.rs::criteria_config`), so a base provision does not warm what an impact base run reads.

### Verdict rules

`plan_patch.leaves_out: [{index, tests, runs_at, reason}]` accompanies a `delete` or a narrowing `replace` of `acceptance` (`orchestrator/adjudication/verdict_narrowing.rs::check_narrowing`, run at recording by `validate_in` only when `plan_patch.field` is `acceptance`, and again at apply time). `runs_at` is `integration-verify`, `ci` or `nowhere`.

- A `delete` needs a `leaves_out` entry; an entry needs a patch that amends its index.
- `check_run` demands base evidence for the criterion that failed with tests executed. A criterion that passes at the base, does not build there, executes zero tests, or fails fewer tests than the entry lists, is the stage's own failure: the judge is told to answer `needs-more-evidence` (or, for a frozen contract, to have the agent file `dispute-contract` with a replacement).
- `at_current_base` keeps only runs whose `base` equals the stage's base read now (`locate_stage_site` and `stage_base`), at record time and again at apply time; an unreadable base refuses the narrowing.
- A `replace` equal to the current entry is refused (`same_entry` compares the value parsed as an `AcceptanceCriterion`).
- A refusal at apply time routes to `apply_needs_more_evidence` with `EVIDENCE_ACTION` and amends nothing (`apply_accept.rs`).

Limits that stay: adapters report counts, not test names, so a base run that fails one pre-existing test lets a `leaves_out` entry name the stage's own new test; a `delete` is checked by failed count against the tests listed, not against the number executed; a narrowing the judge does not declare in `leaves_out` is not detected mechanically, only listed.

### Hand-offs

Each accepted entry becomes a `Handoff` (`stage`, `field`, `index`, `tests`, `runs_at`, `reason`, `dispute`, `verified`) in `reviews/<stage>/handoffs.json`. `verified` is false for a timed-out or unparsed base run and for a deleted contract. They are computed against the stage's list before the amendments and recorded after (`apply_accept::narrowing_handoffs`, `apply_contract_rulings::amend`); `record_handoffs` dedups on replay and `forget_handoffs` undoes them when the amendment does not land. Readers: the integration-verify signal (`orchestrator/signals/handoffs.rs`), the plan completion summary (`commands/status/render/summary.rs`) and the status TUI (`commands/status/ui/tui/handoffs.rs`), which list every hand-off and every amendment of the plan.

A deleted contract is handed off the same way: with no confined formulation of a contract, the adjudicator deletes it and the deletion is recorded.
