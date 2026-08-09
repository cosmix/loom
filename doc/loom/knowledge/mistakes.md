# Mistakes & Lessons Learned

> Record mistakes made during development and how to avoid them.
>
> **Format:** Describe what went wrong, why, and how to avoid it next time.
>
> **Related files:** [conventions.md](conventions.md) for correct patterns, [patterns.md](patterns.md) for design guidance.

## Paths: working_dir Mismatch (Recurring)

**Mistake:** Acceptance criteria, artifact paths, and file checks used absolute paths like `loom/src/...` when `working_dir` was already `loom`, producing double-paths like `loom/loom/src/...`. Occurred in 5+ separate plans.
**Fix:** ALL paths in acceptance/artifacts/wiring/wiring_tests are relative to `working_dir`. If `working_dir: "loom"`, use `src/file.rs` not `loom/src/file.rs`. Set `working_dir` to where `Cargo.toml`/`package.json` lives.

## Stages: Marked Complete Without Implementation (Recurring)

**Mistake:** Multiple stages were marked Completed with no code committed. `stage_type: knowledge` auto-sets `merged=true` which masked missing work.
**Fix:** Always run acceptance criteria BEFORE marking stages complete. Verify actual artifacts exist.

## Phantom Merges: merged=true Without Verification

`merged=true` is a contract with the dependency scheduler — every phantom-merge incident came
from writing it without verifying git ancestry. Six related lessons (defensive "assume merged"
branches, `--force-unsafe`, helpers that abort active merges, merge-conflict session lifecycle).

→ [Phantom Merges](mistakes/phantom-merges.md)

## Orchestrator Loop: Unbounded Subprocess Freezes All Scheduling

**Mistake:** Session teardown (`handle_stage_completed` → `kill_session` → window close) shelled out with `Command::output()` and no timeout, on the orchestrator's single poll thread. On macOS that call is `osascript`, which blocks indefinitely on a TCC Automation prompt, a terminal modal, or an unresponsive terminal app. One user's daemon froze there for 10 hours: the dependent stage sat `Queued`, no `.work/` file was written, and nothing appeared in the log.
**Why it hid:** the daemon's socket thread is separate, so `loom status` kept reporting "● daemon running". Restarting fixed it, which reads as a transient glitch rather than a hang.
**Prevention:** every external command issued from the poll loop goes through `process::run_bounded`. Teardown steps are best-effort — never `?` between removing the session and `try_auto_merge`, because `StageCompleted` is edge-triggered and never fires twice for the same stage. Check `.work/orchestrator.tick`: a stale tick with a live daemon means the loop is stuck, and the second line names the phase.
**Note:** Linux is less exposed only by accident — its `wmctrl`/`xdotool` paths are `which`-guarded and no-op when the tools are absent. The structure was the bug, not the platform.

## Diagnostics: Restart Destroys the Evidence

**Mistake:** `.work/orchestrator.log` was opened with `File::create`, truncating it on every `loom run`. Restarting the daemon is the standard response to a stuck orchestrator, so the log of the run that got stuck was destroyed at exactly the moment it was needed.
**Fix:** rotate to `orchestrator.log.prev` on start. When diagnosing a stall after a restart, read the `.prev` file — the live log only covers the recovery run.

## Binary: PATH vs target/debug/loom

**Mistake:** Agents invoked stale `target/debug/loom` instead of the installed version from PATH.
**Fix:** Always use `loom` from PATH. Exception: integration-verify of unreleased features may use `./loom/target/debug/loom`.

## Security: Consolidated Findings

- **Socket permissions:** Created with default umask (world-accessible). Fix: `umask(0o077)` before bind.
- **PID handling:** `pid as i32` can overflow; raw `libc::kill` mishandles `EPERM`/`ESRCH`. Fix: use `nix::sys::signal::kill`.
- **Script injection:** AppleScript/XTerm strings not escaped. Fix: escape backslashes and quotes.
- **TOML injection:** `config.toml` via string formatting. Fix: use `toml::to_string_pretty`.
- **File locking TOCTOU:** `locked_write` truncated before lock. Fix: extracted `fs/locking.rs` with open-lock-truncate-write-flush.
- **State machine bypass:** `--force-unsafe` and recovery bypass skip validation. Fix: log all bypasses.

## File Locking: Writing to Locked Handles

**Mistake:** `fs::write()` opens a NEW handle that ignores locks held by other handles.
**Fix:** Write to the locked handle: `file.set_len(0)`, `file.seek(Start(0))`, `file.write_all()`.

## String Handling: UTF-8 Truncation Panic

**Mistake:** Byte-level slicing `&s[..n]` panics on multi-byte UTF-8 characters.
**Fix:** Use `chars().take(n).collect::<String>()` for safe truncation.

## Source vs Installed: Editing Wrong File

Seven lessons on what a large removal or rename leaves behind — straggler initializers, stale
comments, stale docs, duplicate modules, and files outside the assignment table that no subagent
owned. Prevention is always the same: grep the whole workspace for the symbol, not just the
files in your assignment.

→ [Refactor Stragglers](mistakes/refactor-stragglers.md)

## Goal-Backward Verification: False Negatives

**Mistake:** (1) `cargo test 2>&1 | tail -1` fails due to trailing newline. (2) `pub fn foo` pattern misses `pub(super) fn foo`.
**Fix:** Filter for target line first, then check. Use regex `pub.*fn foo` to match all visibility modifiers.

## Sandbox: Contradictory Path Rules

Nine lessons on sandbox path rules, permission sync, `excludedCommands` matching, `defaultMode`
being silently ignored, settings env leaking between the main repo and its worktrees, and the
whole-object rebuild of `.claude/settings.local.json` that silently drops any top-level key loom
does not emit. Recurring root cause: settings are _merged_ from several sources, so a rule is only
as good as the last writer.

