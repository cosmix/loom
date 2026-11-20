# Brief: policy-tests (stage confined-test-guards)

## Goal

The five tests of `loom/src/stage_cache/contract_tests.rs` that launch a confined command assert on the policy loom generates and launch nothing. Their kernel-enforcement halves move to a guarded module.

## Files you own

- `loom/src/stage_cache/contract_tests.rs`
- `loom/src/stage_cache/kernel_tests.rs` (new)
- `loom/src/stage_cache/mod.rs` (add `#[cfg(test)] mod kernel_tests;` only)
- `loom/src/verify/criteria/confine.rs`, `loom/src/verify/criteria/mod.rs` (the accessor only)

The stage's frozen contract files (`loom/src/process/sandbox_probe_contract_tests.rs`, `loom/src/stage_cache/policy_contract_tests.rs`) are read-only; your accessor must satisfy the second.

## The accessor (pinned)

```rust
// loom/src/verify/criteria/confine.rs, re-exported from mod.rs; both #[cfg(test)] pub(crate)
pub(crate) struct ConfinedPolicy {
    pub program: PathBuf,
    pub args: Vec<String>,
    pub env: BTreeMap<String, String>,
}
pub(crate) fn confined_policy(spec: &CommandSpec, tree: &Path, cache: &StageCache)
    -> anyhow::Result<ConfinedPolicy>
```

- It calls `prepare_confined` (`confine.rs:198`, `pub(super)`) with `CommandConfinement::Confined`, `tree` as working directory and tree root, and the cache, then reads `PreparedCommand.wrapped` (`:61`, private): `get_program`, `get_args`, `get_envs` (keep variables that are set; drop removed ones).
- `prepare_confined` starts no process. It needs the launcher installed and returns `Err` without one (`confine_seatbelt.rs:186-188`); it creates the cache root and scratch directories.
- `confine.rs` is near the 400-line limit (`loom/maintainability-baseline.txt` is exact). If the accessor pushes it over, put it in a new `loom/src/verify/criteria/confine_policy.rs` declared from `mod.rs` and report the new file.

## The five tests

`stage_env_wins_over_per_command_scratch` (`:144`), `stage_cache_persists_across_confined_commands` (`:171`), `host_caches_stay_read_only` (`:189`), `path_toolchains_are_readable` (`:281`), `credential_dirs_on_path_stay_hidden` (`:314`). Keep their names, their `#[serial]` attributes and their fixtures. Replace `run_confined` (`:89`) and `sandbox_absent` (`:81`) uses with assertions on `confined_policy(..)`:

| Test | Assert on the policy |
| --- | --- |
| stage env | each of the eight variables in `policy.env` equals `cache.root().join(<child>)` |
| persists | macOS: the profile has `(allow file-write* (subpath <cache root>))` and no later `(deny file-write* (subpath <cache root>))`. Linux: argv binds the cache root read-write (`--bind root root`) |
| read-only seeds | keep `:205-208`. macOS: for each seed, the profile's `(deny file-write* (subpath <seed>))` comes after every `allow file-write*` rule. Linux: each seed is bound `--ro-bind` and never `--bind` |
| toolchains readable | keep `:295-301`. Each toolchain root appears as an `allow file-read*` rule (macOS) or a bind (Linux) |
| credential dirs hidden | keep `:321-328`. No allow rule or bind names `.ssh` or `.aws` or a parent of them below HOME |

- On macOS the profile is the argument after `-p` in `policy.args`. Seatbelt applies the last matching rule: order is set in `confine_seatbelt.rs:190-194` and `seatbelt_path_rules` (`:39-77`); assert positions with `find`, as `confine_seatbelt_tests.rs:55-89` does.
- Paths in the profile are canonical (`/private/var/...` on macOS); canonicalize the fixture paths before comparing.
- Branch on `cfg!(target_os = "macos")` inside the test, never with a `cfg` attribute on the test.

## kernel_tests.rs

Move the launched halves here as tests named `<original>_in_the_kernel`, each starting with the guard and then launching through `run_spec_in_stage` as the originals did:

```rust
if !crate::process::sandbox_probe::confined_commands_runnable() {
    return;
}
```

## Acceptance

- `cargo test --manifest-path loom/Cargo.toml --lib stage_cache::` passes in your shell; in a sandboxed shell the `kernel_tests` print `SKIP`.
- No test in `contract_tests.rs` calls `run_spec_in_stage`, `run_confined` or `host_launcher`.
- Report each assertion you replaced and the one that replaces it (the orchestrator files the integrity dispute from this list).
