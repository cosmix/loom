# W3: CLI wiring and the subcommand-list assertion (plus the rules C8 to C10 read)

Tier: haiku (`loom-software-engineer`, model haiku). You own only the CLI wiring and the
completions assertion; codex unit C8 writes `check.rs`, C9 the README and C10 the skill from the
sections below, which stay their reference. Read `doc/plans/briefs/plan-documents/common.md`, then
`doc/plans/briefs/plan-documents/checklist-check/common.md` ("JSON"), then this file. You start
after W2 has returned.

## Files you own

`loom/src/commands/plan/mod.rs`, `loom/src/cli/types_ops.rs`, `loom/src/cli/dispatch.rs`,
`loom/src/completions/dynamic/tests/tests_commands.rs`.

## CLI

- `loom/src/cli/types_ops.rs` `PlanCommands` (`:13-31`): add after `Verify`

  ```rust
  /// Print the project checklist and every document convention problem under doc/
  Check {
      /// Machine-readable JSON output
      #[arg(long)]
      json: bool,
  },
  ```

  Do NOT make `Commands::Plan`'s subcommand optional (`cli/types.rs:272-275`): plan 2 does that.
- `loom/src/cli/dispatch.rs::dispatch_plan` (`:104-113`): add
  `PlanCommands::Check { json } => plan::check::execute(json),`. The file is 375 lines; keep it under
  400 (the arm is one line).
- `loom/src/commands/plan/mod.rs`: `pub mod check;` is seeded; add nothing else.

## `check.rs`

`pub fn execute(json: bool) -> anyhow::Result<()>`:

1. Root: `git rev-parse --show-toplevel` from the cwd with
   `crate::git::runner::run_git_checked(&["rev-parse", "--show-toplevel"], &cwd) -> Result<String>`
   (`git/runner.rs:178`), trimmed. On failure print
   `loom plan check: <cwd> is not inside a git repository` to stderr and `std::process::exit(2)`.
2. `Project::load(&root)`; an `Err` prints `loom plan check: <error>` to stderr and exits 2.
3. `--json`: `serde_json::to_string_pretty` of `{ schema: 1, root, checklist, problems }` to stdout.
   Human: the step tracker (one line per step: number, name, mark), the current step and primary
   offer, the secondary offers, then problems as `<path>:<line>: <code>: <message>` (`<path>: ...`
   when the line is null), each through `crate::context::untrusted::inline_safe` (`context/untrusted.rs:123`,
   `pub(crate)`), since document text reaches the terminal.
4. Exit: `std::process::exit(1)` when problems is non-empty, else return `Ok(())`.

Keep the exit calls in `execute` only; the report builder is a pure `fn report(&Project) -> CheckReport`
(serde `Serialize`) that tests call.

## Tests

`check/tests.rs`: `report` on a fixture project serialises with the key set of
checklist-check/common.md "JSON"; the human renderer strips an ESC in a heading title. Completions:
completions derive from clap (`completions/dynamic/commands.rs:45-51`), so the visible `Check`
variant turns `test_complete_subcommands_plan` (`completions/dynamic/tests/tests_commands.rs:248-252`,
asserting exactly `vec!["verify"]`) red: change that assertion to `vec!["verify", "check"]` and add a
`complete_flags(&["plan", "check"], "--")` case listing `--json`. This is a declared TI-edit event the
orchestrator disputes after the final review (stage description, TEST INTEGRITY).

## Docs

- `README.md` "Plan Commands" (`:458-463`): add `loom plan check [--json]` to the command block and
  one paragraph: what it prints (checklist of section 01's seven steps, current step, offers,
  convention problems), that it reads only files and needs no server, the exit codes 0/1/2, and that
  `--json` is the schema the planning app and sessions use.
- `skills/loom-usage/SKILL.md`: one row or short paragraph near the plan-verify material (`:684`)
  saying sessions run `loom plan check` after editing planning documents, and what a non-zero exit
  means.
- Both: plain prose, no LLM stock phrases (CLAUDE.md Rule 19).

## Proof

`cargo test --lib commands::plan::`, `cargo test --lib completions::`,
`cargo test --test planning_checklist_contracts`, `cargo run -q -- plan check --help`,
`cargo clippy --all-targets -- -D warnings`.
