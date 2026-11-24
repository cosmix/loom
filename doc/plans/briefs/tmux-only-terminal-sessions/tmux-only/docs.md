# Brief: docs (tmux-only stage)

Loom is dropping native terminal-window sessions. Every orchestrated session runs in its own detached
tmux server. You update operator and contributor docs, the installer and one evaluation file.

## Facts to document (pinned; code changes in parallel)

- `loom init <plan>` has no terminal question and no `--backend`; `loom run` has no `--backend`.
- Missing tmux: `loom init` warns and finishes; `loom run` (background and `--foreground`) and any
  direct session spawn fail with exactly `Loom needs tmux on PATH to start sessions. Install tmux and retry.`
  Plan verification, `loom config`, `loom status` and history keep working without tmux.
- `loom attach` opens the tiled overview; `loom attach <stage-id>` opens one stage; detach with the
  tmux prefix then `d` (default `Ctrl-b d`); re-run `loom attach` to reattach.
  `loom status --web --terminals` enables browser view/control.
- `terminal.backend` is gone from `loom config`, dashboard Settings and the planner Run dialog.
  A leftover `[terminal]` section in `~/.loom/config.toml` or `.loom/work/config.toml` is ignored and
  can be deleted. The key count in the config registry drops by one (36 -> 35 keys; 34 -> 33 with a
  project tier); recount from `loom/src/user_config/keys.rs` after the config-cli worker's change
  (the `terminal.backend` KeySpec is the one removed).
- Upgrading: let running native-terminal sessions finish, stop the old daemon, install tmux, then
  start the new binary with `loom run`. No `loom init --clean` is needed.
- Unchanged: `codex.backend`, `plan.summary_backend`, `loom ask --backend`, planning-chat servers.
- Moved paths: `loom/src/orchestrator/terminal/native/` is now `loom/src/orchestrator/terminal/launch/`;
  `native/launch.rs` is now `launch/prepare.rs`; `native/session_settings/` is now
  `launch/session_settings/`.

## Files you own

`README.md`, `CHANGELOG.md`, `install.sh`, `loom/tests/installer_tmux_advisory.rs` (new),
`CONTRIBUTING.md`, `doc/sandbox-release-checklist.md`, `doc/product/feat-planner/section_11.md`,
`doc/product/feat-planner/section_12.md`, `skills/loom-usage/SKILL.md`, `loom/eval/retrieval-cases.yaml`.

## Steps

