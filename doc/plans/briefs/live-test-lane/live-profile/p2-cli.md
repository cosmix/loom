# live-profile P2: the `loom live` commands

Working dir `loom/`. Read `doc/plans/briefs/live-test-lane/common.md` first. The frozen
contracts are `tests/live_cli_contracts.rs` and `tests/live_cli_contracts/**`; never edit them.
The stage description's PUBLIC SURFACE is authoritative. You run in parallel with P1; call
P1's `forward::serve`, `entry::run_entry` and `profile::find_claude` by the names in the PUBLIC
SURFACE.

Files (yours alone): `src/cli/types_live.rs` (new), `src/cli/types.rs`, `src/cli/types_tests.rs`
(new, moved from `types.rs`), `src/cli/dispatch.rs`,
`src/cli/mod.rs` (declare `pub mod types_live;` beside the other `types_*` modules),
`src/commands/mod.rs`, `src/commands/live/{mod.rs, exec.rs, credential.rs, status.rs, smoke.rs,
tests.rs}`, `maintainability-baseline.txt` (one row, below).

## Commands

```text
loom live credential set --kind <api-key|oauth-token>
loom live credential clear
loom live status
loom live smoke
loom live exec --listen <addr> --gate <socket> -- <program> [args...]   (hidden)
```

- `types_live.rs`: `pub enum LiveCommands { Credential { #[command(subcommand)] command:
  LiveCredentialCommands }, Status, Smoke, #[command(hide = true)] Exec(LiveExecArgs) }` with
  doc comments that become `--help` text (credential: "the operator's credential for the live
  profile's gate; stored in this project's state root, mode 0600, read only by loom's host
  job"; smoke: "one minimal API call through the live profile; needs bubblewrap (Linux) or
  sandbox-exec (macOS), a native Claude Code and a credential"). `--kind` parses through
  `CredentialKind::from_str`. `LiveExecArgs { #[arg(long)] listen: SocketAddr, #[arg(long)]
  gate: PathBuf, #[arg(last = true, required = true)] command: Vec<String> }`.
- `types.rs` first gives back room, then gains the variant. It is 392 lines at authoring
  (`wc -l`): imports, the `#[path]` module declarations, `Cli`, the one `Commands` enum, and
  an inline `#[cfg(test)] mod tests { .. }` of 45 lines (lines 348-392: four parse tests). A
  clap `Subcommand` enum cannot be split across files, and every other subcommand enum already
  lives in a `types_*.rs`, so the test module is what moves:
  1. Move the body of `mod tests` verbatim (dedented one level, `use super::*;` kept) into the
     new `src/cli/types_tests.rs`, and declare it where the inline module was:

     ```rust
     #[cfg(test)]
     #[path = "types_tests.rs"]
     mod tests;
     ```

     the form `src/codex.rs` uses for `codex_tests.rs`. The module path stays
     `cli::types::tests`, so no test name changes. `types.rs` matches none of the Rust
     profile's test-file globs (`src/testrun/languages.rs`) and `types_tests.rs` is new, so the
     move raises no `TI-decl`, `TI-assert` or `TI-edit` event. `types.rs` is then 350 lines.
  2. Add `use super::types_live::LiveCommands;` beside the other imports and, in `Commands`,
     `/// Run tests that drive a real Claude Code inside loom's live profile` then
     `Live { #[command(subcommand)] command: LiveCommands },` (five lines plus its blank line).
     `types.rs` ends at about 357 lines; report the count. The autonomous-recovery plan's
     spawn-recovery W1 later adds a hidden `--take-over` argument to `Commands::Run` (about
     four lines), which then fits with no condensing.
- `dispatch.rs` `dispatch` has an exact ledger row (`function src/cli/dispatch.rs dispatch 69`).
  Replace the three-line `Commands::Project { command: ProjectCommands::Detect { path, json } }
  => project::execute(path, json),` arm with the one-line `cmd @ (Commands::Project { .. } |
  Commands::Live { .. }) => dispatch_project_or_live(cmd),` and add the private fn
  `dispatch_project_or_live(cmd: Commands) -> Result<()>` below `dispatch_target`. Read the new
  length of `dispatch` from `cargo test --test maintainability` output and lower its row in
  `maintainability-baseline.txt` to that exact value (a tightening; the stage disputes the
  `TI-ratchet` event it raises).
- `commands/live/mod.rs`: `pub fn execute(command: LiveCommands) -> Result<()>` routing to the
  modules; `pub mod smoke;` (the contracts call `loom::commands::live::smoke::run_with`), the
  other modules private. `src/commands/mod.rs` declares `pub mod live;`.
- `exec.rs`: `forward::serve(listen, &gate)`, then spawn `command[0]` with the rest as args,
  stdio inherited; wait; exit with its code, or `128 + signal` when a signal ended it. The
  forwarder threads die with the process.
- `credential.rs`: `set` refuses when `LOOM_SESSION_ID`, `LOOM_STAGE_ID` or `LOOM_PLAN_SESSION`
  is set ("the live credential is the operator's to set; a loom session may not"), resolves the
  state root with `crate::commands::common::work_dir_path()`, reads one line from stdin (when
  stdin is a terminal, print a prompt to stderr and turn echo off with `nix::sys::termios` for
  the read, restoring it after), builds `Credential::new(kind, &line)`, stores it, prints
  `stored <kind> credential at <path>`. The value never reaches stdout, stderr or a log.
  `clear` refuses in a session the same way, removes it and says whether one existed.
- `status.rs`: four lines, always exit 0, each line computed on its own so one failure never
  hides the others: `boundary: bubblewrap at <path>` | `boundary: sandbox-exec` |
  `boundary: missing (<reason>)`; `claude: <path> (<version>)` | `claude: missing (<reason>)`;
  `credential: <kind> set <time>` | `credential: missing (run loom live credential set --kind
  api-key|oauth-token)` | `credential: not readable here (<reason>)`; `smoke: passed|failed at
  <time> (loom <version>[: <reason>])` | `smoke: never run`. Without a loom workspace the
  credential and smoke lines say `no loom workspace here`.
- `smoke.rs`: `SmokeEnv`, `SmokeReceipt` and `run_with` exactly as the PUBLIC SURFACE gives
  them. `run_with`: refuses nothing itself (the CLI refuses in a session); with no credential
  it writes a failed receipt naming `loom live credential set` and returns it; with `claude`
  `Err` a failed receipt naming the reason; else one `run_entry` with command `claude -p
  --max-turns 1 --model haiku 'Reply with the single word ready.'` (`SMOKE_COMMAND`), a fresh
  `tempfile::TempDir` as tree and working dir, 300 s, `hosts = ANTHROPIC_HOSTS`, no zero-test
  guard. Passed only when the outcome is `Passed` AND the gate log holds a request line for
  `api.anthropic.com/v1/messages` with status 200 (proof the call crossed the gate). Write the
  receipt to `<work_dir>/verification/live-smoke.json` (temp file + rename, 0644) for every run,
  passed or not. The CLI `smoke` refuses in a session, builds the env (`Upstream::public()`,
  `std::env::current_exe()`, `find_claude()`), prints `PASS` or `FAIL: <reason>`, and exits 1
  on `FAIL`.

## Tests (`commands/live/tests.rs`)

Status line rendering for present and missing pieces; the session refusal of `credential set`
and `clear`; the receipt shape for a failed smoke.

## Report

Files changed, the new exact `dispatch` length, and the final `wc -l` of `src/cli/types.rs`.
