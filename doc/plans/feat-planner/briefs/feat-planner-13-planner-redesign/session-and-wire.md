# Stage session-and-wire

Read `common.md` first. Paths are repository-relative. Line numbers were read at commit 56a71845; find the symbol if they moved.

## Lanes and units

**Operator's instruction: all backend implementation in this plan runs on `gpt-6.1-sol` at effort `high`, through `loom-codex-forwarder`. Fixes and verification do not.** W1 to W4 are the backend. W5 (web) and W6 (docs) stay on Claude.

| Work | Who |
| --- | --- |
| Every unit below | `loom-codex-forwarder`, `--model gpt-6.1-sol --effort high`, a stable `--unit-id`, foreground, explicit Bash timeout 600000 ms, forwarded from the worktree root and never from `loom/` |
| A unit that timed out (exit 124) | The same lane, re-split against its partial result, because it is still implementation. Never re-forwarded unchanged |
| A unit that came back wrong, a failing test, a failed gate, a review finding | Claude: a fresh `loom-software-engineer` (sonnet) with the evidence, then `loom-senior-software-engineer` (opus) if that attempt fails. Never back to codex |
| Contract tests | Loom's contract session |
| Compile checks after a wave, fixture regeneration by running the tests that write them, the gate, reviews | The main agent and `loom-code-reviewer` |

If the codex lane is unavailable when the stage runs, do not let the signal reroute this work to Claude. File `loom stage block session-and-wire` naming what is missing.

**Unit rules.**

- A unit is at most one file and its test, or the small pair named below, with at most three steps. Cut the brief from the sections below, anchored by symbol names, and paste each shared signature verbatim: the navigation kit answers from the base layer and cannot see an earlier unit's edit, so name the file and say what changed.
- Each unit's own proof is `cargo check --manifest-path loom/Cargo.toml --lib` (`--tests` for a test-only unit), and it reports only errors in its own files. Errors elsewhere are expected until the wave that fixes them. It runs no fixture-backed test.
- The unit's text says: no git, no writes under `.loom/`, no apostrophe, and it must not use the word "complete" or the phrase `loom stage`, or the forward is blocked.
- It names the target file's line count and the 400-line and 50-line limits with a 10% margin, because `cargo fmt` can push a file over.

After each wave:

1. Run `git status --short` and confirm every unit's files changed (`mistakes/plan-server-delivery.md`, "A Codex Forward Reported Success and Wrote No Files").
2. Check each worker section's proof only once its last unit is in. A "One check" line in a section below is the main agent's command, run after that point.

