# Worker S: kickoff commands, size pinning, openings and the skill

Read `doc/plans/briefs/planner-kickoff-fixes/kickoff-fixes/common.md` first, then the pinned
surfaces in `bridge.md` (worker B: `Bridge::end_within`, `BridgeEvent::Ended.cause`) and
`delete.md` (worker D: `KickoffScope.size`, `SessionScope::kickoff(root, size)`,
`SessionScope::kickoff_resumed`, `ArmRefusal::WrongSize`, `kickoff_target`, `Draft.size`,
`Draft.finished`, `Seed.size`, `drafts::written`, `drafts::remove_written`). Code against those
signatures from the start; B and D write them in parallel with you. Do not edit their files.

Territory brief for the Claude unit `repro` (section 0) and four codex units: `openings`
(section 1), `wire` (the wire and fixture bullets of section 3), `size-pinning` (section 2 except
`StartDraft::of` and `continue_draft`, and the size tests of section 5) and `draft-commands`
(the rest of sections 2 to 5). Each unit owns only the files its unit brief lists.

## 0. Reproduce first

Before changing anything, write a failing test in the new module
`loom/src/plan_server/chat/tests_dispatch/set_aside.rs` that drives the real dispatch: start a kickoff (offer `kickoff`), send one user message, leave
the session in a running turn, send `set_aside_kickoff`, and assert the draft reads
`live: false` within 2 s of harness time. It must fail on the current tree (the draft stays live
until the closing write or the 600 s bound). If it does not fail, find the real cause of "Set it
aside does nothing" before going on: check `on_chat`'s holder rule (`dispatch.rs:274`, a tab that
is not the holder is refused and the notice shows only a status line), the `canAuthorAtom`
reason that disables the button (`web/src/plan/new/draft-view.tsx` `useDraftCommand`), and
whether the notice re-renders when `live` changes. Record the confirmed cause with
`loom memory note "set-aside cause: ..."` either way; fix the cause you found and keep the test.

## 1. The opening and the skill name the size

- `kickoff_opening` (`loom/src/planning/guidance/opening.rs:81`): step `Kickoff, feature` for
  `small == false`, `Kickoff, standalone plan` for `small == true`. Update
  `loom/src/planning/guidance/tests/drafting.rs` (`kickoff_brief_derives_step_and_skill_…`) and
  `loom/src/planning/guidance/opening/tests/drafting.rs` to the new strings.
- `loom/src/planning/guidance/start.rs:119`: the feature kickoff's session title becomes
  `New feature` (the plan one already reads `New standalone plan`). Update the tests in
  `loom/src/planning/guidance/start/tests/lookup.rs` that assert it.
- `loom/src/planning/guidance/continuation.rs` `DraftContinuation` gains
  `target: Option<&'a str>`. When it is set, the continuation text adds, after the name line:
  ``The confirmed document `<target>` exists and may be unfinished. Read it and finish it; do not
  start it again.``
- `plan-plugin/skills/kickoff/SKILL.md`, first section: replace "this skill came from the feature
  offer or from the standalone-plan offer" with a sentence that the opening line names the size,
  `Kickoff, feature` or `Kickoff, standalone plan`, that every `loom-kickoff` block carries that
  size, and that the app ignores any other size. Keep the rest of the skill as it is.

## 2. The size is the offer's

- `loom/src/plan_server/chat/team.rs` `prepare_scope`: write
  `SessionScope::kickoff(ctx.root(), size)` with `Feature` for offer `kickoff` and `Plan` for
  `kickoff-small`; when the `ChatOwner` carries a `resume` grant (below), write
  `SessionScope::kickoff_resumed(ctx.root(), grant.size, &grant.name)` instead.
  `kickoff_scopes` becomes `Mutex<HashMap<String, Option<KickoffSize>>>` (the size of a kickoff
  scope, `None` for other scopes); `is_kickoff` becomes `kickoff_size(...) -> Option<KickoffSize>`.
- `KickoffTracker::observe(&mut self, session, size: KickoffSize, markdown)`
  (`loom/src/plan_server/chat/kickoff.rs`) sets `shape.size = Some(size)` after `parse_shape`,
  before the equality test, so a block whose only difference is the size publishes nothing.
  `seed` does the same with the draft's size.
- `StartDraft::of` (`draft_cmd.rs`) seeds `Seed.size` from the offer. `continue_draft` picks the
  offer from `draft.size`, never from the shape.
- `confirm_kickoff` keeps its wire shape; `arm_kickoff` (worker D) refuses another size and the
  refusal text reaches the browser through the existing `Err` path.

## 3. Set aside, Stop now and the end cause

- `ChatCommand::SetAsideKickoff` (`loom/src/plan_server/wire/chat.rs`) gains `now: bool`;
  `ChatCommand::DiscardKickoff` gains `delete_files: bool`. Update
  `web/src/plan/api/fixtures/chat-commands.json` (both entries, `now: false`,
  `delete_files: true`) and the Rust tests in `loom/src/plan_server/wire/tests_chat.rs`.
