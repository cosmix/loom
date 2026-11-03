# test-escape-policy H4: the facility and live-profile guards, and the operator tests

Working dir `loom/`. Read `doc/plans/briefs/live-test-lane/common.md` first ("Test guards").
The frozen contracts are `tests/test_guard_contracts.rs` (and `tests/test_guard_contracts/**`);
never edit them. The stage description's PUBLIC SURFACE is authoritative. You run in wave 1
beside X1; H5 (wave 2) converts the existing probe call sites to your API, so keep every name
exactly as below. The live-profile stages write the marker this guard reads and import your
constants; they never redefine them.

## Why

`process/sandbox_probe.rs` `skip_unless` turns an environment skip into a panic when
`LOOM_TEST_REQUIRE_SANDBOX_FREE=1`, and the repository's docs and test headers tell a reader to
set it and run host-only suites directly on a host. Two things replace it:

- Strictness per facility: loom's runner (and CI) list the facilities they provide in
  `LOOM_TEST_FACILITIES`; a probe for a listed facility that fails is a failure, any other probe
  failure skips. The variable only adds strictness, so it needs no anti-forgery.
- The live profile: tests that drive a real Claude Code run only inside it. Its marker must not
  be satisfiable by an exported variable alone, because the marker turns on tests that write
  `$HOME/.claude.json` and use the network: the guard needs the variable, a regular marker file
  in `HOME` holding the same run id, and a `HOME` that is not the account's passwd home.

## Files (yours alone)

`src/process/sandbox_probe.rs`, `src/process/sandbox_probe/facilities.rs` (new; declared from
`sandbox_probe.rs` as `pub mod facilities;` with `pub use facilities::{...}` of every public
item, so callers write `loom::process::sandbox_probe::require_live_profile`),
`tests/plan_session_bridge_e2e/operator.rs`, `tests/planning_guidance_operator.rs`,
`tests/plan_chat_operator.rs`, `tests/plan_handoff_operator.rs`. Do not edit
`src/process/mod.rs` (confinement-hardening edits it concurrently; `sandbox_probe` is already
`pub mod` there).

## `sandbox_probe.rs` (261 lines; keep it under 400)

- `skip_unless(probe_ok, test_name, why) -> bool` stays and never panics: it prints
  `SKIP {test_name}: {why}` and returns true when the probe failed; inside the live profile
  (`in_live_profile()`, memoized) it also writes `loom-live-profile: skip {test_name}: {why}`
  through the same stderr-handle helper the profile guard uses, so a live entry whose tests
  skipped fails instead of passing on nothing. Remove every mention
  of `LOOM_TEST_REQUIRE_SANDBOX_FREE`; rewrite the module doc and the function doc to the
  current truth (a probe of a facility the environment advertises goes through
  `require_facility`). The unit test `skip_unless_obeys_policy_when_the_probe_failed` becomes
  `skip_unless_skips_when_the_probe_failed`.
- Rename the private `confined_command_blocker` to `pub fn bubblewrap_blocker() -> Result<(),
  String>`, memoized in a `OnceLock`, and make `confined_commands_runnable` use it. The
  live-profile stage calls it from the runner to decide whether to advertise `bubblewrap`.

## `sandbox_probe/facilities.rs`

The PUBLIC SURFACE in the stage description lists every item and its exact behaviour. Notes:

- `Facility` derives `Debug, Clone, Copy, PartialEq, Eq, Hash`; `Facility::ALL` is the nine in
  the order of common.md; `name()` and `from_name()` use the kebab names there. `Codex` is a
  facility no environment advertises yet; its doc says so and names plan 11's tests as the
  reason it exists (they drive codex, which needs a credential the gate does not serve).
- `facilities_from(value)` splits on `,`, trims, drops unknown names and duplicates, and returns
  them in `ALL` order. `advertised_facilities()` reads `FACILITIES_ENV`.
- `require_facility` prints its skip with `eprintln!` (libtest captures it, which is fine for a
  skip outside the profile), writes the profile skip line inside the profile exactly as
  `skip_unless` does, and panics on `FacilityDecision::Fail` with
  `"{test_name}: {why} ({name} is provided here, so this probe must pass)"`.