| Wave | Unit | Worker | Files | What |
| --- | --- | --- | --- | --- |
| 1 | A1 | W2 | `loom/src/plan_session/owner_watch.rs`, `loom/src/plan_session/mod.rs` | The watch, its trait, `read_holder_pid`, and its `pub mod` line |
| 1 | A2 | W1 | `loom/src/plan_session/sandbox/permissions.rs`, `loom/src/plan_session/sandbox/mod.rs` | `permission_mode`, `defaultMode`, the ask rules, the re-export |
| 1 | A3 | W4 | `loom/src/planning/checklist.rs`, `loom/src/planning/checklist/items.rs` | Delete `Step::Review` |
| 1 | A4 | W4 | `loom/src/planning/checklist/offers.rs` | The labels, and no primary when finished |
| 1 | A5 | W3 | `loom/src/planning/runtime.rs` | The `cfg(test)` home |
| 1 | A6 | W4 | `loom/src/plan_server/wire/checklist.rs`, `loom/src/plan_server/wire/checklist_rows.rs` | Drop `title`, add the row types, the step-name match |
| 2 | B1 | W1 | `loom/src/plan_session/launch/script.rs`, `loom/src/plan_session/launch_tests.rs` | The mode flag and the watch line |
| 2 | B2 | W1 | `loom/src/plan_session/sandbox/assert.rs`, `loom/src/plan_session/sandbox/tests_assert.rs` | The invariant by kind |
| 2 | B3 | W1 | `loom/src/plan_session/spool.rs`, `loom/src/plan_session/spool_tests.rs` | `PermissionDenied` in `SPOOL_EVENTS` |
| 2 | B4 | W1 | `loom/src/plan_session/hook_cli.rs` | The `PermissionDenied` and `OwnerWatch` arms |
| 2 | B5 | W1 | `loom/src/plan_session/bridge/driver.rs`, `loom/src/plan_session/bridge/denied.rs`, `loom/src/plan_session/bridge/mod.rs` | The notice |
| 2 | B6 | W4 | `loom/src/plan_server/wire/checklist_card.rs`, `loom/src/plan_server/wire/checklist_rows.rs` | No title, six-step facts, filling the rows |
| 3 | C1 | W3 | `loom/src/plan_server/chat/mod.rs`, `loom/src/plan_server/summaries/spawner.rs` | No real claude under `cfg(test)` |
| 3 | C2 | W4 | `loom/src/commands/plan/check.rs`, `loom/src/commands/plan/check/tests.rs` | Six steps |
| 3 | C3 | W4 | `loom/src/plan_server/wire/checklist_tests.rs`, `loom/src/plan_server/features.rs` | Expectations |
| 3 | C4 | W4 | `loom/src/planning/checklist/tests.rs`, `loom/src/planning/checklist/scope_tests.rs` | Expectations (`c.steps[6]` becomes `[5]`) |
| 3 | C5 | W4 | `loom/src/planning/checklist/offers_tests.rs`, `loom/src/planning/checklist/order_tests.rs` | Expectations |
| 3 | C6 | W4 | `loom/src/planning/guidance/opening.rs`, `loom/src/planning/guidance/opening/tests.rs`, `loom/src/planning/guidance/opening/tests/names.rs`, `loom/src/planning/guidance/opening/tests/features.rs` | The name and its expectations |
| 4 | D1 | W4 | `loom/src/planning/guidance/start/tests/labels.rs`, `loom/src/planning/guidance/tests/drafting.rs`, `loom/src/planning/guidance/tests/starting.rs` | Label expectations |
| 4 | D2 | W1 | `loom/src/plan_session/sandbox/tests.rs`, `loom/src/plan_session/sandbox/tests_kinds_fixture.rs` | Expectations |
| 4 | D3 | W1 | `loom/tests/plan_session_bridge_core_contracts/support.rs`, `loom/tests/plan_session_sandbox_contracts.rs` | Expectations |
| 4 | D4 | W4 | `loom/tests/plan_feature_card_contracts.rs`, `loom/tests/planning_checklist_contracts.rs`, `loom/tests/planning_guidance_contracts.rs` | Expectations, and the two deleted title tests |
| 4 | D5 | W3 | `loom/tests/plan_session_bridge_e2e.rs`, `loom/tests/plan_session_bridge_e2e/operator.rs`, `loom/tests/plan_session_bridge_e2e/diagnose.rs` | The `Drop` guard |
| 4 | D6 | W3 | `loom/tests/plan_chat_operator.rs`, `loom/tests/plan_handoff_operator/common.rs` | The `Drop` guard |

After wave 4:

1. Run `cargo fmt --all`.
2. Run the tests that regenerate `loom/src/plan_server/wire/fixtures/*.json`.
3. Run each section's check.
4. Spawn W5.

Any other file a unit finds it must change goes to the main agent for a Claude fix, and the unit reports it. W6 runs beside wave 1.

## W1: auto mode and blocked actions (codex sol/high)

Evidence from a real interactive session (Claude Code 2.1.312, Haiku 5.5, `--permission-mode auto --settings <file>`):

- The status line read "auto mode on", and every hook payload carried `"permission_mode":"auto"`.
- An `allow` rule ran without a prompt.
- An `ask` rule fired `PermissionRequest`, and the hook's deny reached the model.
- `PreToolUse` fired for every call.

The classifier block path was not exercised, because the model refused the risky command itself. The `PermissionDenied` shape below comes from `code.claude.com/docs/en/hooks`.

**Mode.**