→ [Sandbox & Settings](mistakes/sandbox-and-settings.md)

## Test Code: Struct Init Without Default

Lint and test-discipline lessons: `--all-targets` is required to lint test modules, ambient git
config leaks into shelling tests, `TODO` in a string literal trips the stub checker, and a
reviewer's behaviour claim must be checked against the diff before acting on it.

→ [Testing & Lint](mistakes/testing-and-lint.md)

## Daemon Module Visibility

**Mistake:** Used `crate::daemon::server::DaemonServer` but `server` module is private.
**Fix:** Use re-export path: `crate::daemon::DaemonServer`.

## Acceptance: Case Sensitivity in Patterns

**Mistake:** Template had lowercase text but acceptance criteria grep pattern required uppercase.
**Fix:** Ensure template text matches the exact case of acceptance criteria patterns.

## detection.rs: Session Exit for Merge States

**Mistake:** `detection.rs` only recognized `Completed` as normal session exit. Merge conflict sessions treated as crashes.
**Fix:** Added `MergeConflict | MergeBlocked` to the matches! pattern. When adding new terminal stage statuses, always update detection.rs.

## loom check: Negation Patterns are Literal

**Mistake:** Wiring check for `!Merge` was a false positive -- `!` is literal, not negation.
**Fix:** Use positive patterns in wiring checks. Use `acceptance` shell commands for absence checks.

## Subagent File Overlap Causes Lost Work

**Mistake:** Multiple subagents writing the same file leads to lost work (last writer wins).
**Fix:** Every subagent MUST have exclusive write access to its files. Use file ownership tables. If overlap is unavoidable, use one subagent or handle sequentially.

## loom knowledge update: Path Resolution

**Mistake:** Running `loom knowledge update` from a subdirectory creates files relative to cwd, not worktree root.
**Fix:** Always run knowledge commands from the worktree root.

## Skill Documentation Freshness

**Mistake:** Skill files referenced old schema state after fields were added/removed.
**Fix:** Update skill files and feature code together when changing schemas.

## Repair Must Propagate Skill-Index Failures (Resolved 2026-08-08)

**Mistake:** Hook repair rebuilt the skill index but discarded a rebuild error, so the action could be counted as fixed without producing a usable index.
**Prevention:** A composite repair step must return the first failed sub-operation; never increment the repaired count after a required side effect fails.
**Fix:** `fix_hooks_with` now returns the skill-index rebuild result, and `hook_repair_propagates_skill_index_write_failure` pins the failure path.

## loom merge Command Removal

**Lesson:** `loom merge` duplicated `loom stage complete` functionality with 5 bugs. Removed entirely rather than fixing. When a command duplicates existing functionality and has multiple bugs, removal is better than repair.

## Using npx Instead of bunx

**Mistake:** Used npx instead of bunx during implementation.
**Fix:** Always use `bun`/`bunx` per project conventions. Check CLAUDE.md tool preferences before running package managers.

## Truths → Acceptance Unification

**What happened:** truths and truth_checks were separate fields on StageDefinition/Stage that overlapped with acceptance criteria. Unified into AcceptanceCriterion enum (Simple|Extended).

**Gotcha:** Old plans with a top-level `truths:` field are now rejected as unknown instead of silently dropping the checks. Migrate those commands to `acceptance`; `before_stage` and `after_stage` remain valid delta-proof fields.

**How to avoid:** Keep plan structs strict with `deny_unknown_fields` so removed or misspelled policy cannot false-pass. When compatibility is required, use an explicit migration path rather than permissive deserialization.

## gawk vs POSIX awk (2026-03-31)

**What happened:** Initial `_common.sh` used gawk-specific `match()` with array capture (3rd argument), which failed with syntax errors on standard awk and macOS default awk.
**Why:** gawk extensions are not available on all platforms. macOS ships with BSD awk.
**How to avoid:** Always use POSIX awk features only. For complex string extraction, use `substr()`+`sub()` approach instead of `match($0, pattern, arr)`.

## Hook Integration Tests Need _common.sh (2026-03-31)

**What happened:** After adding `_common.sh` as a dependency sourced by hooks, 12 integration tests in `hooks_commit_filter.rs` failed because the test setup didn't install `_common.sh` alongside the hook script.
**Why:** Hooks source `_common.sh` via `source "$(dirname "$0")/_common.sh"` — tests must install all dependencies in the temp directory.
**How to avoid:** When adding shared utilities sourced by hooks, update ALL integration test `setup_hook()` functions to also install the shared utility.

## Cross-Platform Timeout in Hooks (2026-03-31)

**What happened:** `git-add-guard.sh` used bare `timeout` command without `gtimeout` fallback, which fails silently on macOS without GNU coreutils.
**Why:** macOS doesn't have `timeout` by default; GNU coreutils provides it as `gtimeout`.
**How to avoid:** All hooks reading stdin MUST use the three-way cascade: `gtimeout` → `timeout` → `cat`.

## Knowledge Commands: CWD Resolution (2026-04-16)

**What happened:** Knowledge commands used `main_project_root()` which followed `.work` symlinks to resolve to the main repo root. In worktree contexts (e.g., integration-verify stages), `loom knowledge update` wrote to the main repo instead of the worktree, causing cross-worktree state pollution.
**Why:** `main_project_root()` was designed to always find the true main repo root, which was correct for `.work/` state but wrong for knowledge files that should be worktree-local.
**Prevention:** Use `project_root()` (cwd-relative) for file writes that should respect worktree isolation. Use `main_project_root()` only for accessing shared state (`.work/`). Always run `loom knowledge update` from the worktree root, not a subdirectory.
**Fix:** Replaced all `main_project_root()` calls in knowledge commands and map.rs with `project_root()`. Updated signal content to require commits for knowledge stages. Removed commit-guard.sh bypass for knowledge stages.

