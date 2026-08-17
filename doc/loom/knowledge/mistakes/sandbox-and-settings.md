# Sandbox And Settings

> Sandbox path rules, permission sync, excludedCommands matching, and settings env leaking between main repo and worktrees.

## Sandbox: Contradictory Path Rules

**Mistake:** `merge_config()` added `doc/loom/knowledge/**` to both `allow_write` and `deny_write`.
**Fix:** Removed auto-add. Knowledge writes go through `loom` CLI (outside sandbox). Same path must never appear in both.

## Permission Sync: Three Related Bugs

**Mistake:** (1) `copy_file_with_shared_lock` overwrote worktree permissions instead of merging. (2) Permissions with parent-relative or worktree paths filtered out. (3) Sync skipped when acceptance failed.
**Fix:** (1) Merge both sets before writing. (2) Transform to portable relative paths. (3) Sync unconditionally before checking acceptance.

## Knowledge Prose Staleness After Sandbox/Permission-Mode Changes (2026-05-14)

**What happened:** After changing `default_mode_for()` in `sandbox/config.rs` to return `AcceptEdits` for Standard and IntegrationVerify stages (previously `Auto`), three knowledge file locations still referenced the old `auto` default:

1. `architecture.md` — Security Model section said `Standard/IntegrationVerify → auto`
2. `entry-points.md` — Remote Control §1 table said `Standard / IntegrationVerify → Auto`
3. `patterns.md` — Sandbox permission_mode Resolution table showed `auto` for both types

These stale entries would have misled future agents into using `permission_mode: auto` when the default at the time was `accept-edits`. (Note: the default was later reverted back to `auto` for all four stage types on 2026-07-01 — see architecture.md / patterns.md — so this entry stands only as a staleness lesson, not a statement of the current default.)

**Why:** The implementation stage correctly updated Rust source + tests, but did not search knowledge files for old values. Knowledge files are not compiled, so no tool catches the mismatch.

**Prevention:** After changing any `default_mode_for()`-style constant or sandbox default:

1. `rg -l "auto\|Auto" doc/loom/knowledge/` — find knowledge files with the old value
2. Update each stale entry with `loom knowledge replace-section` or direct Edit
3. Verify with `rg "permission.mode" doc/loom/knowledge/` that all entries agree

**Generalization:** Any plan that changes an enumerated default (permission modes, stage-type behavior, config field defaults) MUST include a step that searches `doc/loom/knowledge/` for old values and corrects them. This applies even when the code change is a single-line constant update.

## Sandbox excludedCommands: Bare Names Are Matched Exactly, Not as Prefixes (2026-05-26)

**What happened:** Every worktree stage failed at `loom stage complete` with `Read-only file system (os error 30)` writing to `.work/sessions/`, `.work/signals/`, and `.work/stages/`. `.work` is a symlink resolving to the main repo (outside the worktree), so the OS sandbox treats it as read-only. The loom CLI was supposed to be exempt because `default_excluded_commands()` returns `["loom", "git"]`, but the exemption never applied.

**Why:** Claude Code's sandbox matcher (`pK8`/`XR_` in the binary) classifies each `excludedCommands` entry:

- `"loom:*"` → **prefix** → matches `loom` AND `loom <anything>`
- `"loom *"` → **wildcard** → matches `loom <anything>` (NOT bare `loom`)
- `"loom"` → **exact** → matches ONLY the literal command line `loom` with zero args

`generate_settings_json` emitted bare `"loom"`, classified as **exact**, so `loom stage complete <id>` never matched and ran _inside_ the sandbox → EROFS. This regression surfaced on Linux once Claude Code (v2.1.150) enforced the native bubblewrap sandbox; the code's macOS-era comment misattributed it to "excludedCommands does NOT bypass OS-level filesystem restrictions."

**Prevention:** Never repair this by broadening an entry to `"<cmd>:*"`: prefix-wide exclusions move extensible CLI, VCS, interpreter, and build behavior outside the host sandbox. Treat every matcher assumption as a security boundary and verify it against the actual runtime.

**Current resolution (2026-08-08):** Plan-configurable `excluded_commands` are rejected and generated stage settings do not emit broad exemptions. Required orchestration operations need a narrow, structured control-plane boundary; they must not regain access by excluding the Loom CLI. `permissions.allow` entries control prompting only and do not provide an OS-sandbox escape.

