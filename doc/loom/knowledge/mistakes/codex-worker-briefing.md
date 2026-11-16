# Codex Worker Briefing Gotchas

>

## A Bash Command Text Containing Both "loom" and Any "complete" Substring Gets Pinned, Even for Unrelated Commands

**What happened:** `loom-hooks/loom-control-complete.sh` (`PreToolUse:Bash`) blocked a `loom knowledge replace-section ...` call — nothing to do with completing a stage — because the heredoc content passed on stdin contained the word `Completed` (a `SessionExitReason` variant) and the command also invoked `loom`. The hook error was `LOOM_CONTROL_ERROR: completion must be one exact pinned command`.

**Why:** the hook matches on the RAW text of the whole Bash `tool_input.command` — any command matching `*loom*complete*` (case-insensitive) or a `<loom> stage <verb containing complete>` token is held to the exact pinned `loom stage complete <stage>` form, regardless of what the command actually does. A codex forwarding brief hit the same trap from the other side: a forwarder whose task text (passed through an unquoted heredoc) contained the word "complete" or the phrase "loom stage" was blocked before its job ever launched (`loom-hooks/loom-control-complete.sh::strip_heredoc_bodies`).

**Prevention:**

- When a Bash call needs to embed multi-line content that might contain the substring "complete" alongside the word "loom" (doc prose, knowledge bodies, code samples), write the content to a file first via the Write tool (which this hook does not gate) and pipe it in with `< file` instead of a heredoc in the same Bash command — the substring then never appears in the command text itself.
- Codex/forwarder briefs must avoid apostrophes (so the forwarder can single-quote the task as one shell argument) and must never contain the literal substring `complete` or the phrase `loom stage`, even in prose describing what the unit should NOT do.

## Rust Text Written by a Codex Unit Can Silently Break the Whole Crate Build

Three distinct compile-breaking patterns, all found only because the orchestrator compiled right after each unit (a codex unit's own check is compile-only or skipped — see [Owned Waits § Codex unit verification limit](../architecture/owned-waits.md)):

- **Un-escaped braces in `format!(concat!(...))`:** a regex quantifier like `[A-Za-z0-9._-]{1,64}` inside a `format!` string is parsed by rustc as a `{}` placeholder (`invalid format string ... expected }`). Briefs for doctrine/format! text must say to double every literal brace (`{{` `}}`).
- **Bare CLI placeholders in doc comments:** a doc comment containing `<agent-id>` or `<unit-id>` unescaped is an unclosed HTML tag under `rustdoc` with warnings denied — the crate's own gate (fmt/build/clippy/tests) can be green while the repo's pre-commit hook (which also runs rustdoc) still rejects the commit. Write CLI placeholders in doc comments inside backticks: `` `<agent-id>` ``.
- **Reuse means the exact import path, not a second `#[path]` inclusion:** briefing a unit to "reuse" an existing module by `#[path]`-including its file a second time loads that file as two separate modules, which `clippy -D warnings` rejects as `duplicate_mod`. A reuse brief must name the actual import path (`use crate::...`) and, if needed, say the owning file's item may be bumped to `pub(crate)`.

## Integration-Test Submodules Need `#[path]`, Not a Bare `mod x;`

`loom/tests/<target>.rs` is a cargo integration-test crate ROOT; a bare `mod support;` inside it resolves against `loom/tests/`, not `loom/tests/<target>/`, so rustc reports `E0583 file not found for module support` even though `loom/tests/<target>/support.rs` exists on disk. Submodules under a test target's own directory need `#[path = "<target>/<file>.rs"] mod support;` (or a `mod.rs`-style directory target). This applies to every `loom/tests/*.rs` integration target that grows helper submodules (fixtures, scenarios, support).

## Retiring an Evidence Format Requires Sweeping Test Fixtures, Not Just Production Readers

When a unit retires one evidence file/record shape (e.g. a per-agent `<agentId>.json` termination record replaced by a `lifecycle.jsonl` journal line), every test FIXTURE that wrote the old shape to force a scenario (not just production callers) must be converted — a fixture helper like `write_start_and_stop()` that still writes the retired file silently produces the OLD behavior (a stale-but-present record reads as fresh under a debounce window) while the test's assertion still passes, because the fixture's job was to construct that exact prior state. A compile-only check cannot catch this; only running the affected unit's own tests does. Sweep every test fixture that constructs the old format with `rg` before considering the retirement done.

## `2>/dev/null || true` on a jq Call Turns a Program Defect Into a Silent No-Match

A hook's jq filter with an unclosed `select(` is a jq syntax error (exit 3, "unexpected end of file"); piping it through `2>/dev/null || true` (common in hooks that treat "no match" as a normal, silent outcome) makes that syntax error indistinguishable from a legitimate empty result — every real event the filter should have matched silently skipped. **Prevention:** in hooks, check jq's exit status separately from an empty result; do not blanket-swallow jq's stderr/exit code when "no match" and "program broken" must be told apart.

## JSONL Fixtures Must End With a Trailing Newline

A reader that validates transcript/journal freshness by checking the last byte (to detect a torn, still-being-written line) rejects a JSONL fixture whose last row has no trailing `\n` as `Stale('transcript is growing or torn')` — even though the content is complete and correct. Any fixture-writing helper that joins rows with `\n` must also terminate the final row, not just separate them.

## `cargo fmt` Runs After a Codex Unit's Own Size Check, and Can Push a File or Function Over the Limit

A codex unit that checks its own file/function line counts BEFORE the orchestrator's `cargo fmt` pass can pass its own check and still land oversized: `cargo fmt` reflows hand-wrapped code (breaking long argument lists, chained calls, etc. onto more lines), so a file measured at 505 lines pre-fmt can land at 519 post-fmt — over a 514-line ceiling the unit never saw fail. **Prevention:** run `cargo fmt` BEFORE measuring any file or function against a size limit (in a brief or in the orchestrator's own post-unit check), and brief workers to keep at least a 10% margin under each limit, since fmt reliably grows hand-wrapped code, never shrinks it.