## Stale Acceptance Criteria Referencing External Plan Files

**What happened:** An `integration-verify` stage had an acceptance criterion `cargo run -- plan verify ../doc/plans/DONE-PLAN-cwd-knowledge-resolution.md`. That plan file was deleted during housekeeping (`doc: remove completed plans`) AFTER the stage was authored but BEFORE it ran. The criterion failed at execution time with a file-not-found error, requiring `--no-verify` to complete.

**Why:** Plan files in `doc/plans/` are subject to archiving/deletion as a normal maintenance operation. A file that exists when you write a criterion may not exist when the stage executes, especially for long-running plans.

**Prevention:** When generating acceptance criteria for `integration-verify` stages, never reference plan files from `doc/plans/` directly. Instead, use self-contained fixtures: create a temp file via `TempDir` + `write_plan` in Rust tests (see `tests/integration/plan_verify.rs` for the pattern). If a live-CLI smoke test is needed, write a minimal inline plan to a temp path rather than relying on a file that may be archived.

**Fix:** Use test fixtures that are fully controlled by the test suite. Reference `tests/integration/plan_verify.rs` as the canonical example of building plan fixtures without touching `doc/plans/`.

## Schema Root: LoomConfig vs Plan

**Mistake:** Passing the top-level YAML document (which wraps `loom:` key) where a `LoomConfig` (the inner object) is expected, or vice versa. This commonly manifests as "missing field" serde errors.

**Why:** Plan YAML has the structure `{ loom: LoomConfig }`. `parse_plan()` extracts the `loom:` block and deserializes that into `LoomMetadata` / `LoomConfig`, not the outer wrapper.

**Prevention:** The canonical deserialization root is `LoomConfig` (at `plan/schema/types.rs`), not the outer document. Nested fields (execution, stages, sandbox) live on `LoomConfig`.

## Session Identity: Backend Metadata Must Be Persisted

Session identity, liveness routing, spawn-site coverage, and the struct-literal blast radius of
adding a session field. Recurring root cause: a session fact derived at one call site instead of
persisted and read back through the shared service.

→ [Sessions & Liveness](mistakes/sessions-and-liveness.md)

## toml_edit vs toml: Different Use Cases

**Mistake:** Using `toml_edit Item -> serde` for reading nested config sections. `toml_edit` is designed for round-trip writes; its typed access silently drops nested sub-tables.

**Why:** `toml_edit::Item` doesn't implement full `serde::Deserialize` for complex nested structures the same way `toml::Value` does.

**Prevention:** Use `toml_edit` for writes (round-trip safe). Use `toml` (re-parse the full file with `toml::Value`, then `try_into::<T>()` on the section) for typed reads of nested structures.

## macOS GUI App CLI Not on PATH — Detection-Spawn Mismatch (2026-04-27)