1. **README.md.** Locate every edit by the text anchor below (verify each with `rg -n` first):
   - the `(tmux backend only)` phrases: the `Add --terminals (tmux backend only)` sentence, the
     `loom attach [stage-id]` synopsis comment (`# tmux backend only; omit the id ...`) and the
     `loom status --web --terminals` paragraph;
   - the feature bullet "Spawn sessions in a native terminal window or a headless tmux backend": sessions
     run in detached tmux servers, usable over SSH and WSL;
   - the requirements table row whose first cell is `tmux`: tmux required to start sessions;
   - the WSL and headless lines that link `#terminal-backends` (in the platform list near "Windows via
     WSL2" and "Headless (SSH, no terminal emulator)");
   - the two synopsis lines containing `[--backend native|tmux]`: drop the option;
   - the config table row whose first cell is `terminal.backend`: delete it;
   - the sentences "thirty-six keys" and "Thirty-four of the thirty-six keys": recount, and drop
     `terminal` and `terminal.backend` from the lists;
   - the two "whole section" sentences that name `[terminal]` beside `[context]`: keep `[context]`;
   - the `## Terminal Backends` section with `### Selecting a backend` and the WSL walkthrough: rename
     the section to `## Terminal Sessions`, delete `### Selecting a backend`, keep `### Running under
     WSL` and the later subsections (their anchors stay valid), and cover the facts above briefly;
   - the link `[Selecting a backend](#selecting-a-backend)` and every `#terminal-backends` link: repoint
     to `#terminal-sessions` (report any link to the old anchor in files you do not own).
   The `loom subagents ...` synopsis comments that say "Native backend" are Codex worker lines and
   stay; so does the `update.check` config row. Link text `[Terminal Backends]` becomes
   `[Terminal Sessions]`. Final check: `rg -n '\[terminal\]|terminal\.backend|LOOM_TERMINAL|#terminal-backends|#selecting-a-backend|--backend native|--backend tmux|native\|tmux' README.md`
   prints nothing and `rg -n '^## Terminal Sessions' README.md` prints one line.
2. **CHANGELOG.md.** Add `## [Unreleased]` above `## [1.3.x] - 2026-10-22`. The file's sections run
   Added, Changed, Removed: write `### Changed` (the missing-tmux error and the upgrade steps) before
   `### Removed` (the native terminal backend, `--backend` on init/run, `terminal.backend`, the planner
   backend choice, `LOOM_TERMINAL`). Match the file's existing bullet style (bold lead, em dash).
   The stage greps the file for two exact strings, so write them verbatim: the error text
   `Loom needs tmux on PATH to start sessions. Install tmux and retry.`, and the upgrade bullet's
   sentence `let running native-terminal sessions finish, stop the old daemon, install tmux, then start
   the new binary with` followed by `` `loom run` `` (one line in the file, letter case free; no
   `loom init --clean`).
3. **install.sh.** It is bash. The runtime-tool check lives in `check_runtime_tools()` and uses the
   `warn` and `info` helpers. Add a non-fatal tmux advisory there: when `command -v tmux` fails, `warn`
   that Loom needs tmux to start sessions and `info` the install hint (`brew install tmux` on Darwin,
   `sudo apt install tmux` on Linux), following the script's existing `uname` handling. Installation
   still succeeds. The stage greps for the literal `command -v tmux`. Check with `bash -n install.sh`.
   Then write `loom/tests/installer_tmux_advisory.rs` (a new integration test target; Cargo discovers
   `tests/*.rs`), modelled on `loom/tests/installer_decline_contracts.rs`: it sources the installer
   with `LOOM_INSTALL_LIB_ONLY=1` and runs `check_runtime_tools` under `env_clear()` with a `PATH` of
   one `TempDir` `bin` directory. Fill it with executable `#!/bin/sh` stubs for `jq`, `rg` and `fd`
   (the check exits on a missing `jq` and warns on missing search tools) and symlinks to the real
   `uname` and `dirname` (found with `which::which`; the installer's top level calls `dirname`); run
   `bash` by its absolute path. Test 1: without a `tmux` entry, exit code 0 and the combined output
   names tmux and its install hint. Test 2: with a `tmux` stub added, exit code 0 and the output does
   not name tmux. Run it with `cargo test --manifest-path loom/Cargo.toml --test installer_tmux_advisory`
   (an acceptance criterion runs the same command).
4. **CONTRIBUTING.md.** Anchor by text: replace the note "CI runners have no terminal emulator; a test
   that builds an `Orchestrator` must configure the tmux backend ..." with: tests that spawn sessions
   need tmux and guard AF_UNIX denial with the existing `sandbox_probe` helpers. Update the line naming
   `terminal/native/` paths (the sentence about a release that changes the session capsule) to the moved
   paths.
5. **doc/sandbox-release-checklist.md**: the "Before tagging a release that changes ..." sentence at the
   top names `terminal/native/` paths: update to the moved paths.
6. **doc/product/feat-planner/section_11.md**: the list item `4. **The backend.**` and its follow-on
   paragraph, the `**Start.**` paragraph (it runs `loom init <plan> --backend <backend>`), and the
   dashboard paragraph that says "with `--terminals` when the run's backend is tmux": the Run dialog has
   no backend choice; Start runs `loom init <plan>` (with `--clean` when required) and `loom run`; Open
   dashboard always starts the dashboard with `--terminals`. Remove the paragraph about loom's built-in
   default and `SessionBackendKind`.
7. **doc/product/feat-planner/section_12.md**: change the `orchestrator/terminal/native/...` paths to
   `orchestrator/terminal/launch/...` (`rg -n 'terminal/native' doc/product/feat-planner/section_12.md`).
   **skills/loom-usage/SKILL.md**: the sentence "Spawns Claude Code sessions in terminal windows" says
   tmux sessions; change that sentence only.
8. **loom/eval/retrieval-cases.yaml**: delete the two `forbid` entries naming
   `terminal/native/window_ops.rs` (the file is deleted); keep the third entry.

## Rules

- Line numbers here are hints from the plan's base tree. Locate every edit by the quoted symbol or
  text; if a named site is not there, report it instead of editing the nearest line.
- Plain, short prose in the files' existing style. Every fenced code block names a language.
- No history narrative in README beyond the upgrade note; the CHANGELOG carries what was removed.
- After your edits `rg -n 'terminal/native' README.md CONTRIBUTING.md doc/sandbox-release-checklist.md doc/product/feat-planner/section_12.md loom/eval/retrieval-cases.yaml`
  prints nothing.
- The later stage operator-recovery-gaps edits other README sentences (dispute and amend field lists);
  leave those alone.
- Report: sections changed, links you could not update, and anything that contradicted the facts.
