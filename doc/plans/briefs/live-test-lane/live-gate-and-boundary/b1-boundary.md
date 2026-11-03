# live-gate-and-boundary B1: the live boundary in the confined runner

Working dir `loom/`. Read `doc/plans/briefs/live-test-lane/common.md` first ("What the live
profile is", "Test guards", "The profile's environment and HOME"). The frozen contracts are
`src/verify/criteria/confine_live_contract_tests.rs` and its harness
`src/verify/criteria/confine_live_contract_tests/**`; read them before you start, never edit
them. The stage description's PUBLIC SURFACE is authoritative. Effort: this is the boundary
every live entry runs in; the standard runner's behaviour must not change.

## The runner today (verified at authoring; confirm by symbol first)

`verify/criteria/confine.rs` `prepare(spec, working_dir, tree, confinement, network)` builds the
command, rebuilds its environment (`apply_confined_environment`, which after
confinement-hardening calls `crate::process::apply_confined_host_environment` and puts the
running loom's directory first on `PATH` with `LOOM_BIN`), and calls
`confine_host::wrap(&cmd, tree, network)`. `wrap` returns `(Command, CargoScratch,
Option<GitDirs>)`; `executor::run_prepared_with_timeout` spawns, drains both pipes, applies the
timeout and retires the process group. Linux: `bwrap --die-with-parent --unshare-all
--new-session --proc /proc --dev /dev --tmpfs /tmp --tmpfs /var/tmp`, `--share-net` only for
provision, `--bind` of the tree, scratch and linked Cargo home, `--ro-bind` of `read_roots`
(system directories, `~/.rustup`, `~/.bun/bin`, the uv caches, Cargo's `bin`/`registry`/`git`,
`LOOM_BIN`), the git roots, the protected paths, the resolver file's target only (never `/run`),
then `--remount-ro /`. macOS: `sandbox-exec -p` with `(deny default)`, outbound network only for
provision. `HOME` is never bound or allowed, so it is not writable; `/run` is not bound, so no
host socket and no DNS stub is reachable.

## Edits (you own these files)

1. Split, behaviour-preserving: move the Linux `platform_command`, `bind_read_roots` and
   `bind_resolver_target` from `confine_host.rs` into `src/verify/criteria/confine_bwrap.rs`,
   and the macOS `seatbelt_path_rules` and `platform_command` into
   `src/verify/criteria/confine_seatbelt.rs`, each declared from `confine_host.rs` with
   `#[cfg(target_os = ...)] #[path = "confine_<name>.rs"] mod <name>;` like `confine_git.rs`.
   `read_roots`, `writable_roots` and `environment_path` stay in `confine_host.rs`
   (`pub(super)` as the moved code needs). The standard runner's argv and Seatbelt profile stay
   byte-identical: `confine_host_tests.rs`, `confine_host_tool_tests.rs`,
   `confine_cargo_home_tests.rs` and `confine_git_tests.rs` must pass with only the call-site
   changes in steps 3 and 4.
2. `src/verify/criteria/live_boundary.rs` (new, plain `mod live_boundary;` in
   `criteria/mod.rs`, re-exported): `LiveBoundary`, `GateMount`, `live_bwrap_args`,
   `live_seatbelt_rules` exactly as the PUBLIC SURFACE gives them. Both builders are pure and
   compiled on every platform, so their unit and contract tests run on Linux and macOS.
   - `live_bwrap_args`: `--bind <home> <home>`; for `GateMount::UnixDir(d)` `--ro-bind <d>
     <d>`; for each `read_only` path `--ro-bind <p> <p>`. Nothing else: never `--share-net`,
     never `/run`, never the account home.
   - `live_seatbelt_rules`: `(allow file-read* file-write* (subpath <home>))` and the same for
     `tmp`; for each `read_only` path `(allow file-read* (literal <p>))` for a file,
     `(subpath <p>)` for a directory; for `GateMount::LoopbackPort(port)` exactly one
     `(allow network-outbound (remote ip "localhost:<port>"))`. Paths quoted with
     `serde_json::to_string` as `seatbelt_path_rules` does.
3. `confine_host.rs` `wrap(original, tree, network, live: Option<&LiveBoundary>)` hands `live`
   to `platform_command` (step 4). Linux: the live arguments go after the git roots and before
   `protected::bind` and `--remount-ro /`. macOS: the live rules append to the profile after
   the path rules. `apply_environment`: after the scratch's entries, set
   `LOOM_TEST_FACILITIES` for every wrapped command (the runner's facilities, plus
   `live.facilities` when live); when live, then `HOME=<live.home>`, `RUSTUP_HOME` pinned to
   the original's `RUSTUP_HOME` or `<original HOME>/.rustup` when unset (the scratch already
   sets `CARGO_HOME` explicitly), `PATH` with `<live.home>/.local/bin` first, then every
   `live.env` entry in order. `read_roots` keeps reading the ORIGINAL `HOME` (toolchains live
   there); override only after it ran. `wrap` has eight callers: `confine.rs` `prepare` passes
   its `live`, and seven test call sites pass `None` (at authoring `confine_host_tests.rs` 221,
   `confine_host_tool_tests.rs` 83 and 309, `confine_cargo_home_tests.rs` 54, 55 and 83,
   `confine_git_tests.rs` 142; `rg -n '(::|[^a-z_.])wrap\(' src/verify/criteria` lists them
   and the definition, where a plain `wrap\(` also matches every `unwrap(`).
4. `platform_command`, in all three variants (Linux in `confine_bwrap.rs`, macOS in
   `confine_seatbelt.rs`, the unsupported-platform stub left in `confine_host.rs`), takes
   `live: Option<&LiveBoundary>` as an eighth, trailing parameter. Eight parameters trip
   `clippy::too_many_arguments` (its limit is seven) under `-D warnings`, and CI's `lint-macos`
   job compiles the Seatbelt variant, so each variant carries the attribute under a `//` reason
   line, the form `src/orchestrator/terminal/native/wrapper.rs` lines 65-67 use:

   ```rust
   // One production caller (`wrap`) and one test (`confine_host_tests.rs`) pass these
   // positionally; a struct would rewrite both call sites and all three variants.
   #[allow(clippy::too_many_arguments)]
   ```

   The seven existing parameters keep their order, so `wrap` and the test call in
   `confine_host_tests.rs` (line 262 at authoring) each append one argument (`live`, `None`)
   and keep the rest. A struct bundling the eight inputs was rejected: no caller beyond these
   two would build it, and both call sites would be rewritten instead of extended.
5. `src/verify/criteria/runner_facilities.rs` (new, plain module, re-exported):
   `runner_facilities(path: Option<&OsStr>) -> Vec<Facility>`. Linux: `UnixSockets`,
   `Loopback`, `ProcessTree`; `Bubblewrap` when
   `crate::process::sandbox_probe::bubblewrap_blocker()` is `Ok` (the host side can start
   bubblewrap, so the runner's own namespaces leave room to nest); `Tmux` when
   `which::which_in("tmux", path, "/")` resolves. macOS and others: empty. Doc comment: the list
   names what the boundary guarantees, measured outside it, so a runner regression that breaks
   one of them fails the tests that probe it instead of skipping them.
6. `confine.rs`: `prepare` takes `live: Option<&LiveBoundary>` and passes it to `wrap`; add
   `pub(super) fn prepare_live(spec, working_dir, tree, boundary) -> Result<PreparedCommand>`
   (always `Confined`, network `Denied`). `containment_policy_revision` keeps its revision
   string (`host-os-boundary-v10` after confinement-hardening; the autonomous-recovery plan,
   which runs after this one, keys its seam check on it): the facility variable is part of the
   command environment, which the cache key already hashes.
7. `executor.rs`: `pub fn run_live_with_timeout(spec, tree, working_dir, timeout, boundary) ->
   Result<CriterionResult>` = `prepare_live` then `run_prepared_with_timeout` (the spawn
   observer, pipe draining and process-group retirement come with it).
8. `cache_fingerprint.rs`: `pub(crate) fn content_digest(checkout: &Path) -> Option<String>`:
   `checkout_git(checkout)`, then a SHA-256 over the field `b"live-content-v1"` and
   `manifest::hash_manifest(&git, &repo_root, &mut remaining, &mut hasher)` with
   `remaining = HASH_BUDGET_BYTES`;
   `None` whenever the manifest is `None` (a symlink, a submodule, a flagged index entry, the
   budget). Doc: commit identity is left out, so a dirty tree and the commit of the same bytes
   share a digest.
9. `zero_tests.rs`: `zero_test_failure` becomes `pub(crate)`; `criteria/mod.rs` re-exports it
   and every new public item.

## Traps

- `confine_host.rs` is 382 lines; the split is what makes room. Keep every file under 400 and
  every function under 50.
- A file in `read_only` binds as a file (`--ro-bind` accepts a file source); Seatbelt needs
  `(literal ...)` for a file.
- AF_UNIX connect through a read-only bind of the socket's directory works on Linux (connect
  needs write permission on the socket inode, and a read-only mount refuses writes only to
  regular files, directories and symlinks); contract
  `live-profile-reaches-the-gate-and-nothing-else` proves it under the real runner.
- Your session's sandbox cannot start bubblewrap; the run contracts pass only under
  `loom check`. `cargo fmt`; your one check: `cargo test --lib verify::criteria:: --no-run`.

## Report

Files changed with final line counts of `confine_host.rs`, `confine_bwrap.rs`,
`confine_seatbelt.rs`; confirmation that no standard-runner test assertion changed; and any
facility you found the runner does not deliver.
