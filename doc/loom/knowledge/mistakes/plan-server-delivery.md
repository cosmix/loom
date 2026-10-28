# Plan Server Delivery

>

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

## The Web Client Sent Repo-Relative Paths to Doc-Relative Routes (2026-10-27)

**What happened:** `web/src/plan/files/api.ts` put `doc/...` in the `path` query of `/api/doc`, `/api/diff/*` and `/api/save`, while the server reads `path` as `doc/`-relative (`check_relpath_syntax` joins `doc/`). In the real app every document open, diff and save answered 404. Both suites passed.

**Why:** server tests used doc-relative paths and web tests mocked `fetchDoc`, so no test joined the web client's URL builder to the real route.

**Prevention:** a web client test asserts the exact query string the route expects (path=a.md for doc/a.md), and integration-verify's functional run opens a document through the client's own URL builder. `docRel`/`repoRel` in `web/src/plan/files/address.ts` strip any leading `doc/`, so a file at doc/doc/x.md has no round-trip address; the contract pins that form, so a server-supplied repo-relative flag is the fix if it ever matters.

**Fix:** `api.ts` sends `docRel(path)`; the contract tests assert the query string.

## A Watcher-Fed Cache Loaded Before the Watcher Took Its Baseline (2026-10-27)

**What happened:** `loom plan` built the text, validation and search stores, then started the watcher, which drops its initial batch. An edit made while the stores loaded was never reported: on a 544-document tree a save within a second of the printed URL stayed invisible for minutes. The same tree took 40 to 53 s to print its URL because the synchronous startup validation ran `ProjectProfile::discover` (a full checkout walk, about 85 ms) twice per contract in `v2_lints/contracts.rs`, plus three git spawns per plan input.

**Why:** the load order was written for the small scratch repository (7 documents, under 1 s). The functional run on this repository's own `doc/` was the first with a large tree.

**Prevention:** any watcher-fed cache takes the watcher baseline before its first load (`plan_server::load`), and the startup path does no per-plan work before the URL prints. Time `loom plan` on a repository with hundreds of documents, not on a fixture. `Project::refresh` still re-derives every document per batch (0.33 s release, 4.5 s debug on 544 documents).

**Fix:** `load` takes `watch::baseline` first; validation runs on its own thread; the contracts lint discovers once per package dir per plan; plan inputs use three git calls per plan (0.67 s startup).

## Orchestration Slips in the File-Mode Plan (2026-10-27)

