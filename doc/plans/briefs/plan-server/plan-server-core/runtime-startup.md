# plan-server-core / runtime-startup: spec for codex units C7-C14

Lane: codex (`codex-units.md`, units C7-C14, wave 1). Each unit owns the files the units table
names. Read `doc/plans/briefs/plan-server/common.md` first. Spec: `doc/product/section_02.md`
"Project root", "Port", "One server per project", "Runtime state", "Opening the browser",
"Lifetime", "Missing prerequisites"; `doc/product/section_12.md` "Remote binds", "Token lifetime",
"Sandbox prerequisites".

## root.rs

`pub fn resolve_project(cwd: &Path) -> Result<PathBuf>`:
1. `git rev-parse --show-toplevel` through `crate::git::runner::run_git` (`loom/src/git/runner.rs:136`);
   non-zero exit -> error `"loom plan needs a git repository; {cwd} is not inside one"`.
2. `git rev-parse --abbrev-ref HEAD`; a branch starting `loom/` is a stage worktree -> error
   `"{root} is a loom stage worktree (branch {branch}); run loom plan from the main checkout at {main}"`,
   with `{main}` = parent of `git rev-parse --path-format=absolute --git-common-dir`.
3. Return the canonical root. Paths in messages pass through `inline_safe`.
Tests (temp dirs, `git init`): outside a repo errors and names the dir; a subdirectory resolves to
the top level; a worktree on branch `loom/demo` errors and names the main checkout.

## lock.rs

Pattern: `loom/src/fs/daemon_state.rs` `acquire_lock`/`inspect_lock` (O_NOFOLLOW|O_CLOEXEC|O_NONBLOCK
open, `fs2`-style `flock(LOCK_EX|LOCK_NB)`, mode 0600, `set_permissions` 0600). Do not change
`daemon_state.rs`; copy the open flags.

```text
pub struct LockRecord { pub pid: u32, pub url: Option<String>, pub token: Option<String> }   // serde JSON, one object
pub enum Acquired { Held(ServerLock), Running(LockRecord) }
pub fn acquire(path: &Path) -> Result<Acquired>;
impl ServerLock { pub fn publish(&mut self, url: &str, token: &str) -> Result<()>; }   // ftruncate + write + fsync
```

- `acquire`: open-or-create no-follow (a symlink at `path` fails closed with an error naming it;
  a FIFO cannot block thanks to O_NONBLOCK; a non-regular file is an error); lock free -> write
  `{"pid":N}` and return `Held`; lock held -> read the record bounded (4 KiB, through the open fd)
  and return `Running` (a record that does not parse yet: `Running` with `url: None`).
- The `ServerLock` keeps the `File` open for the process lifetime; dropping it releases the lock.
Tests: second `acquire` in the same process on another fd returns `Running` with the published
url and token; mode is 0600 after create; a symlinked lock path errors and leaves the target
unchanged; a crashed holder (fd closed) frees the lock.

## prereq.rs

```text
pub struct ReadOnly { problems: Vec<MissingPrereq> }   // wire::server::MissingPrereq
impl ReadOnly { pub fn detect() -> ReadOnly; pub fn from_problems(p: Vec<MissingPrereq>) -> ReadOnly; pub fn is_read_only(&self) -> bool; pub fn problems(&self) -> &[MissingPrereq]; }
```

`detect`: `which::which("tmux")`, `crate::claude::find_claude_path()` (then
`crate::claude::auth::stage_auth_status(&path)`, `loom/src/claude/auth.rs:87`; `AuthProbe::LoggedIn`
is fine, `NotLoggedIn` is a problem, `Unknown(_)` is a problem with its text). Run the auth probe
on a thread and give up after 10 s with an `Unknown("timed out")` problem (read `stage_auth_status`
first; if it already bounds itself, call it directly and say so in your report). On Linux:
`sandbox_prerequisite_problem(std::fs::read_to_string("/proc/version").ok().as_deref(), which("bwrap").is_ok(), which("socat").is_ok())`
(`commands/run/sandbox_preflight.rs:56`; the orchestrator widens it to `pub(crate)` before wave 1). Messages and fixes: "tmux is not on PATH" / "Install
tmux"; "claude is not on PATH" / "Install Claude Code"; "Claude Code is not logged in" / "Run
claude auth login"; the sandbox problem text / "Install bubblewrap and socat".

## browser.rs

```text
pub struct EnvView { pub ssh_connection: bool, pub ssh_tty: bool, pub display: bool, pub wayland: bool, pub linux: bool }
impl EnvView { pub fn current() -> EnvView; }
pub enum OpenDecision { Open, Skip(SkipReason) }   pub enum SkipReason { NoOpen, Ssh, NoDisplay }
pub fn decide(env: &EnvView, no_open: bool) -> OpenDecision;
pub fn open(url: &str);   // `open` on macOS, `xdg-open` elsewhere; stdio null; spawn and do not wait; errors ignored
```

Order: `--no-open`, then SSH (`SSH_CONNECTION` or `SSH_TTY` set), then (Linux only) neither
`DISPLAY` nor `WAYLAND_DISPLAY`. An empty variable counts as unset. Tests cover every row.

## startup.rs

```text
pub struct Startup<'a> { pub root: &'a Path, pub urls: Vec<String>, pub port: u16, pub remote: bool, pub ssh: bool, pub hostname: String, pub read_only: &'a [MissingPrereq] }
pub fn startup_lines(s: &Startup<'_>) -> Vec<String>;
pub fn already_running_lines(record: &LockRecord) -> Vec<String>;
pub fn tunnel_hint(port: u16, hostname: &str) -> String;   // "ssh -L {port}:127.0.0.1:{port} {hostname}"; Share and plan 11 reuse it
```

Lines, in order: `loom plan: {root}`; each URL (`http://{addr}/?token={token}`); when `ssh`:
`On your machine: ssh -L {port}:127.0.0.1:{port} {hostname}`; when `remote`: `warning: plain HTTP;
anyone who can see the traffic can read the token, then read every document and write through
sessions. An SSH tunnel is the safer way to share.`; per problem `read-only: {message}. {fix}`;
`Ctrl-C to stop; a second Ctrl-C exits at once`. Already running:
`loom plan is already running for this project (pid {pid}).` then `Open {url}?token={token}`
(`url` without a trailing `/` is joined as `{url}/?token=`; when the record has no URL yet:
`It is still starting; run loom plan again in a moment.`). Root and hostname pass through
`inline_safe`. Tests pin every line exactly.

## lifecycle.rs

`pub fn line(message: &str)` prints `{HH:MM:SS} {message}` (local time, `chrono::Local`) to stdout
and flushes; `pub fn safe(value: &str) -> String` = `crate::context::untrusted::inline_safe`.

## Docs and completions

- `completions/dynamic/mod.rs`: `loom plan` completes `--port`, `--host`, `--no-open`, `verify`,
  `check` (plan 1 added `check`; keep it), and nothing for the hidden `hook`. Extend
  `tests_commands.rs` (:236-250 pattern).
- README `### Plan Commands`: bare `loom plan` (flags, the printed URL, already-running exit, the
  runtime directory `~/.loom/plan/<project-key>/`, read-only mode, the two-stage Ctrl-C).
- `skills/loom-usage/SKILL.md`: one short passage on `loom plan` beside `loom plan verify`.
  The operator runs `./dev-install.sh` after merge; say nothing about it in the skill.

## Done when

`cargo test --manifest-path loom/Cargo.toml --lib plan_server::` (your modules) and
`cargo test --manifest-path loom/Cargo.toml --lib completions::` pass. Report signature changes.
