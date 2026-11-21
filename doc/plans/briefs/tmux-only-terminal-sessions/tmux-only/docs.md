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
  can be deleted. The key count in the config registry drops by one (36 → 35 keys; 34 → 33 with a
  project tier); recount from `loom/src/user_config/keys.rs` after the config-cli worker's change
  (the `terminal.backend` KeySpec is the one removed).
- Upgrading: let running native sessions finish, stop the old daemon, install tmux, start the new
  binary with `loom run`. No `loom init --clean` is needed.
- Unchanged: `codex.backend`, `plan.summary_backend`, `loom ask --backend`, planning-chat servers.
- Moved paths: `loom/src/orchestrator/terminal/native/` is now `loom/src/orchestrator/terminal/launch/`;
  `native/launch.rs` is now `launch/prepare.rs`; `native/session_settings/` is now
  `launch/session_settings/`.

## Files you own

`README.md`, `CHANGELOG.md`, `install.sh`, `CONTRIBUTING.md`, `doc/sandbox-release-checklist.md`,
`doc/product/feat-planner/section_11.md`, `loom/eval/retrieval-cases.yaml`.

## Steps

1. **README.md.** Edit these places (line numbers from the base tree): 91 and 835 (drop "(tmux backend
   only)"), 251 (feature bullet: sessions run in detached tmux servers, usable over SSH and WSL),
   436 (requirements row: tmux required to start sessions), 475-477 (WSL and headless lines),
   500-501 (CLI synopsis without `--backend`), 680 (attach comment: drop "tmux backend only"),
   739 (delete the `terminal.backend` config row), 816 and 827 (recounted keys; drop `terminal` and
   `terminal.backend` from the lists), and the `## Terminal Backends` section at 1338 onward plus
   `### Selecting a backend` and the WSL walkthrough. Rename the section to `## Terminal Sessions`
   and update every `#terminal-backends` link in README (`rg -n "#terminal-backends" README.md`);
   report any link to it in files you do not own. The section covers the facts above, briefly.
   README must not contain `--backend native`, `--backend tmux` or `native|tmux` afterwards.
2. **CHANGELOG.md.** Add `## [Unreleased]` above `## [1.3.x] - 2026-10-22` with a `### Removed`
   list: the native terminal backend, `--backend` on init/run, `terminal.backend`, the planner backend
   choice, `LOOM_TERMINAL`; and a `### Changed` line for the missing-tmux error and upgrade steps.
   Match the file's existing bullet style (bold lead, em dash).
3. **install.sh.** Find the runtime-tool check (`rg -n "command -v" install.sh`). Add a non-fatal tmux
   advisory: when `command -v tmux` fails, print that Loom needs tmux to start sessions and the
   install hint (`brew install tmux` on Darwin, `sudo apt install tmux` on Linux), following the
   script's existing message helpers and `uname` handling. Installation still succeeds. Keep POSIX sh
   if the script is POSIX; run `sh -n install.sh` (or `bash -n` if it is bash) to check syntax.
4. **CONTRIBUTING.md.** Line 131: replace the "no terminal emulator ... configure the tmux backend"
   note with: tests that spawn sessions need tmux and guard AF_UNIX denial with the existing
   `sandbox_probe` helpers. Line 197: update the moved paths.
5. **doc/sandbox-release-checklist.md** line 7: update the moved paths.
6. **doc/product/feat-planner/section_11.md** 318-330, 346-347, ~362: the Run dialog has no backend
   choice; Start runs `loom init <plan>` (with `--clean` when required) and `loom run`; Open dashboard
   always starts the dashboard with `--terminals`. Remove the paragraph about loom's built-in default
   and `SessionBackendKind`.
7. **loom/eval/retrieval-cases.yaml** 107-108: delete the two `forbid` entries naming
   `terminal/native/window_ops.rs` (the file is deleted); keep the third entry.

## Rules

- Plain, short prose in the files' existing style. Every fenced code block names a language.
- No history narrative in README beyond the upgrade note; the CHANGELOG carries what was removed.
- Report: sections changed, links you could not update, and anything that contradicted the facts.
