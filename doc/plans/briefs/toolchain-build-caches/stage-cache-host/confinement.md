# Brief: thread the stage cache through the confined runner

Stage `stage-cache-host`, worker `confinement`, loom-senior-software-engineer. Working directory `loom/`.

## Goal

Every confined command that a stage's host job runs gets the stage cache:
- its root writable;
- its seeds and toolchain roots readable;
- the `Consumer::HostJob` environment applied after the per-command scratch environment;
- on macOS, a Unix-socket grant for `<root>/h.sock` only.

That covers criteria, contracts at freeze and completion, the format gate, resolved-completion reruns, `loom check` acceptance, and provisioning. Read `doc/plans/PLAN-toolchain-build-caches.md`, sections **Design** and **Non-goals**.

## Interface you code against (built in parallel by the `core` worker)

The stage YAML description pins `crate::stage_cache::{StageCache, Consumer, seed_read_roots, toolchain_read_roots}`. Use only that surface. You also add:
- `CriteriaConfig::with_stage_cache(self, Option<StageCache>) -> Self`, with field `stage_cache: Option<StageCache>`, in `src/verify/criteria/config.rs` next to `with_tree_root` (line 136).
- `pub fn run_spec_in_stage(spec, tree, working_dir, timeout, confinement, cache: &StageCache) -> Result<CriterionResult>` in `src/verify/criteria/executor.rs`, beside `run_spec_in_optional_tree` (line 79). Re-export it from `src/verify/criteria/mod.rs` (line 89 block).
- `pub(super) fn apply_stage_cache(command: &mut Command, original: &Command, cache: &StageCache, tree: &Path)` in `confine_host.rs`, called from `apply_environment` (lines 88-101) right after `scratch.apply_environment(command)`. The plan's `reachable` check names this function.

## Threading (read each site before editing)

1. `confine.rs`:
   - `prepare_confined` (line 248) and `prepare` (line 266) gain `cache: Option<&StageCache>`.
   - `prepare_provision` (line 260) gains it too.
   - `prepare` passes it to `host::wrap`.
2. `confine_host.rs::wrap` (line 46) gains `cache: Option<&StageCache>`.
   - `writable_roots` (line 187) includes `cache.root()`.
   - `read_roots` (line 141) is per command. Add `seed_read_roots` and `toolchain_read_roots` of the original command's env (build the `BTreeMap` from `original.get_envs()`). Do this for every confined command, with or without a cache.
   - Seatbelt (`confine_seatbelt.rs::seatbelt_path_rules`, lines 39-81) picks both lists up through `read_roots` / `writable_roots`. Bubblewrap (`bind_read_roots`, line 257; writable binds, line 205) does too.
   - Call `cache.ensure()` in `wrap` before building the profile.
3. `confine_scratch.rs`: when a stage cache is given, `cache_overlays` / `cache_lowers` (Linux) are not mounted. Pass a flag into `CargoScratch::prepare`; keep the existing behaviour for `None`.
4. Seatbelt additions in `confine_seatbelt.rs`:
   - `(allow network-bind network-outbound (local unix-socket (path-literal "<root>/h.sock")))`, plus the matching `remote unix-socket` form for connect. Verify the exact SBPL with a real `sandbox-exec` run: start an `sccache` server with `SCCACHE_SERVER_UDS` set inside the boundary and compile one C file.
   - Hide `~/.m2/settings.xml` and `~/.m2/settings-security.xml` when they exist. Use `ancestors::seatbelt_rules` (`confine_ancestors.rs:57`), the same way `host_cargo_home_rules` hides Cargo config files.
5. `runner.rs::run_with_cache` (line 175) passes `config.stage_cache.as_ref()`.
6. Set `with_stage_cache(Some(StageCache::resolve(repo_root, &stage.id, home, xdg)?))` at every place that builds a `CriteriaConfig` for a stage's tree:
   - `src/host_verify/child.rs::acceptance_config` (line 146);
   - `src/verify/contracts/format_gate.rs` (lines 264, 325);
   - `src/verify/contracts/completion.rs` (line 39);
   - `src/verify/completion/resolved.rs` (line 111);
   - `src/commands/stage/acceptance_runner.rs` (line 129).
   Derive `home` and `xdg` from the process environment (`HOME`, `XDG_CACHE_HOME`) through one helper in `confine.rs`, never inline. `commands/plan/verify/dry_run_exec.rs` stays `None`: a dry run has no stage.
7. macOS developer tools: `PATH`, `SDKROOT` and `DEVELOPER_DIR` from `environment(..., DeveloperTools::detect().as_ref())` must be on the ORIGINAL command before `host::wrap` resolves the program (`confine_host.rs::resolve_program`, line 103). Otherwise a criterion's bare `git` still resolves to the `/usr/bin` shim. Apply them in `confine.rs::apply_confined_environment` (around line 335, where `PATH` already gains the `LOOM_BIN` directory). Order: `LOOM_BIN`'s directory, then the developer dirs, then the host `PATH`. Prove it with a test that runs `sh -c 'command -v git'` confined and gets the developer dir's `git`. Then time 5 confined `git --version` runs and report it; the baseline is 7.35 s through the shim and 0.066 s direct.
8. Provisioning:
   - `src/orchestrator/provision.rs::run_provision` (line 40) gains `cache: Option<&StageCache>` and passes it to `run_provision_with_timeout` (`executor.rs:92`, gains the same parameter).
   - `src/orchestrator/core/provision_gate.rs::provision_worktree` (line 98) resolves the stage's cache from `stage_id`.
   - `commands/plan/verify/dry_run.rs:95` passes `None`. That file is not owned by you; if its call breaks, make the parameter defaulted through a second function (`run_provision_in_stage`) rather than editing it.

## Size

`confine_host.rs` is 337 lines and will pass 400. Move `read_roots` and `environment_path`/`cargo_home` into a new `src/verify/criteria/confine_read_roots.rs` (`#[path]` module of `confine_host`, re-exported so existing `super::read_roots` callers compile). Functions stay under 50 lines. Never raise a row in `maintainability-baseline.txt`.

## Done when

- `cargo test --lib verify::criteria::`, `cargo test --lib host_verify::` and `cargo test --lib stage_cache::contract_tests::` pass. The last needs `core`'s module; if `core` has not returned, report that and stop.
- `cargo clippy --all-targets -- -D warnings` is clean.
- Report each call site you changed (file:line), the SBPL you verified with its run output, and anything left open.
