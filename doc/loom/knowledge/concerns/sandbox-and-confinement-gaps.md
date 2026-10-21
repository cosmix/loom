# Sandbox And Confinement Gaps

> Canary, creds, env, caches

## Sandbox Denial Has No End-to-End CI Canary

The generated sandbox policy is covered by unit and flow tests, but nothing proves denial holds
against a live Claude Code runtime: CI has no credentialed Claude Code sandbox, so Bash,
interpreter, build-script, symlink and file-tool denial cannot be exercised end to end there.
That verification is manual release validation. The srt harness that stands in for it outside the
sandbox lacks Claude Code's linked-worktree grant of the git common directory and probes no
`denyRead` path ([Agent Rule-Bending Hardening](agent-rule-bending-hardening.md), G3).
No in-session canary exists; a withdrawn design for one is stage `sandbox-canary` in
the deleted PLAN-sandbox-escape-hardening, recoverable with `git show e6f00616^:` plus its `doc/plans` path.

## ReDoS Potential in Plan Pattern Regex

User-provided regex patterns in plan wiring checks are compiled and executed without complexity checks. While mitigated by trust model (plan authors = trusted), consider adding regex timeout or complexity limits for defense in depth.

## Three Stage Environment Allowlists

Three host-environment lists remain ([The Host Environment Allowlist](../architecture/execution-containment.md)
has the full contents):

| List | Form | Consumer |
| --- | --- | --- |
| `process/environment.rs` `STAGE_HOST_ENV_ALLOWLIST` | Rust `&[&str]` | `spawn_confined`, the native spawner and the tmux server commands |
| `process/environment.rs` `AGENT_SESSION_ENV_NAMES` | Rust `&[&str]` | stage agent sessions: `wrapper/script_text.rs` renders its `exec env -i` shell loop from it |
| `daemon/server/environment.rs` `HOST_ENV_ALLOWLIST` | Rust `&[&str]` | the daemon child |

One constant now drives the wrapper, and tests pin every session name into the other two lists, so
the lists cannot drift apart silently. What remains is a deliberate gap: sessions do
not receive the proxy variables, the CA bundle locations (`SSL_CERT_FILE`, `SSL_CERT_DIR`,
`NIX_SSL_CERT_FILE`), `CARGO_HOME` or `RUSTUP_HOME`, although plan-authored commands do, and no
list forwards `CLAUDE_CONFIG_DIR`.

**Concrete failure mode:** on a host behind a corporate proxy, a plan-authored acceptance command
can fetch and a stage agent session cannot, and the symptom is a network failure inside the agent
with no error pointing at an env allowlist. A relocated `CARGO_HOME` hides the same way.

**Fix when needed:** add the names to `AGENT_SESSION_ENV_NAMES` (the wrapper follows) and keep the
subset tests green.

## Confined Commands Still Reach a Live Credential Bus

`process/environment.rs` withholds `SSH_AUTH_SOCK` with an explicit rationale — it
is a live credential-agent socket, not a location — while forwarding
`DBUS_SESSION_BUS_ADDRESS` (`:33`) and `XAUTHORITY` (`:32`). A session bus address
reaches `org.freedesktop.secrets`, which is a live credential surface by exactly
the argument used to withhold the SSH socket.

**Root cause worth recording:** one allowlist serves two consumers with different
needs. The terminal spawner genuinely needs display and session variables to attach
a window; `spawn_confined` does not need either. **Fix:** split the list into a
common base plus a terminal-only extension, and let `spawn_confined` take only the
base. The wrapper's list is already generated from `AGENT_SESSION_ENV_NAMES`, so the split
only has to separate `STAGE_HOST_ENV_ALLOWLIST`'s two consumers.

See `architecture/execution-containment.md` for the honest statement of what
confinement does and does not guarantee.

## Sandbox-Widening Fields Need No Author Acknowledgement

`sandbox::validate_config` (`sandbox/config.rs`) refuses `sandbox.enabled: false` and
`sandbox.allow_unsandboxed_escape: true` outright, with no acknowledgement possible, so a plan
carrying either cannot run. `allow_all_unix_sockets: true` needs the operator's
`loom run --accept-unix-socket-risk` (`sandbox/config/preflight/unix_sockets.rs`). The remaining
widening fields, `allow_write`, `allow_local_binding` and `linux.enable_weaker_nested`
(`models/stage/types.rs`), need no acknowledgement: `plan/schema/validation.rs` only checks that
`allow_write` entries are valid globs.

## Uncalled Path-Escape Validators Read As Protection

`sandbox/config.rs` ships three `pub` path-escape validators that **nothing in production
calls**: `detect_path_escape` (`:192`), `validate_paths` (`:276`) and
`is_legitimate_work_access` (`:297`). They are re-exported from `sandbox/mod.rs:10-11`, and
`rg` over `loom/src` finds their only callers are their own tests at `:636-730`.

