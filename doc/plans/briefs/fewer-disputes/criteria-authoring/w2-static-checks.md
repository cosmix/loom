# criteria-authoring W2: the freeze static gate and the plan-time wiring rules

Tier: sonnet. Wave 2, with W1a, W1b and W3. Read `common.md` first (the ledger table matters for
`validation.rs`). Frozen contracts: `tests/freeze_static_gate_contracts.rs`, `tests/plan_lints_contracts.rs`
(never edit them).

## Files you own

`src/verify/contracts/format_gate.rs`, `src/verify/contracts/format_gate_tests.rs` (new),
`src/verify/contracts/freeze.rs`, `src/host_verify/freeze_format.rs`, `src/host_verify/freeze_format_tests.rs` (new),
`src/host_verify/digest.rs`, `src/plan/schema/validation.rs`, `src/plan/schema/validation/v2_fields.rs`,
`src/plan/schema/validation/v2_lints/mod.rs`, `src/plan/schema/validation/v2_lints/contracts.rs`,
`src/plan/schema/validation/v2_lints/wiring_definition.rs` (new),
`src/plan/schema/validation/v2_lints/wiring_definition_tests.rs` (new).

## Part 1: the static gate at freeze (Decision 2)

Today `check_formatting` (`verify/contracts/freeze.rs`) runs the stage's formatter-check acceptance criteria
through `format_problems`; any failure refuses the freeze. A refusal is a host-job outcome `format`: the job
fails and `apply_freeze_result` (W1b) decides what happens to it: a job whose only red outcome is `format`
(a static refusal) is returned to the live writer session with this outcome's summary as the failing output, and
it is filed as a `freeze-refusal` dispute on the third static refusal in the session, or when the writer stays
quiet 5 minutes after a refusal; any other failure is filed at once. You widen what that outcome checks and keep its summary complete: the summary is what the writer
reads in `Host job: failed`, so it must hold every problem (each with its quoted output) and nothing else. The
outcome label stays `format`, and `format` fails only through `check_formatting`'s refusal (a formatter check, or
a pure check that names a writer file); a gate that could not run stays an error of the run, not a failed
`format` outcome.

### `format_gate.rs` (keep `is_format_check` and `format_problems` exactly as they behave; move the existing
`#[cfg(test)] mod tests` into `format_gate_tests.rs` with `#[path]` so the file stays under 400 lines)

Add, using the same helpers (`testrun::recognize::{command_args, has_flag, invocations}`):

```rust
pub fn is_pure_check(command: &str, cwd: &Path) -> bool;
pub fn runs_at_freeze(command: &str, cwd: &Path, contracts_compile: bool) -> bool;
pub fn attribute_failure(output: &str, written: &[String]) -> Vec<String>;
pub(crate) fn static_problems(stage: &Stage, worktree_root: &Path, working_dir: &Path,
    confinement: CommandConfinement, written: &[String], contracts_compile: bool) -> Result<Vec<String>>;
```

- Kinds of simple command (per `invocations` argv): `Format` (what `is_format_check_argv` accepts today),
  `Syntactic` (`eslint` without `--fix`; `biome lint` without `--write`, `--fix`, `--apply`; `ruff check` without
  `--fix` or `--unsafe-fixes`), `Compiled` (`cargo clippy` without `--fix`; `tsc` with `--noEmit`; `go vet`;
  `golangci-lint run` without `--fix`), `Search` (`rg` or `grep`, optionally behind a leading `!`; read how
  `invocations` tokenises `!` and `plan/schema/validation/base_tree.rs::Search::parse` for the simple form, and
  accept the same simple commands). `is_pure_check` is true when the command expands to at least one simple
  command and every one is one of these kinds (a compound `cargo clippy && cargo test` is not pure, as for
  formatters). Autofix forms are never pure: they rewrite files the freeze is about to hash.
- `runs_at_freeze` = `is_pure_check` and, when any simple command is `Compiled`, `contracts_compile`. A red
  contract that does not build makes a compiler-backed lint report the missing API in the contract file, which
  is the expected red state, not a defect.
- `attribute_failure(output, written)`: the entries of `written` that `output` names as a whole path, sorted and
  deduplicated. `written` holds each file in its relative forms (working-directory- and worktree-root-relative, as
  `written_paths` builds them) and its absolute (worktree-joined) form, and an output path matches an entry in
  any of them; the returned list is the matching entries. An entry matches where it appears with no `[A-Za-z0-9_./-]` before it and no `[A-Za-z0-9_-]`
  after it. Callers pass each written file in its working-directory-relative and absolute forms.
