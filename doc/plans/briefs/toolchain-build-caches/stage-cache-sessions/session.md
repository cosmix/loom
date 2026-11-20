# Brief: stage cache in sessions and codex workers

Stage `stage-cache-sessions`, worker `session`, loom-software-engineer. Working directory: repo root; code in `loom/`.

## Goal

The stage's Claude session and its native codex workers use the same stage cache root as its host jobs: `Consumer::Session` and `Consumer::Worker` environments, and a sandbox write grant for the root. Sessions stop being granted write access to the host caches that the root replaces. Read `doc/plans/PLAN-toolchain-build-caches.md` sections **Decisions** and **Design**, and the merged `loom/src/stage_cache/` module.

## Files you own

- `loom/src/stage_cache/session.rs`: new; `wrapper_exports`, `apply_session_grant` (signatures pinned in the stage YAML).
- `loom/src/stage_cache/mod.rs`: add `pub mod session;` and `pub mod cleanup;`. The `cleanup` worker writes `cleanup.rs` in parallel. Keep the contract session's `#[cfg(test)] mod session_contract_tests;` line.
- `loom/src/orchestrator/terminal/native/wrapper.rs`, `wrapper/host_env.rs`, `launch.rs`, `launch/host.rs`.
- `loom/src/sandbox/package_caches.rs`.
- `loom/src/native_worker/boundary/linux.rs`, `native_worker/boundary/environment.rs`.

## Steps

1. Launch path:
   - `launch.rs::prepare_session_launch_with` (line 326) resolves `StageCache::resolve(repo_root, &stage.id, home, xdg)` once and calls `ensure()` before the capsule is written. Claude Code skips an `allowWrite` path that does not exist (`sandbox/grant_paths.rs:34-43`).
   - `session_sandbox` (line 216) calls `apply_session_grant` on the merged config.
   - Use the same HOME/XDG helper the host side uses; it was added to `loom/src/verify/criteria/confine.rs` in stage 1. Read it, and if it is not crate-visible, report it rather than duplicating it.
2. Wrapper:
   - Add the `wrapper_exports` output to the `exec env -i` line in `build_wrapper_script` (`wrapper.rs:369-377`), next to `{sccache_env}{host_env}`.
   - Carry it through `WrapperHostEnv` (`wrapper/host_env.rs:38-69`) as a new field, or as a new `create_session_wrapper_script` argument (line 92). Choose the one that keeps `create_session_wrapper_script` under 50 lines.
   - Values come from the operator's environment, so the host env map is built from the daemon's own `std::env::vars_os()` filtered to the names `environment` reads. Pass `DeveloperTools::detect().as_ref()`.
   - The exports come after the allowlisted `PATH` in the `env -i` list, so the exported `PATH` (developer dirs first on macOS) wins. Assert that in a wrapper test.
3. `PACKAGE_MANAGER_CACHE_WRITE_PATHS` (`sandbox/package_caches.rs:40-75`): remove the bun, npm, pnpm, yarn, deno, uv, pip and go entries; keep cargo and rustup.
   - Update the array length, the module doc (lines 1-35), and every test in that file or elsewhere that pins the removed entries (`rg -n 'PACKAGE_MANAGER_CACHE_WRITE_PATHS|\.npm|go/pkg' loom/src`).
   - Keep the test at lines 112-134 that pins never-granted parents.
4. Codex worker boundary (Linux), `native_worker/boundary/linux.rs:150-180`:
   - Bind the stage root writable (`--bind`), add `seed_read_roots` and `toolchain_read_roots` as `--ro-bind` (skip any already under a bound root), and add the `Consumer::Worker` environment to `env` before the `command.env_clear().envs(...)` call.
   - The boundary plan must carry the stage id: find where the plan is built (`loom map --impact native_worker::boundary`). If the stage id is not already reachable there, add an `Option<StageCache>` field to that plan struct and set it where the plan is constructed. Report that file; it may be outside your list, in which case stop and report instead of editing it.
   - `environment.rs` is where worker env names are allowlisted. Read it and add the cache variables only if the boundary drops names not on its list.

## Constraints

- Files under 400 lines, functions under 50. Never raise a row in `loom/maintainability-baseline.txt`.
- Unit-test `wrapper_exports` escaping, and the grant, beside your code in `loom/src/stage_cache/session_tests.rs`. Never touch `session_contract_tests.rs` (frozen).
- Run `cargo test --manifest-path loom/Cargo.toml --lib stage_cache:: sandbox:: orchestrator::terminal::native:: native_worker::` and clippy.
- Report the files changed, the struct you extended for the worker boundary, and any file you needed but did not own.
