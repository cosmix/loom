# feat-planner-06-session-sandbox: common brief

Read this in full before your own brief. It is the authoritative public API of `loom::plan_session`
as plan 6 delivers it, the rules from the product spec it implements, and the size and safety rules.
Where your own brief and this file differ, this file wins; report the difference.

Spec: `doc/product/section_12.md`, "Planning sessions" through "Sandbox prerequisites" (lines
155-456) and "Paths in requests" / "Symlinks under `doc/`" (lines 460-485). Section 07 "Hooks and the
event spool" (lines 124-156) for the hook command form. Metaplan entry 6 of
`doc/plans/metaplan-planner-ui.md`.

## What plan 6 delivers

The confinement every planning session runs under, testable before any session exists:

1. A settings builder that writes section 12's `settings.json` for each session kind (authoring,
   summary, pressure). It is a new builder beside `sandbox::settings`, never
   `sandbox::settings::generate_settings_json` (`loom/src/sandbox/settings.rs:205`), which adds the
   package-manager cache writes (`sandbox/settings/policy.rs:165-167`), the orchestration-state
   reads and plan-authored config a planning session must not have.
2. `loom plan hook PreToolUse --session-dir <dir>`, in Rust, the precise third layer: it decides
   every file-tool call against the current tree and fails closed.
3. A real-sandbox confinement test.

Plan 7 (the session bridge) writes these files at launch and adds the nine spool hook events; it
calls only the API below.

## Inputs from plan 1 (already merged when this plan runs)

Published in `doc/plans/PLAN-feat-planner-01-documents.md` (or its `DONE-` spelling) and its briefs
`doc/plans/briefs/feat-planner-01-documents/common.md`.
Confirm each signature in the tree before using it (`loom map --find-all <name>`):

- `loom::planning::runtime::{RuntimePaths { dir, lock, drafts, claims, summaries_lock, summary_cost, sessions }, runtime_paths(root) -> Result<RuntimePaths>, runtime_paths_under(loom_dir: &Path, root: &Path) -> Result<RuntimePaths>}`.
- `loom::planning::paths::{LockedSet, LockReason, DocArea, classify_write, symlink_component}`:
  `LockedSet::scan(root: &Path) -> Result<LockedSet>`, `LockedSet::lock_reason(&self, rel: &Path) -> Option<LockReason>`,
  `LockedSet::locked_paths(&self) -> Vec<PathBuf>` (existing locked files, repo-relative),
  `classify_write(locked: &LockedSet, rel: &Path) -> DocArea` with
  `DocArea::{Locked(LockReason), Knowledge, Summaries, Doc, OutsideDoc}`,
  `symlink_component(root: &Path, rel: &Path) -> Result<Option<PathBuf>>` (first symlinked
  component below `root`, lstat walk). Locked is by NAME: `IN_PROGRESS-`, `DONE-`, `RETIRED-` plan
  files, everything under `doc/plans/archive/`, `doc/plans/REVIEW-*.md`, and the briefs of a locked
  plan (every file under `doc/plans/briefs/<slug>/` plus every input `collect_inputs` lists),
  including a path under a locked bundle that does not exist yet.
- `loom::planning::slug::plan_slug(file_name) -> Option<String>`.

If any of these is missing or differs, STOP that piece and report it to the orchestrator: never
write a second locked-set rule.

## Public API (exact; contracts are written against it)

Module root `loom/src/plan_session/mod.rs`, declared `pub mod plan_session;` in `loom/src/lib.rs`:

```rust
pub mod kind;
pub mod sandbox;
pub mod guard;
pub mod hook_cli;
```

### `plan_session::kind`