- Add `pub fn permission_mode(kind: &SessionKind) -> &'static str` in `loom/src/plan_session/sandbox/permissions.rs`: `"auto"` for Authoring and Pressure, `"default"` for Summary.
- `launch/script.rs:29-30` calls it with `&spec.kind`.
- The builder at `permissions.rs:73` writes the same value to `defaultMode`.
- `sandbox/assert.rs:28` compares against `permission_mode(kind)` for the session's kind instead of the literal. Its refusal of `bypassPermissions` and `acceptEdits` stays.

**Explicit ask rules.** Kickoff's and Team's "asks the holder" today works only because those commands have no allow rule and `default` mode prompts for anything unlisted (`permissions.rs:83-111`). Under auto mode the classifier would decide them instead. Add these to the `ask` list, beside `WebFetch`:

| Scope | Ask rules |
| --- | --- |
| Open | none added |
| Team | `Bash(loom knowledge *)` |
| Kickoff | `Bash(loom knowledge *)`, `Bash(loom plan summary seal*)`, `Bash(loom plan summary brief*)`, `Bash(loom plan summary release*)` |

Do not use `Bash(loom plan summary *)` as an ask: ask outranks allow, so it would stop Kickoff's pre-allowed `loom plan summary status*`. The subcommands are the output of `loom plan summary --help` (seal, brief, status, release).

The pressure rules in `pressure()` keep their asks; read them and add any command that relied on the same absent-allow behaviour. The `PreToolUse` guard, OS denies and `.git` deny are untouched: the guard "denies or stays silent, never allow" (`doc/loom/knowledge/architecture.md`), and hooks run in auto mode.

**Blocked actions in the chat.** Add `PermissionDenied` to `SPOOL_EVENTS` (`plan_session/spool.rs:22`; its array length changes) and to the arm at `hook_cli.rs:21`, with the same "print nothing" behaviour, so no `retry` is ever requested.

In `bridge/driver.rs:135` add an arm that adds a notice item through `self.notice` (`driver.rs:347`, `NoticeKind` read from `plan_session/model/mod.rs`). `driver.rs` holds 389 lines, so the text building lives in a new `bridge/denied.rs` (`pub(super) fn denied_notice(payload: &serde_json::Value) -> String`) with its unit tests, and the arm is two lines. The text is `Auto mode blocked <tool_name>: <reason>`. Read both fields as optional JSON strings from the spool payload. A missing or non-string `reason` gives `Auto mode blocked <tool_name>`, and a missing tool gives `Auto mode blocked an action`. Pass both values through the same sanitising path other server notices use.

Documented payload for the fixture:

```json
{"session_id":"abc123","transcript_path":"/x.jsonl","cwd":"/x","permission_mode":"auto","hook_event_name":"PermissionDenied","tool_name":"Bash","tool_input":{"command":"rm -rf /tmp/build","description":"Clean build directory"},"tool_use_id":"toolu_01ABC","reason":"[Irreversible Local Destruction]"}
```

**Tests to update:** `launch_tests.rs:121`, `sandbox/tests.rs:133`, `sandbox/tests_kinds_fixture.rs:60`, `sandbox/tests_assert.rs:91`, `hook_cli.rs` tests (`SPOOL` array), `spool_tests.rs:284-299`, `loom/tests/plan_session_bridge_core_contracts/support.rs:155`, `loom/tests/plan_session_sandbox_contracts.rs:175`. Keep each refusal test. One check: `cargo test --lib plan_session::`.

## W2: sessions cannot outlive their server (codex sol/high)

**What was measured** with a fake `claude` in a scratch home:

- SIGTERM to the server tears the session and its tmux server down at once. Keep this.
- SIGKILL leaves claude and tmux running.
- A new server on the same project leaves that session alive; `bridge/recovery.rs:143` resumes it. Keep this.

A project never opened again leaves the session running forever. Eight such processes reached 5 to 49 hours on the operator's machine, and their tmux servers stayed after the processes were killed because of `remain-on-exit`.

**Build.** Add `loom/src/plan_session/owner_watch.rs`:

```rust
pub const OWNER_GRACE: Duration = Duration::from_secs(15 * 60);
pub const OWNER_POLL: Duration = Duration::from_secs(15);

/// What the watch asks of the world; a test supplies a fake.
pub trait OwnerProbe {
    /// A server holds the project: the lock record names a live pid.
    fn owner_alive(&mut self) -> bool;
    /// The session's tmux server still answers.
    fn session_alive(&mut self) -> bool;
    fn teardown(&mut self);
    fn sleep(&mut self, duration: Duration);
    fn now(&mut self) -> Instant;
}

/// Returns when the session is gone, or tears it down once no owner has been seen for `grace`.
pub fn run(probe: &mut dyn OwnerProbe, grace: Duration, poll: Duration);
```

Behaviour:

- Every owner sighting resets the timer, so a restarted server within the grace keeps the session and `recover` resumes it.
- If the session is gone, return at once.
- After teardown, return.

The production probe:

- `owner_alive` reads `RuntimePaths.lock` through `pub fn read_holder_pid(path: &Path) -> Option<u32>` in `owner_watch.rs`: open read-only with `O_NOFOLLOW`, read at most 4096 bytes, and parse the JSON `pid` that `plan_server/lock.rs` writes. It takes no flock, so the watch can never make a starting server refuse, and `plan_session` keeps not importing `plan_server`. A missing file, unparsable content or pid 0 means no owner. Otherwise it uses `kill(pid, 0)`, reading `EPERM` as alive. A reused pid only delays teardown, which is the safe direction.
- `teardown` calls the existing bounded `kill-server` path. `session_alive` uses the socket path check `recovery.rs:133` uses.
- Also tear down when the session record's identity is dead and no owner is alive: the leftover `remain-on-exit` server is the case that survived `killall claude`. `teardown` runs in that case through the same grace timer.

**Wire it.** W1's units B1 and B4 make these edits in wave 2, after A1 in wave 1; A1 owns `owner_watch.rs` and `plan_session/mod.rs`. Paste A1's signatures into B1 and B4.

- `hook_cli.rs::run` gains the arm `"OwnerWatch"`, which runs `owner_watch::run` for the session directory, reading the session record for the socket and runtime paths. It is not in `SPOOL_EVENTS`: Claude never calls it.
- `launch_script` (`launch/script.rs:43`) writes, before the `exec env -i` line: `<loom_bin> plan hook OwnerWatch <session dir> </dev/null >/dev/null 2>&1 &`, escaped with `shell_escape`. Read `plan_session::sandbox::hook_command` (`sandbox/mod.rs:113`) and reuse its argument order; the watch process is outside Claude's sandbox because the script is.
- Update `launch_tests.rs` literals, including `launch_script_drops_the_parent_tmux_environment` (`launch/script.rs:253`).
- Summary sessions get the watch too.

**Do not** deliver a closing message from the watch. With no server there is nobody to read it, and sessions record decisions as they are made (`doc/product/feat-planner/index.md`, Sessions start cold).

One check: `cargo test --lib plan_session::owner_watch`.

## W3: tests stay out of the operator's home and off the real claude (codex sol/high)

**The leak.**

- About 24 modules under `loom/src/plan_server/` call `PlanContext::new`, which resolves `server_runtime_paths` (`context.rs:96`) to the real `~/.loom/plan`. Only `chat/tests.rs` calls `with_runtime` (`context.rs:117`).
- `~/.loom/plan` holds 322 `.tmp*` project directories. Five of them held sessions whose `project_root` is a deleted temp dir.
- `summaries/routes/tests.rs:44` (`start`) serves a writable context whose installed facade is the real spawner, so a fixture with stale summaries starts a real `claude` that nothing ends. The main agent confirms this path before cutting A5 and C1, by reading `routes.rs:89`, `summaries/backend.rs` and `chat/mod.rs:109`. If that reading names a different test, the units fix that one and the main agent records the correction with `loom memory`.

**Fix at the shared owner.**

