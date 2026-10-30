# Plan Server Delivery
>
> Delivery slips

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

## A Filtered Stage Gate Hid Three Failures Until integration-verify (2026-10-28)

**What happened:** `guidance-engine` passed its gate, then the integration-verify full suite found (1) `tests/planning_guidance_contracts.rs` spawning `CARGO_BIN_EXE_loom` without the `LOOM_*` scrub, which fails `tests/integration/binary_spawn_guard.rs`; (2) `web/src/api/fixtures/config.json` regenerated with `serde_json::to_string_pretty`, which `oxfmt --check` rejects because it collapses short arrays; (3) `web/src/components/settings-plan-section.test.ts`, which still expected two plan rows and the old caption after the four `[plan]` authoring keys made it four pairs.

**Why:** the stage's criteria ran filtered targets only (no `--test integration`, one vitest file, no `oxfmt`). A filtered web criterion misses sibling tests over the same fixture. The same spawn-guard failure is in "A New Test Target Spawned the Loom Binary and Failed the Spawn Guard Only in integration-verify".

**Prevention:** a test surface that spawns the loom binary uses the scrub idiom of `planning_check_cli_contracts.rs::check` and joins `SANCTIONED_SPAWNERS`. After `REGEN_FIXTURE=<path>`, run `web/node_modules/.bin/oxfmt` on the fixture (the `config_api` tests compare parsed JSON, so formatting is safe). When a registry key changes the fixture, run every vitest file importing it (`rg -l 'fixtures/config.json' web/src`). Changing the assertion lines of a test that exists at base raises a test-integrity event; dispute it with `loom stage dispute-integrity`. Proposal: [planning-guidance-followups](../concerns/planning-guidance-followups.md#proposals-for-hooks-and-plan-checks).

**Fix:** the caption reads "what loom plan's sessions run on", the test pins all four pairs and `web/dist` was rebuilt.

## Adding `[plan]` Keys Breaks Goldens and the Size Gate Outside the Owned Files (2026-10-28)

**What happened:** adding the four authoring keys broke `user_config/plan/tests.rs::plan_toml_lists_all_resolved_keys` and `user_config/tests.rs::value_of_has_an_arm_for_every_registered_key`, neither in the stage's owned list; the new match arms pushed `user_config/tests.rs` (already 400 lines) and a 409-line operator test over `tests/maintainability.rs`, which counts files over 400 lines and functions over 50 lines in `tests/` too.

**Prevention:** list both golden tests in the briefs that add registry keys, and give every forwarded unit the current line count of each file it extends.

## A Codex Forward Reported Success and Wrote No Files (2026-10-28)

**What happened:** forwarder unit `c6-opening` returned `state succeeded`, exit 0 and an evidence trailer, but wrote nothing: codex read the skills and the brief, then searched for the forwarder wrapper named in the prompt and stopped.

**Prevention:** after every forward, check `git status` for the unit's files, not the trailer. Keep forwarded prompts free of orchestration wording and end them with "write the files before finishing".

## Small Brief Slips in the Guidance Stages (2026-10-28)

- Task briefs name repo-relative paths while `working_dir` is `loom/`; a read from `loom/` fails and a file-tool path using `../` is rejected by the worktree hook. Resolve the path from the working directory and give file tools absolute worktree paths. Locate embedded asset roots through `loom/build/assets.rs` before searching their contents.
- A 120-character display limit was exceeded by an appended ellipsis. Count the truncation marker inside the budget (replace the last retained character) and pin it with a Unicode regression test.
- A mutation check (`rewrite_agent` returning the source frontmatter unchanged) must fail `planning_guidance_contracts::agent_frontmatter_follows_non_default_config`; keep that test, it is the only one that sees a non-default pair reach the agent files.

## Review-Loop Handlers Were Briefed Without Lock, Guard and Subscriber Facts (2026-10-29)

**What happened:** four defects in `plan_server/review/` came from briefs that stated the interface and left out how it behaves under concurrency. (1) `commands.rs` delivered a suggestion with `hub.send_to_holder(None, ..)`, a silent no-op, and `command_tests.rs` asserted the holder received nothing. (2) Routes held the `ctx.project()` read guard across git subprocesses (`hashes`, `compare`, `at_commit`, `built`). (3) `suggestion_answer` sent `suggestion_status` only to the sender while the web waited for it on the holder; `command_tests.rs:157` pinned the bug, so the fix changed that assertion on purpose. (4) The first holder-race fix said only "capture the holder under the box lock"; the re-review found that authoring changes state and broadcasts `Authoring(B)` before `follow_authoring` drains the box, so a suggestion sent between the two still put a card on the new holder, dropped right after, with `dropped` sent only to the sender.

**Why:** the briefs said plan 2 exposes only the holder's public id (so the codex unit passed `None` and encoded the gap in a test), said "use `project.root`" without the guard's lifetime, and did not trace the subscribers of the state change in the order they fire (`broadcast_people` subscribes before `review::install`, `routes.rs`).

**Prevention:** when a brief says a producer does not expose X, search the hub and authoring API for an alternative before accepting `None`; a test asserting that the main-path delivery did not happen is a red flag. A brief for a handler that spawns subprocesses says to copy data out of `ctx.project()` and drop the guard. A brief for a race fix lists every subscriber of the state change in fire order and requires the fix to cover each interleaving. A review that finds a fix touching a pinned assertion files the test-integrity dispute instead of reverting (ten such events were disputed in one filing).

**Fix:** `Authoring::holder_browser()`; `holder_snapshot` read under the box lock with the card and `sent` sent before release; `follow_authoring` sends each `dropped` status to the current holder as well; the answer status goes to the answering holder.

## Chat Units Shipped Review-Class and Size Defects (2026-10-29)

**What happened:** codex and fixer units passed their own checks and failed the full gate or the reviewers. Wave 3 shipped a permission card that stole composer focus (typing `y` approved), an unrendered delivery state, a `StartingBrief` that was never mounted, suggestions held in unmounted component state, and announcements that re-read a timer. Earlier waves added a static `@codemirror` import in `turn-footer.tsx` (the lazy-boundary test failed) and used jest-dom matchers that are not installed. `tests/plan_chat_operator.rs` came back at 497 lines with two functions over 50 lines; fix-round tests pushed `panel.test.tsx`, `start-chat.test.tsx` and `suggestion-cards.test.tsx` from 398, 386 and 382 lines to 483, 456 and 411; a round-2 Rust test of 59 lines failed `tests/maintainability.rs` (clippy, fmt and the scoped `--lib` run were green). A codex unit (`C8-offer-handlers`) again reported `succeeded` and wrote no files (see "A Codex Forward Reported Success and Wrote No Files").

**Why:** briefs gave no size budget, the single narrow check a worker may run does not include `--test maintainability` or the full vitest run, and UI units were not briefed with the section's accessibility rules or the rule that every export is mounted from `dock.tsx`.

**Prevention:** a brief that adds tests to an existing file names its current line count and sends new tests to a sibling file before 400; tell codex units the 400-line and 50-line limits apply to `tests/` files and to cap at 360 lines; Rust fixer briefs name the 50-line limit for test functions and add `--test maintainability` to their one check (never fix by adding a baseline line). Brief web units with "no `@codemirror` import outside the lazy loader; jest-dom is not installed", the section's accessibility rules and "every export is mounted from `dock.tsx`", run the full vitest after each wave, and have a reviewer check wiring before the gate. After every codex forward, confirm the unit's files exist; the trailer is not evidence. Two occurrences make this systemic: a `loom plan verify` or hook check that compares a forwarded unit's owned files with `git status` would end it ([concern](../concerns/plan-chat-followups.md#proposals)).

## Chat Web Logic Slips Found Late (2026-10-29)

- A sent chip found its session by turn id alone; turn ids repeat across the current and previous sessions. Match the turn timestamp and prefer the current turn collection.
- The first `PatchView` treated every repeated `+`/`-` prefix as a file header. Classify content relative to the first hunk and cover repeated prefixes.
- The start-race matcher covered a running chat and missed the authoring refusal "Another browser holds authoring". Trace both `take_authoring` and bridge start errors.
- `ChatAnnouncer` returning early with no chat satisfied `shell.test.tsx` ("renders the dock as nothing") and broke `announcer.test.tsx` ("with no panel mounted"): two tests owned by different units pinned opposite behavior. The test owner decides: the dock renders nothing without a chat session (`shell.test.tsx`), so the announcer tests mount with a session.
- Tests that failed to compile or run: `exact` passed to `getByRole` (role queries take the name string; this recurs from [Web Dashboard Server](web-dashboard-server.md)), fixtures built from guessed wire fields (build them from schema exports), a resolver returning `null` where `SentenceResolver` returns `undefined`, an unquoted optional shell glob that zsh rejects as unmatched (`rg --files` finds optional config paths), `timeout 600000ms` (GNU `timeout` takes `600s`), a nullable property used after narrowing only its local alias, and `Array.map` losing the nonempty tuple a zod `discriminatedUnion` needs.
- oxfmt rewrites the string escapes for U+202E and U+200B into the raw invisible characters; build them with `String.fromCodePoint`.

## A Codex Unit Reported Succeeded With No Files Written (2026-10-29)

**What happened:** the Codex unit `rebuild-engine-c3-wire` ended `outcome=succeeded`, exit 0, with no file written.

**Why:** the wrapper's injected rules told Codex to record a `loom memory` note; the note returned PENDING RELAY and Codex stopped to report it. The prompt also gave brief paths relative to the repository root while the Codex working directory is `loom/`.

**Prevention:** a Codex prompt states that the working directory is `loom/` and repository files are at `../`, that no `loom memory` command runs, and that a run with no files written is failed. After every forward, run `git status` for the unit's files before accepting `succeeded`. Resolve every brief path against the current directory before reading: the shell in a stage can start in the `loom` crate, not the worktree root.

**Fix:** re-forward the unit with those three lines and check the tree.

## A Fixer's Test Passed `cargo test` and Failed the Gate's Clippy (2026-10-29)

**What happened:** a fixer subagent added a scheduler test with a local `[&dyn Fn(&mut Scheduler, &Stub, Instant) -> &'static str; 3]` and ran only `cargo test`. The gate's `clippy --all-targets` failed on `type_complexity`; a plain `type X = dyn Fn(..)` alias then failed with E0597.

**Why:** the brief's one targeted check was `cargo test`. A trait object in a type alias defaults to `'static`.

**Prevention:** a Rust fixer brief that adds tests names `cargo clippy --lib --tests -- -D warnings` as its one check. Alias closures as `type X<'a> = dyn Fn(..) + 'a`.

**Fix:** the alias with the explicit lifetime, in `scheduler/tests_session.rs`.

## A Rust Stage Committed a Web Fixture Unformatted (2026-10-29)

**What happened:** `web/src/plan/api/fixtures/summary-panel.json`, written by the engine stage, was not formatted for `oxfmt` (the `failed` array spanned three lines), so `bun run check` (`format:check`) failed on the merged tree.

**Why:** the Rust stage's gates never run the web formatter.

**Prevention:** a Rust stage that writes a web fixture runs web's `oxfmt` on it, or its acceptance includes `bun run format:check`.

**Fix:** integration-verify ran `oxfmt` on the file.

## A Plan Assumed a Measurement Its Predecessor Had Deferred (2026-10-29)

**What happened:** the knowledge stage of plan 10 was told to record the operator-run rebuild's measured cost from a `rebuild-cost:` memory. No such memory exists: integration-verify could not run a live summary session in its sandbox (no headless Chrome, no live tmux) and deferred the run, so no cost was recorded.

**Why:** the stage description was written as if the predecessor's acceptance would produce the figure, and nothing checked that the figure had a producer that could run in the sandbox.

**Prevention:** a downstream stage that must record a measurement names the producing stage's acceptance command, and that command runs in the producer's sandbox. A distiller greps `loom memory show --all` for the exact tag before writing a measured fact, and records "not measured" with the deferral when the tag is absent; it never writes a figure it did not read.

**Fix:** `architecture/summary-store.md` (Cost Ledger and Estimates) states that no cost is measured.

## Web Test Traps in the Run and Pressure Dialogs

**What happened:** several web tests failed on first run: a deferred built with `Promise.withResolvers` (not in the web ES2023 library target); a role query for background content after reopening a modal (Radix hides it from accessibility queries while the modal is open); `getByText` across an `Untrusted` child span (Testing Library matches direct text nodes); a fixture of `waiting_for_input` where `StageStatus` serde requires `waiting-for-input`; a handwritten abbreviated Stage YAML without required fields. A patch written against a file's expected text also missed because the edit hook had reformatted it.

**Why:** each copied a helper or shape without checking the local contract.

**Prevention:** use an ES2023 deferred; assert a hidden background element by text absence; query the authoring text node and assert the container's `textContent`; build stage fixtures with `Stage::default()` and `serialize_stage_to_markdown`; re-read a file the edit hook may have reformatted before patching it. Web contract tests are one top-level `it()` with the full name (see [run-handoff](../architecture/run-handoff.md)).

# Planner UI Redesign Delivery
>
> Planner redesign delivery slips

Mistakes from the plan that moved the planner to features, kickoff, draft-all teams, a serif reading face and a redesigned shell (2026-10-29).

## Moving a Path Convention Left Hardcoded Homes in the Crate (2026-10-29)

**What happened:** feature-model moved plan homes from `doc/plans/` to `doc/plans/<feature>/`. Review round 1 found four leftovers: feature offers could not start sessions (the chat wire had no feature), the plan-session sandbox and kind hardcoded the standalone home so feature plans lacked deny-writes and could not be pressure-tested, the xref lifecycle-spelling map handled only `doc/plans`, and a metaplan entry naming a plan in another home produced a `WritePlan` offer that created a duplicate slug. `plan/inputs.rs` hardcoded `doc/plans/briefs/<slug>`, so briefs of a feature plan were not found by `loom plan verify`. migrate-repo-docs then moved brief bundles and broke `loom/tests/planning_guidance_contracts.rs`, which reads a brief path from outside the stage's declared files; its search for runtime readers of the moved docs used the pattern `doc/product/(index|section_` and missed `loom/src/planning/guidance/tests.rs`, which joins `../doc/product` and the file name in separate calls.

**Why:** each stage grepped its declared files, and a path convention is spelled in many places, some split across `.join` calls.

**Prevention:** when a path convention moves, `rg` the whole repository, tests included, for every spelling of the old one: `doc/plans/briefs`, `doc/plans/archive`, `doc/plans`, the bare directory (`"../doc/product"`, `doc/product")`) and `CARGO_MANIFEST_DIR` joins. Derive homes from `slug::plan_home` and never spell them.

**Fix:** `bundle_dir` takes `plan_home(plan).join("briefs").join(slug)`; the sandbox, kind and xref paths use the same helper.

## Green jsdom Tests Hid a Broken Browser (2026-10-29)

**What happened:** the integration-verify browser pass found three defects no stage's tests caught: opening Help crashed the planner ("You cannot render a `<Router>` inside another `<Router>`") because help scenes wrapped themselves in `MemoryRouter` inside the app router; concept mode was unreachable because web routes and keys were `/product` and `/engineering` while the server parses only `overview/<f>/<scope>`, `<f>/<scope>/<doc>` and `plans/<doc>` and the strict web schemas lacked the server's `feature` field; and `/fonts/commit-mono-cosmix/*.woff2` answered the app page because `router.rs` served only `/assets/`.

**Why:** the web concept fixtures were stale copies of the Rust wire fixtures (hello, checklist and file fixtures had the same drift earlier), so vitest stayed green against a wire the server no longer sent. Stage workers cannot run the app, and an installed `loom plan` serves its own embedded bundle, not the worktree build, so no stage could look.

**Prevention:** a web stage that changes routes or wire keys copies `loom/src/plan_server/wire/fixtures/*` over the web fixtures in the same change and gets a real-browser smoke criterion. A scene that mounts real planner components renders them without a nested router (`useInRouterContext`).

**Procedure that works in a no-network stage sandbox:** serve a shallow scratch clone of `doc/` (`git clone --depth 1 file://$PWD`, sparse `doc/`, `git checkout --detach` so `loom plan` does not refuse a `loom/<stage>` branch) with `LOOM_HOME=$TMPDIR/loomhome` and the worktree's `loom/target/debug/loom plan --port N --no-open`, then drive a headless Chromium (`playwright-core` from the offline bun cache, the browser revision its `browsers.json` pins) in the same Bash call, since loopback is per call. Open the token URL from line 2 of the server log first. The server is read-only (no Claude login), so no session can start.

## A Plan's "Spec Changes Before Running" Table Had No Owner (2026-10-29)

**What happened:** the plan listed spec rows to change before running; no stage owned them and no criterion checked them. integration-verify found no `doc/product/planner-ui` section mentioning features, standalone plans, `needs-feature` or draft-all, and section 04 (summary ids) missing from the table. It applied the table itself through one codex unit per section, each told to confirm every fact in code first.

**Prevention:** a plan whose spec changes precede running gets a stage, or a `loom plan check` rule, that owns them, with one `rg` criterion per row. Tests that quote spec sections by line range (`planning/guidance/tests.rs` quotes section 03 and section 09) move with the edits.

## Stage Criteria Ran Subsets, So Four Tests Failed at integration-verify (2026-10-29)

**What happened:** the full `cargo test` at integration-verify failed four tests no stage had run: `extract_prefixed_paths` dropped every word containing `|`, so a link in a compact table row stopped being a reference; a guidance test still expected "draft-all-* is not available yet"; `SKILLS` in `planning_guidance_contracts.rs` lacked the kickoff skill; `summary_scope_table` excluded `plans/unlisted` while section 04 gives every plan in its home a summary. Fix units then crossed two limits that only `loom/tests/maintainability.rs` checks, and only in the full run: `summaries/scheduler.rs` reached 401 lines after `cargo fmt`, and `summary_scope_table` 52 lines. `the_gate_reads_the_snapshot_not_the_plan` fails only inside the stage sandbox and passes in the host gate.

**Prevention:** briefs state the 50-line function limit and name the current length of any file near 400 lines (the gate counts test functions too); run the maintainability test locally before the gate; a brief for removing a symbol includes an `rg` for it under `loom/tests`.

## Worker Briefs Omitted Facts Only the Code Held (2026-10-29)

**What happened:** codex and sonnet units followed their briefs and still shipped defects the build or a reviewer caught.

- A brief pinned a join key without reading both producers: gap views matched a file-mode gap to its checklist item by exact title, but tree gap titles are bare vision row names while item text is `section NN: <name>`, so every gap view went empty; the worker's tests overwrote the item text to equal the title and hid it. Join on the document path (`ItemView.path`), never on display text.
- Kickoff and feature components read set-only atoms (`kickoffAtom`, `draftTeamAtom`, `treeAtom`) without scoping them to the session or loading the source. Brief every consumer of such an atom with its scoping key and who fills it.
- Help scenes rendered a hand-copied `RunStripSample` and a fake `RoundDiagram` start scene and left out `InvitationCard` and `RunStrip`, which were unexported or fetch on mount (`DashboardButton` POSTs `/api/run/dashboard`). A brief that says "render production components" names the exports and the fetch-on-mount traps and tells the worker to omit and report, never to substitute a lookalike. `PlanHeader` takes `sample` to skip `HeaderExtras`, which fetches `/api/run/state` and `/api/summaries/panel`.
- Unit tests selected `svg[viewBox=...]` and `.lucide-*`: jsdom does not match camel-case `viewBox` in selectors, and `ActivityGlyph` wraps the svg in a span. Assert through `getAttribute` and `data-kind`.
- After `delta_message` moved to `chat/team.rs` it was still called from `mod.rs`; `tests/planning_guidance_contracts/features.rs` asserted the removed `StartError::NotAvailableYet`; the scope loader accepted extra keys on the `Open` unit variant (serde `deny_unknown_fields` does not cover tagged unit variants).

## Orchestration Slips in the Planner Plan (2026-10-29)

**What happened:**

- A `loom-software-engineer` ran `git rm --cached` on a deleted file despite the no-git preamble, treating it as the deletion step. Briefs for deleting a file say to use shell `rm` only, and the main agent checks `git status` for staged entries before committing.
- `loom check` was run redirected into a file in the background, and the relay refused the ticket. Loom commands run in the foreground, unredirected; wait with `loom request status <id> --wait 90` as its own Bash call.
- A forward ended `succeeded` while its owned file had no diff (its last message was mid-investigation), and two forwards were moved to the background after 180 s because the forwarder's Bash call lacked the 600000 ms timeout. `git diff` every owned file after a `succeeded` forward; a doc unit's brief says the run must write, and caps the reading it needs.
- A `--worker claude:<id> --worker codex:<unit>` watch over the same units is refused; bind forwarder units with `--worker codex:<unit>` alone, and only after the forwarder's Bash call has started.

## The Redesign Shipped Without Its Executors Ever Seeing the Mockup (2026-10-30)

**What happened:** the planner-ui-redesign build looks nothing like the locked mockup (claude.ai artifact 2T7SurQrDQJhCX4yyKi9iu): the home has no Features heading, the feature cards are empty or show raw status text, the activity pill and avatars are missing, chat shows on the home screen, and the feature screen is a checklist instead of the reading workspace.

**Why:** the plan and briefs pointed to the design only through a claude.ai link ("the artifact linked under Draft status shows the target"). Every web stage ran on the codex lane (gpt-6.1-sol), which has no claude.ai session and could not open it. No copy of the mockup HTML or its screenshots went into the repo or the brief bundle. The plan text described components without layout, so roughly 1,500 lines of mockup became a few lines of prose per screen. No acceptance criterion rendered a page: the gates were typecheck, lint and jsdom vitest, and the "inspect at 390px and 1280px" step was an unchecked note.

**Prevention:** a plan with a visual target commits the mockup into its brief bundle (`mockup.html` plus one PNG per screen, as `briefs/web-terminal/web-terminal/mockup.html` does). Every worker brief names those files. Each UI stage gets a real-browser screenshot criterion compared against the PNGs. Visual and layout work goes to a Claude lane, never codex.

**Fix:** the repair plan (`feat-planner-12-ui-repair`) transcribed the mockup into its brief bundle (`doc/plans/feat-planner/briefs/feat-planner-12-ui-repair/`: structure, CSS values and copy per screen, because stage sessions cannot open claude.ai), ran the visual stages on Claude lanes only, gave every web stage a real-browser step (`scripts/plan-screens.sh`, which takes `--themes=<ledger|aubergine|pacific|graphite>,...`) and rebuilt every screen; integration-verify shot nine routes at 1440 and 390 in four themes (72 screenshots, no sideways scroll at 390). The shots ran in read-only mode (the sandbox has no Claude login), so sessions, the pressure dialog, run and done views and help were checked by code and tests, not screenshots. The slips of that plan: [Planner UI Repair](planner-ui-repair.md).