```rust
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum SessionKind {
    Authoring,
    Summary,
    /// `plan` is repo-relative: `doc/plans/PLAN-<slug>.md`.
    Pressure { plan: PathBuf },
}
impl SessionKind {
    pub fn label(&self) -> &'static str;              // "authoring" | "summary" | "pressure"
    /// For Pressure: `doc/plans/briefs/<slug>` from `plan_slug` of the file name; None otherwise
    /// or when the name is not a `PLAN-<slug>.md`.
    pub fn pressure_bundle(&self) -> Option<PathBuf>;
}

/// One session's runtime directory, `<runtime>/sessions/<session-id>/` (section 07 layout plus
/// `guard.json`). Plan 7 creates it 0700; plan 6 only writes into it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionDir(PathBuf);
impl SessionDir {
    pub fn new(path: PathBuf) -> Self;
    pub fn path(&self) -> &Path;
    pub fn settings(&self) -> PathBuf;      // settings.json
    pub fn guard(&self) -> PathBuf;         // guard.json
    pub fn record(&self) -> PathBuf;        // session.json
    pub fn launch_script(&self) -> PathBuf; // launch.sh
    pub fn brief(&self) -> PathBuf;         // brief.md
    pub fn plugin(&self) -> PathBuf;        // plugin/
    pub fn events(&self) -> PathBuf;        // events.jsonl
    pub fn answers(&self) -> PathBuf;       // answers/
    pub fn done(&self) -> PathBuf;          // done/
    pub fn marker(&self) -> PathBuf;        // done/complete
    pub fn codex_log(&self) -> PathBuf;     // codex.log
}
```

### `plan_session::sandbox`

```rust
pub struct SandboxInputs<'a> {
    pub project_root: &'a Path,                       // canonical, absolute
    pub runtime: &'a crate::planning::runtime::RuntimePaths,
    pub session_dir: &'a SessionDir,
    pub kind: &'a SessionKind,
    pub loom_bin: &'a Path,                           // absolute path of the running loom
    pub home: &'a Path,                               // the user's home, absolute
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HookSpec { pub event: &'static str, pub matcher: Option<&'static str>, pub timeout_secs: Option<u32> }
pub struct SandboxFiles { pub settings: PathBuf, pub guard: PathBuf }

pub const PRE_TOOL_USE_MATCHER: &str = "Read|Edit|Write|MultiEdit|NotebookEdit|Grep|Glob";
pub const PRE_TOOL_USE_TIMEOUT_SECS: u32 = 10;

pub fn hook_command(loom_bin: &Path, event: &str, session_dir: &Path) -> String;
pub fn build_settings(inputs: &SandboxInputs<'_>, locked: &LockedSet, extra_hooks: &[HookSpec]) -> anyhow::Result<serde_json::Value>;
pub fn assert_planning_settings(settings: &serde_json::Value) -> anyhow::Result<()>;
pub fn ensure_runtime_grants(runtime: &RuntimePaths) -> anyhow::Result<()>;
pub fn write_session_sandbox(inputs: &SandboxInputs<'_>, extra_hooks: &[HookSpec]) -> anyhow::Result<SandboxFiles>;
```

### `plan_session::guard`

```rust
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct GuardPolicy {
    pub schema: u32,                // 1
    pub kind: SessionKind,          // flattened is NOT used: serialised as {"kind": {...}}
    pub project_root: PathBuf,
    pub runtime_dir: PathBuf,
    pub session_dir: PathBuf,
    pub home: PathBuf,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Decision { Defer, Deny(String) }

pub fn load_policy(session_dir: &Path) -> anyhow::Result<GuardPolicy>;
pub fn decide(policy: &GuardPolicy, payload: &serde_json::Value) -> Decision;
pub fn render_deny(reason: &str) -> String;
pub fn run_pre_tool_use(session_dir: &Path, stdin: impl std::io::Read) -> Option<String>;
```

### `plan_session::hook_cli`

```rust
/// Entry of `loom plan hook <event> --session-dir <dir>`. Writes the hook's answer (if any) to
/// `stdout` and returns the exit code, which is always 0: a non-zero hook exit is a Claude Code
/// hook error, and the guard's deny is a JSON answer, not an exit status.
pub fn run(event: &str, session_dir: &Path, stdin: impl std::io::Read, stdout: impl std::io::Write) -> i32;
```

CLI: `PlanCommands::Hook { event: String, #[arg(long)] session_dir: PathBuf }` with
`#[command(hide = true)]` in `loom/src/cli/types_ops.rs`; `loom/src/commands/plan/hook.rs::execute(event: &str, session_dir: &Path) -> anyhow::Result<()>`
calls `hook_cli::run` with locked stdin/stdout and then `std::process::exit(code)` only when the
code is non-zero (it never is); the `dispatch_plan` arm in `loom/src/cli/dispatch.rs` calls it.
`event` is a positional `String`, NOT a clap `ValueEnum`: an unknown event name must exit 0
silently (clap would exit 2, which Claude Code reports as a hook failure). Plan 6 handles
`PreToolUse`; every other name returns 0 with no output (plan 7 adds the spool events in
`hook_cli.rs`).