**What happened:** `TerminalEmulator::Ghostty` detection succeeded on macOS via a `/Applications/Ghostty.app` path-existence fallback (detection.rs:190-191), but spawn called `Command::new("ghostty")` and failed with "Failed to spawn terminal 'ghostty'. Is it installed?" The Ghostty CLI binary lives inside the bundle at `/Applications/Ghostty.app/Contents/MacOS/ghostty` and is not added to PATH (ghostty-org/ghostty#2483). Detection picked the terminal; spawn couldn't launch it.

**Misleading signal:** `which::which("ghostty")` failing was _handled_ by an explicit `.app` existence check that succeeded. The fallback proved the GUI app was installed, not that its CLI was reachable from a child `Command`. Two-binary detection (`which` OR `.app exists`) silently expanded the set of "detected" terminals beyond the set of "spawnable via PATH" terminals.

**Why it broke:** Detection logic and spawn logic relied on different existence proofs. Detection accepted "the .app exists" as sufficient; spawn assumed the binary was on PATH. The asymmetry produced a guaranteed runtime failure for any macOS user without a manual PATH shim.

**Prevention:**

- For any `TerminalEmulator` variant whose detection has a path-based fallback (anything beyond `which::which(binary())` succeeding), the corresponding `build_command()` arm MUST use a launch path that does not depend on PATH — typically `open -na <AppName> --args ...` (see patterns.md "macOS GUI App Launch Pattern") or AppleScript via `osascript`. Treat any macOS `.app`-bundled tool as PATH-unreachable by default.
- When adding a new terminal emulator: check that detection and spawn agree about _how_ the binary is reachable. If detection falls back to `.app` existence, spawn must NOT call `Command::new(binary())` directly on macOS.

**Fix:** `Self::Ghostty` arm in `emulator.rs:build_command()` is now cfg-gated; macOS reassigns `command = Command::new("open")` and uses `open -na Ghostty --args --working-directory=... --title=... -e bash -c CMD`. Linux behavior unchanged. `binary()` still returns `"ghostty"` (correct for Linux PATH lookup and for any macOS user with a manual shim). Tests `test_ghostty_build_command_macos` and `test_ghostty_build_command_linux` are cfg-gated so each runs on its target platform.

## Aggregated Wiring Re-Verification: Double-Applied working_dir

**What happened:** `run_aggregated_wiring_reverification` in `commands/stage/complete.rs` was called with `acceptance_dir` (already resolved to `worktree_root + integration-verify.working_dir`) and then joined each prior stage's `working_dir` on top, producing paths like `loom/loom/src/...`. The wiring check reported "Wiring source file missing" for every prior stage.

**Why:** `acceptance_dir` is computed as `worktree_root + working_dir`, so it is already a fully resolved path. Joining another `working_dir` on top re-applies it.

**Prevention — Detection rule:** Any code path that loops over prior stages and builds a source-file path MUST start from `worktree_root`, then join the per-stage `working_dir`. Never start from an already-resolved `acceptance_dir`.

**Fix:** Changed call site to pass `worktree_root` (from `StageExecutionPaths`) through `run_verification_phase` into the aggregated re-verifier; each stage's `working_dir` is joined against the worktree root.

## Stage-File Lost Updates: Whole-Stage Save Reverts Concurrent Writers (A-5, 2026-06-09)

**What happened:** `locked_read`/`locked_write` make individual reads/writes atomic, but load → mutate → whole-record save releases the lock between read and write. Three writer classes race on the same stage file — the orchestrator main loop, daemon IPC handlers, and agent-run CLI commands. A writer that loaded a stage minutes earlier can therefore revert status, counters, close reason, session identity, or amended verification policy written in the gap.

**Misleading signal:** patterns.md claimed a "daemon single-writer model" and "no explicit file locking." Both were false — `fs/locking.rs` exists, and the daemon, its dispute thread, and CLI agents all write stage files concurrently. "Each save is locked" hid that locked atomic saves of a STALE whole object still lose updates.

**Why:** per-operation locking serializes the WRITE but not the read→write transaction. Two transactions that both `load(); mutate_field_A_or_B(); save_whole_stage()` interleave as load-A, load-B, save-A, save-B → B's save reverts A's field.

**Prevention — use `verify::transitions::update_stage(id, work_dir, |s| …)` for every existing-stage mutation, never load + mutate + whole-record save.** It re-reads under the stages-directory lock, applies the closure, and writes in one critical section. Mutate only operation-owned fields. Run slow Git, terminal, network, and verification work outside the lock, then apply a short delta. Whole-record persistence is for actual creation only; the orchestrator loop is not exempt because daemon and CLI writers remain concurrent.

**Detection rule:** any `load_stage(); …; save_stage()` pair where the `…` can run while the daemon/dispute-thread/another-CLI is live is a lost-update candidate. Especially when `…` contains a multi-minute step (acceptance, verification, git merge) or increments a counter read from the in-memory stage (`fix_attempts += 1`, `dispute_count += 1`).

## Verifying "Dead Schema" Claims Before Writing Code (2026-06-15)

**What happened:** Plan PLAN-anti-slop-thoroughness described `before_stage` as "dormant / parsed-but-never-run." Stage 3 Subagent 1 was tasked to wire it. It verified the claim against `stage_executor.rs:219-256` and found `before_stage` was already fully wired — runs pre-spawn, blocks session on failure. The task was a no-op.

**Misleading signal:** Plan descriptions are written at planning time and can go stale as other stages implement things. A plan claiming a field is "dead" is as reliable as code comments — it describes intent at authoring time, not current reality.

**Prevention:** Before implementing "wire X" or "add execution of Y," run `rg "before_stage\|after_stage\|<field>" loom/src/` to verify the current execution path. Check `stage_executor.rs` (pre-spawn), `complete.rs` (post-acceptance), `generate.rs` (signal), `plan_setup.rs` (copy). Only skip after confirming absence, not trusting the plan text.

**Fix:** Skipped the no-op task; verified the actual dormant field (code_review) and wired it instead.

## Vendored slash-command / Codex skill must consume the plan arg verbatim (no `doc/plans/` prefix)

**What happened:** Originally-installed `pressure.md` / codex `SKILL.md` used `doc/plans/$1`, which double-prefixed into `doc/plans/doc/plans/PLAN-foo.md`.
**Why:** The `loom pressure` driver hands children the FULL repo-relative invocation (e.g. `doc/plans/PLAN-foo.md`) because they run with `current_dir(repo_root)`. The template then re-prefixed `doc/plans/`.
**Prevention:** When a Rust driver passes a repo-relative path to a slash command or Codex skill, the template MUST use `$1`/`<PLAN>` directly. The driver owns path resolution (`resolve_plan_path`); the template owns none.
**Fix:** vendored `commands/{pressure,address}.md` and `codex/skills/pressure/SKILL.md` use the arg verbatim.

## Gate path resolution on `is_file()`, not `exists()`, before spawning agents

**What happened:** Plan resolution risked accepting a directory argument.
**Why:** `Path::exists()` is true for directories; canonicalizing one and handing it to claude/codex fails confusingly downstream.
**Fix:** `resolve_plan_path` gates on `is_file()` so a directory arg fails cleanly at resolution.

## A `--dry-run` that hand-builds its command string drifts from the real spawn

**What happened:** An early dry-run printed simplified commands missing `--permission-mode`/`--model`/`-C`.
**Why:** Preview re-derived argv independently of the spawn path.
**Prevention/Fix:** share ONE argv builder between preview and spawn (`claude_args`/`codex_args` feed both `render_dry_run` and `spawn_*`). Any preview that re-derives argv is a silent-divergence hazard.

## Stage signal did not embed the plan's inline command/skill bodies

**What happened:** The implement-pressure signal omitted the plan's inline slash-command and codex-skill bodies. Canonical sources had to be recovered from `~/.claude/commands/*.md` (Read tool) and `~/.codex/skills/pressure/SKILL.md` (Bash `cat` — `worktree-file-guard.sh` ALLOWS the Read tool on `~/.claude/` but BLOCKS it on `~/.codex/`). The installed copies were also STALE.
**Prevention:** When a stage depends on file bodies that live outside the worktree, do not trust the signal to inline them or the installed copies to be current — recover from the authoritative source and treat installed versions as suspect.

## Interactive Claude cannot be captured or made to auto-exit without risking API billing

**What happened:** A first cut of the `loom pressure` fixes assumed `claude -p` (or capturing Claude's stdout to a log) was the way to make Claude run one slash command and exit so the driver could proceed. Both are wrong for a subscription user.
**Why:** (1) `claude --help` states Claude runs in non-interactive mode "via -p, or when stdout is not a TTY, e.g. piped or redirected output" — so redirecting/capturing Claude's stdout flips it into the `-p` path, which can bill against pay-per-token API credits instead of the claude.ai subscription (known bug anthropics/claude-code#43333). (2) Feeding EOF on stdin (`< /dev/null`) does NOT let the task finish then exit — the REPL quits _before_ the agentic work completes (data loss), and an empirically-tested run also hit a workspace-trust dialog that `--permission-mode auto` did not skip. (3) There is no `--max-turns`/exit-when-done flag for interactive mode.
**Prevention:** For anything that must keep subscription billing, Claude's stdout MUST stay a real TTY (foreground, uncaptured). Do not reach for `-p` or output redirection to "automate" it. Only ONE process can own the foreground TTY, so anything running concurrently (e.g. Codex) must be backgrounded with captured output.
**Fix:** Mirror the loom daemon's own model — the daemon never relies on Claude self-exiting; it SIGTERMs the session (`event_handler.rs` → `NativeBackend::kill_session`) once the agent signals completion via `loom stage complete`. `loom pressure` does the analog: inject a "`touch <marker>` as your final action" instruction via `--append-system-prompt`, poll for the marker, then SIGTERM the idle foreground session (manual exit as fallback).

## Large-Scale Parallel Doc Editing: Whole-File Writes Fail, Self-Lint Reports Lie (2026-07-01)

**What happened:** During the 61-file skills/ overhaul (4 coordinators × ~6 workers), two recurring failures: (1) workers rewriting very large files (~2-3K lines, e.g. `skills/loom-react/SKILL.md`) with a single whole-file Write died repeatedly with "Connection closed mid-response" (0 tokens, files untouched); the same file succeeded when the worker was re-instructed to apply ~18 small targeted Edits instead. (2) Workers self-reported "markdownlint clean" but a single authoritative `markdownlint-cli2` pass at the gate found 35 residual errors across territories (MD032/MD056/MD038/MD034/MD028) — worker self-verification via ad-hoc greps does not implement markdownlint rules.
**Why:** A multi-thousand-line Write is one giant model response — long uninterrupted output maximizes exposure to connection drops, and a failure loses ALL of the work; incremental Edits checkpoint progress per tool call. Lint self-reports were grep-approximations, not the real linter (bunx was sandbox-blocked for workers: bun needs tempdir writes outside the sandbox allowlist).
**Prevention:** (1) When directing agents to rewrite files >~1000 lines, instruct them to transform via a sequence of targeted Edits, never one whole-file Write. (2) Never trust per-agent lint claims — run ONE authoritative `bunx markdownlint-cli2 "skills/**/SKILL.md"` (needs sandbox escape for bun's tempdir) at the merge/verify gate; the repo `.markdownlint.json` is picked up from the root.
**Fix:** Re-spawned failed workers with the incremental-Edit instruction; ran the gate lint pass and fixed the 12 residual errors (main agent) + 23 (backend coordinator) directly.

## `loom pressure` codex "never starts" — it was invisible, not broken (2026-07-02)

**What happened:** The backgrounded Codex half of `loom pressure` was reported as "never starts (or starts and fails)". Investigation of the leftover logs (`/tmp/loom-pressure-codex-<pid>.log`) proved Codex ran fine in the recent runs: it triggered the `$pressure` skill, spent 170k–260k tokens, wrote its review next to the plan, and `/address` folded it in. Nothing was broken.
**Misleading signals:** (1) The driver printed NOTHING when codex spawned; the only codex UI was the wait-spinner, shown only when codex outlived the foreground Claude session — codex finishing first left zero terminal trace. (2) The codex report is deleted as final cleanup after all rounds, so no artifact survives a full run. (3) Every log contains a scary `ERROR rmcp::transport::worker … AuthorizationRequired` line even on successful runs — it is codex-side and non-fatal.
**Prevention:** Before diagnosing a `loom pressure` codex failure, read `/tmp/loom-pressure-codex-*.log` (one per driver invocation, overwritten per round) and check for `Wrote the pressure review to …` near the tail. Also note codex shares the driver's foreground process group: a Ctrl+C aimed at Claude SIGINTs codex too (`turn interrupted` in the log).
**Fix:** The driver now prints status lines — `→ codex review started in background (log: …)` at spawn, and after exit either `✓ codex review written → <report>` or a warning when codex exited cleanly without writing the report.

## Plans-location rule was prose-only — the one hard rule with no hook enforcement (2026-07-06)

**What happened:** Opus repeatedly wrote plans to `~/.claude/plans/` despite CLAUDE.md.template stating the ban three times (Rule 1, a HARD STOP banner, and the end-of-file reminders).
**Why:** Plan mode injects its save-location suggestion at the moment of the Write call; a prohibition stated mid-file thousands of tokens earlier reliably loses to an instruction present at the decision point. Every other hard rule (commit/complete, git add -A, worktree isolation) had a hook backstop — plans did not: `worktree-file-guard.sh` exits early outside loom worktrees and explicitly whitelists all `~/.claude/**` paths, so interactive sessions (where plan mode runs) had zero deterministic coverage.
**Prevention:** A rule that must never be violated needs a deterministic channel, not more prose. Prose emphasis is also zero-sum — when ~20 rules carry ⛔/NEVER banners, the salience gradient is flat and the load-bearing rules don't stand out.
**Fix:** Added `hooks/plans-path-guard.sh` (PreToolUse on Write|Edit, blocks `.claude/plans` and `.claude/projects/*/plans` path segments, exit-2 message redirects to `doc/plans/`), wired via `fs/permissions/constants.rs`, `fs/permissions/hooks.rs`, and `install.sh`. Restructured CLAUDE.md.template to a 5-item hard-stop tier stated verbatim at top and bottom.

## Repo hook scripts do not need the executable bit (2026-07-06)

**What happened:** After creating a new hook script, attempted `chmod +x` in the repo (blocked by the sandbox on `hooks/`).
**Why:** The repo copies are sources, not the installed artifacts — `install.sh` and `fs/permissions/hooks.rs::install_hook_script` both chmod 755 at install time.
**Prevention:** Skip chmod for files under `hooks/`; run tests via `bash hooks/tests/run-all.sh` (invokes each script with `bash`, no exec bit needed).
**Fix:** None needed — dropped the chmod.

## Per-session identity persisted in settings env blocks goes stale and shadows the wrapper env (2026-07-22)

**What happened:** Inside worktree sessions, `$LOOM_STAGE_ID`/`$LOOM_SESSION_ID` reported the IDs of the plan's FIRST stage (the knowledge stage) instead of the executing one — six stages later. Unqualified `loom memory` calls filed entries into the wrong stage's journal (hit across 2+ plans, 4+ stages), and hooks heartbeat the wrong session.
**Why:** Three writers persisted per-session identity into settings files: (1) knowledge-stage spawns wrote `LOOM_STAGE_ID`/`LOOM_SESSION_ID` into the MAIN repo's `.claude/settings.local.json` env block and nothing ever cleared it; (2) worktree creation copied that file wholesale into new worktrees; (3) `refresh_worktree_settings_local` (triggered by permission propagation on every `loom stage complete`/crash sync) rebuilt each worktree's settings.local.json FROM THE MAIN REPO'S COPY as base, clobbering the worktree's fresh env/hooks/defaultMode mid-session. Claude Code applies settings `env` OVER the process environment, so the stale settings values silently shadowed the wrapper script's correct exports. (`LOOM_MAIN_AGENT_PID` had already hit this exact failure and been removed from settings env — the lesson wasn't generalized to the other identity vars.)
**Prevention:** INVARIANT: per-session identity (`LOOM_MAIN_AGENT_PID`, `LOOM_STAGE_ID`, `LOOM_SESSION_ID`) is exported ONLY by the wrapper script (`pid_tracking.rs` template) and must NEVER be written into any settings file. Any settings-file `env` write of a value that varies per session is a staleness bug by construction — settings env overrides process env. When merging/copying settings across checkouts, the destination's session-specific config (env, hooks, defaultMode) must win; only permissions are unioned.
**Fix:** `fs/permissions/settings.rs::scrub_session_identity_env()` (shared `SESSION_IDENTITY_ENV_KEYS` scrubber) applied in `generate_hooks_settings`, `create_worktree_settings`, the worktree settings.local.json copy, `refresh_worktree_settings_local` (which now uses the worktree's own settings as merge base), and `ensure_loom_hooks_local` (self-heals existing installs on `loom init`/`repair`). `HooksConfig` no longer carries stage/session IDs at all.

## `loom review` wrote through the `.work` symlink into the main repo's doc/plans (2026-07-22)

**What happened:** From inside a worktree, `loom review` printed `✓ Review document written to doc/plans/REVIEW-....md` (exit 0) but the file never appeared in the worktree's `doc/plans/` — it had been written to the MAIN repo's copy, invisible from the worktree.
**Why:** The command resolved its output root via `WorkDir::main_project_root()`, which follows the worktree's `.work` symlink back to the main repo. The success message then printed the path relative to that root, making it look local.
**Prevention:** Commands that WRITE user-visible files must anchor on the current checkout (worktree root when `cwd` is inside `.worktrees/`), not on `main_project_root()` — that helper is for reaching shared `.work` state, not for output placement. Exit 0 + "written to <relative path>" is not proof the file is where the reader thinks; check which root the path was relativized against.
**Fix:** `commands/review/generate.rs::resolve_output_root()` — writes to `find_worktree_root_from_cwd(cwd)` when inside a worktree, else the main project root.

## CI Clippy Failures That Don't Reproduce Locally = Toolchain Drift (2026-07-22)

**What happened:** CI's Clippy job failed on main while `cargo clippy --all-targets -- -D warnings` passed locally with zero warnings. Local toolchain was 1.95.0; CI installs latest stable via `dtolnay/rust-toolchain@stable`, which had moved to 1.97.1 and shipped new lints (`useless_borrows_in_formatting`, broader `question_mark`) that fired on 21 existing sites.
**Why:** The workflow floats on `@stable` while local toolchains only move on explicit `rustup update`. Every ~6-week Rust release can introduce lints that break CI with `-D warnings` even though no code changed.
**Prevention:** When a CI clippy failure doesn't reproduce locally, check `rustup check` FIRST — if stable has moved, `rustup update stable` and re-run before hunting for any other cause. Most new-lint fallout is machine-applicable: `cargo clippy --fix --all-targets --allow-dirty`, then review the diff (non-trivial rewrites like `question_mark` can leave awkward leftover blocks worth hand-cleaning).
**Fix:** Updated local stable to 1.97.1, applied `cargo clippy --fix`, hand-simplified the `?`-operator rewrite in `fs/work_dir.rs`, verified clippy + fmt + full test suite green.

## Delta-Proof `before_stage` Gate Re-Run on Every Re-Spawn Deadlocks the Stage (2026-07-27)

**What happened:** `start_stage` ran a stage's `before_stage` truth checks on _every_ spawn attempt. Those checks are delta-proofs — they assert the feature does NOT exist yet. After a session was interrupted mid-stage (leaving its implementation in the worktree/branch), orphan recovery re-queued the stage, the checks re-ran, found the feature present, and marked the stage `Blocked` with `FailureType::TestFailure` **before spawning any session**. Since no session ever ran, nothing could finish or commit the work, and `loom stage retry` / the next `loom run` reproduced the identical failure forever. The stage could not self-heal.

**Misleading signal:** the failure output is a genuine, correctly-computed check result — "this command exited 0 but the plan says it should exit 1" — so the block looks like a real pre-condition violation rather than the orchestrator tripping over its own prior progress. The comment on the call site ("verify pre-conditions in fresh worktree") described an assumption — a _fresh_ worktree — that `get_or_create_worktree` stops honoring the moment a stage is retried.

**Why:** a one-shot gate was placed on a path that runs many times. Every re-entry route into `start_stage` (orphan recovery → Queued, `loom stage retry`, crash auto-retry) reuses the same worktree and the same `loom/<stage-id>` branch, so the "before" state the check asserts is by construction no longer true after the first attempt.

**Prevention — detection rule:** any check whose _expected_ outcome changes once the stage does its work (delta-proofs, "feature absent" assertions, baseline captures) must be gated on evidence that no work exists yet — not merely placed before the spawn. Before adding a blocking check to a spawn path, ask what it does on attempt #2. And a blocking transition that happens _before_ a session is spawned deserves extra scrutiny: nothing downstream can clear it, so a wrong block is permanent, not merely slow.

**Fix:** `stage_executor.rs::before_stage_gate_passed` calls `verify::before_after::find_prior_stage_work` first and skips the checks (logging the evidence) when the stage branch has commits beyond its resolved base or the worktree has non-scaffold changes. Loom's own worktree scaffolding (`.work`, `.claude/`, root `CLAUDE.md`) is discounted via `git::worktree::is_worktree_scaffold_path` — otherwise, in a repo that doesn't gitignore those, the very first spawn would look "dirty" and silently disable the gate. Note `git::has_uncommitted_changes` excludes untracked files and was useless here (a brand-new module is untracked); `list_working_tree_changes` was added for the "has anyone worked here?" question.

## Hooks: Shell Command Matchers (2026-07-28)

Token-based Bash matchers repeatedly shipped with bypasses because separators that are _glued_
to a neighbour never become tokens. Also: forgeable `glob | head -1` privilege lookups, env
leakage into simulated process trees, and three Bash traps (`&` in `${//}` replacements,
`errexit` under `||`, ERE `\b`).

→ [Shell Command Matchers](mistakes/shell-command-matchers.md)

## Doctrine, Acceptance Criteria, and Cross-Surface Drift (2026-07-28)

A grep for one phrase proves presence, never agreement — the lesson behind a doctrine block that
silently drifted across three surfaces while every criterion passed. Also: exceptions must live
in the block that gets _copied_, sweep for retired phrasing, and never make a stage read
canonical text out of `doc/plans/`.

→ [Doctrine & Acceptance](mistakes/doctrine-and-acceptance.md)

## Verification Harnesses and Stale Binaries (2026-07-28)

When every check in a suite fails at once, suspect the harness: a hardcoded `/tmp` redirect in a
read-only sandbox failed 13 criteria that were all actually fine. Also: the PATH binary does not
contain your plan's changes, and a silent review subagent is a failed delegation.

→ [Verification Harnesses](mistakes/verification-harness.md)

## Knowledge CLI and Filesystem Invariants (2026-07-28)

Invariants belong in the filesystem constructor, not the CLI handler that calls it; sibling-file
refreshes must happen outside the directory lock; `update` appends, so retries duplicate.

→ [Knowledge CLI Invariants](mistakes/knowledge-cli-invariants.md)

## Knowledge Base Drift — The Base Itself Goes Stale (2026-07-30)

Four repeatable failure modes found in this knowledge base: plan-authoring notes frozen as
architecture facts ("New, Stage 2+", `*** INSERT ... HERE ***`) after the feature shipped;
`[UPDATED]` sections that appended rather than replaced, leaving the stale copy on top;
invented CLI commands (`loom hooks`, `loom sandbox`, `loom verify`) inferred from module names;
and features documented that were never built (`.work/facts.toml`, `loom memory promote`).

→ [Knowledge Base Drift](mistakes/knowledge-base-drift.md)

## Stage Fragmentation: Compile-Order Is Not a Stage Boundary (2026-08-07)

**What happened:** the most common loom plan-authoring error is splitting ONE cohesive feature into
one stage per architectural layer (schema → runtime → doctrine → tests) because each layer imports
the one before it. This plan deliberately did not: the whole codex-implementer feature shipped as a
SINGLE standard stage between the knowledge/integration-verify bookends, with a foundation edit
followed by parallel subagents over disjoint files.

**Why:** "B imports A" is a COMPILE-ORDER dependency, and one stage resolves it for free by writing
A first — that is a foundation step, not a stage boundary. Only a MERGE-ORDER dependency is a real
boundary: the dependent work must run against _merged, gate-passed_ code. Each extra stage costs a
worktree, a session, a merge, and a FULL re-run of the acceptance gate.

**Prevention (detection rule):** if a plan has one stage per architectural layer of a single feature
and their `files:` sets are DISJOINT, it is fragmented — merge them. Disjoint file sets are evidence
_for_ merging (parallel subagents can own them), not against it. `/loom-plan-writer` now enforces
this: the Stage Necessity Test (`skills/loom-plan-writer/SKILL.md:388`) requires every non-bookend
stage to name which of Q1-Q4 forced it, the validation checklist re-checks that at `:825`, and
`:771` states outright that a compile-order dependency is a foundation step, not a stage split. A
stage that cannot cite a question is fragmentation.

**Fix:** one stage, foundation edit first, then fan out. Derive the foundation sweep from
`cargo build`, never from the plan's hand-counted file table — in execution the foundation step had
to fix three `Stage` struct literals, not the one the plan named, before fan-out could compile.

## A Scalar Config Field Silently Asserts "Exactly One" (2026-08-07)

**What happened:** the codex lane shipped as `implementer: claude | codex`, a single enum per stage.
That scalar quietly encoded a claim nobody had checked: that every subagent a stage spawns comes
from ONE lane. Real stages do not work that way — an implementation stage routinely wants codex for
routine file work, sonnet for tests, opus for the architectural call, and fable on a second failure.
The doctrine text then hardened the wrong model, telling stages "Codex REPLACES sonnet/haiku."

**Why:** the field was named for the decision it was born from ("which implementer do we use here?")
rather than for the state it represents ("which lanes may this stage draw from?"). A scalar can only
answer the first. Worse, the safety doctrine was gated on `implementer == Codex`, so the mixed case
was not merely inexpressible — it was UNSAFE: a `claude` stage that spawned one codex subagent got
none of codex's blast-radius rules (`.work/` symlink escape, hooks not seeing codex's own shell), and
nothing in the system would say so.

**Prevention (detection rule):** before shipping an enum-valued stage field, ask whether a single
stage could legitimately want two of its values AT ONCE. If yes, it is a set, not a scalar — ship the
list on day one. Then check every gate that reads it: a capability's safety doctrine must be gated on
MEMBERSHIP (`includes_x()`), never on equality with the preferred value. Equality gating is the bug
that hides, because the common cases still look right.

**Fix:** `Implementers`, a `#[serde(transparent)]` newtype over `Vec<Implementer>` with
`Default = [Claude]`, where membership licenses a lane and ORDER names the routine-work preference
(`preferred()`, `includes_codex()`, `is_mixed()`). Signal doctrine gates on `includes_codex()` and
tells the orchestrator to choose per subagent. Validation rejects the two shapes only a list admits:
empty, and a repeated lane (which would make the preference ambiguous). See
[Codex Plugin](architecture/codex-plugin.md) for the wiring.

## Codex Lane Rogue Wrapper (2026-08-07)

A `codex:codex-rescue` spawn received a codex prompt and implemented all 26 edits itself on
sonnet instead of forwarding — plugin agents' `tools:` field is ignored by design (user-scope
agents DO enforce it), `loom_is_subagent()` returns false for in-process subagents, and the
report was indistinguishable from a real forward. Now pinned by `hooks/codex-forward-guard.sh` + the `loom-codex-forwarder` agent + the
evidence-trailer acceptance rule. Full detail: [Codex Lane Rogue Wrapper](mistakes/codex-lane-rogue-wrapper.md).

## tmux Backend: Silent Spawn Failures and Layout Traps (2026-08-08) [DETAILED]

`tmux new-session` can print an error to stderr and **still exit 0**, so exit status alone is never
evidence a server exists — assert on the resource. The same topic collects the rest of the tmux lane's
traps: a spawn helper that cleaned up on only one of its error paths and so left a live agent behind
(two `claude` processes in one worktree), a retry that reused a session id and adopted the previous
attempt's PID from a non-truncated PID file, a sticky fallback marker written before proving the
fallback target was usable, the 104-byte `sun_path` budget, `kill-server` not unlinking its socket,
"cannot read the evidence" having to mean "do not destroy" in a reaping sweep, and the tmux 3.7b
layout/option facts (re-tile after _every_ split; `remain-on-exit` is a window option to set _before_
splitting; a single-string pane command runs under the user's login shell).

→ [tmux Backend](mistakes/tmux-backend.md)

## Tests That Cannot Fail (2026-08-08) [DETAILED]

Three tests in one plan asserted nothing that could fail — a test whose _name_ states a property is
not evidence the property is pinned. Two-part detection rule: (1) for each test ask "if I delete the
production line this covers, does it fail?" and actually delete it; (2) every negative assertion needs
a **positive control asserted at the same moment**. The topic also covers why a shell-injection PoC
must trigger via `$(...)` rather than a trailing `;` command (and must have balanced quotes), and how
macOS/APFS returning `read_dir` entries already sorted by filename silently satisfied a `sort_by`
assertion.

→ [Tests That Cannot Fail](mistakes/tests-that-cannot-fail.md)

## `rg -r` Is `--replace`, Not `--recursive` (2026-08-08)

`rg -rn PATTERN` is **not** `rg -n --recursive`. `rg` has no `-r` shorthand for recursive, so `-r`
consumes the `n` as a `--replace` value and every match prints as the literal `n` — output looks like
a mangled source file (e.g. `pub n(work_dir: &Path)`) rather than an error, so it reads as a corrupt
file. `rg` is recursive by default; never pass `-r` unless you mean `--replace`.

## `pub(crate)` Is Invisible to `tests/` (2026-08-08)

`tests/e2e/*` is an **external test crate**, so it can only reach `pub` items. A stage description that
names a helper both `pub(crate)` _and_ "the e2e test seam" is self-contradictory. Before marking a
helper `pub(crate)`, check whether anything under `tests/` calls it — `src/` unit tests can reach it,
integration targets cannot.

Related: when changing a fn signature under `src/commands/`, the call-site inventory must include the
sibling `#[cfg(test)]` module. `src/commands/init/tests.rs` held a fifth `cleanup_orphaned_sessions()`
call site beyond the four an `rg` for the primary feature symbol surfaced. `rg` the **exact fn name**
across `src/` _and_ `tests/` before writing a subagent's step list.

## Bash Tool CWD Persists — Never Bare-`cd` Into a Subdirectory Crate (2026-08-08)

**Two stages of one plan hit this.** The Bash tool's working directory persists across calls, and this
repo's crate root is the `loom/` subdirectory — so a single `cd loom` silently retargets every later
relative path, **including calls issued in the same parallel message block**.

**Why it keeps recurring:** the failure presents as `No such file or directory`, i.e. as a _missing
file_, not as a wrong-directory error. It reads as "that file doesn't exist" and sends you looking for
the wrong bug. The other tell is `git status` printing `loom/src/...` prefixes instead of `src/...`.

**Prevention:** never bare-`cd`. Prefix every command with its own `cd <dir> && ` so each call is
self-contained and order-independent.