## `Stage::new` Derives a Timestamped Id — a Fixture Must Set `stage.id` Explicitly

`Stage::new(name, ..)` (`loom/src/models/stage/methods.rs::effective_subagent_timeout_secs`) derives `id` as `stage-<name>-<timestamp>`, never the bare `name` passed in. A fixture that calls `Stage::new("notes", None)` and then looks up behavior by `"notes"` (e.g. `handle_stage_completed("notes")`) silently misses: the stage saves as `01-stage-notes-<ts>.md`, session lookups keyed by the derived id return `None`, and the code path under test never runs even though the test may still pass on an unrelated assertion. Set `stage.id` explicitly after `Stage::new` in any fixture that needs a known, stable id.

## `loom subagents watch --worker codex:<unit>` Can Report "unknown" With Exit 0

Running `loom subagents watch --worker codex:<unit>` right after spawning
`loom-codex-forwarder` agents printed `unknown: worker set does not resolve to one Claude
parent UUID`, yet the Bash exit code was 0. Read the watch output itself, never trust the exit
code alone; fall back to Agent completion notifications plus the `LOOM-CODEX-EVIDENCE` trailer
when the watch cannot resolve the worker set.

## `erasableSyntaxOnly` Rejects Codex-Written TypeScript Parameter Properties

A codex (`gpt-5.6-terra`) unit wrote TypeScript constructor parameter properties (`readonly
title: string` in the constructor signature) in two `web/` test files. `web/tsconfig` enables
`erasableSyntaxOnly`, so `tsc -b` fails `TS1294` while `vitest` still passes -- esbuild strips
the construct at test time, so the break is invisible until the gate runs the typechecker.
Web briefs for codex units must state "no parameter properties, enums or namespaces --
erasableSyntaxOnly", and the inter-wave gate must run typecheck, not tests alone.

## Codex Units in `web/` Do Not Run `oxfmt`

7 of 12 files failed `format:check` after all codex units in a `web/` wave returned; codex
units do not run `oxfmt` on their own output. Either the orchestrator runs `bun run --cwd web
format` before the final gate, or each unit's brief must explicitly tell it to run `oxfmt` on
its own files before reporting back.

## A Forward Prompt That Quoted `git commit` Text Was Blocked Before Codex Ran (2026-10-06)