- `static_problems`: over the stage's `acceptance` criteria and `after_stage` checks (a `TruthCheck` is an
  `AcceptanceCriterion::Extended`; convert it, keeping a missing `timeout_secs` at `DEFAULT_CHECK_TIMEOUT` so
  a truth check is not given the acceptance runner's longer default), keep those with `runs_at_freeze` that
  are NOT format checks (`format_problems` keeps those), run them as `format_problems` runs its checks (stage
  `setup` prefix, `run_acceptance_with_config`, `with_tree_root`, one stage), and for each that failed: when
  `attribute_failure(stdout + stderr, written)` is non-empty, one problem:
  ``acceptance criterion `<cmd>` fails on a file the contract session wrote (<paths>): <output>``, with the
  output cut to 2 KiB on a char boundary and quoted with `crate::verify::contracts::refusal::run_output_line`
  (it is attacker-influenced text); when it is empty, no problem (a failure that names no writer file is left
  to completion). A timed-out check gives no problem. Label `after_stage` entries `after_stage criterion`.

### `freeze.rs`

- `written_paths(site, repo)`: the changed paths since the stage base (`stage_base`, `changed_paths`, as
  `check_site_changes` computes them) plus every contract file and every existing harness file
  (`changes::harness_files`), each as the working-directory-relative path, the worktree-root-relative path
  (`site.working_dir_prefix()`) and the absolute path; sorted, deduplicated.
- `check_formatting(site, confinement, written, contracts_compile)`: `format_problems(..)` then `static_problems`;
  `refuse(..)` as today with the suffix text updated to say the formatter OR the stage's lint and search checks.
- `judge_run`: the `contract.test` in the `NotSelected` message is an `Option` now: `contract.test.as_deref()
  .unwrap_or("")`.

### `host_verify/freeze_format.rs`

`format_outcome(site, git, confinement, written, contracts_compile)` (`git: &WorktreeGit`, the pinned git of the
job's site; W1b passes `&site.git`) passes `written` and `contracts_compile` through. The success summary becomes
`every contract and harness file passes the stage's formatter, lint and search checks`.

**The gate's own writes must not break the binding.** Grounded: the freeze job captures a binding (HEAD, tree
digest, file hashes) before the run and compares it with a second capture after `format_outcome`
(`host_verify/freeze.rs::run`, `ContractReports::changes`). `host_verify/digest.rs` hashes every
`git status --porcelain=v2 --untracked-files=all` entry, so an untracked file that is not git-ignored and that a
lint wrote (for example `tsconfig.tsbuildinfo` from `tsc --noEmit` with `incremental`) changes the digest: the
binding check fails the job with a non-static error, which files a dispute, and the file would also fail the
daemon's `check_changes` at application (it is not a contract or harness file). The smallest safe design:
- `digest.rs` gains `pub(super) fn untracked_in(git: &WorktreeGit) -> Result<BTreeSet<String>>`: the paths of the
  entries `entries(&porcelain(git, None)?)` marks `untracked`, minus those `completion_scaffold` skips (exactly
  the untracked paths the digest hashes), relative to the worktree root. Reuse `porcelain` and `entries`; no new
  git command.
- `format_outcome` takes `before = digest::untracked_in(git)?`, runs `check_formatting`, then ALWAYS (also when
  it refused) removes every path in `untracked_in(git)? - before` with `std::fs::remove_file` joined to
  `site.worktree_root` (a path git reports never goes through a symlink, `remove_file` does not follow a final
  symlink, and a path holding `..` or an absolute path is skipped), ignoring a file already gone. Empty
  directories stay; git and the digest ignore them. Only files that were not there before the gate are removed;
  a change the gate makes to a tracked file stays and fails the binding, which is right because the freeze
  hashes that file. A removal failure is an error of the run, not a refusal. Then it returns the gate's result.
- Test (`freeze_format_tests.rs`, git fixture in a `TempDir`, no confinement needed): a stub that creates
  `tsconfig.tsbuildinfo` between the two listings (factor the remove step into
  `remove_created(git, root, &before)` so the test calls it directly) leaves `digest::tree_digest_in` equal to its
  value before; a file untracked before the gate survives; a git-ignored file and a modified tracked file are
  untouched; the refusal path also cleans.

