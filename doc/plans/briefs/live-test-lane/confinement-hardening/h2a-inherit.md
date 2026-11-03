# confinement-hardening H2a: remove `command_confinement: inherit`

Working dir `loom/`. Read `doc/plans/briefs/live-test-lane/common.md` first. You run in wave 3,
after H1 (which added `crate::process::apply_confined_host_environment`) and after the
orchestrator's foundation edit of `src/models/stage/sandbox_config.rs`. The frozen contracts are
`tests/hardening_contracts.rs` and `src/verify/criteria/confine_env_contract_tests.rs`; never
edit them. The stage description's PUBLIC SURFACE is authoritative.

## What the foundation already did

`src/models/stage/sandbox_config.rs`: the `CommandConfinement::Inherit` variant is gone from
plans. In its place, for loom's own unit tests only:

```rust
/// Loom's own unit tests only: run a command with no environment rebuild and no host boundary,
/// for tests of the runner's bookkeeping (timeouts, output capture, caching) that must also
/// pass where bubblewrap cannot start, such as a Claude Code session sandbox, which forbids
/// user namespaces. Never deserialized, so no plan can name it; absent from every non-test
/// build.
#[cfg(test)]
#[serde(skip_deserializing)]
TestUnconfined,
```

A plan's `command_confinement: inherit` now fails to parse ("unknown variant `inherit`,
expected `confined`"). `command_confinement: confined` still parses (plans 04-11 carry it).

## Your edits: every consumer of the removed variant

`rg -n 'Inherit' src tests` lists them (ignore `InheritorGuard` in
`process/lifecycle_tests.rs`, an unrelated name). Production code first:

1. `src/verify/criteria/confine.rs`:
   - `prepare`: the `CommandConfinement::Inherit => {}` arm becomes `#[cfg(test)]
     CommandConfinement::TestUnconfined => {}`; the Confined arm calls
     `apply_confined_environment`, whose body now calls
     `crate::process::apply_confined_host_environment(command)` instead of
     `apply_stage_environment`.
   - `PreparedCommand::containment_policy_revision`: `host-os-boundary-v9` becomes
     `host-os-boundary-v10` (the confined environment changed; cached passes from v9 must not
     certify a run under v10). `rg -n 'host-os-boundary-v9' src tests` must return nothing.
   - Doc comments that describe `Inherit` as a plan opt-in: rewrite to the current truth (one
     level for plans; the test-only variant for loom's own tests).
2. `src/verify/criteria/cache_fingerprint.rs` (`from_prepared`): the `Inherit` early return
   becomes a `#[cfg(test)]` check of `TestUnconfined`.
3. `src/plan/schema/validation/v2_lints/registry_domains.rs`: delete the branch that judged an
   `inherit` stage's registry fetch against its `allowed_domains`; every stage is confined, so a
   registry fetch in a criterion is always the "needs a `provision` entry" error. Update the
   module doc.
4. `src/verify/goal_backward/truths.rs` and `src/verify/criteria/runner.rs`: the `Inherit`
   uses there are inside `#[cfg(test)]` code; switch them to `TestUnconfined` (confirm each is
   test-only; a production use is a finding to report, not to convert).

Tests (switch `Inherit` to `TestUnconfined` where the test only needs an unconfined run; delete
assertions that exist only to prove `inherit` behaviour for plans, and say so in your report):
`src/verify/criteria/tests/{confine_tests.rs, runner_tests.rs, cache_tests.rs,
cache_contract_tests.rs, criterion_timeout_tests.rs}`,
`src/verify/criteria/{executor_lifecycle_tests.rs, confine_host_tests.rs,
confine_host_tool_tests.rs, confine_cargo_home_tests.rs}`, `src/models/stage/methods_tests.rs`,
`src/plan/schema/tests/v2_lint_environment_tests.rs`. In `confine_host_tests.rs`,
`confine_host_tool_tests.rs` and `confine_cargo_home_tests.rs` also replace each
`crate::process::apply_stage_environment(&mut original)` that models a confined command with
`apply_confined_host_environment`. `resolve_confinement_prefers_the_stage_override` and the
`plan_confinement` test keep their meaning with `TestUnconfined` standing in for a non-default
level.

`src/models/stage/methods_tests.rs` also builds a `StageSandboxConfig` with
`allow_unsandboxed_escape: Some(false)`: remove that field there too (H2b removes it
everywhere else; this file is yours alone).

`tests/plan_environment_contracts.rs` (an integration test: no `cfg(test)` variant exists
there): the second half of the registry-domains test writes `command_confinement: inherit`.
Replace it with an assertion that `loom plan verify` now reports an error naming `inherit` for
that plan (the helper `verify_errors` returns the messages). This changes assertion lines; the
stage disputes the `TI-edit` event.

## Traps

- `#[cfg(test)]` on a match arm needs the match to stay exhaustive in non-test builds: with
  one variant left there, write the production code as `if confinement ==
  CommandConfinement::Confined` or a `match` whose test-only arm is cfg'd.
- `serde(skip_deserializing)` keeps serialization: a test that persists a stage with
  `TestUnconfined` writes `test-unconfined`, which never parses back. Do not persist one.
- Do not touch `sandbox/**` (H2b) or `src/process/**` (H1).
- `cargo fmt`; your one check: `cargo test --lib verify::criteria:: --no-run`.

## Report

Files changed, every assertion you deleted or rewrote (file and test name), and any production
use of `Inherit` you found outside test code.
