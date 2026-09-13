# PLAN: Confine Loom Sessions Out of `.loom` Through a Hook-Written Inbox

Status: design approved (owner decisions below); implementation on branch `state-confinement` in the
worktree `.state-confinement/`.

Background: [Live State Pollution](../loom/knowledge/mistakes/live-state-pollution.md). A knowledge
stage's test run adopted the live `.loom/work`, wrote a sticky marker that overrode
`[terminal] backend`, marked a live session crashed and left fixtures in live state. The markers and
the workspace-discovery hole are fixed (`f1a7ef0f`, `3f50f422`). This plan closes the rest: no loom
session may write loom state or loom's control surfaces.

Notation: `R` is the repository root, `W` is `R/.loom/work`, `T` is a stage worktree
`R/.worktrees/<stage-id>`. A **checkout-rooted** session runs with cwd `R`; a **worktree-rooted**
session runs at a `T`.

## Owner Decisions (Binding)

1. No loom session of any kind (Stage, Knowledge, Merge, Adjudication, and their subagents and the
   codex forwarder) may write under `R/.loom` through any agent-invoked process.
2. The configuration chain is the only authority for configurable behavior. No state file overrides
   a setting (done in `3f50f422`).
3. Session requests travel through a **hook-written inbox**: sandboxed processes never write loom
   state; a loom hook running outside the sandbox records the request under its own session identity,
   and the daemon applies it.
4. Agents must not be able to modify or delete hooks, hook configuration, loom configuration, or the
   executables hooks run.
5. `loom run` refuses to start when config disables the sandbox or allows unsandboxed escape.
6. `loom repair --fix` strips loom-written keys from `R/.claude/settings.local.json` and stops writing
   a sandbox block there; `loom run` warns while any remain.
7. Implement on branch `state-confinement` in a separate worktree (`.state-confinement/`, excluded
   locally); merge into main when the owner chooses. Reinstall binary and hooks together only after
   the running plan finishes.
8. **Permission propagation stays, with a new destination.** Permissions approved in one session are
   kept in a loom-owned list and rendered into every later session's capsule. The operator's own
   `R/.claude/settings.local.json` no longer receives them. A rule that grants writes to a control
   surface (`.loom`, `.claude`, `.worktrees`, hook directories, `~/.loom`, the scratch root) is never
   propagated.
9. **Merge gate on control paths.** The daemon refuses to auto-merge a branch whose diff touches
   `.claude/**`, `.mcp.json`, `.loom/**`, or the git hooks directory (`core.hooksPath` when it resolves
   inside the repository; `loom/.githooks/**` here), and moves the stage to `NeedsHumanReview` with the
   list of paths. Sandbox rules stop Bash and the file tools from writing these paths; the gate closes
   the remaining route, a commit that carries the file through git's object store.
10. **Loom's own git calls run with hooks disabled** (`-c core.hooksPath=/dev/null`). `core.hooksPath`
    is `loom/.githooks`, a tracked directory, and nothing in loom's git runner disables hooks today, so
    any daemon git command that runs a hook inside a stage worktree would execute that stage's hook
    unsandboxed.
11. **Toolchain install directories lose their grants.** `~/.rustup/toolchains` and `~/.local/share/uv`
    are removed from session grants; the operator preinstalls toolchains. Private per-session package
    caches are a follow-up plan; shared caches stay writable for now (accepted risk).

## 1. Design Summary

1. Every request goes **by reference**: the CLI writes a ticket into a per-session scratch directory
   and prints one line naming the ticket id and SHA-256. The line carries no path. A request line with
   no matching ticket in this session's scratch directory does nothing, which makes fixture output,
   docs and echoed transcripts inert.
2. A new hook `loom-hooks/loom-relay.sh` (PostToolUse, matcher `Bash`) runs a pure-bash fast path,
   checks the command's shell tokens, then hands off to `"$LOOM_BIN" hook relay`, which proves session
   identity by process ancestry, verifies the ticket and writes the inbox entry.
3. Completion stays on the existing marker plus socket broker; Knowledge sessions join it.
4. The daemon drains `W/inbox/<session-id>/` through its existing handlers under a session-kind ×
   request-kind matrix. A refusal is a ledger outcome, never an `Err`.
5. Every session kind launches from a generated capsule `W/capsules/<session-id>.settings.json`.
   Spawns stop writing `R/.claude/settings.local.json` and `T/.claude/settings.local.json`.
6. Hooks run only executables sessions cannot write: the daemon verifies and exports `LOOM_BIN` and a
   filtered `LOOM_HOOK_PATH`; hook commands are registered as `/bin/bash <installed script>`.
7. Location-based deny rules key on where a session runs, not its kind. Kind decides hooks and the
   matrix. The judge starts inside the disputed worktree when it exists, because criteria such as
   `cargo test` write there (`orchestrator/adjudication/prompt.rs:127-137`).
8. Enforcement lands last.

## 2. Verified Facts

- `Edit(...)` allow rules become OS write grants: `W/handoffs` is mounted read-write from
  `Edit(.loom/work/handoffs/**)` alone. Removing that allow removes the Bash write too, so it waits
  until the relay carries handoffs. `Edit` deny rules reach the OS sandbox as well; any wildcard other
  than a trailing `/**` risks the recursive expansion recorded in `mistakes/sandbox-and-settings.md`
  (2026-09-04).
- The wrapper already exports `LOOM_SESSION_TYPE` (`orchestrator/terminal/native/wrapper.rs:367`).
- Stage sessions already launch with `--settings T/.claude/settings.local.json --setting-sources
  user,project --strict-mcp-config`. Knowledge and Merge get `R/.claude/settings.local.json` from
  `resolve_settings_file` (`session_settings.rs:204-228`), which is why spawns write a sandbox block at
  the repo root.
- Every registered hook command points at installed copies (`~/.claude/hooks/loom/*.sh`,
  `~/.codex/hooks/loom/*.sh`); none point into the checkout.
- Hooks inherit the daemon's PATH and call `loom` through it: `ask-user-pre.sh:43`,
  `ask-user-post.sh:43`, `session-end.sh:79`, `pre-compact.sh:44,81,109`,
  `user-prompt-context.sh:79-83`, `_read_discipline.sh:193`, `codex-apply-patch.sh:99-103`. Only
  `loom-control-complete.sh:25-45` resolves a fixed binary.