**What happened:** a codex forward prompt that quoted `git commit -m` and `git stripspace` was blocked by
`commit-filter.sh` before codex started.

**Why:** the hook scans the whole Bash command text of the forwarder, and the prompt is part of that text.
A stage session may not run `git commit` at all
([Daemon-Owned Commits](../architecture/daemon-owned-commits.md)).

**Prevention:** phrase codex prompts without literal `git commit` text and point to the brief for the git
specifics.

## A Codex Unit That Waited on a Parallel Worker's New Module Timed Out (2026-10-06)

**What happened:** a codex unit (the stage commit command, one file plus tests) polled for a parallel worker's
not-yet-written `git/stage_commit.rs` and hit the 540 s wrapper deadline, although it had already written both
of its files (178 and 225 lines) before the cancellation.

**Why:** the unit consumed a module another worker was still creating, and the wrapper's deadline counts the
waiting.

**Prevention:** forward a codex unit that consumes a parallel worker's new module only after that module
exists, or pin the interface inline in the brief so codex does not wait for it.

## The Codex Lane Fails Inside a Stage Sandbox With No Network (2026-10-15)

**What happened:** token-core and web-insights each forwarded a unit through `loom-codex-forwarder` and got `workspace routing discovery failed` (exit 125, no files written); both re-delegated the unit to `loom-software-engineer` (sonnet).

**Why:** the stage sandbox grants no network, which the wrapper needs for its workspace routing.

**Prevention:** when the signal's Sandbox section says "No network access", route terra-tier units to sonnet and luna-tier units to haiku from the start; a codex unit then costs a brief and a boot and returns nothing.

**Later observation (2026-10-29):** in the planner-ui-redesign integration-verify stage (also no network) the codex lane worked through the direct backend: forwards ended `LOOM-FORWARD-END outcome succeeded` and their edits landed, with denied `vnkt.org:443` and `developers.openai.com:443` connections logged by Codex's own connectors, harmless. The harness persists the wrapper's roughly 64 KB stdout and the forwarder sees only 2 KB, so it never sees the `LOOM-CODEX-EVIDENCE` trailer. Try one forward before routing a whole stage away from the lane, and verify every unit by `git diff` of its owned files, not by the forwarder's report.

## A Brief Wrote a Command Argument Inside an Inline Code Span (2026-10-20)

**What happened:** the bookends unit brief wrote the distill message as the word stage glued to a code span holding `{}`, and the codex unit dropped the space before the span. The sibling brief that set the placeholder `{distill_id}` apart by a space came out right.

**Why:** a code span that abuts prose reads as part of the word, so a worker copying the text loses the separator.

**Prevention:** put every literal a worker must reproduce exactly in its own fenced block or a span set apart by spaces, and name the placeholder (`{distill_id}`), never a bare `{}`.

## Repo-Relative Brief Paths in a Forward Prompt Left Workers Reading Nothing (2026-10-26)

**What happened:** wave-1 prompts for the checklist-check stage gave brief paths as `doc/plans/...`. A codex unit runs from `loom/`, failed to read them, ran `loom memory note`, received `PENDING RELAY` and stopped with exit 0 `succeeded` and no edits (two units). Two Claude workers made the same error, resolving brief paths against the crate directory.

**Why:** the prompt did not anchor paths; the brief paths are relative to the repository root, the worker's cwd is the crate.

**Prevention:** give every brief as an absolute path inside the worktree, say that task paths are repository-root relative, and forbid `loom` CLI commands in a codex prompt.

**Fix:** the units were re-forwarded with absolute paths.

## A `cd` Into a Brief Directory Persists Into the Forwarder Spawn (2026-10-28)

**What happened:** wave 2 of the concept-web stage spawned its codex forwarders while the main session's working directory was `doc/plans/briefs/feat-planner-05-concept-mode/`, left there by an earlier `cd`. Codex started in that directory, and one unit's direct supervisor ended inconsistent.

**Why:** the Bash tool carries `cd` from one call to the next, and a forwarder starts in the main session's current directory.