W1b's `host_verify/freeze.rs` calls `written_paths` and `format_outcome`; the test callers of `check_formatting`
(`commands/stage/contracts/freeze_tests.rs`, W0b's file) are updated by the orchestrator if they break; report
the call sites you see.

## Part 2: plan-time rules (Decisions 5 and 6)

### `v2_lints/wiring_definition.rs` (new) and `v2_lints/mod.rs`

`pub(super) fn check_definition_shaped(ctx: &LintContext<'_>, out: &mut Vec<LintFinding>)`, registered in
`run` beside `regex_patterns::check`. For every stage's `wiring` entry (and nothing else) it reports, as
`LintFinding::in_stage(stage, message, true)` (an error in a v2 plan, a warning in v1 through
`split_lint_findings`), a pattern that starts with a definition keyword. A private
`definition_keyword(pattern: &str, literal: bool) -> Option<&'static str>`:

1. Skip leading whitespace; in a regex pattern (`literal: false`) also skip a leading `^` and the tokens `\s*`, `\s+`
   and `\s`; in a literal pattern `^` and `\` are ordinary characters and stop the skip.
2. Skip any number of modifiers: `pub`, `pub(crate)`, `pub(super)`, `export default`, `export`, `async`, each followed
   by whitespace (or, in a regex pattern, `\s+`, `\s*`, `\s`, or ` +`); in a regex pattern also accept the escaped
   forms `pub\(crate\)` and `pub\(super\)`.
3. The next word must be one of `fn`, `mod`, `struct`, `enum`, `trait`, `impl`, `type`, `def`, `class`, `function`,
   `interface`, followed by whitespace (a space, or the regex tokens above). `fn(`, `class=`, `type:`, `typeof`,
   `functionName(` and `impl<T>` do not match; `pub terminals: bool` does not match (`terminals` is no keyword).
4. Message: ``Wiring #<n> pattern `<pattern>` starts with the definition keyword `<kw>`, so it can only match where
   a name is declared, never where it is used; point it at the call, mount or dispatch site, prove the reach with
   `reachable`, or declare `consumer_deferred` when the consumer arrives in a later plan``. Cut the pattern to 80
   characters in the message.

Completion's definition-site rule (`verify/goal_backward/definition_sites.rs`) stays.

### `validation.rs` (ledger: net zero lines in the file and in `validate` and `check_cross_stage_wiring_coverage`)

- The leaf rule (`validate`, the `Standard leaf stage has artifacts but no wiring or wiring_tests` block): replace
  the two lines `&& stage.wiring.is_empty()` and `&& stage.wiring_tests.is_empty()` by exactly two lines:
  `&& stage.consumer_deferred.is_none()` and `&& !stage.has_wiring_coverage()` (W0 adds the helper). Reword the
  error message inside its existing lines so it says `no wiring, wiring_tests, reachable or consumer_deferred`.
- `check_cross_stage_wiring_coverage`: `if !stage.wiring.is_empty() || !stage.wiring_tests.is_empty() {` becomes
  `if stage.has_wiring_coverage() || stage.consumer_deferred.is_some() {` (one line to one line); reword its warning
  text on its existing lines. The `any_iv_has_wiring` computations stay.
- Run `wc -l` before and after; both numbers must match the ledger rows in `common.md`.

### `v2_fields.rs` and `v2_lints/contracts.rs`

- `push_contract_problems`: the `("test", contract.test.as_str())` pair leaves `push_blank_fields`; add: a `test`
  that is `Some` and blank is `Contract #n test must not be blank when set`.
- `v2_lints/contracts.rs` `check_collection`: a contract with `test: None` is skipped by the scaffold derivation
  (`let Some(test) = contract.test.as_deref() else { continue; }`); the writer's test is not known at plan time.
  Any other `contract.test` read in these two files gets the same treatment.

## Traps

- `format_problems` is public API (`tests/completion_gates_contracts.rs` calls it with its current signature).
- Free-text from tool output is untrusted: only `run_output_line`-quoted text goes into a problem.
- Do not widen `is_format_check`: `verdict_freeze_refusal.rs` and `freeze_refusal_evidence.rs` (W3) decide which
  criteria an adjudicator may replace and call `is_pure_check` after this change.
- `Search` commands that pass at freeze or fail with no output never produce a problem; do not log them.

## Tests

`format_gate_tests.rs`: every existing test moved intact; `is_pure_check` for each listed lint and search form and
each autofix near miss (`cargo clippy --fix`, `eslint --fix`, `biome check --write`, `ruff check --fix`, `cargo
clippy && cargo test`, `tsc` without `--noEmit`, `cargo build`); `runs_at_freeze` with and without compiling
contracts; `attribute_failure` (whole path, substring neighbour `my_tests/foo.rs` vs `tests/foo.rs`, absolute form,
no match); `static_problems` end to end on a `TempDir` with `ruff check` is not portable, so use `rg` and `grep`
negated forms (`! rg -n 'FIXME' tests/`) over a written file (skip with `skip_unless(confined_commands_runnable(),
..)`): a hit in a written file is one problem, a hit in an unwritten file is none.
`wiring_definition_tests.rs`: the keyword table above, v1 warning versus v2 error through
`crate::plan::schema::validate` plus `split_lint_findings`, and the leaf rule (`consumer_deferred`, `reachable` and
`wiring` each satisfy it; none of them fails it) and the coverage warning.

## Done

Your one check once the crate compiles: `cargo test --lib plan::schema::validation::v2_lints::`.
