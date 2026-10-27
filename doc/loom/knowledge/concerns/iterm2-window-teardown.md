# Iterm2 Window Teardown

> iTerm2

## iTerm2 Windows Survive Stage Completion — Spawn Never Names the Window (GitHub #7)

GitHub issue #7 reports that a completed stage leaves the iTerm2 window
open on macOS while Linux DEs close theirs. The per-terminal AppleScript close functions exist,
but the tree still reproduces the report:

**Cause.** Teardown closes by window title: `orchestrator/core/completion_handler.rs::record_completion_time` →
`NativeBackend::kill_session` (`terminal/native/mod.rs::kill_session`) → `close_iterm2_window`
(`terminal/native/window_ops/macos.rs::close_iterm2_window`), which runs `every window whose name is
"loom-<stage-id>"`. The iTerm2 spawn arm (`terminal/emulator.rs::build_command`) does `create window with
default profile` + `write text` and never uses its `title` parameter — `git log -S 'set name of'` is
empty, so no version ever named the session. The query matches nothing, the close returns `false`,
and `kill_session` falls through to `pid_only_terminate` (`native/mod.rs::kill_session`). That kills
`claude`, but `write text` typed the wrapper into an interactive shell, so the shell returns to its
prompt and the window stays. Linux emulators are launched with `-e <wrapper>`
(`emulator.rs::build_command`): the window closes on its own when the exec'd `claude` dies, and
`wmctrl -F -c` also matches the `--title` given at launch. `iterm2_window_exists`
(`terminal/native/window_ops/macos.rs::iterm2_window_exists`) has the same gap, so the `is_session_alive` title fallback is inert on iTerm2
too.

**Why it hid.** `test_iterm2_build_command` (`emulator.rs::test_iterm2_build_command`) never asserts that the title
appears in the generated script.

**A second, independent defect in the same path (verified 2026-08-29).** Spawn addresses
`tell application "iTerm"` (`emulator.rs::build_command`); both teardown paths address
`tell application "iTerm2"` (`window_ops/macos.rs::close_iterm2_window`, `::iterm2_window_exists`). iTerm2's scriptable application name is
`iTerm`, so the close and the liveness check target a name that does not resolve. Naming the window
is therefore necessary but not sufficient — a fix that only adds `set name to` would leave teardown
failing for this second reason, with the same silent `false` return. Found with
`rg -n 'application "iTerm' loom/src/orchestrator/terminal/`, which is also the check that proves
it fixed.

**Terminal.app (the issue's TBD).** Unverified — needs a macOS host. Its arm does `set custom title
of front window` (`emulator.rs::build_command`), so the close can match, with two risks: a Terminal.app
window's AppleScript `name` is the full displayed title (custom title plus process and size
components under default profile settings), and nothing pins the title against Claude Code's own
OSC title updates — the wrapper (`native/wrapper.rs`, `exec env -i …`) emits no title escape and
passes no `CLAUDE_CODE_DISABLE_TERMINAL_TITLE`.

**Fix shape.** In the iTerm2 arm add `set name to "{escaped_title}"` inside `tell current session
of current window` before `write text`, and assert it in `test_iterm2_build_command`. Consider
exporting `CLAUDE_CODE_DISABLE_TERMINAL_TITLE=1` through `AGENT_SESSION_ENV_NAMES` (the wrapper's
list, which the subset tests also require in `STAGE_HOST_ENV_ALLOWLIST` and the daemon list; see
"Three Stage Environment Allowlists" in concerns.md) so neither macOS
terminal's window name drifts before teardown. Verify on macOS with both terminals before closing #7.