- Package grants reach executables on PATH: `~/.local/share/uv` and `~/.rustup/toolchains` are in
  `PACKAGE_MANAGER_CACHE_WRITE_PATHS` (`sandbox/package_caches.rs:34-75`); `~/.local/bin/ruff` and
  friends are symlinks into `~/.local/share/uv/tools/`; `~/.cargo/bin/cargo` is rustup, which runs
  `~/.rustup/toolchains/*/bin/*`.
- The codex lane grants all of `~/.codex` (`loom/src/codex.rs:54-55`), hooks and config included.
- Claude Code's built-in Bash protection covers only the current project: a checkout-rooted session
  can write any worktree's `.claude/settings.json`, which that worktree's session loads.
- Knowledge completion via the broker needs two changes: `session_owns_stage` requires
  `SessionType::Stage` (`daemon/server/self_service.rs:118-124`) and `handle_complete_stage` never sets
  `merged` (`daemon/server/control_complete.rs:36-44`). `try_auto_merge` already skips `merged = true`
  (`orchestrator/core/merge_handler.rs:498-501`).
- `BaseConflict` has no spawner.
- Forwarded codex tasks may run read-only loom commands (`codex-forward.sh:54-61`).

## 3. Threat Model

In scope: a sandboxed agent (main, subagent, teammate, codex forwarder) that claims another session's
identity, emits requests it did not mean to send (fixtures, echoed transcripts, tests spawning the
binary with inherited env), writes loom state directly, or changes code or config that runs outside
the sandbox. An agent acting as itself may make any request its kind allows. Out of scope: see Risks.

## 4. Request Protocol

### 4.1 Ticket

The CLI writes `$LOOM_SCRATCH_DIR/<id>.req` via `.<id>.tmp` plus rename in the same directory:

```json
{"v":1,"id":"4f1c9e0a7b2d4c6e8f00112233445566","kind":"memory","created_at":"2026-09-13T08:00:00Z","payload":{}}
```

No stage id and no session id. Attribution comes from the hook's environment and the inbox directory
name. Parse with `deny_unknown_fields`; require `v == 1`.

### 4.2 Line Grammar

```text
LOOM_RELAY_V1 kind=<kind> id=<32 lowercase hex> sha256=<64 lowercase hex> bytes=<1-7 digits>
```

- Whole line only: after stripping one trailing `\r`, it starts at column 0, has nothing after it, is
  ASCII and at most 160 bytes.
- `kind` is one of `memory`, `block`, `dispute`, `handoff`, `merge-resolved`, `verdict`, `telemetry`.
- `id` is 128 random bits, the same shape as the completion nonce
  (`daemon/server/control_complete.rs:54-56`). `sha256` and `bytes` guard integrity, not
  authentication.
- The prefix uses only `[A-Z0-9_]`, so JSON never escapes it and a fixed-string check on the raw hook
  input finds it.
- The CLI writes its human text to stderr first, then the line as the last stdout line, then flushes.

### 4.3 Payloads and Bounds

| Kind | Payload |
| --- | --- |
| `memory` | `MemoryEntry` |
| `block` | `StageRequest::Block` |
| `dispute` | `StageRequest::Dispute`, failure output truncated to 4 KiB (`dispute_criteria.rs:20`) |
| `handoff` | `{trigger, message}`; the daemon builds the document from the stage, the memory journal, the heartbeat context reading and `git status` of the session's checkout |
| `verdict` | `{dispute_id, verdict}`, validated by `verdict::parse_and_validate` in the CLI and again in the daemon |
| `merge-resolved` | `{}` |
| `telemetry` | the `ContextPulled` event only |

- Ticket size is capped at `daemon::MAX_REQUEST_BYTES` (`fs/stage_request/spool.rs:76-82`). The CLI
  refuses to write a 33rd unconsumed ticket.
- Scratch root: on Linux `$XDG_RUNTIME_DIR/loom/scratch/` when that variable is set, absolute, owned by
  the operator and mode 0700, else `~/.cache/loom/scratch/`; on macOS `~/Library/Caches/loom/scratch/`.
  Never `/tmp` (every sandbox on the host can write `/tmp/claude-<uid>`), never inside `R`, never under
  another grant.
- The daemon creates the root (0700, ownership checked) and `<root>/<session-id>/` (0700) before spawn,
  since the sandbox binds only grant paths that exist at session start. Each capsule grants only its
  own directory, via `allowWrite` and `Edit(//<dir>/**)`.

## 5. Per-Writer Matrix

Attribution is always the session record's `stage_id`; any stage argument must equal `LOOM_STAGE_ID`.

