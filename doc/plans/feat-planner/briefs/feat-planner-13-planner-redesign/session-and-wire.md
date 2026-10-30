# Stage session-and-wire

Read `common.md` first. Paths are repository-relative. Line numbers were read at commit 56a71845; find the symbol if they moved.

## W1: auto mode and blocked actions (sonnet)

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

In `bridge/driver.rs:135` add an arm that adds a notice item through `self.notice` (`driver.rs:347`, `NoticeKind` read from `plan_session/model/mod.rs`). The text is `Auto mode blocked <tool_name>: <reason>`. Read both fields as optional JSON strings from the spool payload. A missing or non-string `reason` gives `Auto mode blocked <tool_name>`, and a missing tool gives `Auto mode blocked an action`. Pass both values through the same sanitising path other server notices use.

Documented payload for the fixture:

```json
{"session_id":"abc123","transcript_path":"/x.jsonl","cwd":"/x","permission_mode":"auto","hook_event_name":"PermissionDenied","tool_name":"Bash","tool_input":{"command":"rm -rf /tmp/build","description":"Clean build directory"},"tool_use_id":"toolu_01ABC","reason":"[Irreversible Local Destruction]"}
```

**Tests to update:** `launch_tests.rs:121`, `sandbox/tests.rs:133`, `sandbox/tests_kinds_fixture.rs:60`, `sandbox/tests_assert.rs:91`, `hook_cli.rs` tests (`SPOOL` array), `spool_tests.rs:284-299`, `loom/tests/plan_session_bridge_core_contracts/support.rs:155`, `loom/tests/plan_session_sandbox_contracts.rs:175`. Keep each refusal test. One check: `cargo test --lib plan_session::`.

## W2: sessions cannot outlive their server (opus, effort xhigh)

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

**Wire it.** W1 owns `hook_cli.rs`, `launch/script.rs` and `launch_tests.rs` and makes these edits after W2 returns; W2 owns `owner_watch.rs` and `plan_session/mod.rs`.

- `hook_cli.rs::run` gains the arm `"OwnerWatch"`, which runs `owner_watch::run` for the session directory, reading the session record for the socket and runtime paths. It is not in `SPOOL_EVENTS`: Claude never calls it.
- `launch_script` (`launch/script.rs:43`) writes, before the `exec env -i` line: `<loom_bin> plan hook OwnerWatch <session dir> </dev/null >/dev/null 2>&1 &`, escaped with `shell_escape`. Read `plan_session::sandbox::hook_command` (`sandbox/mod.rs:113`) and reuse its argument order; the watch process is outside Claude's sandbox because the script is.
- Update `launch_tests.rs` literals, including `launch_script_drops_the_parent_tmux_environment` (`launch/script.rs:253`).
- Summary sessions get the watch too.

**Do not** deliver a closing message from the watch. With no server there is nobody to read it, and sessions record decisions as they are made (`doc/product/feat-planner/index.md`, Sessions start cold).

One check: `cargo test --lib plan_session::owner_watch`.

## W3: tests stay out of the operator's home and off the real claude (sonnet)

**The leak.**

- About 24 modules under `loom/src/plan_server/` call `PlanContext::new`, which resolves `server_runtime_paths` (`context.rs:96`) to the real `~/.loom/plan`. Only `chat/tests.rs` calls `with_runtime` (`context.rs:117`).
- `~/.loom/plan` holds 322 `.tmp*` project directories. Five of them held sessions whose `project_root` is a deleted temp dir.
- `summaries/routes/tests.rs:44` (`start`) serves a writable context whose installed facade is the real spawner, so a fixture with stale summaries starts a real `claude` that nothing ends. Confirm this path by reading `routes.rs:89`, `summaries/backend.rs` and `chat/mod.rs:109`. If your reading names a different test, fix that one and record the correction with `loom memory`.

**Fix at the shared owner.**

- Under `#[cfg(test)]`, when `LOOM_HOME` is unset, the runtime home resolved in `loom/src/planning/runtime.rs` is a per-process directory under `std::env::temp_dir()`. Tests that set `LOOM_HOME` keep working.
- Under `#[cfg(test)]`, `chat/mod.rs:109` and `summaries/spawner.rs` do not resolve the real `claude`. A start under `cfg(test)` with no explicit test binary fails with a clear error. `the_real_spawner_starts_no_session_once_the_server_is_stopping` (`routes/tests_installed.rs:50`) must still see its "stopping" error, so the stopping check comes first.

**The e2e fixtures.** `loom/tests/plan_session_bridge_e2e.rs` starts real sessions (three `loom plan: e2e` processes leaked). Give its fixture a `Drop` guard that runs `tmux -L <socket> kill-server` for every socket it started, so a panic or an early return tears them down. Apply the same to `loom/tests/plan_chat_operator.rs` and any other `#[ignore]` operator test that starts a real session (`rg -l 'find_claude_path|which::which\("claude"\)' loom/tests`).

One check: `cargo test --lib plan_server::summaries::routes`.

## W4: the wire: slug names, six steps, short labels, plan rows (sonnet)

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

Filled in `card_facts`, in metaplan order first, then unlisted plans of the feature. `state` uses the same string the file tree sends for plan state. `pressure_date` uses `files/plan_view.rs::latest_pressure_date`, the same function `files/tree.rs:91` calls: do not re-parse. `checklist_card.rs` must stay under 400 lines; put rows in `wire/checklist_rows.rs`.

**Fixtures.** Regenerate `wire/fixtures/*.json` through the Rust tests that write them. Update `loom/tests/plan_feature_card_contracts.rs`: `title_strips_vision_suffix` and `title_falls_back_to_metaplan_heading` have no behaviour left, so delete them. That raises `TI-decl-rust` and `TI-edit`, which the main agent disputes after the commit.

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
