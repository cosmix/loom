# confined-tools W1: the boundary exposes each tool's roots, on Linux and macOS

Tier: sonnet. Wave 2, after C1. Read `doc/plans/briefs/autonomous-recovery/common.md` first.
Frozen contracts: `src/verify/criteria/tool_roots_contract_tests.rs`.

## Seam check first

The live-test-lane plan must have merged: `rg -n 'host-os-boundary-v10' src/verify/criteria`
finds the containment revision, `rg -n 'Inherit' src/verify/criteria/confine.rs` finds no
plan-facing variant, and `rg -n 'live_seatbelt_rules\(' src/verify/criteria/confine_seatbelt.rs`
finds the split. If any fails, stop and report: the stage's `before_stage` checks should have
refused to start.

## The tree after the live plan's split (live-gate-and-boundary B1)

- `confine_host.rs` keeps `wrap(original, tree, network, live: Option<&LiveBoundary>)`,
  `apply_environment`, `resolve_program`, `environment_path`, `cargo_home`, `read_roots` and
  `writable_roots`, and declares the platform builders as `#[cfg(target_os = ...)] #[path =
  "confine_<name>.rs"] mod <name>;` children.
- `confine_bwrap.rs` (Linux): `platform_command`, `bind_read_roots`, `bind_resolver_target`.
  `bind_read_roots` binds every root `read_roots(original)` returns with `--ro-bind`, a file as
  a file, so it needs no edit.
- `confine_seatbelt.rs` (macOS): `seatbelt_path_rules`, `platform_command`, plus the live rules.
- `wrap` sets `HOME`, `PATH` and the live variables on the wrapped command only after
  `read_roots` ran on the ORIGINAL command, so `read_roots` sees the command's own `PATH` and
  `HOME` in a live run too.

## Edits (you own these files)

1. `src/verify/criteria/confine_host.rs`
   - `read_roots(original)` keeps its signature and adds `tool_roots::tool_roots(program, path,
     home)` for the program resolved as `resolve_program` resolves it and, when the program is
     a shell run as `sh -c <script>` (the form `CommandSpec::Shell` builds; read
     `build_command` in `confine.rs`), for every word of `tool_roots::tool_words(script)` that
     `which::which_in(word, path, cwd)` resolves. `path` and `home` come from the command's own
     environment (`environment_path(command, "PATH"|"HOME")`), never from loom's process
     environment.
   - `wrap` returns a `Wrapped { command, scratch, git, read_roots: Vec<PathBuf> }` struct
     instead of the tuple, so the caller can name what was exposed. `read_roots` is
     `read_roots(original)`; when `live` is `Some(boundary)` it also lists `boundary.home`,
     `boundary.tmp`, the directory of `GateMount::UnixDir` and every `boundary.read_only` path,
     so W2's diagnosis never reports a live profile's tool as hidden.
2. `src/verify/criteria/confine_seatbelt.rs` (`seatbelt_path_rules`): a read root that is a
   file gets `(allow file-read* (literal ...))` and `(deny file-write* (literal ...))`, a
   directory keeps `(subpath ...)`; call `read_roots(original)` once there and reuse the list.
   The live rules B1 added stay byte-identical.
3. `src/verify/criteria/confine.rs`: carry `read_roots` on `PreparedCommand` with
   `pub(crate) fn exposed_roots(&self) -> &[PathBuf]` (empty when unconfined; `prepare_live`
   carries the live paths through the same field). `containment_policy_revision` becomes
   `host-os-boundary-v11`: what the boundary exposes changed, so a pass cached under v10 must
   not certify a run under v11. Update every test that pins the revision string
   (`rg -n 'host-os-boundary-v10' src tests`), listing each changed assertion line.
4. The tuple-destructuring `wrap` call sites in tests. After the live plan merges `wrap` has
   eight callers: `confine.rs` `prepare` (step 3) and seven test sites that pass `None` as the
   live argument. `rg -n '(::|[^a-z_.])wrap\(' src/verify/criteria` lists them (a plain
   `wrap\(` also matches every `unwrap(`); list the result in your report. Call sites move to
   the `Wrapped` fields; no assertion changes:
   - `src/verify/criteria/confine_host_tests.rs`: the one `host::wrap(.., None)` site.
   - `src/verify/criteria/confine_host_tool_tests.rs`: the two sites.
   - `src/verify/criteria/confine_cargo_home_tests.rs`: the three sites.
   - `src/verify/criteria/confine_git_tests.rs`: the one site.

   No other worker of the stage owns the last two files. Any further caller the search lists
   moves the same way; report it.
5. `src/verify/criteria/confine_host_tool_tests.rs` (existing): add Linux and macOS argv tests
   for a Bun-global fixture; do not change existing assertions.

## Traps

- `/opt/homebrew` and `/usr` (which holds `/usr/local`) are fixed roots already: Homebrew on
  macOS needs no new root, and `tool_roots` returns none for a target inside an existing root
  (C1's rule 1 checks containment; pass the fixed list through if C1's signature needs it and
  report the change).
- Never expose `~/.codex` or `~/.claude` wholesale: a `codex` word resolving to
  `~/.codex/packages/standalone/current/bin/codex` exposes that file only (C1's rule 2 last
  case).
- Keep `read_roots` under 50 lines: move the tool part into a helper. Keep every file under
  400 lines: B1 split `confine_host.rs` to make room.
- `platform_command` in both platform files already takes the live boundary from B1; do not
  add a parameter to it.

## Done

Your one check after the stage compiles: `cargo test --lib verify::criteria::confine_host`.
