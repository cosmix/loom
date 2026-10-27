# Plan Server Delivery

> Plan-server misses

## A New Test Target Spawned the Loom Binary and Failed the Spawn Guard Only in integration-verify (2026-10-27)

**What happened:** plan-server-core added `loom/tests/plan_server_cli.rs` and `plan_server_contracts.rs`, which spawn `CARGO_BIN_EXE_loom` and removed only `LOOM_PLAN_STATE` and `LOOM_PLAN_SESSION`; `binary_spawn_guard::only_sanctioned_spawners_spawn_the_loom_binary_directly` failed in the full suite. The contract client also inherited `PATH` and `HOME`, so `ReadOnly::detect()` ran the host's real `claude auth` probe (30 s timeout) and could race the test's 30 s deadline, and its reqwest client honoured proxy variables.

**Why:** the stage gate ran only the plan_server targets, never `--test integration`. A recurrence of [the planning_check contract](verification-v2-delivery.md#a-contract-that-spawns-the-loom-binary-failed-the-spawn-guard-only-in-integration-verify-2026-10-26) one day later.

**Prevention:** a test target that spawns the loom binary removes every `LOOM_*` variable (loop over `std::env::vars_os`, as `planning_check_cli_contracts.rs` does), sets a controlled `PATH`, calls `.no_proxy()` on an HTTP client, and joins `SANCTIONED_SPAWNERS`. Run `cargo test --test integration binary_spawn_guard` in the stage that adds the target; the gate proposal is in [concerns](../concerns/code-quality-and-hook-debt.md#the-binary-spawn-guard-runs-only-in-integration-verify).

**Fix:** integration-verify scrubbed the environment in both targets and sanctioned them.

## A Bundle Check Passed While plan.html Statically Pulled the Terminal Code (2026-10-27)

**What happened:** `web/src/plan/shell.tsx` imported `ShortcutGroups` from `components/help-dialog.tsx`, whose `help-visuals` chain reaches `stage-modal`, `terminal-view`, `history-view` and the emulator, so `plan.html`'s static closure carried the dashboard's terminal wrapper (a 903 KB shared `help-dialog` chunk). The bundle check did not flag it.

**Why:** `forbidChunks` globs match a chunk's `src`/`file`, and a static shared chunk has no `src`; `vite.config.ts` cannot remove a static import.

**Prevention:** `web/bundle-rules.json` entries use `forbidText` sentinels (strings only the forbidden code contains, such as `terminal-takeover-cue` and `Saved terminal history`) so a shared chunk is caught by content. Import shared UI from a light module (`ShortcutGroups` now lives in `components/help-shortcuts.tsx`); rebuild and commit `web/dist` after any such move.

**Fix:** moved `ShortcutGroups`, rewrote the rules, rebuilt the bundle (React got its own chunk).

## A Growth Fix Named One Caller and Missed Two (2026-10-27)

**What happened:** finding F-1-2 (cookie-less requests grew the presence map) was fixed with `peek_public_id` in `router::call` only; `server_info` and `require_authoring` still called `Presence::public_id` for a freshly minted browser id.

**Why:** the brief named `router.rs:187` rather than the function that inserts.

**Prevention:** for a growth or leak finding, `rg` the inserting function and brief every caller. The tree rule: only `public_id`, `set_name` and `opened` record, `name_of` does not, and a minted id never reaches them.

**Fix:** minted ids are refused 401 on holder routes and never recorded.

## Rewriting a Weak Test Raised a Test-Integrity Event (2026-10-27)

**What happened:** fixing "vacuous test" and "fabricated fixture" review findings by rewriting the old assertions raised a `TI-edit-<file>` event at `loom stage complete`, although the new assertions were stronger.

**Prevention:** add the stronger assertions beside the old ones when those are still true; otherwise plan one `loom stage dispute-integrity` filing that names the finding ids.

## Post-Gate Follow-Ups Went Past the Main Agent's Edit Limit (2026-10-27)

**What happened:** the integration-verify orchestrator made three direct edits (a Vite chunk, a bundle-check comment, splitting a 52-line test), 26 lines over three files, past the 20-line/2-file limit.

**Why:** each looked like a one-line follow-up to a gate result.

**Prevention:** batch post-gate follow-ups into one haiku engineer brief once the sum passes the limit.

## Codex Forwards Ran From the Crate Directory (2026-10-27)

**What happened:** wave-0 codex forwards ran with cwd `loom/` because the orchestrator's Bash cwd had drifted there, so the repo-root brief path did not resolve; a worker also looked for its brief under the crate directory.

**Prevention:** `cd` to the worktree root before every forward, and resolve repository-relative paths from the worktree root ([briefing rules](codex-worker-briefing.md)). Tell codex units not to run `loom memory`: with no relay hook in codex it prints PENDING RELAY and the unit stops on its stop-and-report instruction.

## Gotchas That Cost a Round (2026-10-27)

- `loom check` is a relayed host job: `run_in_background` hides its `LOOM_RELAY_V1` line, the request sits at pending relay and a later rerun reports `refused: never reached the daemon`. Three more stages hit it; the hook proposal is in [concerns](../concerns/code-quality-and-hook-debt.md#no-hook-refuses-a-relayed-loom-command-run-in-the-background-or-through-a-pipe). The relay hook also refuses lines from one Bash call that chains several `loom memory` commands with `&&`; use one command per call or a loop.
- A server started with `run_in_background` is unreachable from a later `curl` call (each sandboxed Bash call gets its own loopback). Start the server, wait for its URL line, probe and kill it in one script. `git commit` is hook-blocked even in a scratch repo; `git add` is enough for `loom plan`.
- `tests/maintainability.rs` fails the full suite when any function, test functions included, passes 50 lines; adding cases to an existing test can trip it, so put new cases in a new test function.
- `rustfmt` on `plan_server/mod.rs` formats every module that file declares; to format only owned files, pass leaf files, never a `mod.rs` that declares others.
- `subagent-verify-guard` blocks `cargo build --all-targets` and `cargo fmt` for subagents even when the brief asks; only one narrow `cargo test --test <target>` runs.
- JSX built eagerly inside a `vi.mock` factory fails with `Cannot access __vi_import_N__ before initialization`; build the element with an awaited `import('react')` `createElement`.
- `loom subagents watch --worker codex:<unit>` exits 5 ("expected exactly one Codex execution, found 0") when the forward ran in direct mode; read the forwarder's evidence trailer instead ([direct mode](codex-direct-mode-invisible.md)).
- Brief binding signatures can force non-import edits in tests (`Limits::new(usize)`, `Auth::new(token, cookie_name)`, an extra `&Limits` argument): brief the extraction with the test edits it implies, or those edits surprise the reviewer.