## The settings file (section 12, "The settings file", "Pressure sessions", "Modes per kind")

Paths: `<root>` = `project_root`, `<rt>` = `runtime.dir`, both absolute. Permission-rule paths use
the absolute `//` form: rule text `Edit(/` + absolute path + `)`, so `/home/u/p/doc` becomes
`Edit(//home/u/p/doc/**)`. A single leading `/` in a permission rule resolves against the settings
file's directory (`doc/loom/knowledge/architecture/execution-containment.md`, "Sandbox-List vs
Permission-Rule Path Syntax"); sandbox lists take plain absolute paths. Only `Edit(...)` rules
bind file writes; never write a `Write(...)` rule (`mistakes/sandbox-write-rules-inert.md`) and
never a `Read(...)` deny (stage settings never write one;
`concerns/sandbox-and-confinement-gaps.md`).

Common to every kind:

```json
{
  "sandbox": {
    "enabled": true, "failIfUnavailable": true,
    "autoAllowBashIfSandboxed": false, "allowUnsandboxedCommands": false,
    "network": { "strictAllowlist": true },
    "filesystem": { "denyWrite": [...], "allowWrite": [...], "denyRead": [...], "allowRead": [...] }
  },
  "permissions": { "defaultMode": "default", "disableBypassPermissionsMode": "disable",
                   "allow": [...], "ask": [...], "deny": [...] },
  "worktree": { "bgIsolation": "none" },
  "hooks": { "PreToolUse": [ { "matcher": PRE_TOOL_USE_MATCHER,
             "hooks": [ { "type": "command", "command": hook_command(.., "PreToolUse", ..), "timeout": 10 } ] } ],
             "<extra event>": [ { "matcher": "<matcher or omitted>", "hooks": [ { "type": "command", "command": "...", "timeout": <n or omitted> } ] } ] }
}
```

`network` carries no `allowedDomains`, no `allowLocalBinding`, no `allowUnixSockets`, no
`allowAllUnixSockets`. The hooks object follows the shape `capsule_hooks`
(`loom/src/orchestrator/terminal/native/session_settings/contents.rs:178-234`) writes:
`{"matcher": .., "hooks": [{"type": "command", "command": ..}]}`, with `timeout` in seconds when
set. An extra hook whose event is `PreToolUse` is refused (`bail!`): plan 6 owns that event.

### Layer 1: `sandbox.filesystem`

Every entry is a literal existing absolute path, listed at build time; no entry lies beneath
another `denyWrite` directory (bubblewrap refuses a deny mount inside a read-only bind:
`architecture/execution-containment.md`; `mistakes/sandbox-and-settings.md`). Top-level entries
are read with `fs::safe_read::list_dir_no_follow` (no-follow; a symlink at the top level is listed
by its own path and denied like any entry), dotfiles included, `.git` included.

| Kind | `denyWrite` | `allowWrite` |
| --- | --- | --- |
| authoring | `<root>/<e>` for every top-level entry `e` except `doc`; then the existing locked set collapsed: `<root>/doc/plans/archive` when it exists (never a path below it), each existing locked plan file and `doc/plans/REVIEW-*.md` directly in `doc/plans/`, each existing `doc/plans/briefs/<slug>` directory of a locked plan (never the files below it), and each other existing locked input file not beneath an entry already listed | `<rt>/drafts`, `<rt>/claims`, `<rt>/summaries.lock` |
| summary | `<root>/<e>` for every top-level entry except `doc`; `<root>/doc/<e>` for every entry of `doc/` except `loom`; `<root>/doc/loom/<e>` for every entry of `doc/loom/` except `summaries` | the same three |
| pressure | `<root>/<e>` for every top-level entry except `doc` and `planning-omissions.md`; `<root>/doc/<e>` for every entry of `doc/` except `plans`; `<root>/doc/plans/<e>` for every entry of `doc/plans/` except the plan file and `briefs`; `<root>/doc/plans/briefs/<e>` for every entry except `<slug>` | `<session_dir>/done` only |

