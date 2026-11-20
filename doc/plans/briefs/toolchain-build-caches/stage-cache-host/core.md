# Brief: `stage_cache` module (foundation)

Stage `stage-cache-host`, worker `core`, loom-senior-software-engineer. Working directory `loom/`.

## Goal

Create `src/stage_cache/` with the cache root, the per-ecosystem environment and the extra read roots described in `doc/plans/PLAN-toolchain-build-caches.md` sections **Design** and **Decisions**. The plan's YAML `description` of this stage pins the public surface. Implement exactly that surface; the `confinement` worker codes against it in parallel and the contract tests are already frozen against it.

## Files you own

- `src/stage_cache/mod.rs`: `StageCache`, `Consumer`, `resolve`, `root`, `ensure`, and re-exports of the free functions.
- `src/stage_cache/environment.rs`: `StageCache::environment`.
- `src/stage_cache/roots.rs`: `seed_read_roots`, `toolchain_read_roots`.
- `src/stage_cache/developer.rs`: `DeveloperTools` and `detect()`.
- `src/lib.rs`: add `pub mod stage_cache;` in alphabetical order (after `stage_service`).

The contract file `src/stage_cache/contract_tests.rs` is frozen; never edit it. Its `#[cfg(test)] mod contract_tests;` line in `mod.rs` was added by the contract session; keep it.

## Requirements

1. `resolve` is pure.
   - Base: an absolute `xdg_cache_home`, else `home/Library/Caches` on macOS, `home/.cache` elsewhere.
   - Root: `<base>/loom/stage-cache/<hex>`, where `<hex>` is the first 24 hex characters of `crate::utils::sha256_hex` over the canonical repo root bytes, a NUL byte, and the stage id bytes. If the repo root does not exist, hash it as given.
   - Refuse an empty stage id, or one containing `/`, NUL or `..`.
2. `ensure` creates each missing component from `<base>/loom` down to the root with mode 0700, opening without following symlinks. Reuse the descriptor approach in `src/verify/criteria/confine_linked_home.rs::open_home` (lines 57-85) as the pattern: `nix` `Dir::openat` with `O_NOFOLLOW`, then a uid check.
   - Refuse a symlinked or foreign-owned root, or a foreign-owned `loom/stage-cache` parent.
   - Do not copy that file's `populate` reset logic: the stage cache is meant to hold the stage's own entries.
3. `environment(consumer, worktree_root, host_env)` returns exactly the variables in the Design table.
   - Paths are joined under `root()`.
   - Read seeds from `host_env`; a seed is used only when its directory exists.
   - `GOFLAGS` for `HostJob`: append `-count=1` to the host value with a space, and do not duplicate it if already present.
   - `MAVEN_OPTS`: append to the host value.
   - `SCCACHE_SERVER_UDS`: emit only when the full socket path's byte length is below `SUN_LEN` (`104` on macOS, `108` elsewhere; define one `const` with a comment).
   - Never return a variable whose value would be empty.
4. `DeveloperTools::detect()` (macOS only, `std::sync::OnceLock`): run `xcode-select -p` and `xcrun --sdk macosx --show-sdk-path` with a cleared environment plus `PATH=/usr/bin:/bin`, like `src/verify/criteria/confine_developer_tools.rs` (line 11) does. Trim the output and require both paths to be existing directories. With `Some(developer)`, `environment` sets the values the Design table gives: `PATH` = `<developer_dir>/Toolchains/XcodeDefault.xctoolchain/usr/bin` (only when it exists), then `<developer_dir>/usr/bin`, then the host `PATH`; plus `SDKROOT` and `DEVELOPER_DIR`. Set them identically for all three consumers.
5. `seed_read_roots(host_env)`: the existing seed directories, which are the same ones `environment` uses.
6. `toolchain_read_roots(host_env)`: the existing install roots and `PATH` entries described in Design.
   - Exclude HOME itself, any entry that is not strictly below HOME, and any entry equal to, inside, or containing a credential directory.
   - Credential directories are the paths `crate::models::stage::sandbox_config::default_deny_read` names relative to `~` (read it, `models/stage/sandbox_config.rs:158-179`; strip `/**`).
   - `/opt` and `/nix/store` are included when they exist, on both platforms.
   - Return canonical paths and deduplicate.

## Constraints

- Files stay under 400 lines and functions under 50 (`maintainability-baseline.txt` is a ratchet file; never raise a row).
- No new dependencies: `sha2` (through `crate::utils::sha256_hex`), `nix` and `anyhow` are present.
- Write unit tests beside your code in `src/stage_cache/tests.rs` (`#[cfg(test)] mod tests;`). Make them cover the cases the contracts do not: base selection on each platform via `cfg!`, `GOFLAGS` dedup, `MAVEN_OPTS` append, a seed that is absent, and `ensure` on a pre-created symlink.
- Run `cargo test --lib stage_cache::` and `cargo clippy --all-targets -- -D warnings` when done. Never the full suite.
- Report back the exact signatures you implemented and any deviation from the pinned surface, with its reason.
