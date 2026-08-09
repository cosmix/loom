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

1. **OS layer.** `sandbox/settings.rs:338-344` _deliberately never emits_ `allowWrite` into
   `sandbox.filesystem`. Emitting it makes macOS `sandbox-exec` become over-restrictive about
   **reads**, blocking `~/.gitconfig` (breaks git) and `~/.claude/shell-snapshots/` (breaks zsh). Plan
   `allow_write` paths are emitted **only** as `permissions.allow Write()` entries.
2. **Tool layer.** Per `concerns.md` ("Per-Stage Sandbox `Write(path)` Rules Are Inert"), Claude Code's
   file permission check consults **only** `Edit(path)`; a `Write(path)` rule parses, prints a startup
   warning, and is then ignored.

So `allow_write` is expressed in the one tool-permission form Claude Code ignores, and is absent from
the OS sandbox entirely.

**Prevention:** to give a **subprocess** (tmux, git, any non-Claude-tool binary) write access to a
path, plan `allow_write` is the wrong lever. The OS-sandbox write set is fixed by `sandbox/settings.rs`
— either keep the subprocess inside an already-permitted root (the worktree, `/tmp/claude`, `$TMPDIR`)
or accept that the work cannot run sandboxed. Note the same deny-leak asymmetry documented above:
`denyWrite` _does_ leak into the OS sandbox, `allowWrite` is withheld from it.

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