`denyRead` (every kind): `policy::mandatory_deny_read(false)` (the credential directories and
files plus the codex credential, since no planning session runs codex; reachable after widening
`sandbox/mod.rs:12` `mod settings`, `sandbox/settings.rs:16` `mod policy` and `policy.rs:17` to
`pub(crate)`, the only changes to those files), then
the planning runtime ROOT: plan 1's `RuntimePaths::planning_root` (`<loom user dir>/plan`, the
parent of `runtime.dir`), canonicalized, so a `LOOM_HOME` redirect is followed, plus `<home>/.loom/plan`
when that differs (every project's runtime, wherever the user dir points), then, when `<root>/.loom/work` resolves (`fs::permissions::state_root::resolve_state_root(root)`,
`state_root.rs:105`), `token_deny_paths(<resolved, as a string>)` (`state_root.rs:158`;
it formats `/{resolved}/{name}`, so an absolute `resolved` yields `//abs/...`, the absolute form
sandbox lists accept, exactly as `sandbox/settings.rs:181-185` passes it) and `<resolved>/terminal-history`
(`orchestrator/terminal/tmux/history.rs:58`, `HISTORY_DIR`; there is no exported constant, so the
literal `terminal-history` is named once in `sandbox/reads.rs` with that citation).
`allowRead`: `<rt>/drafts`, `<rt>/claims`, `<rt>/summaries.lock`; pressure: `<session_dir>/done` as well.

### Layer 2: `permissions`

| Kind | `allow` | `ask` | `deny` |
| --- | --- | --- | --- |
| authoring | `Edit(//<root>/doc/product/**)`, `Edit(//<root>/doc/plans/**)`, `Edit(//<root>/doc/<d>/**)` for each existing directory `d` directly under `doc/` other than `loom`, `product`, `plans`, `Edit(//<root>/doc/<f>)` for each existing top-level regular file `f` of `doc/` (listed no-follow; a symlink, a special file and a name holding rule syntax are skipped), `Edit(//<rt>/drafts/**)`, `Read(//<rt>/drafts/**)`, `Read(//<rt>/claims/**)`, `Bash(loom knowledge *)`, `Bash(loom plan summary *)`, `Bash(loom plan verify *)`, `Bash(loom plan check*)`, `Bash(loom map *)`, `WebSearch` | `WebFetch` | the tool denies (below), `AskUserQuestion`, `Edit(//<root>/<p>)` for each existing locked plan file, `Edit(//<root>/doc/plans/REVIEW-*.md)`, `Edit(//<root>/doc/plans/archive/**)`, `Edit(//<root>/doc/plans/briefs/<slug>/**)` for each locked plan's slug |
| summary | `Edit(//<rt>/drafts/**)`, `Read(//<rt>/drafts/**)`, `Read(//<rt>/claims/**)`, `Bash(loom plan summary *)` | none | the tool denies (below), `AskUserQuestion`, `WebSearch`, `WebFetch` |
| pressure | `Edit(//<root>/<plan>)`, `Edit(//<root>/<bundle>/**)`, `Edit(//<root>/planning-omissions.md)`, `Bash(touch <marker>)` with `<marker>` = `session_dir.marker()` absolute, the five authoring `Bash(...)` rules, `WebSearch` | `WebFetch` | the pressure tool denies (below), `AskUserQuestion` |

Tool denies (px-security-02): every kind denies the four `SESSION_TOOL_DENIES` (`LSP`,
`RemoteTrigger`, `CronCreate`, `PushNotification`) and `WORKFLOW_TOOL` (`Workflow`), reusing the
constants of `orchestrator/terminal/native/session_settings/contents.rs:35-36` (widened to
`pub(crate)` with their two enclosing `mod` lines, `native/mod.rs:13` and
`native/session_settings.rs:12`; visibility only, no line-count change). Authoring and summary
also deny `SendMessage` and `ListAgents` (they can address another local Claude Code session,
a self-approval route); pressure keeps both because `/pressure` teammates need them. The stage
capsule denies the same four for the reasons the doc comment above `SESSION_TOOL_DENIES` gives (`LSP` runs language
servers outside the sandbox; `RemoteTrigger` and `CronCreate` act with nobody sending;
`PushNotification` leaves the machine).

