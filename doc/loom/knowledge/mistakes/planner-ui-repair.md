# Planner UI Repair

> Repair slips

## Hiding ChatPanel on the Home Route Dropped the Socket and Notifications

**What happened:** the shell stage hid `ChatPanel` on `/` (no chat on the home screen) and the route lost its chat socket, notifications and pill updates. `ChatPanel` alone called `useChatSocket` and `useAttention`; the contract mocked `ChatPanel`, so nothing failed.

**Why:** the brief named only `ChatPanel` and `ChatRegistrations`; nobody listed the hooks the hidden component owned.

**Prevention:** before conditionally unmounting a component, list the hooks it owns and move any app-wide one to a component that stays mounted. A test of the always-mounted owner must cover the moved hooks, not mock it.

**Fix:** `ChatRegistrations` takes `connect`, which mounts `ChatConnection` when the route has no panel ([shell](../architecture/planner-web-shell.md#header-and-dock)).

## A Conditional Hook Passed tsc, Lint and Tests

**What happened:** `useAtomValue(a) && useAtomValue(b)` was written twice in one stage, by the orchestrator in add-to-chat and by a worker in `concept-route.tsx` `KeyHints`. The second compiled, linted and passed tests, and crashes the reading route when authoring flips.

**Why:** `web/.oxlintrc.json` does not enable the hooks rule, and tests set `canAuthor` before mount, so the order never changed. Turning on `react/rules-of-hooks` through the react plugin also enables its other rules and raises about 80 warnings (refs, set-state-in-effect, immutability, exhaustive-deps) under `--deny-warnings`, so it was left as its own work ([concern](../concerns/planner-ui-repair-followups.md#untrusted-text-and-tests)).

**Prevention:** read atoms into constants first, then combine. One-off scan: `node_modules/.bin/oxlint --react-plugin -A all -D react/rules-of-hooks --format=unix src` (its two `useFakeClock` hits are test helpers).

## Rewriting Tests Under the Replaced-Copy Rule Lost Guards

**What happened:** the former next-step.test.tsx inverted the base guard that the card must not repeat the `write_plan` button label and dropped the `h-auto` and no-clamp assertions, while the integrity dispute named only `aria-describedby`. Features raised 11 test-integrity events in one stage, one per pre-existing test file the briefs ordered rewritten.

**Why:** the rewrite treated the guard as replaced copy when a distinct title existed. A brief that orders tests rewritten ("tests that pin replaced copy change in the same edit") always raises integrity events at completion.

**Prevention:** before rewriting a test under that rule, ask whether the guard still holds in the new design; keep the base assertion line verbatim when it does, and list every lost guard in the dispute. Plan one `dispute-integrity` filing per stage after the commit, naming each file.

## Parallel Web Stages Merged Into Red Tests That No Stage Ran

**What happened:** the shell stage moved Settings and the view tabs into the overflow menu below 1024px and 10 `concept-route` tests failed on the merged tree (jsdom answers `matchMedia` false). Later the chat-help stage removed the help "Chat" shortcuts group and the summary-engine stage added two plan settings rows, while `review/review.test.tsx` and `components/settings-model.test.ts` still pinned the old shape; the first full vitest run was integration-verify's `loom check`. First-round features workers also broke `help/features-scene.test.tsx` (selectors `:scope > h3`, `section[aria-labelledby]`), because the stage criteria never ran `src/plan/help`.

**Prevention:** a web stage that restyles a component a help scene or a full route mounts runs that scene's and route's tests in its proof command; a shell change reruns the tests that mount whole routes (`renderAt` in `concept-route-support` stubs the wide layout). The plan's last implementation stage runs the whole vitest suite once on the merged tree.

## A Merge Resolver Could Not Land a Merge That Edited an Accepted Test File

**What happened:** reading's merge resolver added two `stubWideHeader` lines to `concept-route.test.tsx`, a file holding an accepted test-integrity event. The gate refused the merge as worse than accepted (`verify/integrity/gate.rs` compares the whole file's sha256). A dispute needs `NeedsAdjudication`, which `MergeConflict` cannot reach (`models/stage/transitions.rs` allows only `Completed` and `Blocked`), so the only exit was `loom stage block`, which holds the stage for an operator.

**Why:** the resolver had no exit. The first correction note described a fix that was never committed (HEAD held the opt-in version), another case of memory claims ahead of the tree.

**Fix:** when a resolver's committed content fails `verify_resolved_completion`, the daemon stops it and requeues the stage for a Stage session (`merge_handler/resolver_handback.rs`, `MAX_MERGE_HANDBACKS` = 2, needs a merge commit since `completed_commit`); `MergeConflict` to `Queued` stays illegal, so `try_hand_back_merge` steps through `Blocked`. Every `Err` of `verify_resolved_completion` counts as a failure, transient ones included, on purpose.

**Prevention:** do not run the integrity scan inside an open merge (HEAD has not moved, so the target's own test edits read as lost assertions), and do not replace `accepted_sha256` with a subset check of lost lines (it measures against base only, so a stage could delete the replacement assertions a dispute relied on). The review gate's fingerprint hashes the merge base, so a resolved merge that moved the base is stale unless a review ran after it.

## The Codex Summary Backend Could Not Have Worked

**What happened:** session ids `codex-<millis>-<n>` fail `planning/runtime.rs::check_state`'s hex-and-dash test, so every `loom plan summary` call in a Codex job exited 2. It survived the build because no test ran an issued id through the CLI path and a real `codex exec` cannot run in a stage sandbox (no network; `bwrap: No permissions to create a new namespace`). Wave-1 codex forwards also returned 64 KB dumps of skill text with no readable evidence trailer, and one reported `failed` exit 1 though its edit was correct.

**Prevention:** a spawner that issues ids tests them against the consumer's validator (ids are now `c0de-` plus hex). Verify a codex unit by `git diff` and `cargo check`, not its report. The backend still has no live smoke run ([concern](../concerns/planner-ui-repair-followups.md#codex-summary-backend)).

## Briefs Named What the Screenshot Showed or What the Author Assumed

- **Wrong owner.** A fix brief told a worker to pad `.chat-state-line` for the "Viewer authoring" row, but that row is `RunHeader`'s own element (`chat/run-header.tsx`); named from a screenshot without reading the code. A fix brief names the owning component after reading it (`.chat-holder-row` shares the rule).
- **Rust struct change, no compile.** C1 added a field to the wire struct and missed the `StartRequest` literal in `tests/planning_guidance_operator.rs`, because no worker compiled Rust; the orchestrator's `cargo check --tests` caught E0063. A brief that changes a Rust struct names `cargo check --tests` as the worker's one check.
- **Ambiguous counter.** `card_facts` step 2 used `ProjectState.sections_written` (every written file) over the listed rows, so unlisted files inflated it (4/3), because the table said `<written>/<listed>` without defining written. Derive both counts from the same rows.
- **Prescribed `key={pageKey}`.** The orchestrator prescribed a React `key` on `DocumentKey` to reset its expansion; it remounted the key on navigation, dropped focus from an activated chip and reset `PendingApprovalNotice`. Trace the state a component holds before keying it. The summary header's `SourceActions` reused `useKeptDoc` across paths with no key, so Edit and Discuss acted on the previous page's document; key any `useKeptDoc` user by path and test through the real hook.

## A Negative Test Without a Positive Control Cannot Fail

**What happened:** a follow-up worker's guard that "a does nothing" never set `screenAtom`, so `sessionHereAtom` stayed true and the test could not fail.

**Prevention:** a negative test carries a positive control in the same test (`concept-route-guards.test.tsx` flips `screenAtom` and expects the attachment). A frozen contract that passes before implementation cannot fail either: reading's `shift-escape-on-cap-folds-all` passed in jsdom, which does not blur on removal, while Chromium dropped focus to the body ([reading column](../architecture/planner-web-shell.md#reading-column)); the adjudicator deleted the contract and the real defect was fixed with a direct focus move, checked in a real browser.

## Contract Tests Wrapped in a describe Matched No -t Filter

**What happened:** contract tests inside `describe("features repair")` selected nothing under vitest's `-t` with the plan's space-joined contract name (3 skipped).

**Why:** vitest names a test `describe > it`, so the space-joined name never matches.

**Prevention:** web contract tests are top-level `it()` calls whose names equal the contract string exactly.

## A One-Token Edit Glued Two Tailwind Classes

**What happened:** an Edit to `STOP_LOOK` (`chat/pressure/pressure-header.tsx`) whose old and new strings ended with a space lost the space, gluing `[&_button]:text-(--tone-blocked)` to the next class. Tailwind emitted neither and the Stop button lost its danger colour; tsc, oxlint, oxfmt and vitest stayed green and a reviewer found it in the built CSS.

**Prevention:** anchor an Edit on a whole class token, never a trailing space, and read the diff of a class-string edit. Related: the Edit tool and tool-call strings decode a typed backslash-u202E into the real U+202E; write bidi escapes through `String.fromCodePoint` or a doubled backslash and grep for the raw character in tests you add.

## Script and Tooling Slips

- **Silent exit 0.** `web/scripts/plan-screens.mjs` guarded `main()` with `import.meta.url === file://argv[1]`, which exits 0 without running on a path with spaces or a symlink. Call `main` unconditionally in CLI scripts.
- **Searches in guessed paths.** Two workers searched `loom/src/fs/user_config` and an assumed test path that did not exist. Locate with `rg --files` before narrowing a path.
- **Background `loom check`.** The first gate ran with `run_in_background`, so its `LOOM_RELAY_V1` line went to a task file the relay hook does not read (a recurrence of [the live-state entry](live-state-pollution.md); the hook proposal is in [concerns](../concerns/code-quality-and-hook-debt.md#no-hook-refuses-a-relayed-loom-command-run-in-the-background-or-through-a-pipe)). Build screenshots from a `git archive` export in scratch so the live worktree stays untouched while the host gate reads it. It recurred in the issue-fixes integration-verify stage: the request sat in "pending relay" and its host job came back "refused: the request never reached the daemon". Run `loom check` in the foreground and rerun the same command to keep waiting.
- **Chained `loom memory` commands.** The relay hook refuses a Bash call whose `loom memory` commands are joined with backslash-newline continuations or preceded by a shell variable assignment (`S=... &&`), then recovers those requests on the next relayed call, so a retry duplicates every entry. Chain `loom` commands on one line with `&&`, never retry a refused batch, and wait for the recovery.

## The Repair Passed Every Gate While the Feature Screen Stayed Unusable

**What happened:** plan 12 replaced the feature screen with a step status page. Vision and Metaplan show a path that is no link, Pressure test, Review and Run are blank, Conversation lands on Run, and a finished feature opens on the blank Run page. Concept mode works but only a header tab or a chip reaches it. The operator found all of this on first use, after 12 plans were marked done.

**Why:** the brief said to build to the mockup literally (`briefs/feat-planner-12-ui-repair/common.md:14-16`) and decision 6 overrode the spec's first-screen table (`doc/product/feat-planner/index.md:102-110`) without anyone noticing the conflict. Criteria checked structure, CSS and copy; none walked a user task.

**Prevention:** a UI plan's acceptance includes scripted journeys in a real browser against real documents: start a feature, review a finished one down to source, change a section by chat and by direct edit. A mockup that overrides the spec changes the spec in the same plan.

**Fix:** `PLAN-feat-planner-13-planner-redesign` rebuilt the feature screen as a layout route with a spine over the real documents ([Planner Feature Screens](../architecture/planner-feature-screens.md)). Its design source was a `DESIGN.md` set by a designer worker, not a locked mockup (plan 12's procedure in [Visual Stages](../conventions/plan-web.md#visual-stages-and-the-screenshot-script) is one option), and its driver was a journey script that walks user tasks in a real browser. The slips that plan still made are in [Planner Redesign](planner-redesign.md).

## A Pressed Rebuild Waited Behind an Open Question (2026-11-03)

**What happened:** issue #16 reported that a Rebuild press did not start a session and that the cost ledger logged every summary session as background. The transport (the bridge, the 200 ms tick, recovery) was the obvious suspect.

**Why:** a timeline rig (fake host, real bridge, driver at a 20 ms tick, facade) showed an idle press and a Rebuild-all answer reaching `host.spawn` at the next tick, so the transport was sound. The scheduler held the press: `try_start` returned while a question about a background batch was open, which contradicted the documented rule that Button and approved ids skip the limit; and `prune` dropped a pressed id whose summary was current when a batch arrived during a running session, so the press never started. The ledger rule required every id of a line to be pressed, and `running_triggers` mapped approved ids to background, so a Rebuild-all answer and a press joined by a background id both logged background.

**Prevention:** before blaming transport for a lost action, build the timeline and list every state that defers it; pin each with a named test that fails at base. Whoever writes "X always starts" checks the guards in `try_start` and `prune`, not only the happy path.

**Fix:** [Rebuild Scheduler](../architecture/summary-store.md#rebuild-scheduler) (press path, suspension guards, pruning) and [Cost Ledger](../architecture/summary-store.md#cost-ledger-and-estimates). One press during a running session still waits, by design.

## An after_stage Pattern Matched TypeScript Generics (2026-11-03)

**What happened:** the check `! rg -q "<RunStrip\b" src/plan`, meant to prove no JSX mounts the deleted component, also matched type arguments such as `atom<RunStrip | null>` in `run/use-run.ts` (outside the stage's files) and `Pick<RunStrip, ...>`, so it stayed red until another stage's file changed.

**Why:** `<Name\b` is the shape of a generic as well as of an element.

**Prevention:** anchor an absence check on an element to what only JSX has (`<RunStrip[ />]` followed by an attribute or `/>`), or alias the imported type, and run the pattern at plan-writing time with `rg` over the whole tree to see what it matches. A check that spans files a stage does not own is an ownership conflict.

## Claude Code Wrapped Delivered Prompts and Every Text Match Broke Silently (2026-11-03)

**What happened:** Claude Code began recording a delivered prompt in the transcript and in the `UserPromptSubmit` hook as `<pasted_content id="X">`, the text and `</pasted_content id="X">`. Every match on the delivered text compared against the wrapped form and stopped matching: a chat's opening appeared twice (the sent item and the transcript item, which never replaced it), and a message with an attached document showed the `<loom-attachments>` block as raw markup. No test failed and no log line appeared, because the fixtures carried the bare format.

**Why:** the parsers matched text Claude Code produces, and the fixtures were written by hand in the format that existed when the code was written. Nothing compared them with a real transcript after Claude Code changed.

**Prevention:** after a Claude Code upgrade, capture fixtures from real session transcripts and hook payloads and diff them against the fixtures the parsers use; test a match on Claude Code's output with a recorded wrapped case and a bare case. Unwrap only the exact whole-message shape and treat the tag id as untrusted input (`unwrap_pasted` bounds it to 32 ASCII alphanumerics, `_` and `-`). Unwrap before any length cap, or the close tag of a maximum-size delivery is lost.

**Fix:** `plan_session/attachments/pasted.rs`, the transcript mapper and the hook path ([Delivery](../architecture/planning-session-bridge.md#delivery)). Contracts: `wrapped_message_parses_typed_text_and_attachments`, `wrapped_opening_replaces_the_sent_item`, `wrapped_closing_keeps_its_origin` (`transcript/paste_contract_tests.rs`). A real New chat with no markup and a message with an attached document showing a chip are operator checks ([follow-ups](../concerns/planner-ui-repair-followups.md#planner-issue-fixes-unchecked-here)).