| Writer | Kind | Stage | Knowledge | Merge | Adjudication | Daemon handler |
| --- | --- | --- | --- | --- | --- | --- |
| `loom memory note/decision/change/question` | `memory` | apply | apply | apply | refuse (a judge must not write the disputed stage's journal) | `fs::memory::append_entry` after `validate_spooled_entry` |
| `loom stage block <own>` | `block` | apply | apply | refuse | refuse | `handle_block_stage` |
| `loom stage dispute-criteria <own>` | `dispute` | apply | apply | refuse | refuse | `handle_dispute_criteria` |
| `loom handoff` | `handoff` | document; with `--trigger ceiling` also NeedsHandoff when Executing and owned | same as Stage | document only | refuse | session content builder, `try_mark_needs_handoff` |
| `loom stage merge <own> --resolved` | `merge-resolved` | refuse | refuse | apply | refuse | `finalize_merge_resolution` (ancestry proof), then worktree cleanup |
| `loom stage adjudicate --stage <own> --dispute <n>` | `verdict` | refuse | refuse | refuse | apply | verdict recording moved daemon-side, all four guards (`adjudicate.rs:16-27`) |
| `loom knowledge context` | `telemetry` | apply | apply | apply | apply | `telemetry::append_record` |
| `loom stage complete <own>` | none | socket broker | socket broker (new) | n/a | n/a | `handle_complete_stage` |
| `loom worktree remove` | none | n/a | n/a | prints that the daemon cleans up after `merge-resolved` | n/a | n/a |

`BaseConflict` follows the Merge column except `merge-resolved` (refuse). The hook refuses the control
kinds (`block`, `dispute`, `handoff`, `merge-resolved`, `verdict`) from subagents.

## 6. CLI Behavior

Relay mode is chosen from the environment, never from a write error:

- **Relay:** `LOOM_SESSION_ID` and `LOOM_SCRATCH_DIR` set, and neither `LOOM_HOOK_CONTEXT=1` nor
  `LOOM_CONTROL_BROKER=1`. Hooks that run `loom` (`pre-compact.sh`, `session-end.sh`, `ask-user-*.sh`)
  export `LOOM_HOOK_CONTEXT=1` and keep writing directly, since they run outside the sandbox.
- **Legacy:** `LOOM_SCRATCH_DIR` absent (sessions spawned before the upgrade). Unchanged for one
  release.
- **Operator:** `LOOM_SESSION_ID` absent. Direct write or socket, as today.

Guards before any ticket is written:

1. Under `cfg(test)` the environment-derived target is never used; tests inject one.
2. The scratch directory is a real directory, not a symlink, whose last component equals
   `LOOM_SESSION_ID`.
3. The canonical cwd is inside this session's checkout: canonical `LOOM_WORKTREE_PATH` for Stage, the
   main project root of `LOOM_WORK_DIR` otherwise. A test spawning the binary from a tempdir gets a
   refusal.
4. Any stage argument equals `LOOM_STAGE_ID` (extends `reject_stage_forgery`,
   `commands/memory/handlers/record.rs:76-87`).
5. The matrix row for `LOOM_SESSION_TYPE` allows the kind.
6. Nothing is opened for writing under `W`, including `get_or_create_work_dir` (`record.rs:26`).

Output is stderr text, then the stdout line, exit 0:

```text
Request 4f1c9e0a (memory note) is PENDING RELAY. Nothing is recorded yet.
The loom relay hook confirms receipt right after this command; the daemon applies it within a few seconds.
Keep this command's stdout unfiltered, unredirected and in the foreground: the relay reads the LOOM_RELAY_V1 line from it.
If no "LOOM relay: received 4f1c9e0a" message follows, the relay hook is not installed. Stop and report it; do not retry.
Check later with: loom request status 4f1c9e0a
```

`block` and `handoff --trigger ceiling` add: "End your turn after the confirmation."

## 7. Relay Hook

Registered in every capsule: PostToolUse, matcher `Bash`, command `/bin/bash <hooks_dir>/loom-relay.sh`.
Not added to the global table or to codex hooks.

Bash part:

1. First statement `PATH="${LOOM_HOOK_PATH:-$PATH}"`; no external command before it. Source
   `_common.sh` via `${BASH_SOURCE[0]%/*}`.
2. Exit 0 silently unless `LOOM_SESSION_ID` and `LOOM_SCRATCH_DIR` are set.
3. Read stdin with the 1-second timeout helper.
4. Fast path, pure bash: exit 0 unless the raw input contains `LOOM_RELAY_V1` followed by a space,
   `persistedOutputPath`, or `Full output saved to:` followed by a space. No jq, no sourcing on this
   path.
5. Source `_common.sh`, require jq; if missing, emit additionalContext that the relay is unavailable.
6. One jq call extracts `tool_name` (must be `Bash`), command, `agent_type`, `transcript_path`,
   `tool_use_id`.
7. Forwarder check: if `agent_type` is `loom-codex-forwarder` or `codex:codex-rescue`, or the
   transcript sentinel from `codex-forward-guard.sh:204-215` matches, relay nothing and say that
   forwarded output is never relayed.
8. Allowed kinds from the command: `strip_embedded_content`, `loom_tokenize_command`, walk command
   positions (skip `VAR=` prefixes, see through wrappers and `sh -c` as `loom_tokens_invoke` does). For
   each invocation whose argv[0] basename is exactly `loom`, map the subcommand to its kind. A tokenizer
   failure yields an empty set and a message to rerun the loom command alone.
9. Drop control kinds when `agent_type` is non-empty.
10. Run `"$LOOM_BIN" hook relay --allowed-kinds <csv>` with the payload on stdin and
    `LOOM_HOOK_CONTEXT=1`; print its JSON; exit 0.

Rust helper `loom hook relay`:

1. Collect text from `tool_response.{stdout,stderr,output}` and the older `tool_result.*` shape. A
   persisted-output path is accepted only if absolute, canonicalizing under canonical
   `$HOME/.claude/projects/` with a `tool-results` component, opened `O_NOFOLLOW` as a regular file of
   at most 64 MiB.
2. Extract whole grammar lines, dedupe by id, at most 16 per call.
3. Identity: `peer_identity::caller_is_inside_session(W, LOOM_SESSION_ID, own pid)`
   (`daemon/server/peer_identity.rs:157`); the session record must be Running and name
   `LOOM_STAGE_ID`. Teammates outside the lead's process tree fail and are told to send notes to the
   lead.
4. `LOOM_SCRATCH_DIR` must equal `<root>/<LOOM_SESSION_ID>`, be a directory, not a symlink, owned by
   the operator, mode 0700.
5. A line whose kind is not allowed is skipped (reported only if a ticket exists). A line with no
   ticket is dropped silently.
6. Open `<scratch>/<id>.req` `O_NOFOLLOW`: regular file, one link, operator-owned, size equal to
   `bytes` and within the cap, SHA-256 matching, `v`/`id`/`kind` matching the line.
7. If `W/inbox/<sid>/ledger.jsonl` already records the id, unlink the ticket and report it.
8. Refuse at 128 pending entries (the daemon is not draining).
9. Write `W/inbox/<sid>/.tmp/<id>.<pid>` with `O_EXCL`, mode 0600, fsync, `link()` to `<id>.json`
   (`EEXIST` means already relayed), unlink the tmp file, fsync the directory. The writer lives in
   `fs/inbox`.
10. Unlink the ticket.
11. Reply with one additionalContext, e.g. `LOOM relay: received memory 4f1c9e0a`, plus any refusals.

Every path exits 0 with an explanation once a ticketed line was seen. Output of a backgrounded command
is read later by another tool and never relayed; the stale-ticket check reports it. Only step 4 runs on
an ordinary Bash call. `loom-control-complete.sh` is otherwise unchanged.

## 8. Inbox and Drain

```text
W/inbox/                      0700, created by the daemon
  <session-id>/               0700, created at spawn
    .tmp/                     relay staging (same filesystem, so link() is atomic)
    <request-id>.json         one relayed request
    ledger.jsonl              {"id","kind","state|outcome","reason","at"}
<scratch-root>/<session-id>/  0700, granted to that one session
    <request-id>.req          CLI ticket, consumed by the relay
    verdict-<n>.json          the judge's draft
```

```json
{"v":1,"id":"...","kind":"memory","relayed_at":"...","session_id":"<from hook env>","stage_id":"<from hook env>","agent":"main|subagent","tool_use_id":"...","payload":{}}
```

Drain on each poll tick, next to `drain_stage_spools` (`orchestrator/core/run.rs`):

1. Each directory under `W/inbox/` must pass `validate_id` and open `O_NOFOLLOW`; otherwise log once
   and skip.
2. Load `W/sessions/<sid>.md` via `safe_read`. No record: leave it, log once, remove after 24 hours.
3. List `*.json` regular files, not symlinks, one link, within the cap; sort by `relayed_at`, then id.
4. Validate: parses under `deny_unknown_fields` with `v == 1`, `session_id` equals the directory name,
   `stage_id` equals the record's stage. Otherwise refuse as malformed or mismatched.
5. Id already in the ledger: delete the entry.
6. Append `{"id","state":"applying"}`, apply through the matrix and handler, append the outcome
   (`applied` or `refused` with reason), delete the entry. After a restart, `applying` with no outcome
   is recorded `unknown-after-restart` and never re-applied (at-most-once).
7. A handler refusal is an outcome, not an `Err`. Only I/O failure stops this session's pass until the
   next tick.
8. Retirement (record not Running, process confirmed gone): drain once more, delete the scratch
   directory, remaining entries and `.tmp/`, keep `ledger.jsonl`, delete the capsule (generalizing
   `judge_close.rs:82`).
9. Stale tickets: a Running session's non-telemetry ticket older than 60 s means the relay is not
   working; warn once and show `relay stalled (N)` in `loom status`. Telemetry tickets older than
   10 minutes are deleted silently.

`loom clean --sessions` removes inbox and scratch directories of sessions that are not Running;
`--state` first removes the scratch directories of every session in `W/sessions/` (the scratch root is
shared across the operator's repositories), then deletes `W`.

## 9. Knowledge Completion

1. `loom-control-complete.sh` admits a loom worktree (today, `:159`) or `LOOM_SESSION_TYPE=knowledge`.
   Pinned command: `"$LOOM_BIN" stage complete $LOOM_STAGE_ID`.
2. `sandbox_control_session` (`commands/stage/control_session.rs:68-111`) gets a Knowledge branch when
   `LOOM_SESSION_TYPE=knowledge`, the stage is `StageType::Knowledge`, `stage.session` equals
   `LOOM_SESSION_ID`, and cwd is inside the main project root: acceptance runs in `R`, the marker prints,
   nothing changes on disk. Sessions without `LOOM_SESSION_TYPE` keep the in-process path.
3. Daemon: `session_owns_stage` takes a set of allowed session types (`CompleteStage` allows Stage and
   Knowledge; block and dispute over the socket stay Stage-only). `handle_complete_stage` sets
   `merged = true` for a Knowledge stage before `try_complete`. Confirm `cleanup_already_merged` does
   nothing without a worktree. Dependents start via `sync_graph_with_stage_files`.
4. `complete_knowledge_stage` stays for operators.

## 10. Capsules and Deny Rules

`prepare_session_launch` (`launch.rs:132-265`) calls one builder for all kinds; it writes the capsule
atomically (0600, directory 0700) and passes `--settings <absolute path> --setting-sources user,project
--strict-mcp-config`.

Stop writing: `R/.claude/settings.local.json` on knowledge spawn (`spawn_setup.rs:111-190`),
`T/.claude/settings.local.json` (`stage_executor.rs:375,409`), and the copy of `R`'s local settings into
worktrees (`git/worktree/settings.rs:138-149`). `T/.claude/settings.json` is still generated.

`spawn_adjudication_session` starts the judge in the disputed stage's worktree root when it exists, in
`R` otherwise.

Capsule contents:

- `sandbox`: from `merge_config(plan_sandbox, stage.sandbox, ...)`; must have `enabled: true` and
  `allowUnsandboxedCommands: false`, else the spawn is refused.
  - `allowWrite`: plan `allow_write`, narrowed package caches, codex paths when licensed,
    `<scratch-root>/<sid>`.
  - `denyWrite`: `/R/.loom` (and `/R/.work` on a legacy layout), the section 11 paths;
    worktree-rooted add `/T/.loom` and `/T/.claude`; checkout-rooted add `/R/.worktrees` and
    `/R/.claude`.
  - `denyRead`: unchanged.
- `permissions.allow`: narrow reads (config.toml, signals, handoffs, disputes, memory, plans) in
  relative and resolved form, plan `allow_write` `Edit` rules, `Edit(//<scratch>/<sid>/**)`. No handoff
  `Edit` rule in any spelling.
- `permissions.deny`: `Edit(//R/.loom/**)`, `Edit(.loom/**)` (plus the `.work` forms on a legacy
  layout), the section 11 `Edit` rules; worktree-rooted `Edit(//T/.claude/**)`, `Edit(.claude/**)`;
  checkout-rooted `Edit(//R/.worktrees/**)`, `Edit(.worktrees/**)`, `Edit(//R/.claude/**)`,
  `Edit(.claude/**)`.
- Rule shape, enforced by a test: every entry is a literal path or a literal directory followed by
  `/**`, prefixed `~/` or `//`; no other `*`; never a `Read(` deny. Sandbox lists hold literal paths.
- `hooks`: the global guard set; all seven `HookEvent`s for Stage and Knowledge; `post-tool-use.sh`
  only for Merge, BaseConflict and Adjudication (`session_settings.rs:66-69`); `loom-relay.sh` for every
  kind; `loom-control-complete.sh` (Pre and Post) for Stage and Knowledge. Every command is
  `/bin/bash <verified hooks_dir>/<script>`.
- `enabledPlugins` and `extraKnownMarketplaces` copied from `R/.claude/settings.local.json` when the
  codex lane is licensed; no `env` block; `worktree.bgIsolation: "none"`; `defaultMode`.

New wrapper exports: `LOOM_SCRATCH_DIR`; `LOOM_BIN` (the daemon's canonical binary, verified);
`LOOM_HOOK_PATH` (the daemon's PATH, keeping only absolute existing directories that canonicalize
outside every session-writable root).

Remove the handoff grant everywhere: `sandbox/settings.rs:179,304-313`,
`fs/permissions/constants.rs:211,213,241,243`, and have `ensure_loom_permissions` delete it from an
existing `R/.claude/settings.json` (`fs/permissions/settings.rs:307` already removes inert grants).

Fold-back: `fs/permissions/sync.rs:188-201` drops any rule naming `.loom/`, `.work/`, the resolved state
root, `.worktrees/`, `.claude/` or the scratch root.

## 11. Control Surfaces

Every session kind gets both layers: `sandbox.filesystem.denyWrite` and a `permissions.deny` `Edit`
rule.

| Surface | Legitimate writer | Protection today | Rule |
| --- | --- | --- | --- |
| `~/.claude/hooks/loom/**` (every registered hook) | `install.sh`, `loom install-assets` | Bash: Claude default deny; native tools: none | `~/.claude/hooks`; `Edit(~/.claude/hooks/**)` |
| `~/.codex/hooks/**`, `~/.codex/hooks.json`, `~/.codex/config.toml` | install-assets, operator | none when the codex lane is licensed | the three paths denied inside the `~/.codex` grant, plus `Edit` rules |
| `~/.claude/settings.json`, `~/.claude.json` | operator, Claude Code | Claude default deny | both, plus `Edit` rules |
| `R/.claude/**` | `loom init`, `loom repair` | read-only mount for the current project only | checkout-rooted: `/R/.claude`, `Edit(//R/.claude/**)`, `Edit(.claude/**)` |
| `R/.worktrees/<id>/.claude/**` | daemon | writable by every checkout-rooted session | checkout-rooted `/R/.worktrees`; worktree-rooted `/T/.claude` and `Edit` rules |
| `W/capsules/*`, `W/wrappers/*.sh`, `W/config.toml` | daemon | checkout-rooted sessions write all of `W` | covered by the `.loom` deny |
| `~/.loom/**` | operator | outside every grant | `~/.loom`; `Edit(~/.loom/**)` |
| `LOOM_BIN`, hook helpers via `LOOM_HOOK_PATH` | operator | install dirs reachable through granted `~/.local/share/uv` and `~/.rustup/toolchains` | drop those two grants; narrow pnpm to `~/.local/share/pnpm/store` and `~/Library/pnpm/store`; hooks use `LOOM_BIN`, `LOOM_HOOK_PATH`, `/bin/bash`; deny operator-owned `LOOM_HOOK_PATH` dirs and `dirname(LOOM_BIN)`; refuse when any resolves under a writable root |
| `R/.git/hooks`, `R/.git/config` | operator | Claude default read-only | both; `Edit(//R/.git/hooks/**)`, `Edit(//R/.git/config)` |
| `~/.claude/projects/**` (relay trusts persisted output there) | Claude Code | Claude default deny | `~/.claude/projects`; `Edit(~/.claude/projects/**)` |
| `~/.claude/plugins/**` | Claude Code | Claude default deny; codex lane grants `data/codex-openai-codex` | `~/.claude/plugins`; with the codex lane, deny each existing entry except `data` and each `data/*` entry except the codex directory, as literals at spawn; plus `Edit` rules |
| `~/.claude/{agents,skills,commands,loom-skill-catalog}` | install-assets | Claude default deny for the first three | each, plus `Edit` rules |
| `R/loom-hooks/**` (loom repo only) | stages editing loom | writable by design | none; `loom run` refuses if any registration resolves under `R` |

Native `Edit`/`Write` do not run under bwrap; the explicit `Edit` denies decide them (live check 9).
User hooks (e.g. `~/.bun/bin/ccstatusline`) run unsandboxed in every session: `loom run` refuses when a
user or project hook's executable canonicalizes under a writable root and warns when it has more than
one hard link.

## 12. `loom run` Refusals and `repair --fix`

`loom run` (both `prepare_background_run`, `commands/run/mod.rs:81-116`, and foreground) refuses when:

1. Any stage's merged sandbox config has `enabled = false` or `allow_unsandboxed_escape = true`.
2. An installed hook script is missing or differs from the embedded copy (`drift.rs:197-205`),
   `loom-relay.sh` included.
3. `LOOM_BIN` or the hooks directory (including a `LOOM_HOOKS_DIR` override) canonicalizes under a
   session-writable root.
4. Any of `/bin/bash`, `jq`, `git`, `timeout`/`gtimeout`, `rg`, `mkdir`, `mv`, `cat`, `sed`, `awk`,
   `tr`, `date`, `stat`, `dd`, `head`, `tail`, resolved through the filtered PATH, canonicalizes under
   one.
5. A user or project hook's executable canonicalizes under one.

It warns while `R/.claude/settings.local.json` still carries loom-written keys. One function,
`sandbox/control_surfaces.rs::session_writable_roots`, supplies the roots: `R`, every stage's
`allowWrite`, package and codex grants, the scratch root, `/tmp`, `$TMPDIR`. The daemon repeats checks
1-4 at each spawn and blocks the stage with `SandboxSetupFailure`.

`repair --fix` removes from `R/.claude/settings.local.json`: the `sandbox` block; state-directory read
and handoff rules in every spelling and `Read(//R/doc/plans/**)`; each `Edit(x)` that
`grant_paths::edit_rule` maps from an entry of the removed `allowWrite`; the seven session `HookEvent`
registrations; `env.LOOM_WORK_DIR`. It keeps the global guard hooks,
`CLAUDE_CODE_EXPERIMENTAL_AGENT_TEAMS`, `worktree.bgIsolation` and `defaultMode`. `fix_sandbox_settings`
stops writing a sandbox block (`commands/repair/sandbox_settings.rs:109-121,167-180`).

## 13. Missing Hooks and Legacy Spools

- The CLI text says nothing is applied yet and names the confirmation to expect; the daemon reports
  stale tickets; `loom run` and every spawn refuse on hook drift; capsule registrations cannot drift.
- Worktree stages move to the inbox too. The new binary writes the old spools only in legacy mode.
  `drain_stage_spools` (`spool_drain.rs:48-83`) stays one release; the next release removes spool
  writers, the drain, `SPOOL_RELPATHS` (`git/worktree/settings.rs:21-25`) and `handoff_watch.rs` once no
  Running session lacks a scratch directory. The `/R/.worktrees` deny closes forgery by checkout-rooted
  sessions for new spawns.

## 14. What the Deny Breaks

1. `loom map` and `loom knowledge context` persist to `R/.loom/cache/context-v1`; checkout-rooted
   sessions lose that write as worktree sessions already have, so both must fall back to in-memory
   results (test under srt).
2. `loom context record-edit` from `codex-apply-patch.sh` is already denied in worktrees and
   best-effort.
3. Block and dispute from the checkout fail today (`fs/stage_request/spool.rs:51-60`); the relay fixes
   them.
4. Knowledge-context telemetry from the checkout is dropped today (`telemetry/mod.rs:114-119`); now
   relayed.
5. `loom worktree remove` takes `MergeLock` in `W` (`worktree_cmd.rs:76-112`); the daemon cleans up
   after `merge-resolved`, and the merge signal drops that step (`signals/merge.rs:171-174,188-191,203-206`).
6. `cleanup_session_resources` (`commands/stage/session.rs:20-42`) moves to the daemon.
7. The judge's draft moves from `W/disputes/.../verdict.json` (`adjudication/session.rs:142-144`,
   `prompt.rs:191-229`) to `$LOOM_SCRATCH_DIR/verdict-<n>.json`.
8. Relay mode must not call `get_or_create_work_dir()`.
9. Confirm `fs/locking.rs::locked_read` (`:167-179`), `loom status`, `loom subagents watch` and
   `commands/stage/complete_cache.rs` write nothing in `W` (test under srt).
10. Hooks that run `loom` export `LOOM_HOOK_CONTEXT=1` and use `LOOM_BIN`.

## 15. Implementation Order

0. **Contract (I1):** `loom/src/relay/protocol.rs` — kinds, grammar, ticket and entry schemas, the
   matrix as data, scratch-path rules.
1. **Parallel, no enforcement:** I1 CLI emitters; I2 drain, handlers, Knowledge completion; I3 hook and
   helper; I4 capsules for every kind, wrapper exports, scratch lifecycle, relay registration (grants
   unchanged except the scratch grant).
2. **Gate:** relay e2e and both suites pass.
3. **Enforcement (I4, plus I2's merge gate if the owner approves):** denies, handoff-grant removal,
   grant narrowing, `loom run` refusals, `repair` changes, no more local-settings writes.
4. **Rollout:** srt e2e and knowledge docs; after the running plan finishes, reinstall binary and hooks
   together, then the live checklist.

Ledger rule: no implementer edits `loom/maintainability-baseline.txt`. New files stay within the size
limits; an implementer that shrinks a ledgered file reports the new count, and the orchestrator updates
the ledger.

Additions from owner decisions 8-11:

- **I2, phase 1:** every git command loom runs passes `-c core.hooksPath=/dev/null` (`git/runner.rs`
  and any direct `Command::new("git")` in production code). Acceptance: a daemon merge and a daemon
  commit in a repository whose hooks directory holds a hook that writes a marker file leave no marker.
- **I2, phase 3:** the merge gate in `merge_handler.rs` (decision 9). Acceptance: a branch touching
  `.claude/**`, `.mcp.json`, `.loom/**` or the in-repo hooks directory is not merged and its stage
  moves to `NeedsHumanReview` naming the paths; a branch touching none merges as before.
- **I4, phase 1:** the loom-owned approved-permissions list (new `fs/permissions/approved.rs`, stored
  under `W`), filled from each finished session's recorded approvals through the control-surface
  filter and rendered into every capsule's `permissions.allow`. First determine where Claude Code
  records an approval when a session runs from a capsule (today's fold-back source is the worktree's
  `.claude/settings.local.json`). Acceptance: an approval recorded in one session appears in the next
  session's capsule; a rule granting writes to a control surface is dropped.
- **I4, phase 3:** `fs/permissions/sync.rs` stops writing `R/.claude/settings.local.json`;
  `sandbox/package_caches.rs` drops `~/.rustup/toolchains` and `~/.local/share/uv` (decision 11).
  Acceptance: after a stage finishes, `R/.claude/settings.local.json` is byte-identical.
- Live checklist additions: an approval granted in one stage reaches the next stage's capsule while
  `R/.claude/settings.local.json` stays unchanged; a stage that edits `loom/.githooks/pre-commit` is
  held for review.

Ownership adjustments (2026-09-13, before phase 1):

- **Phase 0 done:** the contract lives in `loom/src/relay/` (`kind`, `line`, `ticket`, `payload`,
  `inbox`, `matrix`, `scratch`), and loom's git runner plus every direct git spawn pass
  `NO_HOOKS_ARGS` (decision 10).
- **Phase 0b, one agent, before phase 1:** `fs/inbox/**` (layout, atomic entry writer, ledger append
  and read, dedupe and pending queries) and `loom request status` end to end (`commands/request/*`,
  its CLI wiring). The relay helper (I3) and the drain (I2) both import `fs/inbox`; building it first
  removes the cross-dependency.
- **I1** changes no `cli/*` file and no longer owns `commands/request/*` or
  `handoff/session_content.rs`.
- **I2** owns `handoff/session_content.rs` (the daemon builds the handoff document) and uses
  `fs/inbox` from phase 0b.
- **I3** owns the `loom hook relay` CLI wiring: the `HookCommands::Relay` variant in
  `cli/types_ops.rs`, its arm in `cli/dispatch.rs`, and the `commands/hook/mod.rs` declaration. No
  other phase-1 agent edits those files. I3 also owns `fs/permissions/constants.rs` in phase 1, to add
  `loom-relay.sh` to the embedded `LOOM_HOOKS` inventory; I4's phase-3 edits to that file come later.
- **Phase 0c, one agent, before I1 and I2:** `relay/emit.rs`, the shared CLI side of the protocol
  (mode detection from an injected env snapshot, the section 6 guards, atomic ticket write, the
  stderr text and the last-line stdout relay line). Every CLI writer calls it.
- **Verdict path, end to end, is I2's:** `commands/stage/adjudicate.rs` (relay-mode emit plus the legacy
  and operator path), a new `orchestrator/adjudication/record.rs` holding the recording logic moved
  out of `adjudicate.rs` unchanged, `orchestrator/adjudication/{mod,prompt,session}.rs`, and the drain's
  verdict handler. I1 owns the other CLI writers and does not touch the adjudication files.

Findings from I4 phase 1 (2026-09-13):

- **Where Claude Code records approvals.** Read from the installed 2.1.269 bundle: a "don't ask again"
  approval is written to `localSettings`, meaning `<root>/.claude/settings.local.json`. The root is the
  canonical git root when that root is operator-owned and not `$HOME`, and the cwd otherwise. The
  write happens even when the session was launched with `--settings <capsule> --setting-sources
  user,project`. So worktree sessions normally write approvals into `R/.claude/settings.local.json`,
  and loom cannot prevent that. Phase-3 acceptance changes from "R local byte-identical after a
  stage" to "loom never writes R local". The fold-back reads the worktree sources and R's local
  allow list into `approved.json`. Live checklist: `canonicalGitRoot` for a linked worktree is `R`;
  an approval reaches the next capsule.
- **Retirement must call the fold-back.** Broker completion skips it (`commands/stage/complete.rs:457`),
  so I2's retirement step calls the fold-back and `cleanup_session_settings` for every kind.
- **Phase-3 I4 items:**
  - Expose a pure settings builder from `sandbox/settings.rs`; `native/session_settings/contents.rs`
    duplicates about 40 lines of its private logic.
  - Refuse a spawn whose hooks dir is missing or unverified, for every kind; today Merge,
    BaseConflict and Adjudication launch hookless with a warning.
  - Accept a `LOOM_BIN` owned by the operator or root, provided it is not group- or world-writable
    and not under a session-writable root.
- **Exposure until phase 3.** `W/permissions/approved.json` is writable by checkout-rooted sessions
  until the `.loom` deny lands, so a planted non-control-surface `Edit` rule would reach later
  capsules. The branch is not installed before phase 3.
- **Path resolution in capsules.** How `/`-prefixed permission and sandbox paths resolve in a
  `--settings` file outside the project is unconfirmed (`grant_paths.rs` and `sandbox/config.rs`
  comments disagree). Added to the live checklist.
- **Security review of I4 phase 1 (2026-09-13).** Fixed before the phase-1 commit:
  - the daemon refuses symlinked `W/capsules` and `W/permissions` directories and target files;
  - the capsule builder calls the `sandbox/settings.rs` helpers instead of copying them;
  - the control-surface filter compares case-insensitively.

  Accepted gap: the approved-permissions filter reads rule text only, so a rule naming a symlink into
  a control surface passes it. The phase-3 OS deny rules close this, since a deny wins over any allow
  and the sandbox resolves symlinks. The review also flagged the legacy fold-back into
  `R/.claude/settings.local.json`, which the phase-3 I4 addition already removes.

### I1: Relay Protocol and CLI Writers

- New: `loom/src/relay/{mod,protocol,emit,scratch}.rs`, `loom/src/handoff/session_content.rs`,
  `loom/src/commands/request/{mod,status}.rs`.
- Changed: `commands/memory/handlers/{record,work_dir}.rs`, `commands/handoff/create.rs`,
  `commands/stage/{state,dispute_criteria,adjudicate,merge}.rs`, `commands/worktree_cmd.rs`,
  `telemetry/mod.rs`, `commands/knowledge/telemetry.rs`, `cli/{types,types_ops,dispatch}.rs` (incl.
  `hook relay` and `request status` wiring), module declarations in `commands/{mod,hook/mod}.rs` and
  the crate root, `loom/tests/common/`.
- Acceptance: the grammar table accepts exactly the grammar; each writer in relay mode writes one
  ticket whose hash and size match the line, prints the line last on stdout and writes nothing under a
  read-only `W`; legacy mode unchanged; the `cfg(test)`, cwd, stage-mismatch, kind and quota refusals
  write no ticket; a test fails when an integration test spawns the binary without the env-cleared
  helper; `loom request status` reads the ledger.

### I2: Daemon Inbox, Handlers, Knowledge Completion

- New: `loom/src/fs/inbox/mod.rs` (layout, the atomic writer I3 calls, ledger),
  `orchestrator/core/inbox_drain.rs` (declared via `#[path]` from `run.rs`),
  `orchestrator/adjudication/record.rs`.
- Changed: `orchestrator/core/{run,spool_drain,merge_handler,completion_handler}.rs`,
  `orchestrator/adjudication/{mod,prompt,session}.rs` (incl. judge in the worktree),
  `orchestrator/signals/merge.rs`, `daemon/server/{control_complete,self_service}.rs`,
  `commands/stage/{control_session,complete,knowledge_complete}.rs`, `commands/clean/{mod,sessions}.rs`.
- Acceptance: the 5 × 7 matrix test equals section 5; refusal recorded, never `Err`; I/O error leaves
  the entry; dedupe and restart at-most-once; mismatched, malformed, symlinked, hard-linked and FIFO
  entries refused; final drain and cleanup at retirement; `loom clean` cases; a Knowledge stage
  completed via the broker ends Completed and merged with no merge attempt and its dependents start;
  `merge-resolved` works with ancestry proof and is refused for another kind or stage; verdict keeps all
  four guards; the stale-ticket warning fires once per session.

### I3: Relay Hook and Helper

- New: `loom-hooks/loom-relay.sh`, `loom/src/commands/hook/relay.rs`, `loom-hooks/tests/loom-relay-*.sh`.
- Changed: `loom-hooks/{_common.sh, loom-control-complete.sh, pre-compact.sh, session-end.sh,
  ask-user-pre.sh, ask-user-post.sh, user-prompt-context.sh, post-tool-use.sh, _read_discipline.sh,
  codex-apply-patch.sh}`, `loom-hooks/tests/{run-all.sh, loom-control-complete.sh}`.
- Acceptance: with jq removed from PATH a plain payload exits 0 silently; a fixture line produces
  nothing; a relay line in output of a non-loom command is not relayed; kind mismatch, subagent control
  kind, forwarder output and tokenizer failure each give their message; a genuine persisted output file
  passes while `..`, symlinks, paths outside `~/.claude/projects`, non-regular and oversized files are
  rejected; the same payload twice yields one entry; a forged or stale session id writes nothing; a fake
  `jq` first on the inherited PATH never runs when `LOOM_HOOK_PATH` is set; Knowledge sessions are
  admitted by the completion hook, Merge sessions are not.

### I4: Capsules, Enforcement, Preflight, E2E

- New: `loom/src/sandbox/control_surfaces.rs`, `loom/tests/e2e_sandbox_confinement.rs`.
- Changed: `orchestrator/terminal/native/{session_settings,capsule,launch,wrapper}.rs` and tests,
  `orchestrator/core/{mod,spawn_setup,stage_executor,sandbox_grants}.rs`,
  `sandbox/{mod,settings,package_caches}.rs`, `sandbox/settings/policy.rs`, `codex.rs`,
  `fs/permissions/{constants,sync,drift,settings}.rs`, `fs/permissions/hooks/config.rs`,
  `hooks/{config,generator}.rs`, `git/worktree/settings.rs`,
  `commands/run/{mod,checks,sandbox_preflight,foreground}.rs`,
  `commands/repair/{sandbox_settings,settings_checks}.rs`.
- Acceptance: capsule JSON matches section 10 per kind and location; no handoff `Edit` allow remains;
  the rule-shape test passes; spawns leave both local settings files untouched (test makes them
  read-only); the wrapper exports the three new variables; each `loom run` refusal has a failing-input
  test and a passing control; a test beside `never_grants_a_credential_bearing_parent` proves no granted
  path is an executable install directory; the codex lane denies hooks and config inside its grant;
  `repair` strips exactly section 12's list; the fold-back filter works; the srt e2e passes.

## 16. Tests

- **Relay e2e (anywhere):** temp `R` and `W`, a Running session record whose PID file names the test
  process, a scratch directory; `loom memory note` in relay mode, its stdout fed to `loom-relay.sh` as a
  PostToolUse payload, one drain. Expect the note in the journal, `applied` in the ledger, the ticket
  gone; a replay changes nothing.
- **Confinement e2e under srt** (Linux; skipped unless `bwrap`, `socat` and `srt` exist): a fake `HOME`
  with `.claude/hooks/loom/`, `.claude/settings.json`, `.loom/`, `.local/bin/loom`,
  `.codex/hooks.json`, `.rustup/toolchains/`; a temp `R` with `W`, `.claude/settings.local.json`,
  `.git/hooks`; a worktree `T` with the `.loom/work` symlink; Stage and Knowledge capsules, each probe
  run through `srt` configured from the capsule's sandbox block.
  - Must fail: `R/.loom/work/x`, `T/.loom/work/x`, `R/.loom/cache/x`, `HOME/.claude/hooks/loom/x`,
    `HOME/.claude/settings.json`, `R/.claude/settings.local.json`, `R/.worktrees/T/.claude/settings.json`
    (Knowledge capsule), `HOME/.loom/config.toml`, `HOME/.local/bin/x`, `HOME/.codex/hooks.json`
    (codex lane), `HOME/.rustup/toolchains/x`, `R/.git/hooks/x`.
  - Must succeed: the scratch file, `R/src/x` (Knowledge), `T/src/x` (Stage), `HOME/.cargo/registry/x`.
  - Latency: median of 20 runs of `true` under the capsule under twice the unsandboxed median.
- **Live checklist after reinstall:**
  1. `loom run` starts a two-stage toy plan (one knowledge stage, one standard stage).
  2. `loom memory note "probe"` shows "received"; `loom memory list` has it within 10 s.
  3. `touch .loom/work/x`, `touch "$LOOM_WORK_DIR/x"` and a native Write to `.loom/work/x` are refused.
  4. `cat` of a file containing a `LOOM_RELAY_V1` line produces no relay message.
  5. `loom handoff --trigger ceiling` makes the daemon take the session down and re-queue the stage.
  6. The knowledge stage completes, merged, and its dependents start.
  7. After a forced conflict, `loom stage merge <id> --resolved` completes the stage and removes the
     worktree.
  8. A dispute gets a verdict recorded from the scratch draft.
  9. Native Edit of `~/.claude/settings.json`, `~/.claude/hooks/loom/_common.sh`, `~/.loom/config.toml`
     and `R/.claude/settings.local.json` in auto mode is refused and names the rule.
  10. A stage's `/proc/self/mountinfo` shows every section 11 row protected.
  11. `time true` in a stage shows no change.
  12. A codex forward runs, its write to `~/.codex/config.toml` is refused, and codex keeps working.
  13. With `loom-relay.sh` missing from a copy of the hooks directory, `loom run` refuses.

## 17. Risks

1. Merging to main changes nothing for running sessions. After reinstall, sessions without
   `LOOM_SCRATCH_DIR` run legacy mode and the daemon keeps the legacy drains and handoff watch; only new
   spawns are confined. Do not reinstall while a plan is running.
2. Enforcement before the relay works breaks memory, handoffs, verdicts and knowledge completion;
   phase 3 waits for the phase 2 gate.
3. Binary and hooks out of step: refused at `loom run` and at each spawn.
4. A lost relay line (filtered, redirected or backgrounded output): the CLI text warns; stale tickets
   are reported.
5. A tokenizer failure refuses a legitimate relay by design; the message says to rerun the command
   alone.
6. A wildcard reaching the sandbox could freeze Claude Code; the rule-shape test and latency probe
   guard it.
7. Codex may try to persist project trust into the now-denied `~/.codex/config.toml` (live check 12).
8. Stages can no longer install a toolchain or a managed Python for the first time.
9. After `repair --fix`, the operator's interactive sessions in `R` lose the sandbox block loom wrote.
10. Before enforcement, checkout-rooted sessions can forge inbox entries, as they can already write
    `W/stages`.
11. User hooks and shared package caches remain routes into unsandboxed execution (Open Question 2).
12. An operator running `loom stage complete` in a worktree executes agent-built code unsandboxed.
    Unchanged.
13. Worktree sessions can still move refs in the shared `R/.git`. Unchanged.
14. `loom run --foreground` has no socket, so broker completion fails there
    (`control_session.rs:107-109`); Knowledge stages now share that limitation.
15. Agent-team teammates outside the lead's process tree fail the ancestry proof and cannot relay.