No `Edit` deny on `doc/loom/knowledge/` or `doc/loom/summaries/`, ever: an `Edit` deny also becomes
an OS write deny and would block `loom knowledge` and `loom plan summary seal`, which run as Bash
subprocesses in this sandbox. No allow rule covers `doc/loom/`: it is excluded from the per-dir
allow list, and top-level files are allowed by their literal paths because `Edit` rule paths
match with gitignore semantics, under which `Edit(//<root>/doc/*)` would match the directory
`doc/loom` and so everything beneath it. `Bash(touch <marker>)` must
be exactly the command `completion_instruction` (`loom/src/commands/pressure/spawn.rs:21-30`)
tells the agent to run: `touch <absolute marker path>`, no quoting added.

`assert_planning_settings` refuses (each with a message naming the key): `permissions.defaultMode`
other than `"default"`, `disableBypassPermissionsMode` other than `"disable"`, `sandbox.enabled`
not `true`, `failIfUnavailable` not `true`, `allowUnsandboxedCommands` not `false`,
`autoAllowBashIfSandboxed` not `false`, any `network.allowedDomains`/`allowLocalBinding`/
`allowUnixSockets`/`allowAllUnixSockets`, any `Write(` or `Read(` entry in `permissions.deny`,
`AskUserQuestion` or any tool deny of the kind missing from `permissions.deny`,
`worktree.bgIsolation` other than `"none"`,
and a missing `PreToolUse` hook. A `catch_unwind` panic path in the guard is not unit-testable
without a fault-injection seam; it is covered by review, and recorded as such.
It mirrors the three refusals of `sandbox::config::validate_config` (`loom/src/sandbox/config.rs:155-175`),
which takes a `MergedSandboxConfig` a planning session does not have, on the emitted JSON instead.

## The guard (layer 3; section 12 "3. The planning hook")

`guard.json` is written by `write_session_sandbox` beside `settings.json`, 0600, and is the guard's
only input besides the payload and the live tree. Reads of it are bounded (64 KiB) and no-follow
(`fs::safe_read::read_to_string_bounded(session_dir, "guard.json", ..)`).

Payload: Claude Code's PreToolUse JSON on stdin (`hook_event_name`, `tool_name`, `tool_input`,
`cwd`, `session_id`, ...), read up to 1 MiB + 1 byte as `commands/hook/worker_brief.rs:97-105`
does; more than 1 MiB is a deny.

Decisions, in this order, for `tool_name` in the matcher list:

1. `hook_event_name` present and not `"PreToolUse"`: print nothing (the spool lesson,
   `mistakes/spurious-waiting-for-input.md`).
2. Any failure (policy unreadable or schema not 1, payload not JSON, `tool_input` not an object,
   the path field missing or not a string, the tree scan failing): `Deny` with a reason naming the
   failure. The guard never panics past `run_pre_tool_use` (wrap with `std::panic::catch_unwind`
   and deny).
3. Path extraction: `Edit`, `Write`, `MultiEdit`, `Read` take `tool_input.file_path`;
   `NotebookEdit` takes `tool_input.notebook_path`; `Grep` and `Glob` take `tool_input.path`, and
   when it is absent the payload's `cwd`. A relative path is joined to the payload's `cwd` (absent
   cwd: `project_root`). The joined path is normalised lexically (`.` dropped, `..` pops a
   component; a `..` above `/` is a deny). The result must be absolute. Any path with a component
   under `/proc` or `/dev/fd` (for example `/proc/self/cwd/...`, `/dev/fd/3`) is a deny for every
   tool (px-security-01).