`test_validate_paths_detects_escape_in_allow_write` (`:711`) asserts `validate_paths` flags an
escape in `allow_write` — but `loom init` runs `validate_sandbox` and `validate_emittable` per
stage, never `validate_paths`, so that escape was in fact emitted unchecked. A green test
standing in for an absent control (`mistakes/tests-that-cannot-fail.md`).

Two things make it invisible: the items are `pub` and re-exported, so `dead_code` never fires
(a fresh build emits ZERO warnings), and the test name reads like coverage of a live
guarantee.

Deliberately not wired in when found: turning `validate_paths` on at plan-validation time
would start REJECTING plans that load fine today, a behaviour change a verification stage
should not make. The parent-traversal filter in `sandbox/settings.rs` already blocks this class
of escape at the point of use, independently of these validators.

**Owner should pick one:** wire it into `loom init` and `plan verify` as a fail-fast check
(preferred — a clear error beats a silently dropped entry), or delete all three and their
tests. **Leaving `pub`-but-uncalled validators is the worst of the three, because it reads as
protection.**

## A Check-Then-Use Window Around the `config.worktree` Check

`WorktreeGit::run` (`git/worktree/pinned.rs`) calls `check_worktree_config` before each git command, for a worktree created after an agent's session spawned (the capsule's `config.worktree` deny covers only worktrees that existed when it was built). An agent with write access to that worktree's admin directory can rewrite `config.worktree` between the check and the command it guards. Closing it needs the file unwritable to the agent for the whole command, which the grant of the git common directory prevents (G2 in [Agent Rule-Bending Hardening](agent-rule-bending-hardening.md)).

## Accepted Gaps From the State-Confinement Work

Two gaps from PLAN-loom-state-confinement were accepted, not closed:

