# confinement-hardening H2b: remove `allow_unsandboxed_escape`

Working dir `loom/`. Read `doc/plans/briefs/live-test-lane/common.md` first. You run in wave 2,
after the orchestrator's foundation edit of `src/models/stage/sandbox_config.rs` (the stage
override `allow_unsandboxed_escape: Option<bool>` is gone, so a plan naming it fails to parse
as an unknown field). The frozen contracts are `tests/hardening_contracts.rs` and
`src/sandbox/settings/policy/live_credential_tests.rs`; never edit them.

## Why

`sandbox/config.rs` `validate_config` already refuses `allow_unsandboxed_escape = true`, so the
field only carries a value that must be `false`. A field whose one legal value is the default is
a knob that invites the next escape. Stage sessions always emit
`"allowUnsandboxedCommands": false`.

## Edits

1. `src/sandbox/config.rs`: remove `allow_unsandboxed_escape` from the plan-level config type
   and from `MergedSandboxConfig`, the merge in `merge_config`, and the refusal branch of
   `validate_config` (update its doc: two settings are refused unconditionally). The plan-level
   field lives on `crate::plan::schema::SandboxConfig` (find it with
   `loom map --find-all SandboxConfig`); remove it there too, so `sandbox.allow_unsandboxed_escape`
   in plan YAML is an unknown-field parse error.
2. `src/sandbox/settings/policy.rs` `sandbox_settings`: emit `sandbox["allowUnsandboxedCommands"]
   = json!(false)` as a literal, keeping the comment about auditable literal `false`.
3. `src/sandbox/config/preflight.rs` (the sandbox-widening field detection at
   `stage.allow_unsandboxed_escape.is_some()`): drop the term.
4. Remove the field from every struct literal and assignment: `src/sandbox/settings/tests.rs`
   (about 20 literals; delete `test_generate_settings_with_unsandboxed_escape`, whose subject no
   longer exists), `src/sandbox/settings/policy/tests.rs` (keep the assertion that
   `allowUnsandboxedCommands` is `false`), `src/sandbox/settings/tests_read_denies.rs`,
   `src/sandbox/config/preflight/tests.rs` (delete the case that asserts the refusal text for
   `true`; the parse now refuses the field), `src/sandbox/grant_paths.rs`,
   `src/orchestrator/terminal/native/build_cache.rs`, and the `mod tests` block in
   `src/sandbox/config.rs`. `rg -n 'allow_unsandboxed_escape' src tests` must return only
   comments you deliberately kept (prefer none).
5. `tests/integration/plan_verify.rs` mentions the field in a comment near line 831: rewrite the
   comment to the current truth (or delete it if it explains only history).

## Traps

- `src/models/stage/methods_tests.rs` is H2a's; it removes the field there.
- `tests/plan_session_sandbox_contracts.rs` mutates `/sandbox/allowUnsandboxedCommands` in a
  planning capsule to prove the planning assertion refuses it: that is the planning builder, not
  this field. Leave it.
- `MergedSandboxConfig` literals in other modules: `cargo build --all-targets` names them.
- `cargo fmt`; your one check: `cargo test --lib sandbox:: --no-run`.

## Report

Files changed and every test you deleted (file, name, why its subject no longer exists).