- Under `#[cfg(test)]`, when `LOOM_HOME` is unset, the runtime home resolved in `loom/src/planning/runtime.rs` is a per-process directory under `std::env::temp_dir()`. Tests that set `LOOM_HOME` keep working.
- Under `#[cfg(test)]`, `chat/mod.rs:109` and `summaries/spawner.rs` do not resolve the real `claude`. A start under `cfg(test)` with no explicit test binary fails with a clear error. `the_real_spawner_starts_no_session_once_the_server_is_stopping` (`routes/tests_installed.rs:50`) must still see its "stopping" error, so the stopping check comes first.

**The e2e fixtures.** `loom/tests/plan_session_bridge_e2e.rs` starts real sessions (three `loom plan: e2e` processes leaked). Give its fixture a `Drop` guard that runs `tmux -L <socket> kill-server` for every socket it started, so a panic or an early return tears them down. Apply the same to `loom/tests/plan_chat_operator.rs` and `loom/tests/plan_handoff_operator/common.rs`. The main agent runs `rg -l 'find_claude_path|which::which\("claude"\)' loom/tests` before cutting D5 and D6, and adds a unit for any other `#[ignore]` operator test that starts a real session.

One check: `cargo test --lib plan_server::summaries::routes`.

## W4: the wire: slug names, six steps, short labels, plan rows (codex sol/high)

**Remove the title.** Delete `ChecklistView.title` (`wire/checklist.rs:51`, the fallback at `:323`, `checklist_card.rs:23`, `document_title`, `strip_title`, the suffix constants). `StandalonePlanView.title` stays.

**Remove step 6.**

- Delete `Step::Review` (`planning/checklist.rs:60`, `:325`), `commands/plan/check.rs:78`, `wire/checklist.rs:371`, `checklist_card.rs:108`.
- Run becomes step 6, and `step_facts` matches 6.
- Keep `loom plan check` output and `planning_checklist_contracts` in step: rewrite expectations that name seven steps. Comments and doc strings saying "seven" change too (`checklist_card.rs:1`, `wire/checklist.rs`).
- Check `planning/guidance/` for step 6 text with `rg -n 'Review' loom/src/planning/guidance`. Retro guidance stays; only its step name changes.

**Labels** (`planning/checklist/offers.rs:152-171`):

| Offer | Label |
| --- | --- |
| `StartVision` | `Start the vision` |
| `FixVisionTable` | `Fix the section table` |
| `WriteSection` | `Write section NN` |
| `AddSectionToVision` | `Add section NN to the vision` |
| `StartMetaplan` | `Start the metaplan` |
| `WritePlan` | `Write plan <slug>` |
| `PressureTest` | `Pressure-test <slug>` |
| `StartRun` | `Start a run` |
| `RefineSpec` | `Compare spec with build` |
| `NewMetaplanRound` | `New metaplan round` |
| `DraftAllSections` | `Draft all with a team` |
| `DraftAllPlans` | `Draft all with a team` |
| `MoveIntoFeature` | unchanged |

`opening.rs:114` changes `done, Review what was built` to `done, Compare spec with build`.

**Finished features have no primary.** In `offers.rs:240`, `CurrentStep::ReadyAllDone` gives primary `None` and secondaries `[RefineSpec, NewMetaplanRound]`. Change `choose` (`offers.rs:227-246`) accordingly. `offers_tests.rs:137` changes.

**Plan rows.** `ChecklistView` gains:

```rust
pub metaplans: Vec<MetaplanRowView>,   // { path: String, title: String }
pub plan_rows: Vec<PlanRowView>,       // { slug, path, title: String, state: String, pressure_date: Option<String>, listed: bool }
```

Filled from `card_facts`, in metaplan order first, then unlisted plans of the feature. `state` uses the same string the file tree sends for plan state. `pressure_date` uses `files/plan_view.rs::latest_pressure_date`, the same function `files/tree.rs:91` calls: do not re-parse. `wire/checklist.rs` holds 378 lines and `checklist_card.rs` 364, so the row types and the code that fills them live in `wire/checklist_rows.rs`, with `checklist.rs` re-exporting the types. A6 writes the types and B6 the filling.