- `loom/src/plan_server/wire/kickoff.rs`: `KickoffDraftView.size` becomes `KickoffSize`, and
  `KickoffDraftDetail` gains `written: Vec<String>` and `committed: bool`, exactly as pinned in
  `delete.md` section 3. Update that file's own tests and
  `web/src/plan/api/fixtures/kickoff-drafts.json` to match.
- `draft_cmd::set_aside(ctx, conn, session, now)`:
  - `now` or no `draft.target`: `bridge.end(id, EndMode::Teardown, EndCause::Holder)`.
  - otherwise: `bridge.interrupt(id)` (a "no turn is running" error is expected and ignored;
    log any other at debug), then
    `bridge.end_within(id, EndMode::FinalWrite, EndCause::Holder, SET_ASIDE_BOUND)` with
    `const SET_ASIDE_BOUND: Duration = Duration::from_secs(60);`.
  - A second call while the first is still ending is accepted (the bridge hardens a later
    Teardown); `on_chat`'s `CLOSING` text stays for a session that already ended.
- `chat/mod.rs` `BridgeEvent::Ended { id, kind, outcome, cause }` passes `cause` to
  `draft_cmd::ended(ctx, state, id, cause)`, which sets `live = false` and
  `finished = cause == Some(EndCause::Agent)`.

## 4. Continue and Delete

- `ChatOwner` (`dispatch.rs`) gains `resume: Option<KickoffGrant>`. `continue_draft` sets it when
  `draft.target` is set and `drafts::written(root, target)` lists at least one path; it then
  also passes `target` to `DraftContinuation`, and after the launch records the owner as
  `kickoff_cmd::record_owner` does (feature or plan by size), and seeds the tracker view with
  the same target (`KickoffTracker::confirm`) so the Write button reads written.
- `draft_cmd::discard(ctx, conn, session, delete_files)`:
  - a live draft is refused with the existing "Set it aside first";
  - `written` non-empty and `delete_files == false`: refuse
    "This draft wrote files; delete them too or keep the draft";
  - `delete_files == true` and a target: `drafts::remove_written(root, target)` first; on Err
    refuse with its message and keep the draft; on Ok remove the draft and publish;
  - no target: remove the draft as today.

## 5. Tests

Extend `loom/src/plan_server/chat/tests_dispatch/drafts.rs`, `tests_dispatch/kickoff.rs`,
`kickoff_tests.rs` and `tests_dispatch/draft_commands.rs`: a feature kickoff whose block says
`"size": "plan"` publishes `feature` and its draft view lists `feature`; `confirm_kickoff` with
`plan` on a feature kickoff is refused and arms nothing; set aside of a confirmed kickoff sends
an interrupt and ends within the 60 s bound; `now: true` during that end tears down at once; a
draft with an existing target whose session was set aside stays listed and Continue starts a
session whose scope allows writing the target; a done-fence end with the target present removes
the draft; discard with `delete_files: true` removes the plan and its bundle and nothing beside
them. Run `cargo test --lib plan_server::chat::`, `cargo test --lib plan_server::wire::` and
`cargo test --lib planning::guidance::` from `loom/`.

## Files

`loom/src/plan_server/chat/draft_cmd.rs` (224 lines: move the continue path into a new
`chat/draft_continue.rs` if it passes 400), `loom/src/plan_server/chat/draft_continue.rs`,
`loom/src/plan_server/chat/dispatch.rs`, `loom/src/plan_server/chat/mod.rs`,
`loom/src/plan_server/chat/team.rs`, `loom/src/plan_server/chat/kickoff.rs`,
`loom/src/plan_server/chat/kickoff_cmd.rs`, `loom/src/plan_server/chat/kickoff_tests.rs`,
`loom/src/plan_server/chat/tests_dispatch/drafts.rs`,
`loom/src/plan_server/chat/tests_dispatch/kickoff.rs`,
`loom/src/plan_server/chat/tests_dispatch/draft_commands.rs`,
`loom/src/plan_server/wire/chat.rs`, `loom/src/plan_server/wire/kickoff.rs`,
`loom/src/plan_server/wire/tests_chat.rs`, `loom/src/plan_server/wire/tests_schema.rs`, `loom/src/planning/guidance/opening.rs`,
`loom/src/planning/guidance/continuation.rs`, `loom/src/planning/guidance/start.rs`,
`loom/src/planning/guidance/tests/drafting.rs`,
`loom/src/planning/guidance/opening/tests/drafting.rs`,
`loom/src/planning/guidance/start/tests/lookup.rs`, `plan-plugin/skills/kickoff/SKILL.md`,
`web/src/plan/api/fixtures/chat-commands.json`, `web/src/plan/api/fixtures/kickoff-drafts.json`.