**Prevention:** `cd` to the worktree root in the same Bash call that precedes a forwarder spawn, and give every brief path as an absolute path inside the worktree. A codex unit that reads a stylesheet in a vitest test uses an `@/...css?raw` import (`web/src/plan/editor/theme.test.ts:3`); `readFileSync(new URL(..., import.meta.url))` fails under jsdom with "URL must be of scheme file".

**Fix:** the units were re-forwarded from the worktree root.

## A Move Brief That Did Not Name the Production Functions Turned One Into a Test Helper

**What happened:** a Codex unit moved the pressure models module into `loom/src/pressure/`. It made the production function `resolve_rounds` `#[cfg(test)]` and re-implemented the precedence chain inline in `execute`, so the rounds tests covered a test-only copy. Separately, units imported `current_branch` through the private `git::branch::operations` module, `PlanMessage` through `plan_server::wire`, and `WorkDir` through `crate::fs`, none of which re-export them.

**Why:** the brief said "move the tests with the code" without listing every function the caller still uses; units followed the module a brief or a neighbour named instead of the public facade.

**Prevention:** a move brief lists every function the call sites use and requires them to stay non-test; the reviewer diffs the moved file against HEAD. Import through the facade the compiler or an existing caller uses (`crate::git::current_branch`, `crate::plan_server::PlanMessage`, `crate::fs::work_dir::WorkDir`).

**Fix:** `resolve_rounds` is a plain `pub fn` in `pressure/models.rs` and `execute` calls it.

## Replacing a Placeholder With Delete-and-Add Patches Wasted Codex Calls

**What happened:** ten Codex units in one plan had `apply_patch` rejected before it wrote anything, because each replaced a stub file with a delete and an add for the same path.

**Why:** the patch tool forbids two operations on one target; the stage's file-ownership step created placeholder files the units then had to replace.

**Prevention:** brief "replace the placeholder with one Update File hunk" (and `Add File` only for new paths), or do not create placeholders. A rejected patch changes nothing, so the cost is a wasted call, not a wrong tree.

**Fix:** one `Update File` per placeholder.

## A Stalled Codex Thread Reads as Running (2026-10-30)

**What happened:** five Codex threads stopped at about 19:50 UTC with no error, and an interactive session reported "No active thread is available". The companion kept reporting each job as `running`, and its progress log stopped growing. One job was interrupted and still read `running`. The orchestrator waited with `status --wait` and learned of the stall only when the operator asked why it was slow.

**Why:** a new `codex exec` hung during session setup while the `vnkt` MCP server, which answers an unauthenticated request with 401, stayed enabled in `~/.codex/config.toml`. With only that server disabled, the same command replied in 3 s. Whether it also caused the threads that stalled mid-turn was not shown. A status wait reports job state and a blocked thread has a live process, so nothing changed state. `codex doctor` passed its connectivity checks, and Codex's log held no error-level entry.

**Prevention:** when waiting on Codex outside a stage's owned wait, also watch the rollout under `~/.codex/sessions`: no new event for 5 minutes during a turn is a stall, and a `turn_aborted` event is a dead job whatever its status says. On a hang, bisect with `codex exec --ignore-user-config`, then `-c mcp_servers.<name>.enabled=false` per server, with stdin closed.

**Fix:** `enabled = false` on `vnkt` and on `node_repl` (its binary is missing) in `~/.codex/config.toml`.

## A Codex Brief With `../` Paths Reported Success With Zero Edits

**What happened:** a doctrine unit's brief named its edit targets relative to `loom/`. Codex started in `loom/`, `loom-hooks/worktree-file-guard.sh` rejected every edit for its parent-directory components, and the unit still reported `succeeded` with no changes.

**Why:** the brief was written relative to the crate directory, and a forwarded job's success trailer says nothing about whether any file changed.

**Prevention:** a codex brief names an absolute path for every edit target. After every forward, run `git status --short` before trusting a `succeeded` trailer.

**Fix:** the unit was re-briefed with absolute paths.

## Brief Paths Are Not Repository Paths, and Briefs Can Contradict the Code

**What happened:** source outlines used `src/...` paths from a brief while the crate lives under `loom/src`; a brief asked for a zero digest while `store::create_invocation` validates the digest against the brief bytes (`sha256_hex(b"brief")` works); `common.md` spells `--unit-id` while the I2 brief named the clap field `unit`; the runtime brief and stage text asked for a `mod testkit;` line the freeze scaffold check refuses. In each case the worker followed the brief literally or guessed.