4. Writes (`Edit`, `Write`, `MultiEdit`, `NotebookEdit`), per kind:
   - under `<root>/doc/`: `rel` = path relative to `<root>`. `symlink_component(root, rel)` returning
     `Some` is a deny ("symlink in path"); an existing target that is not a regular file
     (`symlink_metadata`) is a deny. Then `classify_write(&LockedSet::scan(root)?, rel)`:
     authoring allows `DocArea::Doc` only (Knowledge: "use loom knowledge"; Summaries: "use loom
     plan summary seal"; Locked(r): "locked: <reason>"); summary denies all; pressure allows only
     `rel == plan` or `rel` under `pressure_bundle()`, and denies even those when
     `lock_reason(rel)` is `Some` (the plan renamed by hand mid-run).
   - `<root>/planning-omissions.md` exactly: pressure allows (same symlink and regular-file checks
     against `<root>`); others deny.
   - under `<rt>/drafts/`: authoring and summary allow after the symlink check below `<rt>`
     (`symlink_component(rt, rel_to_rt)`); pressure denies.
   - under Claude Code's per-user temporary directory (section 12's last write-table row: "yes"
     for every kind): `<tmp>/claude-<uid>/` where `<tmp>` is the hook process's `TMPDIR` when set
     and absolute, else `/tmp`, and `<uid>` is `libc::getuid()`; allowed after the symlink check
     below that directory. The implementing worker confirms the directory name against the
     installed Claude Code (`claude --version` and a Bash `echo $TMPDIR` in any session) and
     records it with `loom memory note`; plan 7's operator end-to-end run re-checks it.
   - anything else: deny ("outside the session's write set").
   `LockedSet::scan` runs on every write decision, never cached, so a plan locked mid-session is
   refused at once.
5. Reads (`Read`, `Grep`, `Glob`), every kind, checked on BOTH the lexical path and its
   `std::fs::canonicalize` result (px-security-01: a symlink under `doc/`, a symlinked ancestor of
   the project root, or any other link must not reach a secret; the deny entries are compared in
   canonical form too, each canonicalized when it exists). An existing path whose canonicalize
   fails is a deny. Deny when either form equals or lies inside a
   read-denied path (the `denyRead` list of the settings, recomputed by the same function
   `sandbox::read_denies(home, root, rt)` returns, with `~/` expanded to `home` and a
   trailing `/**` meaning "the directory and everything in it"), unless it equals or lies inside
   one of the session's own `allowRead` grants. `Grep` and `Glob` are also denied when the search
   root is a strict ancestor (lexically or canonically) of a home-side denied path (a credential
   path or the planning runtime root), because a recursive search would read inside it, and when
   the search root itself is a symlink (ripgrep follows a symlink named on its command line). A search root that is an ancestor only of
   the project's own token files (`.loom/work/...`) is allowed: the daemon's `.ignore` at the
   state root (`state_root.rs:94`, `search_ignore_body`) keeps ripgrep out of them.
   `read_denies` refuses a path that is not valid UTF-8 after symlink resolution (a lossy
   conversion would deny a path that does not exist); the settings build fails on it, and the
   guard denies every read and search while it fails.
6. Otherwise `Defer`.

Output: `Defer` prints NOTHING. Never print `permissionDecision: "allow"`: an allow from a
PreToolUse hook skips Claude Code's permission rules, which would void layer 2 and the holder's
prompt. `Deny(reason)` prints one line:
`{"hookSpecificOutput":{"hookEventName":"PreToolUse","permissionDecision":"deny","permissionDecisionReason":"<reason>"}}`
(reason through `serde_json`, never string formatting). Reasons are short and name the rule, e.g.
`loom plan: doc/plans/IN_PROGRESS-PLAN-x.md is locked (running plan)`.

## Rules for every worker

- 400 lines per file, 50 per function, enforced by `cargo test --test maintainability` and the
  exact ledger `loom/maintainability-baseline.txt`, which fails on growth AND shrinkage of a listed
  file. No file this plan touches has a ledger row; `cli/dispatch.rs` (375 lines at
  `b0f91c00`, more after plans 1, 2 and 4 add arms) and `cli/types_ops.rs` (165) must stay under
  400. Never edit the ledger.
- Host-side opens of agent-writable paths (`doc/`, the runtime dir) go through `fs::safe_read` /
  `fs::safe_fs`, never `std::fs::read`/`write` (`patterns/security-sandbox-and-hooks.md`).
- No new dependency. `serde`, `serde_json`, `shell_escape`, `tempfile`, `serial_test` exist.
- Comments: match the surrounding density; explain non-obvious constraints with the spec or
  knowledge citation.
- Never run git. Record surprises with `loom memory note` (the subagent preamble has the form).
