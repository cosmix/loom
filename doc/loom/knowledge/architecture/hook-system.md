---
sources:
- loom/src/hooks/mod.rs
- loom/src/hooks/generator.rs
- loom/src/hooks/config.rs
- loom/src/fs/permissions/hooks.rs
- loom/src/fs/permissions/codex_hooks.rs
- loom-hooks/_common.sh
verified: 5546d3c47ddc1f8890b40157134f057393b8b90e
---

# Hook System

>

## Hook System Architecture (loom/src/hooks/)

The top-level `loom/src/hooks` module owns `HookEvent`, hook discovery and event-log readers. Global guard registrations come from `fs/permissions/hooks/config.rs`. The capsule builder combines those guards with shared session event rules, relay hooks and interpreter handling; stage, contract and knowledge capsules register every event, while other capsules register heartbeat.

Installed shell guards consume the shared `loom-hooks/worktree-policy.json` rule data and additionally enforce context, filesystem and shared-root boundaries. The production capsule builder consumes the session-event rules through `session_event_rules`, while guard and relay registration remain capsule-specific. Installed guard, mutation and installer tests exercise the shared owner.

## Hook System — Session-Start Behavior and hookSpecificOutput Pattern

### session-start.sh Behavior

- Captures stdin into a variable (not drained) using cross-platform gtimeout/timeout/cat, 1s timeout
- Validates LOOM_STAGE_ID, LOOM_SESSION_ID, LOOM_WORK_DIR — silently exits if missing
- Writes initial heartbeat: `.loom/work/heartbeat/<LOOM_STAGE_ID>.json`, through the shared
  ownership-checked lock and atomic-replacement protocol
- Logs SessionStart event to `.loom/work/hooks/events.jsonl`
- **Parses `.source` field from stdin JSON**: when `.source == "compact"` or `"resume"`, emits `hookSpecificOutput.additionalContext` JSON with a re-anchor pointer (signal file path), redirecting the agent back to its signal after context compaction or resume
- Stdin must be captured (not drained with `>/dev/null`) so the source field can be parsed — same pattern as `post-tool-use.sh`

**Compaction recovery flow (current):**

```text
pre-compact.sh phase 1 → blocks compaction + creates handoff
pre-compact.sh phase 2 → allows compaction
Claude Code emits SessionStart with source="compact"
session-start.sh → parses source → emits hookSpecificOutput additionalContext re-anchor
```

### hookSpecificOutput JSON Pattern

Used by hooks to inject context into Claude's next turn:

```json
{
  "hookSpecificOutput": {
    "hookEventName": "<EventType>",
    "additionalContext": "<string content>"
  }
}
```

**Examples:**

- `prefer-modern-tools.sh`: PreToolUse warning when a command-position token invokes `grep`/`find`
- `skill-trigger.sh`: UserPromptSubmit skill suggestions
- `session-start.sh`: SessionStart re-anchor pointer on compact/resume source