## Worktree-Isolation Hooks Gated on LOOM_STAGE_ID, Which Leaks Into Plain Sessions (2026-05-28)

**What happened:** `worktree-isolation.sh` (and `worktree-file-guard.sh`) decided "are we in a loom worktree?" solely via `if [[ -z "${LOOM_STAGE_ID:-}" ]]; then exit 0; fi`. `LOOM_STAGE_ID` is exported into the worktree session's shell (pid_tracking.rs) and persists in the user's interactive shell environment afterward. A normal Claude Code session in the **main** repo on `main` then had `LOOM_STAGE_ID` still set, so the hook activated and blocked ordinary commands — e.g. any Bash command line merely _containing_ the substring `.worktrees/` (like an `rg`/`ls` that references another stage's dir) was rejected as "cross-worktree access," even though the session was nowhere near a worktree.

**Misleading signal:** `LOOM_STAGE_ID` being set _looks_ like proof you're executing a stage. It isn't — env vars outlive the process that set them. The hook even had a comment acknowledging `LOOM_STAGE_ID` "can be stale" but still used it as the activation gate.

**Why:** Worktree membership is a property of **location** (`<repo>/.worktrees/<stage-id>/`), not of an env var. Gating on an env var that leaks conflates "a loom run happened in this shell once" with "this command is running inside a worktree right now."

**Prevention:** Decide worktree membership from the working directory (cwd inside `.worktrees/<stage>/`), or from `LOOM_WORKTREE_PATH` only when it points at an existing `.worktrees/` dir. Never gate isolation enforcement on `LOOM_STAGE_ID` alone. Derive the current stage from the worktree path (`basename`), not from the possibly-stale env var.

**Fix:** Added `loom_current_worktree()` to `hooks/_common.sh` (returns the worktree root by cwd/`LOOM_WORKTREE_PATH`, else non-zero). Both `worktree-isolation.sh` and `worktree-file-guard.sh` now gate on it and derive the stage from the path. `worktree-file-guard.sh` now also sources `_common.sh`. Remember to reinstall hooks (`install.sh`) after editing — the runtime copy lives at `~/.claude/hooks/loom/`, separate from the repo source (see "Source vs Installed: Editing Wrong File").

## Worktree-Relative Escape Deny Rules Leak Into Main-Repo settings.local.json (2026-06-02)

**What happened:** The default sandbox config (`default_deny_read`/`default_deny_write` in `plan/schema/types.rs`) bakes in worktree-escape rules — `../../**` and `../.worktrees/**`. The **worktree** settings generator, `sandbox::write_settings(config, target)`, was being pointed at the **main repo root** by two callers: `commands/repair.rs::fix_sandbox_settings` (`loom repair --fix`) and `orchestrator/core/stage_executor.rs:438` (knowledge-stage spawns, which run in the main checkout). At a worktree (`.worktrees/<stage>/`) `../..` is the repo root — the intended isolation boundary — but at the repo root `../..` is the repo's **parent**, typically `$HOME`. So `Read(../../**)` denied all of `$HOME` (including `~/.gitconfig` → git lost its identity → commits failed) and `Write(../../**)` denied writes across the whole home dir.

**Misleading signal:** The bug is invisible inside worktrees, because there the exact same string is _correct_. It only bites when Claude runs at the repo root (interactive sessions, knowledge stages). A prior partial fix made `generate_settings_json` filter `Read(../…)` (because it also leaks into the macOS OS sandbox), which silenced the git-read symptom — but the **Write side was never filtered** (a comment called it "harmless," true only in a worktree), so `Write(../../**)` survived and kept denying `$HOME` writes. Three fossils prove an old file was written by an older binary: tilde-_expanded_ creds (`Read(/home/u/.ssh/**)`), the un-filtered `Read(../../**)`, and bare `excludedCommands: ["loom"]`.

**Why:** Path-traversal rules are _relative to wherever `settings.local.json` lives_. They are meaningful only in a worktree. Reusing the worktree-shaped generator for the main repo writes rules that resolve to a completely different (and dangerous) location. Worktree-ness is a property of **location**, not something the generator should assume — same root lesson as the `LOOM_STAGE_ID` hook bug above.

**Prevention:** Before writing path-traversal deny/allow rules, ask "relative to _which_ directory will Claude Code resolve these?" Never emit `../`-based rules into a settings file that can live at the repo root. A worktree never _depends_ on inheriting these from main — it regenerates them relative to itself at spawn (`write_settings(worktree.path)`), the create-time copy + refresh union only _adds_, and the worktree hooks enforce isolation independently. So stripping them from the main repo is safe.

**Fix:** `sandbox/settings.rs::write_settings` now computes `target_is_worktree(path)` (a `.worktrees` path component, or a symlinked `.work`) and calls `strip_worktree_escape_denies(&mut config)` for non-worktree targets, so the rules are emitted _only_ where `../..` means the repo root. This guards every main-repo caller at once. `merge_existing_permissions(.., is_worktree)` also scrubs stale `Write(../…)`/`.worktrees` entries from an already-polluted main file (the Read-side filter was already unconditional). The fold-back path (`fs/permissions/sync.rs`) already drops `../`/`.worktrees` via `transform_worktree_path`, so it needed no change.

## settings.local.json `defaultMode: "auto"` is silently ignored — must pass `--permission-mode` on the CLI (2026-07-01)

**What happened:** Every loom stage was supposed to start in `auto` permission mode (the default for all four stage types), but sessions actually started in `default` mode and prompted for every action — defeating autonomous execution. Loom set the mode ONLY by writing `permissions.defaultMode: "auto"` into each worktree's `.claude/settings.local.json` (via `generate_settings_json` / `apply_default_mode`, and again via the hooks generator). Nothing passed `--permission-mode` on the `claude` command line.

**Misleading signal:** The value `"auto"` is correct — `claude --help` lists it among `--permission-mode` choices (`acceptEdits`, `auto`, `bypassPermissions`, `default`, `dontAsk`, `plan`), and `apply_default_mode` emitted the right camelCase string. Loom's own tests asserted `defaultMode: "auto"` was present in the generated JSON, so the settings file _looked_ correct. The bug was the DELIVERY MECHANISM, not the value.

**Why it broke:** Claude Code v2.1.142+ **deliberately ignores `permissions.defaultMode: "auto"` when it comes from project or local settings** (`.claude/settings.json` / `.claude/settings.local.json`) — a security measure so a checked-in repo cannot grant itself auto mode. Auto from those files is dropped silently (no error), and the session falls back to `default`. `auto` is honored ONLY from the `--permission-mode` CLI startup flag, `~/.claude/settings.json` (user settings), or managed settings. (This gating is specific to `auto`; `acceptEdits`/`plan`/`default` ARE honored from local settings, which is why the bug hid — only auto was affected.) Confirmed against the installed binary (v2.1.197) and the official docs (code.claude.com/docs/en/permission-modes).

**Prevention:** To make a loom-spawned session START in a given permission mode, pass `--permission-mode <mode>` on the `claude` CLI (done in `build_claude_command`, resolved in the unified `spawn()` from `merge_config(read_plan_sandbox, stage.sandbox, stage.stage_type)`). Do NOT rely on `settings.local.json` `defaultMode` for `auto`. When a Claude Code setting "isn't taking effect," check the docs for file-scope gating (project/local vs user/managed) before assuming loom emits it wrong — the value can be right while the _source file_ is ignored. Note `auto` also has account/model/provider requirements (Opus 4.6+/Sonnet 4.6+, enabled on the account); an unsupported account falls back regardless of how the mode is requested.

**Fix:** `build_claude_command` now emits `--permission-mode {mode}` (before the positional prompt) using the resolved mode; `settings.local.json` still carries `defaultMode` (harmless, honored for non-auto modes). Unit test `build_claude_command_passes_permission_mode_before_prompt`.

## Claude Code Applies the MAIN Repo's settings.local.json env to Worktree Sessions (2026-07-23)

**What happened:** After the 2026-07-22 identity-scrub fix shipped, worktree sessions on kairos still ran with `LOOM_STAGE_ID=knowledge-bootstrap`: all 1,476 tool events and every lifecycle hook event across the whole 5-stage run carried the FIRST stage's identity, and the only heartbeat file written was `knowledge-bootstrap.json`. Verified live on the `knowledge-distill` session: the claude process env (via `/proc/<pid>/environ`) had the CORRECT wrapper-exported IDs, and the worktree's own settings files were clean (env = `LOOM_WORK_DIR` only) — yet its SessionStart hook logged the stale pair, which existed in exactly one file on the machine: the MAIN repo's `.claude/settings.local.json`.

**Why (two compounding causes):**

1. Claude Code (observed on v2.1.217) applies the **main repository's** `.claude/settings.local.json` `env` block to sessions running in **linked worktrees**. Settings env overrides process env, so the wrapper's correct exports are shadowed by whatever the main-repo file carries. Scrubbing the worktree-side settings files (the whole thrust of the 2026-07-22 fix) is therefore necessary but NOT sufficient. The per-repo values prove the source: the loom repo's sessions get loom's stale pair, kairos sessions get kairos's — a user/managed file can't produce repo-specific values.
2. Nothing in the run path heals a previously polluted main file: `ensure_loom_hooks_local` self-heals only on `loom init`/`loom repair`, and the permission fold-back (`fs/permissions/sync.rs`) rewrites the main `.claude/settings.local.json` mid-run (observed mtime seconds before a spawn) while leaving the stale `env` block intact. Pre-fix pollution therefore persists indefinitely. The main repo's committed-scope `.claude/settings.json` can carry the same pollution from even older loom versions and is never scrubbed by any path.

**Misleading signals:** clean worktree settings + correct wrapper exports made all spawn-side code look exonerated. `rg` over `.claude/` silently skipped `settings.local.json` because it is gitignored — use `rg -uu` when searching ignored config files. Sandboxed diagnosis shells run in a PID namespace, so `ps -p <host-pid>` / `/proc/<pid>/...` false-negatives made live processes look dead.

**Prevention:** Treat the MAIN repo's `.claude/settings.json` and `.claude/settings.local.json` as env sources for ALL sessions, including worktree ones. Per-session identity must be scrubbed from the main-repo settings files in the RUN path — at daemon startup and in every code path that rewrites those files (the sync fold-back especially) — not only on `loom init`/`repair`.

**Fix:** three-site run-path healing. (1) `scrub_main_repo_settings_identity(repo_root)` (`fs/permissions/settings.rs`) scrubs BOTH main-repo settings files, called from `prepare_repo_for_run` (`commands/run/checks.rs`) so every `loom run` — background and foreground — heals before spawning; (2) `merge_permissions_with_lock` (`fs/permissions/sync.rs`) scrubs while holding the fold-back lock, so every stage completion re-heals mid-run; (3) `migrate_hooks_to_local` (`ensure_loom_permissions`) drops identity keys from `settings.json` on init/repair. `LOOM_WORK_DIR` is stable per-repo and deliberately survives.

## Worktree Settings Are a Whole-Object Rebuild — Unemitted Keys Vanish (2026-08-07)

**What happened:** `.claude/settings.local.json` is not merged, it is REBUILT.
`generate_settings_json` (`sandbox/settings.rs:246`) starts from `json!({})` and assigns exactly
three top-level keys — `sandbox` (`:367`), `permissions` (`:435`, always present via
`apply_default_mode` at `:440`), and `worktree` (`:452`) — then `write_settings` overwrites the
whole file (`:197`). Only two things survive from the previous contents: `permissions.allow`/`deny`
(`merge_existing_permissions`, `:187`) and the two-key allowlist
`PRESERVED_SETTINGS_KEYS = ["enabledPlugins", "extraKnownMarketplaces"]` (`:580`, applied at `:191`).
Every other top-level key — `env`, user-authored `hooks`, `hasTrustDialogAccepted`, anything another
Claude Code feature wrote there — is silently dropped. This happens in worktrees AND in the main
repo root (`stage_executor.rs:373` and `:584`, `commands/repair.rs:879`).

**Why:** the rebuild is deliberate — loom owns the sandbox and permission blocks and must not
inherit drift from a previous run. But it makes the file hostile to every _other_ writer, and it
fails silently: no warning, no diff, the key is simply gone on the next stage spawn.

**Prevention (detection rule):** when a Claude Code feature configured through settings works in the
main repo but NOT inside a loom worktree, do not debug the feature — check whether
`generate_settings_json` emits that key at all:

```bash
rg -n '"<yourKey>"' loom/src/sandbox/settings.rs      # emitted anywhere?
rg -n "PRESERVED_SETTINGS_KEYS" loom/src/sandbox/settings.rs   # or carried forward?
```

Neither hit means the key is dropped every spawn, and no amount of re-configuring the feature will
survive.

**Fix:** either add the key to `PRESERVED_SETTINGS_KEYS` (a foreign key loom carries forward) or
emit it from `generate_settings_json` (a key loom owns). `preserve_unowned_keys` skips any key the
generated object already contains (`:595`), so generated always wins — the allowlist can never be
used to smuggle privileges past loom's own sandbox/permission blocks. Negative-control tests at
`settings.rs:1695-1742` pin exactly that: `enabledPlugins` and `extraKnownMarketplaces` carry
forward, a seeded `env` key and an arbitrary unknown key are both dropped.

**Related trap:** `git/worktree/settings.rs:97-104` copies the main repo's `.claude/settings.local.json`
wholesale into a new worktree _before_ the rebuild, so a main-repo local-scope key appears to
propagate — then loses everything outside the allowlist on the first stage spawn. The copy is not
evidence that the key survives.

## A Plan's `allow_write` Cannot Grant a Subprocess OS-Level Write Access (2026-08-08)

**What happened:** the tmux work needed `tmux` to `mkdir` its own socket directory, and reached for a
plan-level sandbox `filesystem.allow_write` entry to permit it. It has no effect on a subprocess, by
design — and the reason is not obvious from the plan schema.

**Why — it is inert on both layers:**

1. **OS layer.** `sandbox/settings/policy.rs` emits `sandbox.filesystem.allowWrite`, but only ever
   with loom's own fixed entries (`CODEX_SANDBOX_WRITE_PATHS`) — a plan's `allow_write` is never
   copied into it.
2. **Tool layer.** Per `concerns.md` ("Per-Stage Sandbox `Write(path)` Rules Are Inert"), Claude Code's
   file permission check consults **only** `Edit(path)`; a `Write(path)` rule parses, prints a startup
   warning, and is then ignored.

So `allow_write` is expressed in the one tool-permission form Claude Code ignores, and never reaches
the OS sandbox.

**Amended 2026-08-10 — the reason given here for withholding `allowWrite` was wrong.** This note
claimed emitting `allowWrite` makes macOS `sandbox-exec` over-restrictive about **reads**, blocking
`~/.gitconfig` and `~/.claude/shell-snapshots/`. That is the signature of the _deny-leak_ bug
documented above ("A Worktree-Only Escape Rule Applied at the Repo Root"), which was fixed
separately by filtering `Read(../…)`/`Write(../…)`; the blame was misattributed. The settings schema
is explicit that `allowWrite` means "additional paths to allow writing within the sandbox, merged
with paths from `Edit(...)` allow rules" — additive, and OS-enforced for child processes. It is the
documented, recommended lever for exactly this, preferred over `excludedCommands`.

**Prevention:** to give a **subprocess** (codex, tmux, any non-Claude-tool binary) write access to a
path outside the worktree, `sandbox.filesystem.allowWrite` is the lever — plan `allow_write` still is
not, because nothing copies it there. Wiring plan `allow_write` through to the OS layer is a live
follow-up (see `concerns.md`); until then, adding a path means adding it to the generator, as the
codex lane does. Note the deny-leak asymmetry documented above still holds: `denyWrite` leaks into
the OS sandbox from `permissions.deny` as well.

## `excludedCommands` Does Not Reliably Bypass the OS Sandbox for Compound Commands (2026-08-08)

`excludedCommands` entries (`tmux:*`, `cargo:*`) in `.claude/settings.local.json` only take effect when
the command **literally starts with** the excluded token. A script beginning with a variable
assignment before `tmux ...` still runs sandboxed, and tmux's own `mkdir` for its socket dir then fails
with `Operation not permitted` outside the `allowOnly` paths. Full `cargo test ...` invocations do
bypass it reliably.

**Consequence for tmux work specifically:** you cannot smoke-test tmux from a Bash tool call inside a
loom worktree without `dangerouslyDisableSandbox`. The sandbox allows unix sockets only under
`/tmp/tmux-*/**` and writes only under `/tmp/claude`, `$TMPDIR` and the worktree — but `/tmp/tmux-<uid>`
does not exist and `mkdir` on `/tmp` is denied, so every socket dir you _can_ create is one tmux
_cannot_ bind in. Validate tmux behaviour through `cargo test` (the e2e suite works around it per-test
via `tests/e2e/tmux_backend.rs`'s `TmuxTmpDirGuard`), not raw shell tmux.

**Detection:** `couldn't create directory /private/tmp/tmux-<uid> (Operation not permitted)` means
sandbox, not a tmux bug.

## A Credential That Must Be Read Cannot Express a Narrow Capability (2026-08-11)

**What happened:** no worktree stage could complete through its only sanctioned path. `hooks/loom-control-complete.sh` runs the completion broker, which called `read_user_token()`, which `sandbox/settings.rs` denies to every worktree agent (S-1). The failure surfaced as `trusted completion broker could not read .work/user.token`, and the stage simply could not finish.

**Why:** both halves were individually correct and nobody reconciled them. `.work/user.token` authorizes EVERY User-capability RPC, so handing it to a stage agent is privilege escalation and denying it is right. But completing its own stage is the one RPC that agent is supposed to make. One global secret cannot say "this caller may complete its own stage, and nothing else" — the capability it grants is fixed by the token, not by who presents it.

**Prevention:** when a deny rule and a required read are generated by the same tool, they are one decision, not two — check both directions before shipping either. More generally: if a capability needs to be scoped to a caller, a shared secret is the wrong instrument. Ask whether identity can carry it instead.

**Fix:** `daemon/server/peer_identity.rs`. `SO_PEERCRED` (`LOCAL_PEERPID` on macOS) gives the connecting pid from the kernel at `connect(2)`, which no request body can forge; the caller must be that session's recorded process or a descendant, with the same start-time verification the backends use against pid reuse. `authorize_preface` returns `PendingPeerIdentity` instead of a flat refusal for a User request with no valid token, `handle_client_connection` admits that outcome for `CompleteStage` alone, and the handler finishes the check once the body names a session. The S-1 deny stays exactly as it was.

**Two properties worth preserving if this is ever touched:** the request-type gate lives in `handle_client_connection`, not inside each arm, so a NEW User request added later is refused by default rather than silently inheriting the peer path. And `caller_is_inside_session` fails closed on `Unverifiable`, unlike the liveness helpers that deliberately fail open — liveness errs toward "still running" so nothing is reaped on unread evidence, while authorization must err the other way.

## A Credential Tied to the Daemon Locks the Operator Out When the Daemon Is Down (2026-08-11)

**What happened:** `loom stage complete <stage> --no-verify` failed with "the daemon credential could not be read" on a project whose daemon was stopped — exactly when an operator most needs to force-complete a stuck stage.

**Why:** the proof is HMAC'd with `.work/admin.token`, which is published at daemon start and removed when the daemon is not running. No daemon ⇒ no verifier ⇒ no obtainable proof, for anyone, operator included. The gate was written as though the credential were always available.

**Prevention:** when a check depends on ephemeral state, ask what it does in that state's absence. A gate that cannot be satisfied is not a stricter gate; it is an outage.

**Fix:** privileged completion skips the proof requirement when no credential exists at all. Safe because the daemon's absence removes a stage agent's ability to ACT, not merely its credential: an agent completes through the daemon broker (which requires the daemon) or by writing `.work/stages/*.md` directly (which `denyWrite: .work/**` forbids). With no daemon it can do neither, whatever the authorization returns — while an unsandboxed operator can still write, which is precisely the asymmetry the proof was standing in for. `refuse_operator_inside_a_session` still turns an agent away by name first.

## A `--settings` Path Must Survive the Wrapper's `cd`, and the Existence Guard Must Resolve Where the CONSUMER Will (2026-08-17)

**What happened:** every spawned stage session died within ~15s with no log output,
surfacing only as `Process no longer running` in the crash report, then retry, then
`Blocked`. The wrapper script `cd`s into the worktree before `exec`ing claude, but
`native/capsule.rs` built `--settings` from a `cwd` that arrives RELATIVE
(`./.worktrees/<stage>`, derived from `Stage::worktree_path`). After the `cd` the flag
resolved to `<worktree>/.worktrees/<stage>/.claude/settings.local.json`, and claude exited
before startup with `Error: Settings file not found`.

**Why it hid:** the `is_file()` guard in front of the flag PASSED. A relative path is
resolved against the _daemon's_ cwd — the main repo — where the file genuinely exists. So
the guard confirmed a file the spawned process would never open: the checker and the
consumer resolved the same string against two different roots. `wrapper.rs` had an
`absolute()` helper doing exactly the right thing for the `cd` line, with a doc comment
reading "Paths are absolutized because the script may `cd` elsewhere" — the capsule simply
never used it.

**Why tests missed it:** the capsule unit tests only ever passed an already-absolute path
(`/w/.claude/settings.local.json`), so the relative case — the only case production
produces — was never exercised.

**Prevention:** any path handed to a process that will `cd` must be absolutized at the
point it is built, through the SAME helper the `cd` target uses, so both resolve to one
root. Absolutize BEFORE the existence check, never after — a guard that resolves against a
different cwd than its consumer is worse than no guard, because it reports success. When a
value crosses into a subprocess with a different cwd, unit-test the relative input
explicitly; an absolute-only fixture proves nothing about the production path.

**Detection:** a session that dies inside ~15s with an empty log is almost always the
`claude` argv itself failing, not the agent. Read the generated wrapper in
`.work/wrappers/*-wrapper.sh` and run its `exec` line by hand from the worktree — the flag
error surfaces immediately, where the crash report only says `Process no longer running`.

## `loom memory` Was Unwritable in Every Sandboxed Stage (2026-08-18)

**What happened:** `loom memory note` failed with `Read-only file system (os error 30)` in
every worktree stage, for as long as the sandbox has been enforced. Stages fell back to
writing prose into `.work/handoffs/`, so the loss looked like agents choosing not to record
rather than being unable to.

**Why:** three things compounded. `.work` in a worktree is a symlink to the main repo, so
the write target is outside the worktree boundary. `sandbox/settings.rs` grants
`Read(.work/memory/**)` but no matching `Edit` — only `.work/handoffs/**` gets a write
grant. And the loom binary is **not** exempt from the sandbox: `validate_emittable` rejects
`excluded_commands` outright. So `record.rs`'s direct `append_entry` hit the kernel and lost.

**Misleading signal:** the comment in `sandbox/settings.rs` asserted that "memory and dispute
state are daemon-owned, so direct file-tool writes must never be authorized" — describing an
RPC that **does not exist**. `daemon/protocol.rs` has `CompleteStage` and `DisputeCriteria`
and nothing for memory. The comment read as deliberate design, so the missing write grant
looked intentional rather than like a gap. A second fossil pointed the same wrong way:
`fs/permissions/constants.rs` still declares `LOOM_PERMISSIONS_WORKTREE` with
`Write(.work/**)` and `Bash(loom *)`, which reads like a blanket grant but has no consumers
outside its own unit test — and `Write(path)` rules are inert anyway.

**Prevention:** when a comment says state is written "through the daemon", verify the RPC
exists in `daemon/protocol.rs` before treating a missing write grant as intentional. An
architecture note describing an unbuilt mechanism is indistinguishable from one describing a
real one. More generally: after removing a sandbox escape (here, the 2026-08-08
`excluded_commands` rejection), audit every operation that depended on it — `loom stage
complete` was given a broker, `loom memory` was not, and nothing failed loudly enough to
notice.

**Fix:** spool + drain. The sandboxed CLI appends to `<worktree>/.loom/memory-spool.jsonl`
(inside the worktree, no new grant needed) and the daemon drains it. Attribution is by
worktree location, not by any claim in the payload. See
[architecture/memory-spool.md](../architecture/memory-spool.md).

**Found alongside:** `validate_stage_id` rejects path separators but not a sibling stage's
id, so `loom memory note --stage <other>` could write another stage's journal — and journals
are injected into other stages' prompts (`orchestrator/signals/generate.rs`). Now refused
whenever `LOOM_STAGE_ID` is set and disagrees.