- **The approved-permissions filter reads rule text only** (`fs/permissions/sync.rs`'s fold-back).
  A rule naming a symlink into a control surface (`.loom`, `.claude`, `.worktrees`, a hook
  directory, `~/.loom`, the scratch root) still passes the filter. The OS deny rules close this
  in practice — a deny always wins over an allow, and the sandbox resolves symlinks before
  applying either — but the filter itself does not detect the symlink.
- **Shared package-manager caches stay writable** (`sandbox::PACKAGE_MANAGER_CACHE_WRITE_PATHS`).
  `~/.rustup/toolchains` and `~/.local/share/uv` are not granted, but the cargo, bun, npm, pnpm,
  yarn, deno, uv, pip and go caches are one directory shared by every concurrent session and read
  by the operator's own builds. [State Confinement Gaps](state-confinement-gaps.md) has the
  consequence. The gap is open: no plan makes them per-session.

## No `Read(...)` Deny Rule May Exist in Any Settings File

Claude Code (verified against 2.1.259) runs two checks on `rg`, `grep`, `egrep`, `fgrep`, `diff`,
`git`, `cp` and `mv`. Both return `ask` with `circuitBreaker: deniedPathInsideDirectory`,
bypass-immune and not classifier-approvable, so auto mode stalls on an operator prompt:

1. **Location check.** Each path argument (`rg` with no path means `.`) is compared with every
   `Read(...)` deny rule's location, the rule's path up to its first wildcard. A location inside or
   equal to the searched directory prompts, so a concrete token rule such as
   `Read(//home/you/src/app/.loom/work/admin.token)` prompts on every search rooted at the project.
2. **`cd` check.** If the compound command contains a `cd` anywhere, even `cd /absolute/path`, and
   the path argument is relative, the location is treated as unknowable and the check prompts
   whenever ANY `Read` or `Read(...)` entry exists under `permissions.deny` in ANY settings source
   (user, project, local, worktree). Shape and location are irrelevant. The predicate is
   `Object.values(alwaysDenyRules).flat().some(r => r === "Read" || r.startsWith("Read("))`.

Globbing the project directory out of a token rule
(`Read(//home/you/src/*/.loom/work/admin.token)`) defeats only check 1 and adds a worse defect: on
Linux every `Read(...)` deny is fed to the OS sandbox, whose glob expander takes the wildcard-free
prefix (`/home/you/src`), runs `readdirSync(prefix, {recursive: true})` synchronously on the main
thread and regex-tests every entry, per sandboxed Bash command. Only a prefix of exactly `/` is
refused as too broad. On a `~/src` holding 2.7 million inodes that froze the TUI for long stretches.

Loom therefore never writes a `Read(` entry under `permissions.deny`, anywhere.
`sandbox::settings::generate_settings_json`, `build_settings` (the document each session capsule
is written from, `write_session_capsule`) and `git::worktree::settings` emit none; `carry_forward_denies` and the worktree refresh drop every `Read(` deny on shape alone;
`fs::permissions::sync` never promotes one; `fs/permissions/write_rules.rs::prune_loom_read_denies`
strips loom-written ones (token denies in any spelling, and mirrors of
`state_root::CREDENTIAL_DENY_READ_PATHS`) from both `.claude/settings.json` and
`.claude/settings.local.json` on `loom init`; `loom repair` check 14 (`check_read_denies`) strips
them from every loom-written file and reports, warn-only, an operator-authored `Read(...)` deny it
will not remove. `generated_settings_carry_no_read_deny_rules`
(`sandbox/settings/tests_token_rules.rs`) pins the property.

The boundary those rules described is kept by other layers. `sandbox.filesystem.denyRead` is an OS
list, not a permission rule: it triggers neither check and keeps Bash out of the credential
directories, both tokens and the completion attestation key (`policy::MANDATORY_DENY_READ` carries
all five credential paths, so a plan's `deny_read` cannot drop them). `loom-hooks/credential-guard.sh`
is a PreToolUse guard on Read, Glob, Grep, Edit, MultiEdit, Write and NotebookEdit. Its first rule
blocks `admin.token`, `user.token` and `completion-attestation.key` under any state root
unconditionally. Its second rule, which applies a `denyRead` list to the file tools
(`deny_read_blocks`), reads `$PROJECT_DIR/.claude/settings.local.json`. Loom no longer writes that
file (every session launches from its capsule, `W/capsules/<session-id>.settings.json`), so the
rule acts only on an operator-authored file, and a session's own `denyRead` list never reaches the
file tools through it. A worktree session's file tools stay inside the worktree through
`worktree-file-guard.sh`; for a checkout-rooted session no loom hook checks the file tools against
the credential paths. The second rule reading only an operator-authored file is an open gap.
A hook can be switched off by `disableAllHooks` and shares the check-then-open race noted under
"PreToolUse File Guards Cannot Eliminate Path-Swap Races"; that is the accepted trade for a
prompt-free auto mode. Never reintroduce a `Read(...)` deny of any shape, and never emit a
`denyRead` glob whose wildcard-free prefix lies above the project or above a small home
subdirectory.

## Locked-Write Symlink Fix Was File-Only, Not Directory-Component

`fs/locking.rs` now opens `<path>.tmp` with `O_NOFOLLOW`, closing the specific
`<file>.tmp`-as-tracked-symlink redirect found during `knowledge-bootstrap-command` integration
(`mistakes.md`, "A Tracked Symlink Named `<file>.tmp` Redirected a Locked Write"). Directory
components of the target path are not similarly checked anywhere in the crate: a tracked
symlinked `doc/loom/knowledge` or `.loom` directory is still followed and written through by
`locked_write`, `loom map`'s overlay, and any other writer that resolves a path under it.
**Open:** no component-wise no-follow check exists for directories, only the final path segment.

## Bubblewrap Confinement Reads as Stage Evidence in Remote-Tools Mode (unconfirmed)

`_loom_confinement_evidence` (`loom-hooks/_codex_forward.sh`) treats a pid namespace whose pid 1 is `bwrap` as loom stage evidence. Claude Code runs project hooks through `wrapWithSandbox` (bubblewrap with `--unshare-pid`) when this machine executes tool calls for a remote session. A reconstruction of that confinement (`bwrap --ro-bind / / --dev /dev --unshare-pid --unshare-user --cap-drop ALL --proc /proc`) made `codex-forward-guard.sh` classify the session as a stage and block the stock `codex:codex-rescue` agent ("command is not an exact forwarding-wrapper invocation"); if those payloads also lack `agent_type` and `transcript_path`, the missing-metadata check (`codex-forward-guard.sh:345-347`) would block every call. Not reproduced in a real remote session. Local interactive sessions run hooks unsandboxed and are unaffected. Deciding it means weighing the plugin's usability there against the fail-closed stage classification that confinement evidence exists for.

## Plan Files Are Not Merge-Gate Control Paths

`orchestrator/core/merge_handler/merge_gate.rs::is_control_path` protects `.claude/`, `.mcp.json`, `.loom/` and the tracked git hooks directory. A plan file under `doc/plans/` is ordinary content: a stage whose sandbox grants `doc/**` can edit its own plan, and the edit merges automatically. Anything the daemon runs on the host must therefore come from a snapshot taken at `loom init` in the work directory (the `[plan_sandbox]` precedent in `fs/work_dir/config_sections.rs`), never from the live plan file at spawn time. PLAN-stage-exits-and-environment's `provision` follows this rule (`[plan_provision]`). Adjudicator amendments rewrite the plan file too, but they only touch sandboxed fields.

## A Stage Agent's Bash Allow-List Reaches Outside the Worktree

A stage agent's Bash is confined by the bubblewrap sandbox (the file-tool guard `worktree-file-guard.sh` does not see Bash), but
the allow-list still reaches beyond the worktree. The harness adds the whole git common directory: `HEAD`, `refs/heads/main`,
`packed-refs`, `objects`, `info` and other worktrees' metadata are writable. Under it loom denies only `.git/hooks`, `.git/config`
and the `config.worktree` of every worktree registered at spawn (`worktree_admin_dirs` in
`sandbox/control_surfaces/session_denies.rs`), so a sibling worktree's administrative directory stays writable apart from that
one file. `package_caches.rs` grants the operator's cargo, bun and npm caches, so a tampered
`~/.cargo/registry/src` crate would run on the host at the next build. `denyRead` covers only five credential paths, so
`~/.config/gh/hosts.yml`, `~/.netrc` and `~/.npmrc` are readable. The gap is open; metaplan section 7
(the untracked token-optimization metaplan) plans the credential-file and sibling-administrative-directory denies. The
withdrawn PLAN-sandbox-escape-hardening (see above for recovery) holds the
measurements (Claude Code 2.1.286 adds the git common dir itself; `allowWrite`/`denyWrite` globs are skipped on Linux; a deny
on an absent path shows the host an empty placeholder file; a deny cannot be reopened by a nested allow) and a design, D1-D7.

## Credential Reads Are Confined to Five Home Paths

The OS `denyRead` list is `fs/permissions/state_root.rs::CREDENTIAL_DENY_READ_PATHS` (`~/.ssh/**`,
`~/.aws/**`, `~/.config/gcloud/**`, `~/.gnupg/**`, `~/.claude/.credentials.json`), the plan's own
`deny_read`, and loom's state tokens and attestation key
(`sandbox/settings/policy.rs::deny_read_patterns`, `add_resolved_state_root_rules`). From a live
stage session `~/.config/gh/hosts.yml` (a GitHub token), `~/.netrc` and `~/.npmrc` read fine.
Unlisted and therefore readable wherever they exist: `~/.git-credentials`,
`~/.docker/config.json`, `~/.kube/`, `~/.pypirc`, `~/.cargo/credentials.toml`, `~/.claude.json`,
shell histories, browser profiles, password-manager stores, and any credential location an
environment variable relocates (`DOCKER_CONFIG`, `KUBECONFIG`, `GH_CONFIG_DIR`, ...). A session the
codex lane is not licensed for can read `~/.codex/auth.json` too. The network filter narrows
exfiltration to the plan's allowed domains; it does not remove it (a registry publish to an
allowed domain is one channel).

How Claude Code applies a `denyRead` entry on Linux: a file is bound from `/dev/null`, a directory
is covered by a tmpfs (with `allowRead` and `allowWrite` paths re-mounted inside it), an absent
path mounts nothing, and a glob is expanded into one mount per match (a warning above 256). A plain
path costs one mount. bubblewrap mounts every non-dev bind `nodev`, so opening a read-denied file
fails with EACCES (`Permission denied`); it does not read empty (measured with bubblewrap 0.11.1,
also nested in a host-job-style `bwrap --unshare-all`; Claude Code 2.1.306 binds with
`--ro-bind /dev/null <p>`). `npm config get`, `bun install`, `cargo build --offline` and
`git credential-store get` still succeed with the credential files bound this way.

The list is unchanged and the gap is open: no merged change closes it (the token-optimization
programme's sandbox route-closure plan targets it).

## Sibling Worktrees and the Main Checkout Are Readable From Bash

Rule 10 forbids a stage from reading outside its worktree, but only the file tools enforce it
(`loom-hooks/worktree-file-guard.sh`, PreToolUse on Read, Write, Edit, MultiEdit, NotebookEdit,
Glob and Grep). From Bash a stage reads every sibling worktree under `R/.worktrees/` and the whole
main checkout, untracked operator files such as `R/.env` included. `R/.git/config` stays readable
whatever loom does, because in-session git reads it; remote URLs there can carry tokens.

Sibling-worktree reads are open. Reads of the main checkout must stay open for in-session loom,
which resolves its retrieval config, context store and knowledge through `main_project_root`
(`context/retrieve.rs`, `context/store.rs`).

## The Codex Lane Grants All of `~/.codex`

A capsule whose stage licenses codex adds `codex.rs::CODEX_SANDBOX_WRITE_PATHS` (`~/.codex` and
`~/.claude/plugins/data/codex-openai-codex`) to `allowWrite`; inside it
`sandbox/control_surfaces.rs::HOME_SURFACES` denies only `~/.codex/hooks/**`, `~/.codex/hooks.json`
and `~/.codex/config.toml`. Writable to the stage, and trusted later by the operator's own codex
sessions: `~/.codex/AGENTS.md` (doctrine), `~/.codex/rules/` (command approval rules),
`~/.codex/skills/`, `~/.codex/plugins/`, `~/.codex/packages/` and `~/.codex/vendor_imports/`.
`~/.codex/auth.json`, the operator's OpenAI credential, is readable and writable. Codex runs inside
the stage sandbox (the forwarder's Bash call starts the companion, which spawns `codex app-server`
itself), so the outer sandbox is the only boundary for its writes.

`loom-hooks/codex-forward.sh` points `CLAUDE_PLUGIN_DATA` at `~/.codex/plugin-data`, where
host-side code reads job state and forward receipts (`models/forward_receipt/locator.rs`,
`codex_lifecycle/authorization.rs`, `codex-forward-guard.sh`); every codex-licensed session can
write it.

The grant is unchanged and the gap is open: no plan addresses it.

## Tool Routes That Run Outside the Bash Sandbox

The OS sandbox confines only what Claude Code wraps. In Claude Code 2.1.286 the binary calls `wrapWithSandbox`/`wrapWithSandboxArgv` for the Bash tool, for hooks in its "locked" (remote-execution) mode, and for one internal git invocation. Every other tool runs in, or is spawned by, the unsandboxed Claude Code process. (Verified from the binary's strings, 2026-10-01.)

- **MCP servers: closed.** Capsule sessions get `--strict-mcp-config` (`orchestrator/terminal/native/capsule.rs`, emitted in `orchestrator/terminal/native/mod.rs`). A `claude -p` probe with loom's flags (`--strict-mcp-config --setting-sources user,project`) reported `mcp_servers: []`: plugin MCP servers such as Playwright and the claude.ai connectors are not loaded. (Verified 2026-10-01.)
- **Plugins: loaded.** `--setting-sources user,project` keeps the user scope's `enabledPlugins`; the same probe loaded 13 plugins, five of them LSP plugins (rust-analyzer-lsp, typescript-lsp, gopls-lsp, pyright-lsp, clangd-lsp), and `LSP` is in the session's tool list. Plugin LSP servers are started by the Claude Code process, not through `wrapWithSandbox`. rust-analyzer runs the build scripts and proc macros of the workspace it indexes, and tsserver loads language-service plugins named in `tsconfig.json`, so a stage that writes a `build.rs` or a tsconfig plugin and then calls the LSP tool can plausibly run that code on the host. Unverified end to end.
- **Cross-session messaging.** `SendMessage` and `ListAgents` are in the tool list and the session advertises a messaging socket under `/run/user/<uid>/cc-socks/`. They run in the Claude Code process, so the seccomp `AF_UNIX` block that confines Bash does not apply. Unverified whether a stage session can address the operator's interactive session, which runs unsandboxed.
- **`WebFetch` and `WebSearch`** run in the Claude Code process; the sandbox proxy's `allowedDomains` and `strictAllowlist` filter sandboxed Bash only. Permission rules and the auto-mode classifier govern them. Unverified.
- **Other tools.** `RemoteTrigger`, `CronCreate`, `Workflow` and `PushNotification` are also in the tool list; `RemoteTrigger` starts work off the machine.
- **File tools** are confined by hooks only (`worktree-file-guard.sh`, `credential-guard.sh`), covered above.

Fix direction (none chosen): per-session tool denies in the capsule (`--disallowedTools`, or tool-name entries under `permissions.deny`, never a `Read(` entry, see the section above), disabling LSP plugins in capsules, and an in-session canary that exercises each route.

## A File Deny Stops Protecting a Live Session Once the Host Replaces the File

Measured with bwrap 0.11.1, git 2.53.0, Linux 7.1.5. A read-only bind of a single file (`--ro-bind f f`, how a file `denyWrite` is enforced) refuses writes and renames inside the sandbox while the host leaves the file alone. Once a host process replaces the file by rename (`echo new > f.tmp && mv f.tmp f`, which is how git writes refs, `config`, `config.worktree` and `packed-refs`: `open(<ref>.lock, O_EXCL)`, then `rename`), the running sandbox resolves the path to the new file and can write it, and the write lands on the host. So every per-file deny (loom's `.git/config` and per-worktree `config.worktree` denies in `sandbox/control_surfaces/session_denies.rs`, and Claude Code's own `config`/`config.worktree`/`commondir` denies) protects a session only until the host first rewrites that file during the session.

| Host action during the session | Bind | Sandbox write |
| --- | --- | --- |
| replaces the file by rename (E1) | file | lands on the host |
| rewrites the file in place, same inode (E5) | file | still refused |
| unlinks the file (E6) | file | sandbox creates it; lands on the host |
| replaces a file inside the directory by rename (E2) | directory | refused |
| none; sandbox creates `dir/new` or renames a file inside (E3) | directory | both refused |
| replaces the directory itself (E4) | directory | lands on the host |

A directory deny therefore holds against files replaced, created or renamed inside it, and stops holding once the host replaces the directory. Related measurements: an empty loose `refs/heads/<b>` shadows `packed-refs` and breaks the branch (`No commits yet`, commits fail), which is what a deny on an absent ref path would leave on the host; `git update-ref` to the ref's current value does not turn a packed ref into a loose file.

Git's `reference-transaction` hook, measured on the same host:

- A `reference-transaction` hook in `.git/hooks` fires for `update-ref`, `branch -f`, `push .`, `fetch .`, `merge --ff-only`, `commit` and `reset --hard` (H1).
- It is skipped by `-c core.hooksPath=/dev/null` and by a raw write of the ref file: the ref moves and the hook never runs (H2).
- Run from a worktree under bwrap with `.loom/` read-only, a hook that appends to a ledger under `.loom/` cannot write it; the hook fails and git refuses the update (exit 128, ref unmoved) (H3). The same command with `-c core.hooksPath=/dev/null` moves the ref and writes nothing (H4).
- An interactive operator Claude Code session in the main checkout can write `.loom/work` (H5).

A git hook is therefore no gate against a sandboxed session, which can skip it. The [Target Guard](../architecture/target-guard.md) uses the hook as an attestation source for host-side moves and as a refusal for sessions that do not skip it, never as the only barrier.

## Session-Planted Replace Refs and Grafts Change What Loom's Git Sees

A session can write `<common>/refs/replace/*`, `<common>/info/grafts`, `<common>/objects/info/commit-graph` and `<common>/shallow`: the capsule denies only `.git/hooks`, `.git/config` and each `config.worktree` in the common dir (`loom/src/sandbox/control_surfaces/session_denies.rs`). Git honors all of them in every `diff`, `merge-base` and ancestry test unless told otherwise.

Measured on git 2.53:

- `git replace X D` makes `git diff -z --name-only --no-renames A X` print only D's paths, hiding X's real change from `control_paths::changed_paths`, the merge gate's input.
- A graft line `<A> <S>` makes `git merge-base --is-ancestor S A` succeed for a stage commit S that is not in A. Settling a stage from ancestry would then mark it merged without the target moving.
- `GIT_NO_REPLACE_OBJECTS=1` (or `--no-replace-objects`) disables replace refs but NOT grafts. Only `GIT_GRAFT_FILE` pointing at a file that cannot be opened disables grafts. `/dev/null` itself makes git 2.53 print a deprecation advice on stderr, so loom uses `/dev/null/loom-no-grafts`.
- A forged commit-graph entry changes the parents or tree git reads for a commit reached in a walk; a commit named on the command line is parsed from its object, not the graph.

**What the tree does.** `crate::git::runner` (`loom/src/git/runner.rs`) sets `GIT_NO_REPLACE_OBJECTS=1` and `GIT_GRAFT_FILE=/dev/null/loom-no-grafts` (`NO_GRAFT_FILE`) on every command in `git_command`, and `global_args` prepends `-c core.hooksPath=/dev/null -c core.fsmonitor=false` plus `-c core.commitGraph=false` (`NO_COMMIT_GRAPH_ARGS`), so a session-written commit-graph cannot forge parents either. The evaluation, `merged_into_accepted`, `verify_merge_succeeded` and `changed_paths` all run through the runner. See [Target Guard](../architecture/target-guard.md).

**Still open.** Production git spawned with `Command::new("git")` outside the runner gets none of the three settings: `version/derive.rs`, `verify/criteria/cache_ignore.rs`, `commands/handoff/create.rs`, `orchestrator/adjudication/prompt/sources.rs`, `git/worktree/checks.rs`, `git/cleanup/batch.rs`, `commands/knowledge/annotate.rs`, `commands/pressure/paths.rs`, and `git/branch/cleanup.rs` (`cleanup_merged_branches`, exported and apparently unused). None decides ancestry or a merge. `<common>/shallow` is session-writable and not neutralised: it can only cut history edges (extra holds, or `stage_work` seeing no branches); whether git honors `GIT_SHALLOW_FILE` for the runner is unverified. Git run by the operator honors every one of these files.

## What `allow_all_unix_sockets` Still Exposes

On Linux the session sandbox blocks `socket(AF_UNIX)` with a seccomp filter. `allow_all_unix_sockets` (`allowAllUnixSockets`) lifts it for every path; `allow_unix_sockets` (`allowUnixSockets`) is a path list the Linux sandbox ignores, and `loom plan verify` warns about it on Linux (`v2_lints/sandbox_capability.rs`). The sandbox binds the host root read-only and `connect()` needs no write access to the mount, so without more a flagged session reaches every socket it can see: a probe under Claude Code 2.1.299 got `/version` from `/var/run/docker.sock` (root for a `docker`-group user) and connected to the session D-Bus (`systemd-run --user` escapes) and gpg-agent (signing).

Loom therefore adds host socket locations to a flagged session's `denyRead` on Linux (`sandbox/host_sockets.rs`, collected by `orchestrator/terminal/native/session_settings/host_sockets.rs` for every session kind): `/run` (and `/var/run`, collapsed into it when it is a link), `XDG_RUNTIME_DIR`, the ssh agent socket and `GNUPGHOME`, the tmux socket directories, `/tmp/.X11-unix`, the daemon socket, and every socket a bounded scan finds under `/tmp` when the capsule is written. Paths are canonicalized; one that is, holds or lies inside the cwd or repo is never hidden (except a socket file, so the daemon socket stays hidden), and `/run` holding one is hidden around it, entry by entry. A path holding `*`, `?`, `[` or `]` is left out with a warning: sandbox-runtime 0.0.78 expands such `denyRead` entries as globs on Linux (`sandbox-manager.js` `resolveReadPathEntries`), so a literal name would hide nothing and a pattern could hide too much. A masked daemon socket answers `ECONNREFUSED` on a non-socket, which `daemon/rpc.rs` `refused_by_mask` and `daemon/server/core.rs` classify as unreachable, like a sandbox `EPERM`. `/tmp` itself cannot be hidden: the network proxy's bridge sockets live there.

Still reachable, and stated in the `loom run` refusal and the plan-verify note: every socket outside `/run`, `/var/run` and the `/tmp` scan (`/var/snap/lxd/...`, `/var/lib/...`, `$HOME` such as Dropbox's `command_socket`), a `/tmp` socket created after the capsule is written or nested deeper than `TMP_SCAN_DEPTH` (3), and the daemon socket after a daemon restart (the daemon re-creates `orchestrator.sock`, which detaches the mask from a session that outlives it). The `/tmp` scan and the `/run` walk stop at their entry caps with a warning.

## Terminal History Is Guarded by a Hook, Not a Boundary

Saved tmux scrollback (`.loom/work/terminal-history/<session-id>.terminal`) is in every capsule's OS-level `denyRead`, so no session's Bash can read it. The native file tools run outside the OS sandbox, and `loom-hooks/credential-guard.sh` rule (b) stands in for it in a loom session (`LOOM_SESSION_ID`, `LOOM_STAGE_ID` and `LOOM_WORK_DIR` all set; the operator's own sessions are exempt): it refuses a target inside the directory, a Grep or Glob rooted at the state root or its parent, and a Grep `glob` or a Glob absolute or `..` pattern on a search rooted above the state root, since an include glob overrides `.gitignore`. A worktree session is covered, because ripgrep does not follow its `.loom/work` link. A session in the main checkout (knowledge, merge, base-conflict) is not fully covered:

- A path-less Grep after a Bash `cd .loom/work` (or `cd .loom`) is judged against `CLAUDE_PROJECT_DIR` while ripgrep searches the current directory, and reads every transcript.
- A plain repo-root Grep relies on `.loom/work` being ignored, and the session can write `.ignore` or `.gitignore`.
- A Glob with a relative pattern such as `**/*.terminal` lists transcript names, not contents.

In a legacy `.work` checkout the parent rule refuses a plain repo-root Grep, because the state root's parent is the repo root. Rule (a) matches the token files only by the `.loom/work`/`.work` spellings, so with a state root symlinked to a directory of another name `completion-attestation.key` (and, from the main checkout, `admin.token`) is readable; this predates rule (b). `credential-guard.sh` is 575 lines, over the 400-line limit.

Fix direction: keep transcripts outside the repository (a per-user state directory), so no repo-rooted search can reach them and the hook needs only a path-prefix rule and a refusal of searches rooted at that directory's ancestors; check rule (a) against the canonical `$LOOM_WORK_DIR`; move rule (b) into a sourced file. Nothing prunes the directory; each file is capped at 4 MiB.

## Per-Command Package-Manager Caches Start Empty

`verify/criteria/confine_scratch.rs::apply_environment` points `UV_CACHE_DIR`, `BUN_INSTALL_CACHE_DIR` and `XDG_CACHE_HOME` at subdirectories of the per-command scratch. The read-only host caches that `read_roots` exposes are therefore never consulted.

With network denied:

- `uv sync`, `uv run` and resolution fail outright. This was collaterai `shared-contracts`' "no network and an empty UV_CACHE_DIR".
- An offline `bun install` fails; an already-populated `node_modules` still works.
- XDG-based caches (Go's build cache, for example) are cold on every command.

The Cargo home was the same defect and is now stable ([execution containment](../architecture/execution-containment.md#read-this-first-what-containment-means-in-loom)). These caches need their own design: a stable per-tree cache reset like the linked Cargo home, or the read-only host cache.

## Tests and Criteria That Pass in a Session Fail Under the Confined Runner

A session's `loom check` and `loom stage complete` relay a host job, and the daemon's host child runs each acceptance criterion through `verify/criteria/confine_host.rs::wrap`, outside the session sandbox ([Criteria Run Outside the Session Sandbox](#criteria-run-outside-the-session-sandbox)). The wrapper gives Linux criteria a private `/tmp` and `/var/tmp` (`TMPDIR=/tmp`), binds the resolver's target when it exists, mounts a linked worktree's git metadata read-only, and accepts `timeout_secs` on an extended criterion ([Execution Containment](../architecture/execution-containment.md#read-this-first-what-containment-means-in-loom)). What still differs from the session shell:

| Symptom confined | Cause | Direction |
| --- | --- | --- |
| a criterion fetching packages (`bunx`, `npx`, `uvx`, `pip install`, `cargo install`, `curl`) fails `ConnectionRefused` | criteria never get network, whatever the plan's `allowed_domains` say; those reach only the session | by design: install the tool from a `provision` entry and call the installed binary; `loom plan verify` reports it (`v2_lints/registry_domains.rs`) |
| an inner confined run cannot find `~/.cargo/bin` | an outer Linked Cargo home on Linux (only when the Cargo home overlaps the tree) is inherited by the inner wrap | open |
| `plan_environment_contracts::failed_provision_names_command_and_dir` passes without the command running | `contains("registry refused")` matches the command text embedded in the asserted prefix | assert on `reason[prefix.len()..]` |

Environmental, not defects: `cargo audit` fails in a session because the user's global git `url.ssh://git@github.com/.insteadOf` rewrites the advisory-database fetch to SSH and the sandbox denies `~/.ssh`; a link step once aborted with `relocation R_X86_64_32 out of range` in `.debug_info` of one test binary and linked on an identical second run (treat as a linker flake and report both runs).

**What to do when a criterion cannot pass confined.** Dispute it: one `loom stage dispute-criteria <stage> --criterion-index <n> [--criterion-index <n> ...]` filing names every criterion that fails only because of where completion runs, and the adjudicator replaces or deletes each. `loom stage block` is refused while the latest `loom stage complete` shows a failing criterion. The incident that forced this rule is [Completion Runner Blocks](../mistakes/completion-runner-blocks.md).

## Criteria Run Outside the Session Sandbox

A session cannot run confined criteria itself. Claude Code's Linux Bash sandbox starts with
`bwrap --disable-userns` (Claude Code 2.1.306+), so a `bwrap` nested inside it cannot create the user
namespace it needs. Loom emits no `sandbox.allowNestedUserNamespaces` key and reads no opt-out for it.
`loom check` and `loom stage complete` in a session relay a host job instead (`commands/verify_relay.rs`,
`commands/stage/complete_relay.rs`). The daemon runs `loom stage verify-host` on the host, and that child
runs criteria through the confined wrapper outside the session's sandbox
(see [Stage-to-Daemon Channels](../patterns/stage-daemon-channels.md)).

**Trade-off.** The session's own shell keeps `--disable-userns`, so a sandboxed command cannot create a user
namespace. Claude Code documents that a command which creates its own user namespace can bypass
`filesystem.denyWrite`, `denyRead` and credential-file denies in any folder it may write, so keeping that
route closed is the point. The cost falls on the project: in-session work that needs user namespaces
(its own `bwrap` or `unshare` calls, rootless containers) cannot run inside a stage session.

**Non-goal.** Projects whose in-session work needs user namespaces are not supported. Revisit when such work
can run as a host job, or when Claude Code's sandbox offers a nested-namespace setting that keeps the
denyWrite and credential denies in force. The history of this failure is in
[Sandbox Tooling and Network](../mistakes/sandbox-tooling-and-network.md).

## Host-Side Git Needs a Pin, Not Hardening

- **Hardening does not stop a planted filter.** `NO_HOOKS_ARGS` (`git/runner.rs`: `core.hooksPath=/dev/null`, `core.fsmonitor=false`) does not stop a planted gitdir's `filter.<x>.clean` from running under `git status` or `git diff`. Only `WorktreeGit::pinned` (`git/worktree/pinned.rs`) does. Any git that the host child or the daemon runs in a stage worktree must be pinned.
- **The main checkout cannot be pinned.** `WorktreeGit::pinned` accepts registered linked worktrees only. Git in a knowledge stage's checkout, in the merge flow and in every daemon git call there runs unpinned, while the session's allowlist grants writes under `<project>/.git`. A knowledge session can therefore plant a `.git/config` that daemon or host-child git reads. Fix direction: a pinned form for the main checkout in `git/worktree/pinned.rs`, plus a knowledge-session deny on `.git/config` and `hooks`.
- **Possible pin bypass through a gitlink, unreproduced.** A pinned `git status` or `git diff` recurses into any gitlink (mode 160000) index entry, reading the nested repository's own `.git` and config (`is_submodule_modified`). A stage that `git add`s an embedded repository with `filter.x.clean` and a stat-dirty file could make a pinned run in the host child (`tree_digest_in`, `committed_content`, the stage-commit checks) run that filter. Probe: add a gitlink to a stage worktree index, run `tree_digest_in` on the pinned git, and check for the filter's marker. If it fires, refuse an index holding a 160000 entry in the pinned run and `read_isolated`, or pass `--ignore-submodules=dirty`.

## Bubblewrap Hides a Secret Only If It Exists at Command Start

`bwrap` can mask a state-root secret only when the file exists when the command starts, because `--ro-bind /dev/null` needs a target. `completion-attestation.key` (`handoff/completion/attest.rs`) is created lazily, so the host child creates it with `attestation_key` before any criterion runs (`host_verify/child.rs`). A mask added for a file that a criterion's command has not yet created does not hold. Revisit when a new state-root secret is added: create it before the wrapper starts.
