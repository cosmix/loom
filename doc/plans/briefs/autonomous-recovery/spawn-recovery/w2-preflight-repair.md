# spawn-recovery W2: typed preflight refusals and hook self-repair for every spawn

Tier: opus. Wave 1, in parallel with W1 (code against W1's pinned
`daemon_binary_replaced() -> Option<PathBuf>`). Read
`doc/plans/briefs/autonomous-recovery/common.md` first. Frozen contracts:
`src/sandbox/config/preflight/repair_contract_tests.rs`.

## Why

Every session spawn (stage, merge resolver, adjudicator, knowledge) calls
`crate::sandbox::preflight::require_confined_host(&host.facts)` in
`src/orchestrator/terminal/native/launch.rs` (`prepare_session_launch_with`). A refusal is a
list of strings (`SandboxPreflightRefusal` in `src/sandbox/config/preflight.rs`), classified
only as "sandbox setup failure, never retried" (`orchestrator/core/crash_classification.rs::
spawn_failure_type`). Hook scripts that are missing or differ from this build are the common
refusal after an install, and two different repairs apply: the daemon is current and the
installed hooks are stale (reinstall them from the embedded copy, which `loom repair --fix`
does), or the daemon itself is stale (W1 restarts it; reinstalling the OLD daemon's hooks over
the new ones would break the new binary).

## Interface (pinned)

In `src/sandbox/config/preflight.rs`:

```rust
pub(crate) enum RefusalKind { HookDrift, NoHooksDir, WritableInstall, WritableHookTool, Policy, StaleDaemon }
impl SandboxPreflightRefusal {
    pub(crate) fn with_kinds(refusals: Vec<(RefusalKind, String)>) -> Self;
    pub(crate) fn kinds(&self) -> Vec<RefusalKind>;
}
```

`new(refusals)` stays (kind `Policy` for each, its current callers are policy checks).
`host_refusals` returns `(kind, message)` pairs: `hook_script_refusals` gives `HookDrift` (a
drifted or missing script in a verified directory) or `NoHooksDir`; `install_refusals` gives
`WritableInstall`; `tool_refusals` gives `WritableHookTool`. `Display` is unchanged.

New `src/sandbox/config/preflight/repair.rs` (declared from `preflight.rs`):

```rust
pub(crate) enum HookRepair { None, Reinstalled(usize) }
/// Checks 2-4 with one repair: when every refusal is `HookDrift`, the hooks directory is
/// verified (`facts.hooks_dir` is `Some`), and `daemon_current` is true, reinstall the hook
/// scripts into that directory (`crate::fs::permissions::install_loom_hooks_to`) and check
/// again. With `daemon_current` false and a `HookDrift` refusal, refuse with an extra
/// `StaleDaemon` entry: "the running daemon's binary was replaced at <path>; loom restarts the
/// daemon onto it". Any other refusal is returned as is, nothing written.
pub(crate) fn require_confined_host_repairing(facts: &HostFacts, daemon_current: bool, replaced: Option<&Path>) -> Result<HookRepair, SandboxPreflightRefusal>;
```

## Edits (you own these files)

1. `src/sandbox/config/preflight.rs`, `src/sandbox/config/preflight/repair.rs`: the interface
   above. A reinstall logs `tracing::info!` with the count and the directory.
2. `src/orchestrator/terminal/native/launch.rs`: `prepare_session_launch_with` calls
   `require_confined_host_repairing(&host.facts, replaced.is_none(), replaced.as_deref())`
   with `replaced = host::daemon_binary_replaced()` (W1's function in `launch/host.rs`). You
   own the two re-export lines too: add `daemon_binary_replaced` to `launch.rs`'s
   `pub(crate) use host::{...}` and to `src/orchestrator/terminal/native/mod.rs`'s
   `pub(crate) use launch::{...}`, so W1's tick check calls
   `crate::orchestrator::terminal::native::daemon_binary_replaced()`. Confirm the tmux lane
   reaches the same function
   (`rg -n 'prepare_session_launch' src/orchestrator/terminal`); if the tmux lane runs its own
   preflight, route it through the same call.
3. `src/commands/run/confinement.rs` (`loom run`'s preflight) keeps refusing on drift with no
   repair: `loom run` is the operator's start, and its refusal text already says
   `loom repair --fix`. Only update it for the new `host_refusals` return type.
4. Stage and merge-resolver spawns: find where a spawn error becomes a blocked stage
   (`rg -n 'SandboxSetupFailure' src/orchestrator/core`: `spawn_setup.rs`, `stage_preflight.rs`,
   `merge_handler/spawn_failure.rs`). A refusal whose kinds include `StaleDaemon` leaves the
   stage `Queued` (stage) or the merge pending (resolver), logs once per stage, and records no
   `failure_info`: the restart W1 performs retries it. Every other refusal keeps today's
   behaviour.
5. `src/orchestrator/core/crash_classification.rs`: add
   `pub(crate) fn preflight_kinds(error: &anyhow::Error) -> Option<Vec<RefusalKind>>`
   (downcast through the chain, as `spawn_failure_type` does); `spawn_failure_type` unchanged.
6. Tests beside each module: kinds per refusal source; the stage spawn path leaves a
   `StaleDaemon` refusal queued; a `WritableInstall` refusal still blocks.

## Two hazards the repair must close

- Atomic replacement: read `crate::fs::permissions::install_loom_hooks_to`. If it writes a
  script in place, a session running that hook at the same moment can read half a file: make
  each write a temp file in the same directory plus a rename, and test it.
- Two loom builds sharing one hooks directory (two projects whose daemons run different
  versions) would each reinstall their own copy on every spawn. Record each reinstall's time in
  `<work_dir>/hook-repairs.json`; a second drift within 10 minutes of a reinstall is not
  repaired again but refused with kind `Policy` and the message "another loom build keeps
  installing its own hooks into <dir>; one hooks directory serves one loom build at a time",
  which W3 classifies as permanent. Test both.

## Traps

- `owned_dir` verification (`launch/host.rs::verified_hooks_dir`) is what makes a reinstall
  safe: never install into a directory that check did not verify, and never create one.
- The repair re-checks with a fresh `hook_script_refusals(facts.hooks_dir)`; it does not
  recompute the other checks' facts.
- `loom run`'s refusal stays: this repair is for a running daemon, whose operator is not at the
  terminal.

## Done

Your one check after the stage compiles: `cargo test --lib sandbox::config::preflight`.