**Why JSON over plain text:** Claude Code has reliability issues with plain-text stdout from certain hook types (see issue claude-code#13912); JSON additionalContext is more reliable for context injection.

**Construction:** Always use `jq -nc --arg ctx "..."  '{hookSpecificOutput: {hookEventName: "...", additionalContext: $ctx}}'` — never manually escape JSON strings.

**jq availability:** a hook must never fail open just because jq is missing. Blocking guards
(header documents exit 2) call `loom_require_jq` at the top of the script, right after sourcing
`_common.sh`; advisory hooks call `loom_warn_no_jq` instead — both defined in `loom-hooks/_common.sh`.
Lifecycle hooks (`session-start.sh`, `post-tool-use.sh`, `subagent-start.sh`, `subagent-stop.sh`)
keep their own explicit `command -v jq` skip rather than either helper. See
[stack.md](../stack.md#hook-runtime-dependencies-jq-rg-fd) for the Rust-side preflight/repair checks.

### PostToolUse context-ceiling boundary

`loom-hooks/post-tool-use.sh` owns transcript-tail usage measurement and threshold messaging, but it owns no
configuration parsing. It calls the hidden, deterministic `loom hook context-ceilings` command,
which uses Rust's stage loader and `ContextConfig` TOML deserializer and returns
`<main>:<subagent>`. The hook validates and caches that complete pair per Loom session before
selecting the branch for the classified caller. This keeps teammates/subagents on the plan-wide
subagent ceiling while the parent receives the stage-aware main ceiling, with one canonical config
read and no shell TOML/YAML grammar to drift from Rust. Missing, failed, malformed, or out-of-range
command output uses the shell fallback constants; a valid main zero is instead Rust's explicit
disabled sentinel for a stage record it could not verify.

### PostToolUse main-session size advisory

`_loom_main_edit_advisory` in `loom-hooks/post-tool-use.sh` is the single owner of the small-change limit
(20 changed lines, 2 files; BLOCK-B point 1). It acts only when `LOOM_SESSION_TYPE=stage`, the caller is the
main agent (never a subagent) and the edited path lies inside `LOOM_WORKTREE_PATH`. Each Write/Edit/MultiEdit/
NotebookEdit call appends an `edit` row (path, changed lines) to the `mainedits` ledger of the session; past the
limit it appends a `warned` row and prints one `additionalContext` JSON object on stdout, so the model sees it, and
the ledger stops growing. It never denies and exits 0. It runs after the ceiling checks, whose exit 2 would discard
its stdout. Exempt paths: `.loom/`, `doc/`, `*.md`, `.kb_tmp_*`, `.distill-body-*` and any path containing `/../`.
Pinned by `loom-hooks/tests/post-tool-use-main-edit-advisory.sh` and
`loom/tests/guidance_gate_contracts.rs`; the review-time hardening candidates are in
[code-quality-and-hook-debt.md](../concerns/code-quality-and-hook-debt.md).

## Subagent Isolation

Three-layer defense: documentation (the subagent preamble `spawn-guard.sh` prepends to every typed spawn; CLAUDE.md Rule 5), signal injection (cache.rs prefix), and hook enforcement, split across these hooks:

| Hook                       | Enforces                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                      |
| -------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `commit-filter.sh` | Subagents may not run git operations; blocks AI attribution in commit messages. Matches argv TOKENS (raw-regex fallback on an unterminated quote), so quoted prose about git is not a git invocation. A command whose every git segment provably targets a scratch repository (`loom_git_targets_scratch_only`, `_git-command.sh`) is exempt from the subagent `git commit`/`git add` rule and the no-git-commit rule; the subagent completion deny and the isolation-bypass rule are never exempt |
| `subagent-verify-guard.sh` | **still matches raw command strings** (see concerns.md). A subagent may compile (`cargo check`, `cargo build`, `cargo test --no-run`, `tsc --noEmit`, `<pm> run build`) and is blocked on every `cargo fmt` form, `cargo clippy`, unfiltered test suites and `lint`/`typecheck` scripts. In a live standard stage it also blocks the main agent from an unfiltered test suite of any adapter runner; build, clippy and fmt stay allowed. Subagents of `integration-verify` stages are carved out, because that stage keeps its own gate. Ordinary sessions are never blocked (`live_loom_session`, `loom-hooks/subagent-verify-guard.sh`, needs a live `LOOM_MAIN_AGENT_PID` ancestor). Deliberately has **no opt-out env var** |
| `worktree-isolation.sh` | Resolves each `../` token and each `git -C`/`--git-dir` target against the tracked effective cwd (`loom_track_cwd`, `_containment.sh`: physical resolution, `cd` tracked through `&&`, `;` and newline, taint after a subshell or splice) and blocks only a target outside the worktree and scratch; an unknown cwd warns for a traversal and blocks a git target. The scratch class is keyed on `LOOM_SCRATCH_DIR` only, never `TMPDIR` |
| `worktree-file-guard.sh` | Component-aware file boundary for the file tools; write confinement unchanged. The Read tool may open this session's `tool-results/*` files and regular uid-owned files under `/tmp/claude-<uid>` (outside other sessions' uuid directories) |
| `loom-control-complete.sh` | Only a payload classified main gets the hook's `allow` for the pinned completion command, and the plain `loom stage complete <own-id>` (bare, `2>&1`, one `\| tail/head -N`) is rewritten to it; a subagent gets none |
| `_read_ledger.sh` (via `read-guard.sh`, `poll-guard.sh`) | An unoutlined first whole read of a large file warns; an outlined one is denied once with its outline |

Every guard in this table records its blocks through `loom_record_block` (`_common.sh`) into `$LOOM_WORK_DIR/heartbeat/<stage>.blocks.jsonl` on the Claude lane only (the codex lane's hooks cannot write it); `loom hook blocks [--since-hours N] [--by-rule]` reads that directory verbatim. The resolver, each guard's decisions and the ledger are described in the sections below.

**Detection is payload-first, not a PPID comparison.** Both hooks gate on `loom_is_subagent()` in
`loom-hooks/_common.sh`, which first requires `LOOM_MAIN_AGENT_PID` to be a **live ancestor** of the
current process — this scopes the globally-installed hooks to a loom stage session. Once that
passes, it classifies the caller from the hook's stdin JSON payload via `loom_payload_agent_verdict`
(`.agent_type` / `.transcript_path` / `.session_id`, which the caller cannot forge): a Task-spawned
subagent always carries a non-empty `.agent_type` and is classified SUBAGENT immediately, and a
main-session payload is recognized by its main-shaped `.transcript_path` and classified MAIN
immediately. Only a payload-less caller, or one whose verdict is "unknown", falls back to the
process-tree walk (`find_nearest_claude_ancestor` / `count_claude_processes_between`). In that
FALLBACK ONLY, a 2-level claude chain is classified MAIN AGENT and a 3-level chain is a SUBAGENT
(verified empirically with `COMMIT_FILTER_DEBUG=1`).

**Consequence worth knowing:** agent-team _teammates_ are not in the main agent's process tree,
so `LOOM_MAIN_AGENT_PID` is set but is not a live ancestor and `loom_is_subagent` returns false
for them before either check runs. Globally installed enforcement hooks gated on it therefore do
**not** fire inside teammates. The per-session `loom-hooks/post-tool-use.sh` is intentionally different: its
validated `LOOM_*` identity already scopes it to the stage, so a positive payload verdict marks a
teammate as a subagent before ancestry. That keeps the parent heartbeat intact and applies the
subagent ceiling. A Task-tool subagent, by contrast, runs **in-process** (the same claude process
as the main agent), so the process-tree walk alone finds no intervening Claude process between it
and `LOOM_MAIN_AGENT_PID` — payload identity is what classifies both shapes correctly.

## Portable Shell Helpers and the SubagentStop Ledgers

Hook scripts run on GNU and BSD (macOS) userlands under bash 5 and macOS `/bin/bash` 3.2, and the forms
differ where it matters. The rules, in `loom-hooks/_lifecycle.sh` unless noted:

- **Padded `wc`.** BSD `wc -c` left-pads its count, so a raw value fails `^[0-9]+$`. Every `wc -c`
  site strips whitespace inline with `| tr -d "[:space:]"` before testing it (never `$(( ))`); an
  unstripped count makes a hook skip silently, which on macOS meant no review round and a blocked codex forward.
- **`loom_lifecycle_sha256`** uses `sha256sum`, else `shasum -a 256`; `loom_lifecycle_have_sha256`
  tells the caller whether either exists, and every bare `sha256sum` goes through the helper.
- **`loom_lifecycle_epoch`** tries the BSD form first (`loom_lifecycle_bsd_epoch`: `date -j -f`,
  accepting fractional seconds and a `Z` or `+hh:mm` zone) and falls back to GNU `date -u -d`. On macOS
  `date -u -d` is the set-kernel-DST option and can change the DST flag when run as root, while GNU
  date rejects `-j`, so the BSD attempt fails harmlessly there.
- **`loom_run_bounded <secs> <cmd...>`** (`_bounded.sh`, sourced by `_read_ledger.sh`) bounds a command with `gtimeout`, else
  `timeout`, else a pure-bash watchdog for stock macOS: the command runs in the background with `<&0`,
  `_loom_watchdog` runs as a process substitution reading a pipe on fd 9, TERMs the command after
  `<secs>` (`read -t`) and KILLs it a second later, and the caller writes one line to that pipe to
  retire it and never signals it, because a signal reaching a freshly forked bash runs the caller's
  EXIT trap there (see [Hooks Shell Portability](../mistakes/hooks-shell-portability.md)). The fallback signals only the direct child (not its process group) and
  returns 143/137 where `timeout` returns 124. `_read_discipline.sh` bounds `loom map --outline` with
  it; hooks that only read stdin keep the `gtimeout` → `timeout` → `cat` cascade.
- **Bash 3.2.** Guard every value expansion of a possibly empty array with `${arr[@]+"${arr[@]}"}`
  (`${#arr[@]}` is safe). Use `$$`, never `$BASHPID`. Here-strings and here-documents need a temp file
  (`$TMPDIR`, `/tmp`, `/var/tmp`, `/usr/tmp`, then the current directory; `set -e` does not stop the
  script when none is writable, the command just reads nothing), so a block path prints with `printf` inside `{ ... } >&2 || true` after `trap '' PIPE`, and trims
  with parameter expansion, never `read <<<`.
- **Ledger appends** (`loom_lifecycle_journal_ready`, the stop-skips append) require a regular file or an
  absent path, never a symlink: a planted FIFO would block the hook.
- **Heartbeat lock** (`_heartbeat.sh`). Every shell heartbeat and lifecycle-journal writer serializes with `mkdir` and `rmdir` alone: stock macOS Bash 3.2 has no `flock(1)`, and a hook primitive must not depend on the `loom` binary on `PATH`. A held lock is `<lock-dir>/owner.<pid>.<created>.<nonce>/`. A reclaimer removes only the name it examined and `rmdir` of the lock directory refuses while an owner is published, so a stale decision cannot hit a later owner. An unparseable entry is never removed, so the lock stays held and writers skip their refresh. Residuals: a clock step of over 5 s, and PID reuse within 5 s.
- **Tests.** `loom-hooks/tests/run-all.sh` runs in the `hook-syntax` CI job twice, plain and with
  `LOOM_HOOK_TEST_BSD=1`, which puts `loom-hooks/tests/bsd-shims/` (padded `wc`, BSD `stat` including
  `-f '%Lp'`, BSD `date` with `-r` and `-j -f`) first on PATH. Each shim carries a `# loom-bsd-shim`
  marker line, resolves the real tool from the PATH entries after its own position (skipping marked
  copies), stops with exit 125 when re-entered three deep (`LOOM_BSD_SHIM_DEPTH`), and execs the real
  tool unchanged when it is already BSD. `scripts/check-hook-syntax.sh` parses the extensionless shims as well. The
  `hooks-macos` job runs the suite on macOS with the real BSD tools, under `/bin/bash` 3.2. See
  [macOS CI Jobs](../concerns/platform-and-commit-gaps.md#macos-ci-jobs-and-remaining-bsd-gaps).

**Stop-skips ledger.** A skipped `loom-code-reviewer` stop leaves the review gate with no round and no
message, so `subagent-stop.sh` (`loom_subagent_stop_skip`) appends one row to
`.loom/work/subagents/<stage>/stop-skips.jsonl`: `{"ts":"<UTC>","agent_id":"<id>","agent_type":
"loom-code-reviewer","reason":"<code>"}`. It is diagnostic only: it never creates the directory,
follows a symlink, or changes the hook's exit code. A harvest that failed writes `review_harvest_failed`, suffixed with its cause when one is known: `.transcript_unusable`, `.no_start_evidence`, `.worktree_changed` (`commands/hook/review_harvest_failure.rs`) or `.timed_out`. A repeated stop for a round already recorded is a duplicate and writes no row. `verify/review/gate.rs::harvest_hint` compares the
`starts.jsonl` reviewer spawns with the recorded rounds, opens both ledgers read-only (no symlink, regular
file, size cap), and `loom stage review status` and the review gate's failure message print
`N reviewer spawns, M rounds, K stop events not harvested` with the last recorded skip reasons and a
pointer to re-run with `LOOM_HOOK_DEBUG=1`. Ledger text is filtered through `ledger_text` because a
session can write these files.

## Prompt-Submit Hooks and the `loom hook` Delegates

Two global `UserPromptSubmit` hooks run as separate processes on every prompt, each
printing at most one `hookSpecificOutput` object (`fs/permissions/hooks/config.rs`;
Codex registers `user-prompt-context.sh` as well, through
`fs/permissions/codex_hooks.rs`):

- **`skill-trigger.sh`** — a Python script despite the extension. It tokenizes the
  prompt (words, 2- and 3-word phrases, a plural-stripping stem pass, and a
  stopword list that a keyword present in the index overrides) and scores skills
  from `hooks/loom/skill-keywords.json`: a multi-word phrase, or a keyword that
  names the skill itself (or a 4+ character prefix of its name), scores 2; any other
  keyword hit scores 1. Skills with no `SKILL.md` on disk are dropped. Repository
  detection (`loom hook project-types`, which returns `root`, `types` and
  `truncated` from `ProjectProfile::discover`) is a **tie-breaker, never a
  qualifier**: a detected type adds 1 point to the matching `loom-<kind>` or
  `<kind>` skill, and with `MIN_SCORE = 2` a repo-type skill still needs a prompt
  keyword hit of its own to qualify. At most `MAX_SUGGESTIONS = 5` are printed,
  strongest first; the `loom-skills` catalog loader is dropped whenever another
  skill qualifies; two or more catalogued matches get one combined
  `Skill(skill="loom-skills", args=...)` line. `--codex` switches the output to
  native `SKILL.md` read paths: a high-confidence match (an exact skill name or a
  multi-word token) says "read ... in full", any other match says "read ... if the
  task touches <kind>" (`loom-hooks/skill-trigger.sh`). Plain mode only suggests loads
  under a neutral "skills matching this request" header and makes no mandatory
  claim. The 4+ character prefix rule still counts toward ranking on both surfaces;
  only the Codex read-in-full directive is gated.
- **`user-prompt-context.sh`** — a thin wrapper around `loom hook user-prompt`. It
  exits silently unless `LOOM_WORK_DIR` names a directory or a `.loom/work`, the legacy
  `.work`, `doc/loom/knowledge` or `.loom/cache/context-v1` directory exists walking
  up from the working directory, runs the delegate under a 5 s timeout, and re-checks
  the 16 KiB payload ceiling on its own side because the `loom` on PATH may be older
  than the script. All retrieval, gating and delivery logic is Rust — see
  [Context Retrieval](context-retrieval.md).

**The delegates** (`commands/hook/`, filesystem-only — no subcommand may make a
model or network call; `loom hook --help` lists only `user-prompt`, `reconcile-graph` and
`pre-compact`, the rest are hidden): `user-prompt`, `pre-compact` (deletes only the compacting
session's prompt delivery record; `pre-compact.sh` calls it in worktree sessions and
`knowledge-orient.sh` on SessionStart `compact` in ordinary ones),
`reconcile-graph` (the detached, debounced source-graph self-heal), `context-ceilings` (the
`<main>:<subagent>` pair `post-tool-use.sh` caches), `project-types`, `relay` (behind
`loom-relay.sh`), `review-harvest` (`subagent-stop.sh`), and the receipt and brief delegates:
`forward-receipt` (`post-tool-use.sh` pipes the PostToolUse payload to its stdin after a Bash
call containing `codex-forward.sh task`; it takes no flags) and
`worker-brief` (`spawn-guard.sh`, skipped for Codex-bound spawn types) and `reviewer-brief` (`spawn-guard.sh`, only for `loom-code-reviewer` spawns; see "Reviewer Brief Hook"). The shell runs the hook-called delegates through `loom_run_bounded`
(`_bounded.sh`): 3 s for `context-ceilings` and `pre-compact`, 5 s for `worker-brief`, `reviewer-brief` and
`forward-receipt`, 10 s for `review-harvest`; every failure is a silent no-op.
Phases and state files: [Token Accounting and Receipts](token-accounting-and-receipts.md).

**`HookTarget`** (`commands/hook/target.rs`) is the one environment-to-scope
resolution the `user-prompt`, `pre-compact` and `reconcile-graph` delegates share,
so the prompt-time reader and the background writer cannot derive different overlay
addresses. `from_environment` tries a real stage first — `LOOM_STAGE_ID` (validated
with `validate_id`) plus `LOOM_WORK_DIR`, the stage record loaded, the plan from
`delivery::plan_key`, overlay `OverlayScope::Stage`, and a `pull_stage` the brief's
"Pull more" footer may name — then falls back to the checkout: `local_overlay_key`
(`_local` / `map-<dir>`), `OverlayScope::Local`, no `pull_stage`.
`snapshot_policy()` maps those to `SnapshotPolicy::StageOverlay` and
`SnapshotPolicy::LocalCurrent`, and `retrieval_config()` loads `RetrievalConfig`
from the MAIN project root behind a worktree.

**Telemetry.** The prompt hook appends a `prompt-brief` event (`items`,
`estimated_tokens`, `omitted`) when it prints a brief, and `prompt-abstained` with a
reason — `floor`, `all-delivered`, `over-ceiling`, `no-retrieval` or `no-target` —
when it does not. Both are written only when the state directory already exists, so
an unrelated repository never grows a `.loom/work/` tree. `loom knowledge telemetry`
summarizes them per stage.

## Hook Environment Variables and Embedding (constants.rs)

### LOOM_* Env Vars Available to All Hooks

Set by the wrapper script before `exec claude` (`orchestrator/terminal/native/wrapper.rs`: `kind_env`,
`work_dir_env`, and `LOOM_SESSION_TYPE`):

| Variable               | Purpose                                                                                   |
| ---------------------- | ----------------------------------------------------------------------------------------- |
| `LOOM_SESSION_ID`      | Current session ID                                                                        |
| `LOOM_STAGE_ID`        | Current stage ID                                                                          |
| `LOOM_SESSION_TYPE`    | Session kind (`stage`, `knowledge`, `merge`, `base_conflict`, `adjudication`), every kind |
| `LOOM_WORK_DIR`        | Absolute path to the state directory (`.loom/work`)                                       |
| `LOOM_MAIN_AGENT_PID`  | Process PID (set dynamically, NOT in settings.json)                                       |
| `LOOM_WORKTREE_PATH`   | Absolute worktree path (worktree sessions only)                                           |
| `LOOM_MERGE_SESSION=1` | Set for merge resolution sessions only                                                    |

**Per-session identity gotcha (LOOM_MAIN_AGENT_PID, LOOM_STAGE_ID, LOOM_SESSION_ID):** Must NOT be in ANY settings-file env block — settings `env` overrides the process environment, so a persisted value from an earlier session shadows the wrapper's fresh exports (wrong-stage `loom memory` entries, heartbeats for the wrong session, commit-filter misidentifying the main agent). The wrapper script is the ONLY writer; `fs/permissions/settings.rs::scrub_session_identity_env()` strips these keys wherever settings are generated, copied, or merged (`generate_hooks_settings`, `create_worktree_settings`, worktree settings.local.json copy, `refresh_worktree_settings_local`, `ensure_loom_hooks_local`). Only the stable `LOOM_WORK_DIR` is persisted in settings env. `refresh_worktree_settings_local` merges main-repo permissions INTO the worktree's own settings (worktree base wins for env/hooks/defaultMode). **Claude Code applies the MAIN repo's settings env to sessions in linked worktrees** (observed v2.1.217), so worktree-side scrubbing alone is insufficient — the run path heals the main files too: `scrub_main_repo_settings_identity()` at `loom run` startup (`prepare_repo_for_run`) and `scrub_session_identity_env()` inside the sync fold-back (`merge_permissions_with_lock`), which rewrites the main settings.local.json on every stage completion (see mistakes.md 2026-07-23).

### Hook Embedding (constants.rs)

`LOOM_HOOKS` in `loom/src/fs/permissions/constants.rs` embeds hook assets with `include_str!`; `install_loom_hooks()` installs them under `~/.claude/hooks/loom/`. The binary uses the embedded sources, so source changes require a rebuild and asset installation. The inventory includes invoked hooks and sourced libraries; sourced libraries have no event trigger. Global and session hook registration is defined separately in `fs/permissions/hooks/config.rs` and `loom/src/hooks/config.rs`; Codex registrations are in `fs/permissions/codex_hooks.rs`.

`knowledge-orient.sh` is registered globally on `SessionStart` and points a fresh non-stage session to `doc/loom/knowledge/INDEX.md`. It exits silently inside a stage, on `compact` or `resume`, and when no index exists inside the git root.

## Forwarded Codex context hooks

`loom-hooks/codex-forward.sh` exports `LOOM_CODEX_FORWARD=1` before launching either the companion or direct Codex lane. With a nonempty marker, `skill-trigger.sh --codex`, `user-prompt-context.sh`, and `knowledge-orient.sh` exit 0 without output before processing input. Claude-mode skill suggestions retain their normal behavior. Guard hooks still run; the marker suppresses only these context-injecting hooks. The wrapper retains its instruction against opening skill files, while hook suppression prevents forwarded jobs from receiving injected skill/context directives.

## Pre-commit chaining and control paths

`install_pre_commit_hook` (`git/hooks.rs`) installs in the directory git runs hooks from (`git/hooks/location.rs`): `.git/hooks`, or a `core.hooksPath` inside the repository, untracked and reached through no symlink. Another `core.hooksPath` is refused with its reason, unless the hook is already there, which is how an operator serves a shared one. A foreign executable hook is preserved byte-for-byte as `pre-commit.loom-original` and invoked after Loom through the installed wrapper. An earlier Loom section is replaced in place, keeping every other byte; markers that do not bound one section, a hook without an executable shebang, and a symlink whose target is missing are refused untouched. Hook control-path classification refuses existing symlink ancestors when their link and destination cannot be represented by one protected prefix.

## Reviewer Brief Hook (`loom hook reviewer-brief`)

`spawn-guard.sh` appends a brief that loom computes to every `loom-code-reviewer` spawn made inside a live stage session, beside the worker brief (`_spawn_policy.sh::loom_reviewer_brief`, appended last through `loom_append_brief`). The hook reads no stdin, gets `/dev/null`, runs under `loom_run_bounded` for 5 s and fails open: a missing command, a timeout, a bad envelope or empty output changes nothing, and is reported only through `loom_debug`. The body sits inside a nonce envelope (`<!-- loom-reviewer-brief ... -->`, the same nonce rule as the worker brief), so text in an agent-written statement cannot close it; `clean`/`single_line` (`verify/review/report.rs`) strip control and Unicode format characters (bidi overrides, zero-width) from every agent-written value, and `fence_for` picks a code fence longer than any backtick run inside the text. The guard appends unconditionally, so a reviewer prompt that already pastes a brief holds two; `agents/loom-code-reviewer.md` therefore says only the LAST `loom-reviewer-brief` section in a prompt is loom's. The stage signal tells the orchestrator not to paste the brief (`orchestrator/signals/v2_section_review.rs`).

- **Code** (`commands/hook/reviewer_brief.rs`, `reviewer_brief_plan.rs`). The brief lists the stage's current test-integrity events (`integrity::current_events`) with each lost assertion statement (at most 25 per event, 400 bytes per statement) and asks for a finding for every lost guard that no equal or stronger check replaces. An event the stage description names as expected, with its reason, needs a finding only when the changed statement no longer asserts what that reason describes (the stage orchestrator pastes its INTEGRITY line into every reviewer prompt); an accepted event is context only. The body is capped at `MAX_BRIEF_BYTES` (16 KiB) with a line naming omitted events. A stage the daemon cannot read yields `{}` and the guard adds nothing.
- **Integration-verify** also gets the files the plan's merged stages changed, the plan's brief directory and every stage's `consumer_deferred`. Loom records no plan base commit, so the files are the sorted union, over every other non-knowledge stage with a `completed_commit`, of the paths between `merge-base(<merge>^1, completed_commit)` and `completed_commit`, where `<merge>` is the merge in the target's first-parent history whose second parent is that commit (`plan_changed_files`, using `control_paths::{accepted_change_base, changed_paths}` as `orchestrator/signals/retrieval.rs` does). A stage with no such merge is listed as not found and adds no files; knowledge stages commit on the target directly and are skipped. A 10 KiB budget lists the paths that fit and summarises the rest by directory. A failed stage listing reports `stage listing failed: ...` rather than "no stage of this plan has completed".
- **Fixtures** for a command that spawns a stage-bound `loom` write `test_support::write_main_checkout_identity(repo, work_dir)` first: every pinned-git path requires `main-checkout-identity.json`, and a fixture without it can never pass against a correct implementation ([mistakes/contract-autonomy-delivery](../mistakes/contract-autonomy-delivery.md#a-reviewer-brief-fixture-spawned-a-stage-bound-command-without-a-checkout-identity)).
- **Reviewers have no Skill tool** (`tools: Read, Glob, Grep`), so a brief that tells a reviewer to load a security or concurrency skill cannot be followed; quote the checklist into the brief instead.
- Integration-verify's parallel review gains a performance dimension; its reviewer reads the whole plan's change and the briefs.

## Path Resolution and Working-Directory Tracking

`loom-hooks/_containment.sh` is the one resolver the Bash and file guards share. It decides soundly or says it cannot.

- **Physical resolution.** `loom_normalize_path <base> <path>` returns the absolute physical path a token names from `<base>`. Before a `..` pops a component the prefix built so far is made physical (`cd -P`), so `<wt>/.loom/work/../x` is `<repo>/.loom/x` (every worktree's `.loom/work` is a symlink into the main checkout). `loom_normalize_word` wraps it for git targets, the scratch exemption and `cd` tracking: it rejects a word with a glob, brace or backslash-escape component, which bash expands before `..` applies. The resolver returns 1 for `~user`, any other `$` and a backtick.
- **`loom_track_cwd`** (after `loom_tokenize_command`) fills `LOOM_TOKEN_CWD[i]`, the physical directory token `i` runs in, or `""` when unknown. It follows `cd` through `&&`, `;` and newline only. A `cd` behind `&&` after an unverified one stays unverified; behind `||`, `|`, `&` or a redirection the cwd becomes unknown. **Taint rule:** a separator holding `(`, `)`, a backtick or an empty body (a `$(` or `sh -c` splice) makes every segment after the first directory change unknown, and a segment with `cd`, `pushd`, `popd`, `eval`, `source` or `.` counts as a directory change. A `CDPATH` mention makes later `cd` unknown. Every extension only adds unknown cases (fail closed).
- **`loom_path_class <abs>`** prints `state` (`<repo>/.loom/work`, `.work`, `.git`), `sibling` (another stage's worktree), `worktree`, `scratch` or `outside`. The scratch class is keyed on `LOOM_SCRATCH_DIR` alone, never on `TMPDIR`: the root must be absolute, an existing non-symlink directory owned by the uid (`_loom_scratch_root`), and a root inside the repo is never scratch. Hooks inherit the daemon environment, and the Bash tool's `TMPDIR=/tmp/claude-<uid>` exists only inside each command, so a TMPDIR key would class every uid-owned fixture as scratch.
- **Scratch expansion.** A `$LOOM_SCRATCH_DIR` or `${LOOM_SCRATCH_DIR}` token expands to the hook's value only while `LOOM_SCRATCH_EXPAND=1` (`_loom_scratch_expand_allowed`). The command forfeits it when it could reassign the variable (`${LOOM_SCRATCH_DIR:=x}`, `eval`, `source`, `.` at a command word) or when the tokenizer set `LOOM_TOKEN_LITERAL_DOLLAR`, which it does for a single-quoted, backslash-escaped or nameless `$`. The tokenizer owns that test because it drops quotes and escapes, and `$\LOOM_SCRATCH_DIR` or `"$"LOOM_SCRATCH_DIR` produce the same token text as a real read.

## What Each Changed Guard Decides

- **`worktree-isolation.sh`.** `check_path_traversal_resolved` resolves each `../` token against its tracked cwd and blocks only a target whose class is neither `worktree` nor `scratch`. A PAIR candidate (a `..` pair in the text) whose cwd is unknown is warned; one whose glob, brace or escape component precedes a later `..` cannot be resolved and blocks. A WIDENED candidate (a single `..`) that cannot be resolved stays allowed. Accepted residual: a glob component before one `..` (`cat .loom/w*/../x`) resolves lexically and reaches only `<repo>/.loom`. `git -C`, `--git-dir` and `--work-tree` are blocked unless the target is this worktree or scratch, and always when the cwd is unknown. `check_cross_worktree_tokens` exempts an absolute token with a known cwd that classes `worktree` or `scratch`; its own-stage exemption (`own_re`) is unanchored.
- **`worktree-file-guard.sh`.** Write confinement is unchanged. The Read tool may open this session's harness `tool-results/*` files (`allow_tool_results`) and a regular uid-owned file under the literal `/tmp/claude-<uid>` outside another session's uuid directory (`allow_tmpdir`); Grep and Glob get neither. The root is the literal `/tmp/claude-<uid>` because hooks never see the Bash tool's TMPDIR. A hook test whose fixtures sit under that root cannot prove a block, so fixtures live outside it (`loom-hooks/tests/_fixture_base.sh`: `<uid-root>/loom-fixtures/<fixed uuid>`).
- **`loom-control-complete.sh`.** Only a payload classified main (`completion_caller_is_main`) gets the hook's `allow`, with `updatedInput` carrying `timeout` and `description` only: for the exact pinned command and for `loom stage complete <own-id>`, bare or followed by `2>&1` and one `| tail/head -N`, rewritten to the pinned one. A subagent gets no allow and the plain spelling stays refused. An unknown verdict rewrites only with no `transcript_path` and `loom_is_subagent` false; an unrecognised `transcript_path` never counts as main. `run_in_background` and `dangerouslyDisableSandbox` never ride on the allow.
- **`commit-filter.sh`.** `loom_git_targets_scratch_only` (`_git-command.sh`) lifts the subagent `git commit`/`git add` rule and the no-git-commit rule only when every git segment targets scratch, the hook's rev-parse puts toplevel, git dir and common dir in scratch (or the same `&&` chain ran `git init` there), every non-git word is in `{cd mkdir true : echo printf touch ls pwd cat}`, no token names a `.git` component or starts `GIT_`, and no git segment is redirected. A command with no git segment is never exempt. The subagent completion deny and the isolation-bypass rule (`unset LOOM_MAIN_AGENT_PID`) are unchanged.
- **`subagent-verify-guard.sh`.** A subagent may compile: `cargo check`, `cargo build`, `cargo test --no-run` (the flag counts only before `--`), `tsc --noEmit` without a watch flag or `false`, `<pm> run build`. It still blocks every `cargo fmt` form, `cargo clippy`, unfiltered test suites and `lint`/`typecheck` scripts, whose bodies are not inspected. `tsc --noEmit false` is read as tsc with file argument `false` and passes. Integration-verify subagents stay carved out. The main agent of a live standard stage is blocked only on an unfiltered test suite.
- **`_read_ledger.sh`.** An unoutlined first whole read warns; an outlined one keeps the deny-once ([Read Gate](read-gate.md#rule-1-is-deny-once)).

## Block Ledger and `loom hook blocks`

`loom_record_block <guard> <rule> <tool> <subject>` (`_common.sh`) appends one JSON line `{ts,stage,session,role,guard,rule,tool,subject}` to `$LOOM_WORK_DIR/heartbeat/<stage>.blocks.jsonl`. `role` is `subagent` when `loom_payload_agent_verdict` says so, else `main`; `subject` keeps its first 300 bytes (UTF-8 safe). The Claude lane only: it writes nothing when `LOOM_CODEX_FORWARD` is set (the codex sandbox denies `<repo>/.loom`), when `LOOM_WORK_DIR` is not an absolute existing directory, or when `jq` is missing. It appends only when the ledger is absent or a regular non-symlink file of at most 4 MiB (size read with `stat`, never `wc`, which would open a swapped-in FIFO), under `umask 077`, with no lock, and every failure is silent. `loom init` and `loom clean` delete it unarchived.

Recorded guards and rules: `worktree-isolation` (the isolation rules, `policy-load`), `worktree-file-guard` (per rule, `bad-payload`), `loom-control-complete` (`not-pinned` only; identity, missing-checkout and no-trusted-binary exits stay unrecorded), `subagent-verify-guard` (`subagent-verify`, `gate-once`), `commit-filter` (`subagent-git`, `no-git-commit`, `subagent-complete`, `isolation-bypass`, `ai-attribution`), `git-add-guard` (`dangerous-add`) and `read-discipline` (`first-read`). Every recorded block's first non-empty stderr line starts with `[guard/rule]`. The guards that record nothing are listed in [Hook and Code-Quality Debt](../concerns/code-quality-and-hook-debt.md).

`loom hook blocks [--since-hours N] [--by-rule]` (`commands/hook/blocks.rs`) reads `$LOOM_WORK_DIR/heartbeat` verbatim when the variable is set (non-UTF-8 included), else `WorkDir::new(".")`, and prints `reading <dir>` first. It reads the last 4 MiB and 50,000 lines of each `*.blocks.jsonl`, opened `O_NOFOLLOW|O_NONBLOCK` and checked as a regular file on the opened handle, counts a line over 4,096 bytes as malformed, and prints every control character as `?`. The default window is 168 hours and lists the 20 newest blocks; `--by-rule` prints counts per `guard/rule`, split by role, with three sample subjects each.

## Testing Guards

- **Fixture base.** Fixtures that must not sit under `/tmp/claude-<uid>` (`allow_tmpdir` opens every plain uid-owned file there, and the Bash tool's `TMPDIR` is that root) go to `<uid-root>/loom-fixtures/<fixed uuid>` (`loom-hooks/tests/_fixture_base.sh`), not `<checkout>/loom/target/tmp`: in a stage worktree the checkout path holds `.worktrees/<id>`, so `loom_cwd_worktree` would put every fixture cwd inside that worktree. A test whose fixtures come from `mktemp` in `${TMPDIR:-/tmp}` and expects a Read outside the worktree to block passes no more.
- **Rust contract fixtures** (`loom/tests/hook_false_positive_contracts/`) set `GIT_CEILING_DIRECTORIES=<temp root>` for every hook run, because the temp root (`CARGO_TARGET_TMPDIR`) sits inside the checkout and `git rev-parse` in a scratch dir would find the outer repository. The copied hook directory gets an empty `tests/` subdirectory because `loom-control-complete.sh`'s `resolve_trusted_loom` honours `LOOM_CONTROL_TEST_BIN` only when `$(dirname $0)/tests` exists. Their temp paths hold `.worktrees/`, which is why `check_cross_worktree_tokens` exempts absolute tokens that resolve to class `worktree` or `scratch`.
- **Ledger contract.** The first non-empty stderr line of every recorded block, after `trim_start`, starts with `[guard/rule]`; for `worktree-isolation`'s heredoc banners that is the `=====` line, so the tag goes there.
- **Run hook tests plainly.** The installed `commit-filter.sh` blocks any command line containing `env -u LOOM_MAIN_AGENT_PID` as an isolation bypass, even one that only runs a hook test; the scripts scrub the hook environment inside `run_hook`. A functional test of a subagent verdict builds its own claude chain: the session's `LOOM_MAIN_AGENT_PID` is not visible in the stage Bash sandbox (PID namespace).
- **Sandbox limits.** `UnixListener::bind` fails with `EPERM` inside the stage Bash sandbox; `commands/hook/tests_blocks_files.rs` makes the socket inode with `nix` `mknod(S_IFSOCK)`. `loom/tests/integration/hooks_subagent_verify_guard.rs` asserts `std::env::temp_dir()` holds no `claude`, so run it with `TMPDIR` set to a claude-free writable directory.
