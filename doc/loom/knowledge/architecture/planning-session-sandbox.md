# Planning Session Sandbox

>

## Overview (`loom::plan_session`)

Every planning session (the planner's authoring chat, its summary runs, a pressure run) starts under
settings this module writes. It is a builder beside `sandbox::settings`, never `generate_settings_json`,
which adds package-manager cache writes, orchestration-state reads and plan-authored config a planning
session must not have; the stage builder is the first settings producer
([Execution Containment](execution-containment.md#claude-code-grants-a-linked-worktree-its-whole-git-common-directory)).
The contract is `doc/product/feat-planner/section_12.md` ("Planning sessions"); the code is `loom/src/plan_session/`.

| File | Owns |
| --- | --- |
| `kind.rs` | `SessionKind` (`Authoring`, `Summary`, `Pressure { plan }`; `label`, `pressure_bundle`) and `SessionDir` (the per-session paths: `settings`, `guard`, `record`, `launch_script`, `brief`, `plugin`, `events`, `answers`, `done`, `marker`, `codex_log`) |
| `sandbox/mod.rs` | `write_session_sandbox` (entry), `build_settings`, `ensure_runtime_grants`, `hook_command`, the `PreToolUse` matcher and timeout |
| `sandbox/filesystem.rs` | layer 1 `denyWrite`/`allowWrite` (`write_lists`, one function per kind), `OMISSIONS` |
| `sandbox/permissions.rs` | layer 2 `permissions` and `hooks` per kind |
| `sandbox/reads.rs` | `read_denies` -> `ReadDenies { denied, carve_outs }`, shared with the guard |
| `sandbox/assert.rs` | `assert_planning_settings`: refuses a settings object that lost a switch, a deny or the guard hook |
| `guard/` | layer 3, `loom plan hook PreToolUse`: `mod.rs` (decide), `paths.rs` (read rules), `search.rs` (Grep/Glob) |
| `hook_cli.rs` | `loom plan hook <event> --session-dir <dir>`: always exit 0, prints a deny line or nothing; `PreToolUse` is the guard, the ten spool events are in [Planning Session Bridge](planning-session-bridge.md) |

## Three layers, per kind

`write_session_sandbox` creates the runtime grants, scans the locked set, builds the settings, asserts
them and writes `settings.json` and `guard.json` (0600, no-follow) into the session directory.

1. **OS (`sandbox.filesystem`).** `denyWrite` lists every top-level entry of the project root that
   exists at spawn, `.git` included, except `doc`; inside it the locked plans, run reviews and
   the archive, as literal existing paths. `allowWrite` grants the runtime `drafts/`, `claims/` and
   `summaries.lock`. `denyRead` is `policy::mandatory_deny_read`, `state_root::token_deny_paths`,
   the state root's `terminal-history` and the planning runtime root (`~/.loom/plan`, following
   `LOOM_HOME`), with the three runtime grants carved back out through `allowRead`.
2. **Permission rules.** `Edit(//<abs>)` allow and deny rules only (`Write(path)` is never emitted:
   [Sandbox Settings Emission](execution-containment.md#sandbox-settings-emission--editpath-never-writepath)),
   the five `Bash(loom ...)` commands of authoring, `WebSearch` allowed, `WebFetch` asking,
   `AskUserQuestion` and the capsule tools denied, `defaultMode` from `permission_mode(kind)`
   (`sandbox/permissions.rs`: `auto` for authoring and pressure, `default` for summary, where an
   unlisted tool is denied at once; the launch command passes the same value as `--permission-mode`
   and `assert.rs` refuses settings whose mode differs for the kind), `disableBypassPermissionsMode`, `autoAllowBashIfSandboxed: false`, `strictAllowlist` with no domains,
   `worktree.bgIsolation: "none"`.
3. **The guard**, below.

| Kind | Writable by file tools | Notes |
| --- | --- | --- |
| authoring | `doc/` except the locked set and `doc/loom/`; runtime `drafts/` | `Edit(doc/product/**)`, `doc/plans/**`, one `Edit(doc/<d>/**)` per existing other directory of `doc/`, one literal `Edit` per top-level regular file of `doc/` |
| summary | runtime `drafts/` only; `loom plan summary seal` writes `doc/loom/summaries` | layer 1 denies every `doc/` entry but the path down to `doc/loom/summaries`; no file-tool rule reaches `doc/`; `WebSearch` and `WebFetch` denied |
| pressure | the plan under test, its brief bundle, the omissions file (`OMISSIONS`); `Bash(touch <marker>)` writes the session's `done/` marker | layer 1 denies every `doc/` entry but `doc/plans` and, inside it, all but the plan and `briefs/<slug>`; `Bash(touch <marker>)`; keeps `SendMessage` and `ListAgents` for `/pressure`'s teammates |

## Rules the builder keeps

- **Literal paths, no nested denies.** Every layer 1 entry is an existing literal path, `collapse` drops
  any entry under a kept one, and a path that does not exist is not denied. bubblewrap refuses a mount
  point beneath a read-only bind, so a deny nested inside a denied directory aborts the sandbox
  ([Sandbox And Settings](../mistakes/sandbox-and-settings.md)). That is why layer 2, not layer 1,
  carries `Edit(briefs/<slug>/**)` for every locked plan slug, including an archived plan whose bundle
  directory does not exist, and carries no per-file rule under `archive/**`.
- **No glob rule over `doc/loom/`.** Claude Code matches `Edit` paths with gitignore semantics, so
  `Edit(//<root>/doc/*)` would match the directory `doc/loom` and cover `doc/loom/knowledge/**` whenever
  the guard hook does not answer. Top-level files of `doc/` are therefore listed one literal rule each; a
  file created later matches no rule, so in auto mode it reaches the classifier while the guard
  decides it. A name holding rule syntax
  (`* ? [ ] { } ( ) \ !`) is skipped for an allow and refused for the pressure plan or bundle
  (`permissions.rs` `RULE_SYNTAX`).
- **Auto mode needs explicit asks.** Under `auto` an action no rule lists goes to the classifier, so
  authoring and pressure carry `Edit` ask rules (`file_tool_asks`): `doc/loom/**` and each top-level
  entry of the project except `doc` (and a pressure session's omissions file), enumerated at spawn,
  a directory with everything beneath it; kickoff also asks on its `loom ...` writer commands
  (`KICKOFF_WRITER_ASKS`) and the team scope on `KNOWLEDGE_BASH`. An ask outranks an allow, so no
  catch-all covers the root or `doc`; an entry created after spawn stays with the guard. Ask rules
  match the literal `loom ...` spelling only, so `/abs/path/loom knowledge update` or a shell append
  into `doc/loom` reaches the classifier (section 12 accepts this).
- **Why summary sessions deny `doc/` entries.** A summary run's only file-tool write is a draft; the
  seal is a CLI call that needs OS write access to `doc/loom/summaries` and nothing else, so layer 1
  leaves that one path and denies the rest of `doc/`.
- **`summaries.lock` is created with `open_safely(O_RDONLY|O_CREAT|O_NONBLOCK)`**, never `safe_append`,
  whose exclusive `flock` would block every launch while a summary seal holds the lock.
- **A non-UTF-8 name** at the top level or under `doc/` fails `build_settings` for the whole kind
  (`filesystem::text`) with a message naming the path to rename.
- **`assert_planning_settings` infers a pressure session from a `Bash(touch` allow** to decide whether
  the `SendMessage`/`ListAgents` denies are required, because its signature takes only the settings.

## The guard (`guard/`)

`loom plan hook PreToolUse --session-dir <dir>` (the command line `hook_command` shell-escapes once)
reads `guard.json` (`GuardPolicy`: schema, kind, project root, runtime dir, session dir, home; at most
64 KiB) and the payload (at most 1 MiB). It handles `Read|Edit|Write|MultiEdit|NotebookEdit|Grep|Glob`.

- **Decision order.** Failure to read the payload, a non-object `tool_input`, a missing policy and a
  panic are denies. Another event's payload is silent. A tool outside the matcher is deferred. Then:
  writes go through `check_write` (a path under `doc/` runs symlink and regular-file checks, a fresh
  `LockedSet::scan` and `classify_write`, then the kind's rule; the omissions file is pressure
  only; runtime `drafts/` is not pressure's; Claude Code's own `<tmp>/claude-<uid>` is writable except
  where it contains the project, planning root, session dir or home); reads and searches go through
  `ReadRules`; a Grep/Glob additionally goes through `search::check_search`.
- **It never prints `allow`.** A PreToolUse `allow` skips Claude Code's permission rules, voiding
  layer 2 and the holder's prompt. The guard denies or stays silent, so a path it passes still meets
  the rules.
- **A plan locked mid-session is refused at once** because every `doc/` write rescans the locked set;
  layers 1 and 2 were built at spawn and miss it.
- **The read-deny ancestor rule.** A read of a denied path is refused in its lexical and canonical
  forms. A Grep/Glob is also refused when its root is a symlink or a strict ancestor of a credential
  path or the planning runtime root. The project's state root is `search::check_search`'s, which ports
  `loom-hooks/credential-guard.sh`'s `walks_into_state_root`: a root inside the state root, a root at
  its parent (a legacy `.work` root's parent is the project root and stays searchable) and a filtered
  Grep above it are denied. The state-root `.ignore` is not relied on: it lists only
  `STATE_ROOT_SECRET_FILES` (not `terminal-history`), ripgrep does not prune an explicitly named root,
  a Grep `glob` overrides ignore files, and native Grep runs outside the OS sandbox.
- **Glob patterns** are refused when absolute, `~`-rooted, holding any `..`, or holding `/` or `~`
  right after `{` or `,` (a brace alternative climbs or roots without a leading `/`).
- **`/proc` and `/dev/fd`** are denied for every file tool (`/proc/self/cwd` reaches token files).
- **The temporary directory.** Claude Code exports `TMPDIR=/tmp/claude-<uid>` to its commands, so a
  `TMPDIR` already ending in `claude-<uid>` is the directory itself (`claude_tmp_dir`).
- **`loom plan hook` is exempt** from terminal recovery and the update check in `main.rs`
  (`is_plan_hook`); `loom plan` itself keeps both. The hook's stdout is a pipe, so recovery would not
  have written ANSI bytes there either (`write_terminal_sequence` writes only to a terminal).

## Under srt on Linux

- A read-denied directory is covered by a private writable tmpfs
  ([Execution Containment](execution-containment.md#claude-code-grants-a-linked-worktree-its-whole-git-common-directory),
  "Reads"), so a `touch` under the planning root exits 0 and never reaches the host. The write-table
  property tested is that the host runtime directory is never written, not the shell's exit code.
- `LOOM_PLAN_STATE`'s `check_state` accepts a writable parent when `drafts/`, `claims/` and
  `summaries.lock` are mounts on another device than the state directory
  ([Planning Document Model](planning-document-model.md#runtime-directory)).
- `loom map` and `loom knowledge context` work with `.loom` write-denied: a cache write creates its own
  parent, and a denied one falls back to memory, so neither calls `ContextStore::ensure` up front
  (`architecture/source-graph.md`).

## Confinement test

`loom/tests/plan_session_confinement.rs` translates the authoring `settings.json` for srt
(`plan_session_confinement/srt.rs`) and probes from the project root with a fake home: the planning
commands run, raw writes to code, `.git` and a locked plan are refused, credential, runtime and token
files cannot be read, and the loopback and Unix-socket routes are closed. It is Linux only and skips,
printing the reason, unless `bwrap`, `socat` and `srt` are on `PATH` and `srt -c true` runs. A stage's
criteria runner and session nest a sandbox that cannot start srt, so the real-sandbox run is the
operator's on the host: `LOOM_TEST_REQUIRE_SANDBOX_FREE=1 cargo test --test plan_session_confinement`
turns the skip into a failure. Recorded runs: srt 0.0.78, Linux x86_64, 14 of 14 passing; no macOS run
exists. Unit tests mutate the builder (hard-coded deny list, skipped `collapse`, single-slash rule,
summary built with authoring rules, guard returning `allow`, symlink check removed) and each goes red.

Open items and accepted limits: [Planning Session Sandbox Limits](../concerns/planning-session-sandbox-limits.md).