- `account_home() -> Option<PathBuf>`: `libc::getpwuid_r(libc::geteuid(), ...)` with a 16 KiB
  buffer, the `pw_dir` copied out; `None` on any error. Every `unsafe` block gets a
  `// SAFETY:` line (`src/lib.rs` denies undocumented unsafe). Do not use `dirs::home_dir()`:
  it returns `$HOME` first.
- `check_profile(home, run_id, account_home)`: `Err` names the first thing missing, in this
  order: run id unset or empty; `HOME` unset or relative; `HOME/.loom-live-profile` missing,
  a symlink (`symlink_metadata`) or not a regular file; its trimmed content differs from the run
  id; the account home unknown; `HOME` equal to the account home after `canonicalize` of both.
- `require_live_profile(test_name, needs)` writes the decision's line plus `\n` with
  `std::io::stderr().write_all(...)` (not `eprintln!`: libtest captures the print macros and
  would hide the line in an ordinary run) and returns true only for `ProfileDecision::Run`.
- Unit tests in `facilities.rs` (a `#[cfg(test)] mod tests` at the bottom, or a
  `facilities_tests.rs` beside it declared the same way) for `facilities_from`, each
  `facility_decision` branch and each `check_profile` refusal, with a `TempDir` playing `HOME`.
  Tests that set process variables are `#[serial_test::serial]` and restore them.

## The operator tests

Each test that drives a real Claude Code: rewrite its `#[ignore = "..."]` reason to start with
`loom-live-profile:` and name what it needs (`"loom-live-profile: drives a real Claude Code in
tmux"`), and make its first statement
`if !loom::process::sandbox_probe::require_live_profile("<test name>", &[Facility::ClaudeCode,
Facility::Tmux, Facility::Bubblewrap, Facility::NetworkGate, Facility::WritableHome]) {
return; }` (a test returning `Result` returns `Ok(())`). `Bubblewrap` is there because loom's
planning sessions run with `sandbox.failIfUnavailable: true`, so Claude Code's own sandbox must
start inside the profile. Confirm each name with `loom map --outline <file>` first; a file that
does not exist is a finding to report, never a file to create.

- `tests/plan_session_bridge_e2e/operator.rs` `operator_end_to_end_with_claude_code` (plan 07).
  Its `TrustGuard` doc says it removes the entry "from the real `~/.claude.json`": rewrite it to
  "from `$HOME/.claude.json`, the live profile's private home". The test already reads `HOME`
  from the environment and `untrust_worktree` resolves `dirs::home_dir()`, which reads `$HOME`
  first; change no logic.
- `tests/planning_guidance_operator.rs`: the guard goes in the shared `fn operator(...)` that its
  four tests (`operator_{vision,section,metaplan,plan}_session_loads_its_skill`, plan 08) call,
  replacing `missing_prerequisites` (delete it; the guard and the facility list cover tmux and
  claude). The `LoomHome` doc "Keep the real HOME for Claude authentication" becomes "HOME is
  the live profile's private home; Claude Code authenticates through the profile's gate".
- `tests/plan_chat_operator.rs` (plan 09, `real_session_attach_send_restore` per its brief) and
  `tests/plan_handoff_operator.rs` (plan 11: `operator_pressure_run_records_one_line`,
  `operator_stopped_run_writes_no_record`, `operator_commit_and_start_run` per its brief): each
  `#[ignore]` test, or their shared helper when one exists. Plan 11's tests also drive codex:
  their needs list adds `Facility::Codex`, so inside the profile they report unavailable rather
  than skip their codex probe; their own prerequisite `skip_unless` stays after the guard.

`rg -n '#\[ignore' tests src` and check every other ignored test: one that drives a real
`claude` gets the guard too; list the rest in your report with why they do not (terminal
emulator, measurement harness, stress fixture).

## Traps

- Never create `.loom-live-profile` in a real home from a test; `check_profile` takes paths so
  its tests use a `TempDir`.
- Probe call sites outside the four operator files are H5's.
- `cargo fmt`; your one check: `cargo test --lib process::sandbox_probe:: --no-run`.

## Report

Files changed, every operator test guarded (file and name), every `#[ignore]` test left
unguarded with the reason, and the final line count of `sandbox_probe.rs`.
