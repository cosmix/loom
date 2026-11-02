# Iterm2 Window Teardown

>

## macOS Window Teardown Closes by Title and Is Unverified (GitHub #7)

Teardown closes by window title: `orchestrator/core/completion_handler.rs::record_completion_time` → `NativeBackend::kill_session` (`terminal/native/mod.rs::kill_session`) → `close_iterm2_window` (`terminal/native/window_ops/macos.rs`), which runs `every window whose name is "loom-<stage-id>"`.

**iTerm2.** The spawn arm (`terminal/emulator.rs::build_command`) sets `set name of loomWindow` before `write text`, and spawn, close and `iterm2_window_exists` all address `tell application "iTerm"`, so the `is_session_alive` title fallback can match too. `write text` types the wrapper into an interactive shell, so a close that misses leaves the window at a shell prompt after `pid_only_terminate` kills `claude`. `test_iterm2_build_command` asserts the name and the application in the generated script; no macOS run of the close is on record.

**Terminal.app.** Unverified, needs a macOS host. Its arm does `set custom title of front window` (`emulator.rs::build_command`), so the close can match, with two risks: a Terminal.app window's AppleScript `name` is the full displayed title (custom title plus process and size components under default profile settings), and nothing pins the title against Claude Code's own OSC title updates. The wrapper (`native/wrapper.rs`, `exec env -i …`) emits no title escape and passes no `CLAUDE_CODE_DISABLE_TERMINAL_TITLE`.

**Linux** emulators launch with `-e <wrapper>`: the window closes when the exec'd `claude` dies, and `wmctrl -F -c` also matches the `--title` given at launch.

**Remaining work.** Consider exporting `CLAUDE_CODE_DISABLE_TERMINAL_TITLE=1` through `AGENT_SESSION_ENV_NAMES` (the wrapper's list, which the subset tests also require in `STAGE_HOST_ENV_ALLOWLIST` and the daemon list; see "Three Stage Environment Allowlists" in concerns.md) so neither macOS terminal's window name drifts before teardown. Verify on macOS with both terminals before closing #7.
