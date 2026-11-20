# G4: `loom stage contracts freeze` refuses unformatted contract files

Stage `completion-gates`, wave 1, tier sonnet. Read `../common.md` first. Run no check: G2
changes `DisputeKind` in parallel and the crate compiles again only after G3.

You own `loom/src/verify/contracts/format_gate.rs` (new), `loom/src/verify/contracts/mod.rs`,
`loom/src/commands/stage/contracts/freeze.rs`, `loom/src/orchestrator/signals/contract.rs` and
`loom/src/orchestrator/signals/contract_tests.rs`.

## Why

A contract session froze a 56-line test function (this repo's limit is 50) and another froze a
file that was not rustfmt-clean. A frozen file cannot change, so each cost a contract dispute
and extra sessions. Nothing in the freeze runs a formatter (`freeze.rs::checked_reports`,
about lines 146-172, checks only which paths changed, then runs the contracts), and the
contract-session signal (`signals/contract.rs::append_rules`, about lines 233-256) says
nothing about formatting or size limits.

## 1. `verify/contracts/format_gate.rs`

Declare it `pub mod format_gate;` in `verify/contracts/mod.rs` (the contract test imports
`loom::verify::contracts::format_gate`).

    /// Whether `command`, run from `cwd`, is a formatter check: a simple command that checks
    /// formatting without rewriting files.
    pub fn is_format_check(command: &str, cwd: &Path) -> bool

Recognise, in any simple command that `crate::testrun::recognize::invocations(command, cwd)`
yields, after `crate::testrun::recognize::strip_prefixes` (which removes `env`, `VAR=x`,
`bunx`/`npx`, `pnpm exec`, `uv run`, `python -m` and similar):

- `cargo fmt` with `--check` anywhere in its arguments (`cargo fmt --all -- --check`,
  `cargo fmt --check`);
- `rustfmt --check`, `prettier --check` (or `-c`), `oxfmt --check`, `ruff format --check`,
  `black --check`;
- `biome format` and `biome check`, unless the arguments hold `--write`, `--fix` or `--apply`;
- a `format:check` package script: `npm run format:check`, `bun run format:check`,
  `pnpm format:check`, `pnpm run format:check`, `yarn format:check`,
  `yarn run format:check`.

`gofmt -l` is NOT recognised: it lists unformatted files and still exits 0, so running it can
never produce a problem.

A criterion is a formatter check only when `invocations(command, cwd)` yields at least one
simple command and EVERY simple command it yields is one of the forms above. A compound
criterion such as `cargo fmt --check && cargo clippy -- -D warnings`, or an `npm test` whose
script mixes a formatter with other commands, is not a formatter check and is not run: the
freeze runs while the contracts are red and uncompilable by design, so running the other half
would fail every freeze with a "run the formatter" message nothing can clear.

`strip_prefixes` removes `yarn` (and `yarn run`), but keeps `npm`, `bun` and `pnpm`. After it,
`yarn format:check` and `yarn run format:check` read `["format:check"]`; match
`["format:check"]`, `["npm" | "bun" | "pnpm", "run", "format:check"]` and
`["pnpm", "format:check"]`.

Use `recognize::command_args(argv, &["cargo", "fmt"])` and `recognize::has_flag(args, "--check")`
style helpers rather than hand parsing; read `testrun/recognize.rs` first.

    /// The stage's acceptance commands that are formatter checks, run as acceptance runs them;
    /// one problem per check that fails, naming its command.
    pub fn format_problems(
        acceptance: &[AcceptanceCriterion],
        setup: &[String],
        working_dir: &Path,
        confinement: CommandConfinement,
    ) -> Result<Vec<String>>

Run them through the acceptance runner so setup, `${..}` expansion, timeouts and confinement
match what `loom stage complete` does: build a `Stage` holding only the recognised criteria,
`setup.to_vec()`, and `sandbox.command_confinement = Some(confinement)`, and call
`crate::verify::criteria::run_acceptance_with_config(&stage, Some(working_dir),
&CriteriaConfig::default())` (no pass cache: `CriteriaConfig::default()` has no cache
directory). Each failed or timed-out criterion gives the problem

    acceptance criterion `<command>` fails on the contract files: run the repository's formatter over them, then freeze again

Read `verify/criteria/result.rs` for how `AcceptanceResult` reports failures. No recognised
criterion means `Ok(vec![])` and runs nothing. The runner applies its own timeouts: a Simple
criterion gets `CriteriaConfig::default().command_timeout` (300 s), an Extended one 30 s
(`verify/criteria/runner.rs`, `prepare_criterion`); keep both.

Unit tests in the module: recognition of each listed form and of near-misses that must not
match (`cargo fmt --all` without `--check`, `biome format --write`, `prettier --write`,
`cargo test`, `gofmt -l .`, `cargo fmt --check && cargo test`); and `format_problems` over a
TempDir crate with an unformatted `src/lib.rs` (`cargo fmt --check` gives one problem naming
it; a formatted file gives none).

## 2. The freeze runs it

In `freeze.rs::checked_reports`, after the changed-paths refusal and before `run_contracts`,
refuse on format problems through the existing `refuse(stage_id, problems, fix)` helper, with
the fix text "Every contract file and harness file must pass the stage's formatter checks before
it is frozen: run the repository's formatter over them (for Rust, `cargo fmt --all`), then".
Keep that text as one string literal on one line of `freeze.rs` (break the surrounding call,
not the literal): the stage's wiring check matches `must pass the stage's formatter checks
before` there, which proves the freeze refuses on the gate's result.
Compute the confinement once in `checked_reports`
(`resolve_confinement(stage.sandbox.command_confinement, plan_confinement(&site.work_dir))`,
as `run_contracts` does now) and pass it to both. A formatter check that already fails at the
stage's base blocks every freeze; plans keep formatter commands green at base.

`freeze.rs` is 382 lines; keep it at or under 400 (the maintainability test fails a new file
over 400). The logic lives in `format_gate.rs`.

## 3. The contract session is told

`signals/contract.rs::append_rules`: add one numbered rule before the freeze step, and
renumber the rest:

    Before freezing, run the repository's formatter over every file you wrote (for Rust,
    `cargo fmt --all`), and keep every test function under 50 lines and every file under 400
    (CLAUDE.md Rule 17): the freeze runs the stage's formatter checks, and a frozen file cannot
    change afterwards.

Test in `contract_tests.rs`: `contract_rules_name_the_formatter_and_size_limits`.

## Contract your code must satisfy

`freeze-refuses-unformatted-contract-files` (scenario in the plan's YAML).