**What happened:** (1) Two stages ran the relayed check command with `run_in_background`; the relay hook never saw its `LOOM_RELAY_V1` line and the request was refused or left pending. (2) The subagent watch started right after spawning background codex forwarder agents exited 5 (`found 0` executions, or a worker set that does not resolve to one Claude parent UUID) because no codex job had launched yet. (3) A codex unit reported `succeeded` with exit 0 and wrote neither file. (4) Codex forwards run from the orchestrator's current directory; a `cd loom/` left codex probing for `doc/plans/briefs` paths. (5) The verify-split worker made `execute` call `read_plan`/`check_text` instead of `verify_path`, failing the stage's `verify_path\(` wiring check and tripping clippy `result_large_err`, because the brief left the early-exit path open and did not carry the wiring patterns. (6) A fix engineer added a 54-line test; only `cargo test --test maintainability` caught it. (7) file-server's contract spawned `CARGO_BIN_EXE_loom` without the `LOOM_*` scrub, caught only by integration-verify's `binary_spawn_guard`; a recurrence of [the plan-server-core entry](#a-new-test-target-spawned-the-loom-binary-and-failed-the-spawn-guard-only-in-integration-verify-2026-10-27). (8) TIMING instrumentation was removed with a line filter on `__t|TIMING` across `src/`, which also deleted real lines in 5 unrelated files. (9) `C="curl --noproxy * ..."` expanded unquoted globbed the `*`, so loopback calls went through the proxy and looked like sandbox denials.

**Why:** relayed commands need the foreground to emit their receipt; forwarders launch their codex job some time after the Agent call returns; a codex trailer is not proof of output; briefs omitted gates the stage enforces.

**Prevention:** relayed commands (check, commit, complete) run in the foreground only. Start the subagent watch after the forwarders have begun, or rely on their completion notifications. After every codex unit check that its files exist before accepting; keep the orchestrator's cwd at the worktree root before forwarding. Paste the stage's wiring patterns and `clippy -D warnings` into every worker brief, and say the 50-line function limit covers tests (move long literals into consts). Strip instrumentation only from the files you instrumented, with a unique marker, then check `git diff --stat`. Keep curl arguments in an array: `C=(curl --noproxy '*' ...)` and `"${C[@]}"`. A new `loom/tests/*.rs` that spawns the binary copies the `spawn_loom_plan` scrub and joins `SANCTIONED_SPAWNERS`; the gate proposal is in [concerns](../concerns/code-quality-and-hook-debt.md#the-binary-spawn-guard-runs-only-in-integration-verify).

**Fix:** each was corrected in place (foreground rerun, retried unit, wired `verify_path`, test shortened, five files restored with `git checkout`, scrub added to `file_mode_contracts.rs`).

## Web Test Tooling Hid or Created Defects in File Mode (2026-10-27)

**What happened:** (1) jsdom's `DOMException` is not `instanceof Error`, so a quota check ordered `instanceof Error` before the `QuotaExceededError` name and reported the store unavailable. (2) vitest returns an empty string for CSS `?raw` imports unless `test.css.include` matches them, so a theme-token check saw `''`. (3) `oxfmt` decodes character references in Markdown link destinations (`java&#x09;script:` becomes a tab), silently destroying entity-encoded XSS fixtures in `fixtures/*.md`. (4) URL pathname normalisation re-encoded decoded document names. (5) `Range.comparePoint` returns 0 for any point inside the range, so headings after the selection start counted as preceding. (6) `plan-view`'s `stage-graph.tsx` imported `buildEdges` from `components/graph/stage-graph`, pulling the dashboard stage modal and terminal into `plan.html` and failing `scripts/bundle-check.mjs`. (7) Radix Dialog handles Escape on a document capture listener, so a child's `stopPropagation` cannot keep it open. (8) Registering the Files tab rewrote plan 2's `tabs.test.ts` assertions that `PLAN_TABS` is empty and raised a test-integrity event.

**Prevention:** match errors by `name`, not `instanceof`; keep `css.include` for `?raw` CSS in `vite.config.ts`; keep entity-sensitive adversarial Markdown as strings in the test file, not in fixtures; normalise decoded path segments without reserialising them as a URL; collapse a cloned range to its start before comparing headings; code under `web/src/plan/` imports only leaf graph modules (`thread-edge`, `lib/graph`) and builds edges locally; the tree drawer disables Radix's Escape (`onEscapeKeyDown` preventDefault) and closes from the content's React `onKeyDown`; a wiring change that must rewrite an old test's assertion is disputed as a test-integrity event.

**Fix:** each was corrected in the stage that hit it.

## Functional Runs of the Planning Server Hit Three Sandbox Limits (2026-10-27)

**What happened:** an integration-verify functional run of `loom plan` inside a stage sandbox failed three ways: (1) the server writes `~/.loom/plan/<key>`, which is read-only there (`safe_fs: mkdirat failed on plan: Read-only file system`); (2) `claude auth status` reports not logged in, so the server starts read-only and every `POST /api/save` answers 403 `read_only` (`authoring.rs`); (3) a subagent cannot `git commit` or `git add -A` even in a scratch repository under `TMPDIR`, and Read/Write/Edit refuse paths outside the worktree.

**Why:** the sandbox confines `~/.loom`, the login probe has no credentials, and the subagent hooks apply everywhere.

**Prevention:** set `LOOM_HOME` (`user_config/mod.rs`) under `TMPDIR` for the run, put a stub `claude` on `PATH` that prints `{"loggedIn":true}`, and have the main agent make any committed baseline, or `git clone --depth 1 file://<worktree>` then `git branch -m main` (`loom plan` refuses `loom/*` branches). Subagents write `TMPDIR` files with Bash heredocs.

**Fix:** the run used all three; no code change.

## A Plan Artifact Glob Ending in `/**` Can Never Pass (2026-10-27)

**What happened:** the summary-store plan declared `artifacts` of `src/fs/work_dir/config_sections/plan_config/**` and
`src/commands/plan/summary/**`. `verify_artifacts` (`loom/src/verify/goal_backward/artifacts.rs:43`) fails them whatever
the stage writes, and `dispute-criteria` has no `artifacts` field to file the failure against.

**Why:** `glob` 0.3.3 treats a trailing `**` as "directories only": `glob("d1/**")` over a directory holding only `tests.rs`
returns nothing, and over one holding a subdirectory returns the directory, which `verify_artifacts` then fails to read as
a file.

**Prevention:** list artifacts as files or `dir/*.rs` patterns, never a trailing `/**`. A check in `loom plan verify` that
rejects a trailing `/**` in `artifacts` would end the recurrence.

**Fix:** none in the tree: `verify_artifacts` still reads a matched directory as a file, and the plan file still lists
both patterns.

## The Session Sandbox's `.mcp.json` Mount Made `loom check` Fail Every Time (2026-10-27)

**What happened:** a stage whose `working_dir` is `loom` got "the tree does not match the request" from every
`loom check`, whatever it changed.

**Why:** the session sandbox bind-mounts `/dev/null` at `loom/.mcp.json` (the working directory, not the repository root),
so the session's tree digest differs from the daemon's; the digest's `.mcp.json` mount-point exemption covers only the
repository root (mount behavior: `architecture/execution-containment.md`, "A Denied Missing Path Shows Up in the Session's
`git status`").

**Prevention:** when `loom check` reports a mismatch with no change between runs, look for a mount point under
`working_dir` before editing anything. Extending the exemption to `<working_dir>/.mcp.json` is the open fix.

**Fix:** none in the tree.

## A Parallel Wave Left the Library Not Compiling (2026-10-27)

**What happened:** the config wave added `read_plan_config` and `PlanConfig` but not the `pub use
config_sections::{read_plan_config, PlanConfig}` line in `loom/src/fs/work_dir.rs`. `planning/summary/config.rs` in the
next wave failed with E0432 and the lib did not compile; that wave verified its tests in a scratch copy with the line
added.

**Why:** the re-export file belonged to no worker's ownership table, so every brief assumed another had written it.

**Prevention:** a worker that adds a public item owns the re-export that makes it reachable, and the brief names that
file (the same rule as the shared-files entry in [Verification V2 Delivery](verification-v2-delivery.md)). After a wave,
the orchestrator runs `cargo check` before briefing the next one.

**Fix:** the re-export was added after the wave.

## Codex Forwards Failed on Cwd, Parallelism and Network (2026-10-27)

**What happened:** the codex forwarders for c6-docs found no repository-relative brief paths and recovered by searching;
c8 and c9 failed with "codex direct supervisor state was inconsistent" while four units ran in parallel, after writing
every file. The sandbox denied outbound network (the stage has none), and the `sandbox_violations` for the MCP hosts were
noise: c2-front succeeded in direct mode under the same denials.

**Why:** forwarders run from the orchestrator's cwd, not the worktree root. The parallelism was first blamed for the
supervisor failure; a later identical failure in `planning-sandbox` (2026-10-28, forwarder `a553c87ffd6ab3091`) showed the
real cause: codex printed over 64 KiB of command output in one `item.completed` event, `_codex_direct_events.py` marked
the stream invalid, the supervisor returned `unknown`/125 with ownership released, and `codex-forward.sh` accepted only
`unknown:125:true`. c8 and c9 were not re-examined, but nothing ties the failure to concurrency.

**Prevention:** spawn forwarders with the worktree root as cwd. After a wrapper exit 1, read the written files and run
the narrow test before deciding to resend: a failure naming the supervisor, with the files complete and tests green, is a
lost status, not lost work. Oversized `item.*` events are now skipped and `unknown:125` with either ownership value is an
accepted END outcome.

**Fix:** c8 and c9 output was accepted after `cargo test --lib` over their modules passed 170 of 170 and review covered
them.

## Web Gate Traps in a Stage That Touches `web/` (2026-10-27)

**What happened:** four separate gate failures: (1) `vitest -t` did not select a `describe` plus `it` name joined by
spaces, so a contract test was skipped; (2) a `bun run test` run failed `file-mode.test.tsx` "scrolls to the anchor"
once and passed three of three alone; (3) a criterion ending in `git diff --quiet -- dist` fails for an uncommitted
rebuild even when `diff -r` matches; (4) `bun run check` failed `oxfmt --check` on four token-strip files committed outside
a gated stage, and `format:check` stops the script before vitest, so the format failure also hid the test results.

**Prevention:** write a web contract test as ONE top-level `it()` carrying the whole contract title; treat a single
full-run failure that passes alone as a timing flake and say so; commit a rebuilt `dist/` before the criterion that
diffs it; run `oxfmt` on every file a commit touches. In a stage session the commit-filter hook blocks any command text
containing `git commit`, even in a scratch fixture repo, so prove a no-diff reseal with `git add` and
`git diff` between index and worktree.

## Moving Tests Into a New File Raises a Test-Integrity Event (2026-10-28)

**What happened:** shrinking an over-400-line test file by moving whole tests verbatim into a new test file raised a `TI-edit-<file>` event (assertion lines removed from a file that exists at base). The rule that a moved line does not count does not follow lines across files.

**Prevention:** to shrink a test file, extract non-assertion setup into helpers and leave the tests in place.
