# test-escape-policy H5: probe call sites follow the advertised facilities

Working dir `loom/`. Read `doc/plans/briefs/live-test-lane/common.md` first ("Test guards").
You run in wave 2, after H4 wrote `loom::process::sandbox_probe::{Facility, require_facility,
FACILITIES_ENV}`. No contract covers your files; the stage's review and `cargo clippy
--all-targets` do. Every change is a call-site conversion or a text edit; no test changes what
it asserts.

## Rule

A probe that measures a facility loom's runner provides uses
`require_facility(Facility::<X>, probe_ok, test_name, why)` (same `bool` meaning as
`skip_unless`: true means return early). A probe that measures host software being installed
(`jq`, `bash`, `git`, `python3`, `srt`, `codex`) stays `skip_unless`: an absent tool is not a
boundary defect.

| Probe | Facility |
| --- | --- |
| `process_tree_visible()`, `process_group_visible()` | `ProcessTree` |
| `unix_socket_bindable(..)` alone | `UnixSockets` |
| `loopback_bindable()` | `Loopback` |
| `confined_commands_runnable()`, a direct `bwrap` start | `Bubblewrap` |
| tmux present and able to bind its socket (`skip_unless_tmux_can_bind`, `tmux && unix_socket_bindable`) | `Tmux` |
| `srt -c true` failing while `srt`, `bwrap` and `socat` are on `PATH` | `Bubblewrap` (srt starts bubblewrap) |

## Files (yours alone)

- `tests/host_verification_e2e.rs`, `tests/host_verification_contracts.rs`: the
  `confined_commands_runnable()` skips become `Bubblewrap`; the `jq`/`bash`/`git` presence
  skips stay.
- `tests/plan_session_bridge_e2e.rs` (`recovery_closes_a_live_session_and_leaves_an_unverifiable_one`):
  `Tmux`. Its module doc's "the operator's end-to-end run with a logged-in Claude Code
  (`--include-ignored`)" becomes "the end-to-end run with a real Claude Code, which runs only
  inside the live profile (`operator.rs`)".
- `tests/integration/hooks_poll_guard.rs`, `tests/integration/hooks_subagent_verify_guard.rs`,
  `tests/integration/hooks_spawn_guard.rs`: each `skip_unless_gate_visible` helper uses
  `ProcessTree`. Their callers in sibling files keep calling the helper unchanged.
- `src/plan_server/ws/tests.rs`: `Loopback`. `src/process/tests.rs`
  (`unreaped_dead_child_is_not_alive`): `ProcessTree`.
- `tests/e2e/tmux_backend.rs` `skip_unless_tmux_can_bind` and `tests/e2e/web_terminal.rs`
  `skip_or_panic`: the `LOOM_E2E_REQUIRE_TMUX` switch goes; both use
  `require_facility(Facility::Tmux, ...)` (the probe's error text stays in `why`). Rewrite the
  module docs that name the old switch.
- `tests/plan_session_confinement/srt.rs` `skip` and `probe_srt`: return the missing-tool
  reasons through `skip_unless` (platform, `bwrap`/`socat`/`srt` not on `PATH`) and the
  `srt -c true` failure through `require_facility(Facility::Bubblewrap, ...)`. Rewrite the doc
  comments: inside loom's confined runner on Linux bubblewrap nests (the runner advertises
  `bubblewrap` when the host can start it), and srt is missing there only when its install's
  symlink target is outside the runner's read roots, which is a host-software skip, never a
  nesting failure.
- `tests/plan_session_confinement.rs` header: replace "skipped (with a printed reason) unless
  `bwrap`, `socat` and `srt` are on PATH and a trivial `srt -c true` runs: the criteria runner
  and a session nest a sandbox that cannot start srt. `LOOM_TEST_REQUIRE_SANDBOX_FREE=1` turns
  a skip into a failure." with the current truth: skipped when one of the tools is missing; a
  failing `srt -c true` fails the test wherever `bubblewrap` is advertised (loom's runner on a
  host that can start bubblewrap, CI's probe step) and skips elsewhere (a Claude Code session
  sandbox forbids user namespaces).
- `src/orchestrator/terminal/native/tests_capsule_bwrap.rs`: `Bubblewrap`; rewrite its header's
  `LOOM_TEST_REQUIRE_SANDBOX_FREE` sentence the same way.
- `src/context/refresh/tests_source_graph/enumeration.rs`: the comment that mirrors
  `LOOM_E2E_REQUIRE_TMUX` names `tests/e2e/tmux_backend.rs`'s facility guard instead (its own
  `LOOM_TEST_REQUIRE_UNREADABLE_FILE` switch stays: file-mode enforcement is not a runner
  facility).
- `../.github/workflows/ci.yml`: the Linux and macOS "Run tests" steps set
  `LOOM_TEST_FACILITIES: "tmux"` in place of `LOOM_E2E_REQUIRE_TMUX: "1"`; the "Run the
  bubblewrap capsule probe" step sets `LOOM_TEST_FACILITIES: "bubblewrap"` in place of
  `LOOM_TEST_REQUIRE_SANDBOX_FREE: "1"`. Same strictness as before, nothing more.
- `../scripts/rust_test_environment.py`: the pass-through tuple becomes
  `("LOOM_TEST_FACILITIES",)` (keep the loop shape).
- `../doc/testing.md`, `../doc/sandbox-release-checklist.md`, `../CONTRIBUTING.md`: every
  sentence naming either old switch states the current truth (`LOOM_TEST_FACILITIES` lists what
  the environment provides; loom's runner sets it; tests that drive a real Claude Code run only
  in the live profile through `loom check --live`).

Then `rg -n 'LOOM_TEST_REQUIRE_SANDBOX_FREE|LOOM_E2E_REQUIRE_TMUX' --glob '!doc/plans/**'
--glob '!doc/loom/knowledge/**' ..` must list nothing but `blockages.md`,
`planning-omissions.md*`, the plan-schema fixture X1 owns, and dated reports under `../doc/`
that record history (leave those; the knowledge files are the distill stage's).

## Traps

- Keep each probe's own `why` text; change only the guard.
- `tests/plan_session_bridge_e2e/operator.rs` and the other three operator files are H4's.
- `cargo fmt`; your one check: `cargo test --no-run --test plan_session_confinement`.

## Report

Files changed, each converted call site with the facility chosen, and every probe you left as
`skip_unless` with why.