**Fixtures.** The main agent regenerates `wire/fixtures/*.json` after wave 4, through the Rust tests that write them. Update `loom/tests/plan_feature_card_contracts.rs`: `title_strips_vision_suffix` and `title_falls_back_to_metaplan_heading` have no behaviour left, so delete them. That raises `TI-decl-rust` and `TI-edit`, which the main agent disputes after the commit.

One check: `cargo test --lib plan_server::wire`.

## W5: web consumers of the wire (sonnet)

After W4. In `web/src/plan/`:

- `api/schema.ts`: drop `title`, add `metaplans` and `plan_rows` (strict zod, required). Update fixtures and kits.
- Every consumer of `checklist.title` shows `checklist.feature`: `feature/feature-route.tsx:46` (the screen is rebuilt in the next stage; keep it compiling), `home/feature-card.tsx:81`, `header/next-step.tsx:130`, `header/feature-switcher.tsx:54-126`, `concept/concept-route.tsx:79`, `chat/session-home.ts:83`, `chat/attention.ts:61-72`, `concept/page-heading.tsx:41`, `files/doc-heading-view.tsx:271`, `files/file-mode.tsx:36`, `files/heading-parts.tsx` (`featureCrumb`), `api/schema-run.ts`.
- Remove `STEP_COUNT` logic that assumes seven: `feature/steps.ts`, `help/sample.ts:16`, `home` progress strip (`home/progress-strip.test.tsx:39`), `help/legend*`, `help/topics.test.tsx:94,116,197`, `help/features-scene.test.tsx:137`, `ui/glyphs`.
- **Tab title:** on `/f/:feature/*` the tab reads `<slug> · loom plan`; elsewhere `<project> · loom plan`. Owner: `chat/attention.ts`, whose marker prefix `(!)` stays. Its tests at `attention.test.ts:110-133` change.
- `run/plan-run-actions.tsx:72` label follows the table.

Tests that pin titles change to slugs: `concept/concept-route.test.tsx:185`, `concept/concept-route-headers.test.tsx:261`, `home/feature-card.test.tsx:14`, `home/features.contracts.test.tsx:18,49`, `header/feature-switcher.test.tsx`. Keep every other assertion in those tests.

One check: `node_modules/.bin/vitest run src/plan/header src/plan/home`.

## W6: the spec and the metaplan (sonnet)

The spec states the tree's new truth, with no "was" narrative.

- `doc/product/feat-planner/index.md`: rewrite "The screen" and "What the first screen shows" to `common.md`, the Vision paragraph on concept mode's reach, the Problem step list (6 Review stays as a human act, with no app step), Principle 5, Goals, and "seven" everywhere (six app steps). Add a decision: "Permission mode (07, 12). Authoring and pressure sessions run in auto mode; a block shows in the chat as a notice. Summary sessions stay in Manual."
- `section_01.md`: six steps, the landing table, step 6 gone.
- `section_05.md`: how concept mode is reached, Key chips stay.
- `section_06.md`, `section_11.md`: "seven" mentions.
- `section_07.md:103-112`, the permission section, the spool table (`PermissionDenied`), and a paragraph on the owner watch (15-minute grace, restart keeps the session).
- `section_08.md`: Layout, Starting a chat, contextual prompts, viewed-document attachment (`chat-redesign.md`), no offers in the panel.
- `section_12.md:369,377,424,472,511-513`: the mode table per kind, ask rules per scope, and the statement that the classifier decides anything unlisted for authoring.
- `doc/plans/feat-planner/index.md`: add rows for plans 12 and 13 to the plan table, which fixes the "11/11 against 12 plans" mismatch.

Read each section around those lines before writing. Keep Markdown tables aligned and every fenced block tagged with a language.

One check: `loom plan check` from the repository root prints no new convention problem for `feat-planner`, apart from the `implements-duplicate` notes this plan's own `Implements:` line raises while it is unlocked (a redesign implements sections earlier plans also implemented, and locked plans do not raise it). The "not in a metaplan" lines for plans 12 and 13 must be gone.