**Why:** briefs were written from the design, not from the final tree, and the contract freeze checks only the scaffold lines it lists.

**Prevention:** resolve a path with `loom map --find-all` before using it, read the validating function before choosing a fixture value, and when a brief and the code disagree record a `decision` memory naming both and follow the code. `loom-control-subagents.sh` accepts `--unit` as an alias of `--unit-id` for this reason.

## Plausible Helpers and Imports That Do Not Exist

**What happened:** a handler test called a nonexistent `mint_message_id` (message ids only need `validate_message_id`); a codex unit used `format!("{:x}", Sha256::digest(..))`, which does not compile with this `sha2` version (`crate::utils::sha256_hex` does); a sibling worker trimmed an import in `freeze_refusal.rs` and `freeze_refusal_tests.rs` lost `replay`, which it had received through `use super::*`; moving `contents.rs` to `build_settings_for_backend` left `sandbox::build_settings` with no non-test caller, so the re-export warned unused and the function warned dead.

**Why:** identifier helpers were assumed by analogy, and glob imports hide what a test depends on.

**Prevention:** confirm an identifier helper with `loom map --find-all` before using it; import test dependencies by name instead of `use super::*` where a sibling may edit the parent module; after moving the only caller of a function, re-export the replacement and mark the old one `#[cfg(test)]`.

## Counts and Display Text That Changed a Public Contract

**What happened:** a status JSON regression asserted 16 fields while the contract defines 15 (the text worker label was counted beside `unit_id`); `StoreError`'s `Display` prefixed `Corrupt` details and moved the invocation id away from the start of an `Unverified` cause, which `NativeDisposition::blocker` parses.

**Why:** an inferred count replaced the explicit contract, and generic error labelling conflicted with an exact public cause text.

**Prevention:** assert the explicit key set, not a count. Render an error whose text is parsed elsewhere verbatim, and pin its prefix in a test (`index_tests.rs`).

**Fix:** the `Corrupt` detail renders directly and the index tests assert the cause prefix.

## Edits Anchored to Repeated Fixture Text, and `rustfmt` Across `mod` Declarations

**What happened:** a cleanup guard attached to an existing test instead of the new native-worker test because the patch matched repeated fixture setup; a native gate test seeder assumed the state directory existed before the CLI commit fixture created it; a reducer patch assumed placeholder text it had not read (the source graph had no symbols for the one-line file); `rustfmt --edition 2024` on `supervisor/mod.rs` reformatted every declared submodule, including four tracked `commands/subagents` files, and re-running with 2021 did not revert them.

**Why:** edits were placed by pattern instead of by name, and `rustfmt` follows `mod` declarations.

**Prevention:** anchor an edit to the test function name and check every occurrence; create the state directory before seeding; read a placeholder file before replacing it; read the crate edition from `Cargo.toml` and run `rustfmt --check` on the exact entry files, remembering it recurses.

**Fix:** the four tracked files were restored by hand and `git diff` was clean.

## Verification Loops Hid the Tool They Called

**What happened:** a loop variable named `path` hid `rg` from the command, because `path` is tied to `PATH` in zsh. A Bash heredoc whose text held both the `stage` and `complete` tokens was refused by `loom-control-complete.sh` as an unpinned completion command.

**Why:** zsh special variables and a text-scanning hook.

