# test-escape-policy X1: `loom plan verify` refuses escape hatches

Working dir `loom/`. Read `doc/plans/briefs/live-test-lane/common.md` first. The frozen
contracts are `tests/escape_lint_contracts.rs` and `tests/escape_lint_contracts/**`; never edit
them. The stage description's PUBLIC SURFACE is authoritative. You run in wave 1 beside H4.

## The lint: new file `src/plan/schema/validation/v2_lints/escape_hatches.rs`

Mirror `registry_domains.rs` (same imports from `super::{stage_commands, visit_argvs,
LintContext, LintFinding, StageCommand}`; `stage_commands` covers acceptance, setup, wiring
tests, before/after-stage checks and the dead-code check). Register it in
`src/plan/schema/validation/v2_lints/mod.rs` `run` (`mod escape_hatches;` and
`escape_hatches::check(ctx, &mut out);`) and add one line to the module doc.

`pub(super) fn check(ctx: &LintContext<'_>, out: &mut Vec<LintFinding>)` scans every
`stage_commands(stage)` of every stage, and every plan-level `ctx.metadata.loom.provision`
command (label `provision entry #n`), and reports each escape once per command with
`error_in_v2: true`. Expose the scan of one command as
`pub(super) fn escapes_in(command: &str, allow_ignored: bool) -> Vec<&'static str>` so the
later live-profile stage can scan `live:` commands with `allow_ignored = true`.

| Escape | Detected as |
| --- | --- |
| `--include-ignored` | an argv word equal to it (it runs every `#[ignore]` test, and those need the live profile or a host the runner is not); skipped when `allow_ignored` |
| `--dangerously-bypass-approvals-and-sandbox`, `--yolo` | an argv word equal to it, or starting with it followed by `=` (Codex) |
| `--dangerously-skip-permissions` | an argv word equal to it (Claude Code's bypass flag) |
| `--sandbox danger-full-access` | the word `--sandbox` or `-s` followed by `danger-full-access`, or `--sandbox=danger-full-access` (Codex; only `loom-hooks/codex-forward.sh` may use it, and a plan command never is that wrapper) |
| `LOOM_TEST_REQUIRE_SANDBOX_FREE` | the text anywhere in the command: the variable no longer exists and its use meant "run this unconfined on the host" |
| `LOOM_LIVE_PROFILE` | the text anywhere in the command: only the live profile sets the profile marker |
| `dangerouslyDisableSandbox` | the text anywhere in the command (Claude Code's per-call sandbox opt-out) |

Message: ``<label> `<command>` uses <escape>, which leaves loom's confinement: plan-authored
commands always run confined, and a test that needs a real Claude Code, network or a private
HOME runs through a `live:` entry``. Argv words come from `visit_argvs`, so nested `sh -c`
scripts and command substitutions are scanned; the text checks run on the raw command string so
assignments and quoted JSON are caught.

`--ignored` is not an escape: it selects only ignored tests, a test that needs the live profile
skips outside it with a visible `loom-live-profile: skip` line, and loom's measurement harnesses
(`signal_replay`, the dependency harness) are run that way on purpose. Say so in the module doc.

Not scanned, by design: stage `description`, `summary` and `name`. Say so in the module doc.

## `src/plan/schema/tests/validation_suite_tests.rs`

`test_filtered_runs_the_full_suite_does_not_execute_are_not_subsumed` lists
`"cargo test --lib foo:: -- --include-ignored"` and
`"LOOM_TEST_REQUIRE_SANDBOX_FREE=1 cargo test --lib tests_capsule_denies::bwrap::"` as
subsumption fixtures. Read its helper `subsumed_warnings`: if it returns only the subsumption
warnings, leave the test alone; if the new errors reach it, replace those two fixtures with
commands that keep the test's intent without an escape (`"cargo test --lib foo:: -- --ignored"`
and `"cargo test --lib tests_capsule_denies::bwrap::"`) and list the changed lines.

## Unit tests

`src/plan/schema/tests/v2_lint_escape_tests.rs` (declare it in `src/plan/schema/tests/mod.rs`):
one test per row above, one for a nested `sh -c` script, one proving a description mention is
silent, one for a provision entry, one for `escapes_in(.., true)` dropping only
`--include-ignored`. Mirror `v2_lint_environment_tests.rs`.

## Traps

- `src/plan/schema/validation/v2_lints/registry_domains.rs` and
  `src/plan/schema/tests/v2_lint_environment_tests.rs` belong to confinement-hardening, which
  runs at the same time; do not touch them. `src/process/**` and every test file H4 and H5 own
  are theirs.
- `--sandbox` alone (other values) is not an escape.
- `cargo fmt`; your one check: `cargo test --lib plan::schema:: --no-run`.

## Report

Files changed, and every existing plan file under `doc/plans/` (not `archive/`) whose commands
the new lint would refuse (run `target/debug/loom plan verify` on each `PLAN-*.md` there after
building; report, never edit them).
