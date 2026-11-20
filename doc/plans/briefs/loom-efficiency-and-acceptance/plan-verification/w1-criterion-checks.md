# W1 — criterion hazard lints and base-tree evaluation

Tier: opus (`loom-senior-software-engineer`). Read `../common.md` first.

## Goal

`loom plan verify` rejects acceptance criteria that cannot pass in a stage sandbox, and warns on
grep criteria that already pass on the untouched tree. Evidence: report section 4.1 (20% of
evaluable criteria were green before the work; 57 network, 15 `HOME=`, 25 `vitest -t` instances).

## Files you own (write)

- `loom/src/plan/schema/validation/acceptance_command.rs`
- `loom/src/plan/schema/validation/criterion_hazards.rs` (new)
- `loom/src/plan/schema/validation/base_tree.rs` (new)
- `loom/src/plan/schema/validation.rs` (call sites only)
- `loom/src/commands/plan/verify.rs`
- `loom/src/plan/schema/tests/acceptance_tests.rs` and new test files beside it

Read-only: `loom/src/git/runner.rs`, `loom/src/models/stage/types.rs`,
`loom/src/plan/schema/structural_checks.rs` and `structural_checks/` (W2 owns those).

## Where things are

- `validate_acceptance_criterion` — `acceptance_command.rs:17-44`. Three rules today. Its only
  call site is `validation.rs:367`, inside `validate()` (`validation.rs:228`), looping
  `stage.acceptance`. It is not called for `setup` or `wiring_tests` commands.
- Errors: `validate()` returns `Vec<ValidationError { message, stage_id }>`. Warnings: plain
  strings from `validate_structural_preflight(stages, repo_root)` (`validation.rs:773`).
- `commands/plan/verify.rs:462-469` buckets warnings into `JsonWarnings { structural, knowledge,
  sandbox }` (`verify.rs:35-53`). `find_repo_root` (`verify.rs:89-100`) may return `None`:
  plan verify must keep working with no git repository.
- `AcceptanceCriterion` — `models/stage/types.rs:427-432`, untagged `Simple(String)` or
  `Extended(TruthCheck)`; `.command()` gives the string; `TruthCheck.exit_code: Option<i32>`.
- `run_git_checked(args, repo_root) -> Result<String>` — `git/runner.rs:79`. No `git grep`
  helper exists; do not add one.

## Steps

1. Hazard lints, in `criterion_hazards.rs`, called for every `acceptance` command, every
   `wiring_tests[].command` and every `setup` command. Tokenise with the same care the hooks use:
   a hazard inside a quoted string argument to `rg` is not a hazard.
   Errors: `|| true` (or `|| :`) anywhere; `HOME=` assigned from a variable or command
   substitution; bare `mktemp -d` that is not of the form `mktemp -d "${TMPDIR:-/tmp}/..."`.
   Warnings: a network binary in command position (`curl`, `wget`, `gh`, `npm install`,
   `bun install`, `cargo install`, `cargo audit` without `--no-fetch`); a path under
   `doc/plans/`; `vitest` with `-t`; `PIPESTATUS`; `rg -r`; a test runner (`cargo test`,
   `cargo nextest`, `bun test`, `vitest`, `pytest`, `go test`) inside `wiring_tests`; a hardcoded
   `/tmp/` that is not the value of a `TMPDIR=` prefix assignment.
   Keep the two functions W2 exposes in mind (step 4); do not duplicate them.
2. Base-tree evaluation, in `base_tree.rs`. A criterion is evaluable when its command is a single
   `rg` or `grep` invocation with no pipe, separator, substitution or redirection, using only
   `-q`, `-F`, `-i`, `-e`, `-n`, `-w` and their combined short forms, one pattern, and one or
   more path operands. For each evaluable criterion: resolve operands against
   `repo_root/working_dir`; list tracked files with `git ls-tree -r --name-only HEAD -- <path>`;
   read each with `git show HEAD:<path>`; match with the `regex` crate (it is the dialect `rg`
   uses; `-F` escapes the pattern, `-i` and `-w` map to regex flags). Expected exit is
   `TruthCheck.exit_code` when set, else 0. Outcomes: every operand absent at HEAD → skip
   silently (the stage creates it); already meets the expected exit at HEAD → warning
   `criterion passes on the untouched tree; it cannot detect this stage doing nothing`, naming
   stage, index and command; operand present but untracked → warning that worktrees will not
   see it. No repo root → skip the whole check with one note.
3. Surface base-tree results as a fourth bucket `baseline` in `JsonWarnings` and in the human
   output. `--strict` already promotes warnings to errors; confirm the new bucket is included.
4. Wire W2's two checks into `validate_structural_preflight`. Pinned signatures (W2 writes them):
   `structural_checks::declared_skills::check_declared_skills(stages: &[StageDefinition], repo_root: Option<&Path>) -> DeclaredSkillsReport`
   where `DeclaredSkillsReport { errors: Vec<(String, String)>, warnings: Vec<String> }` (stage
   id, message); and
   `structural_checks::worker_table::check_worker_granularity(stages: &[StageDefinition]) -> Vec<String>`.
   Errors go through `validate()`'s `ValidationError` channel; warnings into `structural`.

## Traps

- Knowledge and knowledge-distill stages legitimately carry repo-wide gates; base-tree warnings
  apply to `standard` and `integration-verify` stages only.
- A criterion with `exit_code: 1` that is red at HEAD is the delta-proof shape; it must not warn.
- `deny_unknown_fields` is set on `StageDefinition`; you add no schema fields.

## Proof

`cargo test --offline --locked --manifest-path loom/Cargo.toml --lib plan::schema` — run once.
Tests must include: each error and warning above with a passing twin; a temp git repo (find how existing
git tests build one: `rg -n 'git.*init' loom/src/git --glob '*test*'`) where one criterion is green at HEAD,
one is red, one targets a new file, one expects `exit_code: 1`; and verify with no repo root.