**Prevention:** use task-specific loop variable names; write test files and scripts with the Write tool, not a heredoc ([Relay Kinds](../patterns/stage-daemon-channels.md#relay-kinds-and-their-hook-mapping)).

## `loom check` Keys on the Working Tree

**What happened:** editing files while a host `loom check integration-verify` was pending started a fresh job (criteria-done went from 1 to 0). Separately, `loom check native-consumers > file 2>&1` and a backgrounded `loom check integration-verify` were refused because the relay reads the `LOOM_RELAY_V1` line from foreground, unfiltered stdout.

**Why:** the gate result is bound to the tree it ran on, and CLAUDE.md's log-redirect advice does not apply to relayed `loom` commands.

**Prevention:** run the gate only after every fix worker has returned, in the foreground, unredirected, with a long Bash timeout, and rerun it to wait. This is a recurrence; the proposed hook is in [No Hook Refuses a Relayed `loom` Command Run in the Background or Through a Pipe](../concerns/code-quality-and-hook-debt.md#no-hook-refuses-a-relayed-loom-command-run-in-the-background-or-through-a-pipe).

## Sandboxed Subagents Cannot Prove Boundary or Socket Behaviour

**What happened:** in a subagent's Bash sandbox `UnixListener::bind` fails with `EPERM` (proxy server tests) and bubblewrap cannot create a namespace, so every `skip_unless(confined_commands_runnable())` boundary test skipped and about 58 unguarded `verify::criteria` confine tests failed regardless of the code. `stop_process_ends_a_sleeping_boundary_and_namespace_is_released` only proves anything on a host with a working pid namespace.

**Why:** the subagent sandbox is itself a bubblewrap sandbox.

**Prevention:** run boundary payload tests and `verify::criteria` from the stage main session or the host job, never from a sandboxed subagent, and read a skipped test as unproven.

## No Test Chained a Scripted Run to the Final Settled Result

**What happened:** `Succeeded`, `OwnershipReleased` and `result_in` were each covered under different settlement paths (`runtime_contract_tests.rs`, `supervisor/tests.rs`, `native_cli_view_tests.rs`), and no test chained a scripted `run_session` through `settle(Released)` to `subagents result`; integration-verify added `native_cli_e2e_tests.rs`.

**Why:** each stage tested its own seam against a hand-built record.

**Prevention:** when stages build consecutive seams of one lifecycle, the integration stage adds one test that drives the first seam's real output into the last seam's reader.

## Native Proxy Socket Dir Umask

**What happened:** the first native codex smoke run admitted its unit, then the supervisor failed before any thread with `proxy socket directory /tmp/loom-nw-<rand> has mode 775, expected 700`. The distill stage then recorded the 775 as unexplained, on the belief that tempfile creates directories 0700.

**Why:** `supervisor/mod.rs` created the directory with `tempfile::Builder::new().prefix("loom-nw-").tempdir()`. tempfile 3.27 creates a directory as 0777 less the process umask unless `permissions` is set, so umask 002 gave 0775 and `require_private_dir` refused it. The proxy tests chmod'ed their directories to 0700 by hand, so none exercised the creation path.

**Prevention:** pass the mode to the creating call whenever a later check demands an exact mode, and have the tests build their fixture through the production constructor. Read the dependency's source for its default mode; never recall it. **Fix:** `proxy::socket_dir` passes `Permissions::from_mode(SOCKET_DIR_MODE)`; the supervisor and every proxy test use it; `socket_dir_is_private_under_any_umask` fails on any umask other than 077 without the mode.

## Native Worker Temp Dir

**What happened:** a native Codex unit could not link a test binary ("could not create a temporary file on the read-only filesystem"). A first fix assumed the bubblewrap `/tmp` was read-only; it was already a private writable tmpfs. The real block was the turn policy (`protocol/wire.rs`) sending `excludeSlashTmp` and `excludeTmpdirEnvVar` true with `TMPDIR=/tmp`. Flipping both to false was then rejected by security review: with `/tmp` writable to sandboxed commands, a nested host TMPDIR let a command rename the bwrap-created ancestors of the proxy socket directory and plant its own `proxy.sock`, and the persistent Linked Cargo home under `/tmp` became plantable.

**Why:** the brief guessed the mount layer; Codex's own sandbox policy is a second layer that decides writability inside the boundary. Opening a whole shared parent (`/tmp`) exposes everything bound below it to rename-and-replace.

**Prevention:** check the Codex turn `sandboxPolicy` as well as the bwrap argv before changing writability. Grant sandboxed commands a dedicated empty directory, never a parent that holds other mounts, and keep sockets and launchers at fixed paths on the read-only root.

**Fix:** `TMPDIR=/tmp/loom-tmp` created 0700 on the private tmpfs, `excludeSlashTmp: true`, `excludeTmpdirEnvVar: false`, proxy directory bound read-only at `/loom-proxy` (`boundary/tmp_tests.rs` pins all of it).
