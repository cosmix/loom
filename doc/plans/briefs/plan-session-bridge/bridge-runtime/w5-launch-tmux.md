# W5 (bridge-runtime): launch, tmux input, the tmux host

Read `doc/plans/briefs/plan-session-bridge/common.md` first ("Session kinds, naming, directory",
"Hooks registered per session"). You run in wave 2, after W4 returned
(`plan_session::bridge::host::{SessionHost, LaunchSpec, Spawned}` exist), beside W6.

## Files you own (write only these)

- `loom/src/plan_session/launch.rs` (`pub fn launch(spec: &LaunchSpec) -> anyhow::Result<Spawned>`)
- `loom/src/plan_session/tmux_io.rs` (`paste_text`, `press`, `pane_pid`)
- `loom/src/plan_session/launch_tests.rs`, `loom/src/plan_session/tmux_io_tests.rs`
- `loom/src/orchestrator/terminal/tmux/mod.rs`: visibility only, no line-count change:
  `teardown_socket` -> `pub(crate) fn`, `tmux_control_command` -> `pub(crate) fn`,
  `run_tmux_control` stays `pub(super)` unless you need it (then `pub(crate)`).

Codex units (`codex-units.md` in this directory) own `launch/script.rs` (C15, the script text of
step 1, which your `launch` calls), `tmux_host.rs` (C16, after you return; keep the function
names of steps 2-3) and the `socket.rs` skip (C17). Steps 1 and 4 below are their specification.

## Steps

1. `launch/script.rs`: `pub fn launch_script(spec: &LaunchSpec, settings: &Path, env: &[(OsString, OsString)]) -> String`:
   ```sh
   #!/bin/sh
   cd <root> || exit 1
   exec env -i <NAME=value ...> <claude> --session-id <id> --name <display name> --settings <settings> --setting-sources user,project --model <model> --effort <effort> --permission-mode default --plugin-dir <plugin> --strict-mcp-config --append-system-prompt-file <brief>
   ```
   every word shell-escaped with `shell_escape::escape`. There is NO positional prompt unless
   `spec.opening_argv` is `Some` (the bridge validated it as a slash command plus a plan path,
   px-seams-R04), in which case it is the last shell-escaped word
   (px-security-04: `/proc/<pid>/cmdline` is world-readable, so the opening message, which can
   quote documents, must never be an argv word; the bridge pastes it after `SessionStart`). The
   display name is the only free text on the line and is shell-escaped; it never holds document
   text (plan 8 builds it from the step name). `env` = `process::environment::agent_session_environment_from(std::env::vars_os())`
   (`process/environment.rs:143`) plus `LOOM_PLAN_STATE=<runtime dir>`, `LOOM_PLAN_SESSION=<id>`,
   then `spec.extra_env`. `LOOM_PLAN_STATE` is read by plan 1's `planning::runtime::runtime_paths` (which checks it is
   absolute, a real directory owned by the user, named `project_key(root)`) and so by plan 4's
   summary CLI inside the session, so a `LOOM_HOME` redirect, which `env -i` drops, is still
   honoured; never `LOOM_STAGE_ID`, `LOOM_SESSION_ID`, `LOOM_WORK_DIR` (assert in a
   test that they are absent even when the parent environment has them).
2. `launch.rs::launch`: (a) `plan_session::sandbox::write_session_sandbox(&SandboxInputs { project_root, runtime, session_dir, kind, loom_bin, home }, &spool::SPOOL_EVENTS)`;
   (b) write `launch.sh` 0700 through `safe_fs::safe_write_with_mode_in_workdir`;
   (c) `orchestrator::terminal::tmux::spawn_in_tmux(socket, tmux_session, project_root, launch.sh)`
   (`tmux/mod.rs:243`; it keeps the stderr-with-exit-0 guard and the `has-session` spawn probe and
   applies `PRESENTATION_OPTIONS`); (d) `tmux_io::pane_pid(socket, tmux_session)`, then
   `ProcessIdentity { pid, start_time: process::identity::process_start_time(pid) }`; a pid that
   cannot be read or whose start time is missing is a failure. On ANY failure after (c) began,
   `teardown_socket(socket)` before returning the error (`mistakes/tmux-backend.md`: tear down a
   half-started server); a teardown failure is added to the error, never swallowed.
3. `tmux_io.rs` (all on `-L <socket>`, through `tmux_control_command`-built commands and
   `process::run_bounded_output` with a 5 s bound; stderr non-empty with exit 0 is an error):
   - `pub fn paste_text(socket: &str, target: &str, text: &str) -> anyhow::Result<()>`:
     `load-buffer -b loom-input -` with `text` on stdin (`std::io::pipe()`: the reader is the
     child's stdin, a writer thread writes the text and closes, since up to 100 KB exceeds a pipe
     buffer; join the thread after the command returns), then
     `paste-buffer -p -d -b loom-input -t <target>` (bracketed paste; `-d` deletes the buffer),
     then `send-keys -t <target> Enter`. NEVER `load-buffer -w` (it reaches the clipboard and
     crashes tmux 3.6a with a client attached). The caller sanitised the text; assert in debug
     builds that it holds no C0 control other than `\n`/`\t`.
   - `pub fn press(socket: &str, target: &str, key: &str) -> anyhow::Result<()>` (`send-keys -t <target> <key>`; used with `Escape`).
   - `pub fn pane_pid(socket: &str, target: &str) -> anyhow::Result<u32>`
     (`display-message -p -t <target> '#{pane_pid}'`).
4. `tmux_host.rs`: `impl SessionHost for TmuxHost`: `spawn` = `launch::launch`, `deliver` =
   `paste_text`, `interrupt` = `press(.., "Escape")`, `teardown` = `teardown_socket`, `identity` =
   `process::identity::verify_process_identity`.
5. Tests: the script text for a display name with quotes, `$(...)`, newlines and a leading `-`
   (executed with `sh -n` for syntax, and with a fake `claude` that prints its argv as JSON, then
   compared); absent `LOOM_STAGE_ID`/`LOOM_SESSION_ID`/`LOOM_WORK_DIR`; the fake claude's cwd is the project
   root and its argv holds each of `--session-id <id>`, `--name`, `--settings <abs>`,
   `--setting-sources user,project`, `--model`, `--effort`, `--permission-mode default`,
   `--plugin-dir`, `--strict-mcp-config`, `--append-system-prompt-file`, no positional prompt,
   and never `--resume`/`--continue`; neither `launch.sh` nor the fake claude's argv contains any
   byte sequence of a distinctive opening message the test sets in `LaunchSpec.guidance`; the written `settings.json` registers all nine
   `SPOOL_EVENTS` (PermissionRequest with timeout 3600) plus plan 6's PreToolUse; `launch.sh` mode 0700;
   a real tmux round trip, skipped through `process::sandbox_probe::skip_unless(unix_socket_bindable(dir) && which tmux, ..)`
   (the pattern of `tmux/tests_spawn.rs`, with `TmuxTmpDirGuard`-style `TMUX_TMPDIR` pinning and
   `#[serial]`): spawn a pane running `cat > <file>`, `paste_text` a 120 KB text with newlines,
   check the file, `pane_pid` matches the `cat` process, `teardown_socket` removes the socket; a
   spawn whose launch script exits at once is torn down and reported.

## Proof

`cargo test --lib plan_session::launch`, `cargo test --lib plan_session::tmux_io`,
`cargo test --lib orchestrator::terminal::tmux::` and `cargo clippy --all-targets -- -D warnings`
from `loom/`. Report whether the real tmux test ran or skipped on the stage host.
