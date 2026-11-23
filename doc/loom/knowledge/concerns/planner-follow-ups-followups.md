# Planner Follow Ups Followups

> Planner, chat, guidance, kickoff and typed-config open follow-ups

## Summary levels and fragments

Open items from the plan-summary levels, fragment merging and the reading column ([summary store](../architecture/summary-store.md#seal), [reading column](../architecture/planner-web-shell.md#reading-column)):

- `seal/levels.rs` lets two opening sentences open the same detail paragraph; when the two are not adjacent, opening both repeats the detail text. A seal rule or a prompt line would end it.
- `terminal_safe` runs where `levels.rs` builds the unknown-target refusal and again per message in `commands/plan/summary.rs` (the only terminal consumer); `links.rs` messages rely on the CLI alone. Idempotent, but the comment's "for every consumer" overstates it: pick one owner.
- `evaluate.rs` `own_current` evaluates a plan summary's detail sentences, and the main pass evaluates them again; negligible cost, the first results could be reused.
- `concept/sentence.tsx` draws each fragment member in its own span without `unicode-bidi: isolate` (as `IsolatedSentence` does), so one member's right-to-left text can reorder a neighbour's leading digits or punctuation.
- `SamePassageLine` (`concept/passage.tsx`) is a block `p`, so the chip, the reference and the end cap sit on separate lines where the brief asked for one; confirm in a screenshot.
- `column-model.test.ts` "one cited id under two citers" asserts A's `risks` passage and the absence of B's but nothing positive for B's own `d1` passages (`expect(keys).toContain` of B's `d1` passage key would pin it); no reading-order test has one id open under both parents with passages under each.
- The fold cap's keyboard focus ring (the round ring on `.concept-run-cap:focus-visible`) was never shot: the Help reading scene's caps do not take focus and the repository has no built summaries. It rests on the CSS rule and the reading-fixes contracts.

## Themes and contrast

- `ALL_LIGHT_VARIANTS` and `ALL_DARK_VARIANTS` (`aurora-ui/shared/atoms/theme.ts`) have no consumer, and the light `gray` and `cool` variants (`tokens-light.css`) are offered by no picker: delete them if nothing selects them.
- Control edges (`--border`, `--hairline`) measure about 1.3-1.5:1 against `--card` in every theme and the segmented control's chosen fill about 1.1:1, under 1.4.11's 3:1. A project-wide pattern older than this plan; the text colour and weight carry the state.
- `kind-control.css` at 390px: each kind segment has about 98px of content and "Standalone plan" at 13px/600 is about 100px, so it may clip inside the track; check the 390x844 shot.
- Two tone tokens keep hue 325 under Graphite ([dashboard](web-dashboard-latent-issues.md#dashboard-settings-themes-and-notifications)).
- `web/src/plan/summaries/summary-status.test.tsx` is 420 lines, over the 400-line limit; split it.
- `statistics.contracts.test.tsx` stubs `matchMedia` with `matches = query.includes('reduce')`, true for `(prefers-reduced-motion: no-preference)` too: a stub answering true to both reads as reduced motion, so the count-up animates only when no-preference matches and reduce does not.

## Hearth chat

- The client `userWrote` mirror (`chat/closing.ts`, used by `transcript.tsx` `ChatOutcome`) lacks the server's recovered-session exception (`plan_session/bridge/ending.rs` `plan_end`): a recovered session shows no WrapUpCard until its closing turn arrives. A sound fix adds a `recovered` field to the session view (`plan_session/model`, `schema-chat.ts`, the chat fixtures and help scenes); matching the display name "recovered <kind> session" was tried and reverted because `set_name` can change it.
- The fallback `CLOSING` text (`plan_server/chat/guidance.rs`) does not ask for the `loom-closing` block, and the `(None, None)` opening arm is kept only for test callers; `start()` can no longer reach it.
- `closing.ts` throws the whole block away for one entry over 500 characters, more than 50 entries or a missing `questions` key, so every decision disappears from the card. `.default([])` on each key and `slice` instead of refusal would keep the rest.
- `offer-flow.ts` `confirmEndFirst` mentions open questions only; the closing write records settled decisions too.
- The composer placeholder is inconsistent: "Ask about <name>…" against "Ask about this feature" (`chat/composer.tsx`).
- `tool-group.ts` `editedPath` ignores `MultiEdit` while `closing.ts` `WRITE_TOOLS` counts it, so a `MultiEdit`-only run reads "edited 0 files".
- `chat/polish.contracts.test.tsx` (frozen) still mocks an `EndChatMenuItem` export that `end-chat.tsx` no longer has.

## Spine and pages

- `spine-model.ts` `REVIEW_FILE` (`/^REVIEW-/`) is looser than the server's RunReview rule (`REVIEW-<plan slug>` directly in the plan home or `archive/`): a stray file named like `REVIEW-<anything>` shows as Run in the spine while the server classes it Other.
- `web/src/plan/api/schema.ts` makes `built` optional in the strict checklist schema (the brief chose it for hand-built test literals), so a server checklist without it is accepted; a fixture helper would let it be required.

## Run review

- `fs/plan_review/sections.rs`: a deferred `spec-conformance:` decision appears in both Spec Conformance and Open Findings, so the findings import also files it in the ledger as an open finding.
- `orchestrator/signals/generate/product_sections.rs` has no signal-level test of the `Implements: none` case, and no assertion through `assemble_signal`, so removing the `generate.rs` call fails no test (the plan's wiring pattern `product_sections::append_product_sections(` catches it at `loom stage complete`).

## Requirement walk: not checked here

The integration-verify walk of the planner-follow-ups requirements (R1-R13) found every requirement done in code and by contracts, except:

- Knowledge rewrites (R6) were left to knowledge-distill and are applied in this tree.
- Reading-size sizes and the specimen were checked by screenshot; the Settings **statistics block** could not be shot in a stage sandbox (it shows only "Claude Code is not logged in").
- A real summary rebuild (R8) and a real run review with `spec-conformance:` lines (R4) were not run: the stage sandbox has no Claude login and the repository holds no built summaries.
- Journey J2 fails in a stage with "no openable sentence on the product page" because `doc/loom/summaries/` is not in the tree; J3 fails on the known `/tmp` plan-lock 500 (`/api/save`); J4, J5 and J8 need tmux. All run by an operator outside a stage.
- `feat-planner` lists as "no plans yet" in a clone because its `DONE-` plans are gitignored.

## Operator checks after the run

Rebuild the summaries and open a plan summary (opening, detail, source link, fragment at rest); confirm New chat spends no tokens before the first message and a real ending shows the designed card; start a follow-up from `/new` and confirm the entry appended under `## Follow-ups` in `index.md`; run a plan whose `Implements:` names sections and read the spec-conformance lines in its run review; open the dashboard theme picker in Aubergine and Graphite; open the Settings statistics block with a Claude login.

## Open Items After the Trace Screens Redesign

Reviewer suggestions not applied and questions left open when `PLAN-trace-screens-redesign` finished, each with the file that owns it. See [Planning Trace browser screens](../architecture/planning-trace.md#browser).

### Behaviour and accessibility

- `web/src/plan/concept/page-heading.tsx`: the muted document number in the title is `aria-hidden`, so heading navigation omits "04"; the breadcrumb still carries it.
- `web/src/plan/trace/coverage-page.tsx`: "No heading matches this filter." has no status region, so a filter that empties the list is announced only through the pressed tile's description.
- `coverage-page.tsx` and `findings-page.tsx`: an addressed row or finding is scrolled into view but not focused, so keyboard order after a coverage chip or Covered-by link starts at the top.
- `findings-cards.tsx`: `onFold` only sets mode `fold`; with `aria-expanded` true a second press does nothing. It should close the picker.
- `maps-findings.css`: a decided finding's status is one ellipsised line, whole only in its `title`; a long dismissal reason is unreadable by touch or keyboard at 390px.
- `trace-settle.css`: `overflow-y: auto` on the popover surface would keep the actions reachable when the popover is clipped.
- `settle-confirm.tsx`: the key `kind:index:quote` repeats for two My calls with the same quote; add the array position.
- `coverage-model.ts`: a stage cover without `stage` falls back to `cover.anchor` (`stage:build`) and `stageHref` adds a second `stage:` prefix; strip it in the fallback.
- `coverage-rows.tsx`: linked phrasing draws `SentenceText` without `IsolatedSentence` for the Entry title in an open row.
- `loom/src/planning/trace/build/docs.rs`: the server counts `(my call` inside code fences while `settlePoints` skips fences, so a fenced marker gives a count with no quote. Align the two rules ([settle control](../architecture/planning-trace.md#browser)).
- `page-heading.tsx`: a feature plan page with `plan_state` set but no `path` shows no pressure date and no chip; the server sets `path` for plan pages, so this is theoretical.
- `concept/passage.tsx`: a Ledger screenshot of a changed document showed a small dashed circle after every reading sentence, while the design says reading text carries no marks at rest. The Firefox walk found none at rest; which component drew it is unconfirmed.

### Comments and design record

- `coverage-rows.tsx` says the toggle is a row's only control, but the Covered-by cell holds plan and stage links that are tab stops. Its header comment (and `findings-model.ts`) still says every string is drawn with `Untrusted`; entries use `SentenceText`.
- `ui/doc-heading.css`: a comment describes a separate settle group and a "settle pill"; `page-heading.tsx` puts the settle control in one group with Edit, Discuss and Open all.
- `ui/DESIGN.md`: the line that the menu "flips above only when less than 18rem is free below" holds only for content taller than 18rem; the Pressure-test exception names the plan's state row, but in file mode it sits on the status card's pressure row; the Coverage tile's inset focus ring (`maps.css`) is not among the displaced rings the design names.

### Tests

- `settle-flow.test.tsx`: add a second Mark settled after Not yet, asserting a second confirm-less post and a second dialog.

### Open questions

- Should the spine's group counts follow the design ("n of m settled", "n done", "n running") instead of the server checklist counts? The merged spine has no Run group, because `/f/<f>/run` redirects to the Plans overview.
- `bun run build` warns that two lazy mermaid chunks exceed 500 kB (`elk-*.js` 1.5 MB, a 629 kB shared langium parser chunk). Both load only after `mermaid-block.tsx`'s dynamic import and no entry holds them. Options: drop mermaid's ELK layout, accept the warning, or set a documented `chunkSizeWarningLimit` for lazy vendor chunks.
- Turn footer ("Now out of step with what you settled") and Settled-line writes were not walked in a browser: a stage sandbox cannot start a session or write the `/tmp` plan lock. Walk both outside a stage.

## Typed Config Values: Known Gaps

- **No control-character stripping in the TUI's value cell.** `commands/config/tui/render/table.rs::value_cell` renders
  `ConfigValue::Text` straight into a ratatui `Cell`. Harmless today because no `KEYS` entry uses
  `ValueKind::String` and the TUI only edits the operator's own `~/.loom/config.toml`; strip control
  characters before the first `String`-kind key is registered.
- **No non-interactive way to unset a user-tier config key.** The dashboard POST `value:null` and config TUI clearing action both reach `user_config::unset`. The TUI stages the clear and applies it on save. The non-interactive CLI has no unset option; a bare negative CLI value needs `--` because clap reads `-1` as a flag.
- **`#[serde(untagged)] ConfigValue` cannot carry the registry's error wording for a malformed JSON
  number.** A `POST /api/config` body with a `Number` field of `-1` or `1.5` fails all three untagged
  variants before any validator runs, so the response is serde's generic "data did not match any
  variant" instead of the registry's "is not a u32 (expected a non-negative integer)". Accepted as a
  wire-format limitation, not a bug to fix reactively.
- **TUI test coverage cannot drive a `ValueKind::String` row through `ConfigState::cycle`.** Rows are
  built only from the static `KEYS` registry (no key has kind `String`), and `ConfigState`'s fields
  are private to `tui::state`, unreachable from the sibling `tui::tests` module. `String`'s
  refuse-to-cycle behaviour is covered only indirectly (sharing the `Number` arm in `cycle`, plus an
  `opens_editor(&ValueKind::String)` assertion). Closing this needs either a registered `String` key
  or a `pub(super)` test constructor for `ConfigState`.

## Hook proposal: a stale `web/dist`

The plan changed `web/src` and shipped without rebuilding `web/dist`, which `loom/build/assets.rs` embeds: HEAD's `plan.js` held none of "Stop now" or "Delete for good", so the binary served the old kickoff UI until integration-verify rebuilt it. It is the third occurrence ([web/src fixes committed without rebuilding web/dist](../mistakes/web-dashboard.md), [the brief rule](../mistakes/subagent-briefing.md)). Proposal: a `loom plan verify` check, or a stage-completion hook, that fails when a stage's diff touches `web/src` and `web/dist` is not rebuilt from it (the CI step "Verify the committed bundle is current" already catches the drift, but after the merge).

## Reviewer suggestions not implemented

Server, `draft_cmd.rs` and the bridge:

- A repeated `now: false` set aside during the closing turn is already a no-op (`close_within` skips the interrupt), but with no chat running set aside answers `NO_CHAT`, and a Preparing session "the chat is closing"; both messages mislead (UI race only).
- `draft_cmd.rs` `set_aside` reads `draft.target` to decide confirmed or not; after a logged `on_confirm` draft-write failure it tears a confirmed kickoff down with no closing write, and discard and continue lose the target too. The scope's grant is authoritative.
- `discard` ignores the result of `drafts::remove` after the files went (the answer is ok and the draft stays listed), and holds `ChatState::starting()` across `remove_written` (up to 4096 entries plus `git cat-file` calls), so sends and starts wait for a large delete.
- `wrote_files` reads an I/O refusal from `written()` (a symlinked `doc/plans/briefs`) as "no files", so a discard-only removes the draft and leaves the document; the doc comment names only the invalid-target case.
- `end_plan.rs`: clamp an end's own bound to `final_write_bound` (`plan.bound.map_or(bound, |b| b.min(bound))`) so drain and recovery, which wait `final_write_bound + END_SLACK`, never return before a longer per-end bound expires. `Bridge::end_within` repeats the body of `end`; share one private helper taking `Option<Duration>`. The branch of `close_within` that returns `Ok(texts)` after `end_within` fails has no test (a stop would have to be injected between the two calls). A test, `a_repeated_set_aside_leaves_the_closing_turn_running`, does not discriminate the end-planned skip: the session already reads `Ending`, and `Session::interrupt` refuses with `NotRunning`.
- `drafts/delete.rs`: `removed` lists the document even when `unlinkat` returned `ENOENT`. `open_root` errors forward the absolute project root to the browser (every browser holds the access cookie). `grant_of` takes the size from the target's spelling, not `draft.size`: cross-check `grant.size == draft.size` (discard checks it; the resume path in `draft_continue.rs` does not). Feature targets have no locked spellings, so Delete does not check whether one of the feature's plans under `doc/plans/<name>/` is executing before removing `index.md`. `kickoff_cmd.rs`: the holder check guards only confirm, so a document created later by a non-kickoff writer under a confirmed draft's name can still be adopted by that draft's Continue or Delete (accepted; revisit if other writers of `doc/plans` appear in the chat flow).
- `fs/safe_fs/write/tree.rs`: no `st_dev` check, so a mount point inside the bundle is emptied across the filesystem boundary (`rm --one-file-system` equivalent). `delete_tests.rs` `a_bundle_refused_after_the_document_says_it_is_already_deleted` relies on `chmod 0o500` refusing the unlink; skip it when running as root (`nix` has no `user` feature here: use `libc::geteuid()` under an `unsafe` block with a SAFETY comment).
- `team.rs` `kickoff_size` caches only sizes it read; the scope read error path is not covered by a test.

Web:

- `draft-view.tsx`: the "Setting aside" disabled reason repeats the notice text (two messages for one state; the spec `section_08.md` requires it, so consider hiding Set it aside while ending). An agent done-fence end (`bridge/done_fence.rs`) is also "ending", so the notice says "being set aside…" for a finishing kickoff. After a "wrote files" refusal the page still offers only Discard; refetch the detail on that refusal. `useRestoreFocus` finds the opener through `button[data-look="quiet"]`, so a second quiet action would take the focus; a `data-*` attribute or a ref through `DisabledReason` would remove the dependence. `isFolder` infers a folder from a missing `.md` suffix; an explicit marker in `written` would stop the web restating a server naming rule.
- `kickoff.css`: `.kickoff-draft__confirm > p` outranks `.plan-fine`, so the committed note renders at `--type-body` and sits on the button row.
- No shared Rust and TS fixture covers `KickoffDraftDetail.written` and `committed`: `web/src/plan/api/fixtures/kickoff-drafts.json` is a list fixture and holds no detail.
- `header/feature-switcher.test.tsx` has a title saying the offer is a fallback; `kindOf` now always prefers the offer.

## The criterion sandbox failed kickoff-fixes for environment reasons

Local runs passed; the gate failed on: a clippy run that recompiled every dependency and hit the 300 s criterion default ([cold builds](worktree-build-cold-start.md), [the 300 s limit](../mistakes/host-verify-group-containment.md)); vitest `EPERM` opening a `package.json` above the tree while resolving jsdom and `kill EPERM` on fork workers; process-reaping tests (a side process, pressure teardown) that cannot signal; and `policy_tests::stage_gate::forward_guard_allows_when_no_stage_evidence_exists`, which sees stage evidence inside a stage worktree. Dispute such criteria per criterion index; do not rerun them hoping for green. The cargo "Could not read repository exclude" failure on the same gate came from `~/.config/git/ignore`, not `.git/info/exclude`; see [the mistake entry](../mistakes/completion-runtime.md).

## Unverified Against a Live Session

The `#[ignore]` operator test `real_session_attach_send_restore` in `loom/tests/plan_chat_operator.rs` (start a chat with an attachment, wait for a turn with `changed_files`, `POST /api/restore`, the file equals the turn-start text, a `Stop` written after the turn) has never run: the sandbox has no tmux, no unix sockets and no logged-in `claude`, so a smoke server starts read-only and `POST /api/restore` answers 403 `read_only` before the missing-turn check. Also unexercised: queued messages and `queue_returned` after Stop or End with a real bridge, permission cards answered against a real `PreToolUse` hook, and Restore while a turn runs. Covered instead by Rust route, unit and contract tests, the vitest suite and a confined HTTP smoke (open questions, ref validation, compare, the served bundle). Run the operator test on a host with all three after merge.

The in-process operator cannot pick a hook-capable binary: `plan_server/chat/mod.rs` sets `BridgeConfig.loom_bin` from `current_exe()`, which under `cargo test` is the test binary, and the only override, `chat::install_with`, is `pub(crate)`. `loom/src/plan_session/bridge/flow.rs` builds the opening `s-` item with no attachments, so a lost start that carried attachments matches only its `u-` item (not verified end to end).

## Review Server Follow-ups

- `compare::is_commit_id` accepts 40 hex digits only, so a SHA-256 repository makes `/api/hashes` answer 500 and `/api/ref` 404; the helpers in `git/merge/control_paths.rs` and `context/worktree_graph.rs` accept 40 or 64.
- `commit_exists` goes through `run_git_bool`, which turns a git spawn failure or timeout into `available: false` (compare) or 400 `bad_commit` (at-commit) instead of a 500.
- `cat_file` treats "could not get object info" as unreadable for any object, so a missing blob in a partial clone or a corrupt repository reports the document as absent at the commit; restrict it to a gitlink entry (tree entry mode, or `cat-file --batch-check` with `%(objecttype)`). `blob_size` runs two git processes per document; one `--batch-check='%(objecttype) %(objectsize)'` call returns both.
- `changed_since` de-duplicates the diff records only: a path removed from the index but still on disk (`git rm --cached`) is listed as removed by the diff and again as added by `ls-files --others`; merge the two into a change.
- `Delivery` is a value re-parsed into `PlanMessage` with a silent `.ok()` in `commands.rs` and `setup.rs`, and `Delivery.to_browser` and `PendingSuggestion.sender_id` do not route anything (`ReviewState.senders` does); `tests/plan_review_contracts.rs` freezes the shape, so simplify when that contract reopens.
- `to_doc_rel` with `check_relpath_syntax` repeats in `restore.rs`, `presence.rs` and `suggest.rs`; one helper in `wire/paths.rs`. `commands.rs` turns a `PublicId` into a `String` through a serde round trip although `PublicId::as_str` exists. `wire/review_tests.rs` and `wire/tests_review.rs` overlap in name and in round-trip coverage.
- `turn_routes.rs` takes the project guard and `end_turn` for the GET of an ended turn's changes; a read-only `TextStore` accessor would avoid both. The per-file `turn_running` re-check in `restore_files` has no test (a turn starting mid-request is hard to make deterministic).

## Chat Web Follow-ups

- **Model.** `item_added` appends without checking the id. `knownPrevious` can falsely confirm: the current chat holds "yes", a second "yes" is lost, the chat ends during the drop and the reconnect snapshot's previous shows 1 > 0 (low impact; the draft keeps the text). `sameSend` merges two identical sends that were both in flight at the drop. `markInterruptPending` also covers End; `markQueueReturnPending` would say so. The holder's tab receives every queue delta first with texts hidden, then with texts (`plan_server/chat/mod.rs`), so a queued entry drops on any later queue change while the server still holds it. `deliveredQuote` expands every quote into characters on each `confirmSends` pass; cache `messageIdentity` per item (`WeakMap`).
- **Reconnect.** The socket phase is `open` at `onopen`, before the chat snapshot, so Resend is enabled in that gap; gate on a snapshot-received flag. When `resendId` is given and its entry is gone, `send()` falls back to the current draft (return early). A successful composer Resend of an old entry clears the current draft when the texts match (clear only for a fresh send). The transcript Resend is not gated on `canAuthorAtom` like the composer, so a free hold leaves a message with no Resend; on `CONNECTION_LOST` the error is set and the control renders `null`; its busy state is component-local. Held pre-hello text is stored only on the next write, so a reload after hello and before a keystroke loses it.
- **Storage and state.** `storageFullAtom` is always written to `getDefaultStore()`, which makes `composer.tsx` read two stores. `ChatRegistrations`' error clears only on a later success. `useChangedSince` recomputes the full list in every tree row and key chip (a derived atom computes it once). `review/live.ts` ignores failed `/api/hashes` and `/api/open-questions` loads silently. `StartingBrief` and `BriefDialog` each poll `useSessionBrief` for one session; pass the state into the dialog. After dispose, an in-flight `startChat` still writes `offerErrorAtom` and calls `reveal()` (return when the controller is aborted). Send to author keeps its own React root (`send-to-author.tsx`), which loses the app providers; a portal from the main tree would keep them.
- **Layout state.** The `c` shortcut and the offer flow reveal the chat through `revealChat` (`chat/sheet-state.ts`), which opens the sheet only below `DESKTOP` (1280px); `composer-reveal.test.tsx` covers both widths.
- **Content.** Suggestion cards do not check the suggested anchor and stage against `doc.sections` before attaching, so the server's token check fails at send time with a generic path error; an empty note fills the composer with the "asks:" prefix and nothing after it. A whole-file attachment hits the 16 KiB cap for most real sections and plans, so the fill usually fails with an error. `line-diff.ts` counts `a` to `a\n` as +0 −0 (the row, Diff and Restore remain; the server hash treats the final newline as significant). A nested tool whose Agent parent is not in the turn stays in the item list and breaks fold runs (`tool-group.ts`). Merging a timed thinking marker with an untimed one counts the untimed one as 0. A failed plans reload after a success swaps the plans for the error block. The "Connection lost" banner prints the raw phase ("connecting") where section 8 says "reconnecting". `open-questions.tsx` hard-codes the `open-questions` anchor, so a numbered duplicate heading links wrongly (let the server send the anchor).
- **Duplication.** The path-to-summary-id conversion repeats in `suggestion-cards.tsx` and `chips.tsx`; `beginChat` and `useNewChat` repeat `createOfferStarter`'s guard, confirm, abort, end, wait, start and reveal flow; the "changed since you looked" glyph repeats in `tree-entry-extras.tsx` and `key-chip-extras.tsx`; `useComposerShortcut` is called in `ChatRegistrations` and in the composer; `messageProblem` formats the message twice and `deliveredBytes` is used only by tests; `RestoreControl` is 51 lines (over the function limit) and its Restore-all dialog should split out; about a dozen exports of `review/api.ts` and `schema-review.ts` are imported nowhere. Review schemas use `z.object` where chat schemas use strict objects.
- **Tests missing.** A mounted ended chat that receives `queue_returned` and leaves the texts in the stored draft (the reason `useQueueRefill` moved to `ChatRegistrations`); `next_delivery` in its real order (queue pops, then the `s-` item arrives), which takes the leave-queue drop and not confirmation; a summary attachment with a null anchor (`resolve.rs` refuses it); `sameQuote` for a raw quote over 2,000 characters that already ends in a marker line, a quote of exactly 2,000 characters and a quote with a trailing empty line. The control regex in `format-attachments.ts` differs from `CONTROLS` on purpose (it also strips newline, within one line) and needs a comment saying so.

## Accessibility and Layout Follow-ups

- The chat panel has no overflow menu. The resize separator has no Home/End jump to 20rem and 40rem. `.chat-permission-badge` gets its styles from `chat-surface.css`. Tabs that scroll past the edge have no visual cue, and only `scrollbar-width` hides the bar (older WebKit draws its own).
- Permission-card input shows only `\p{Cf}` characters visibly; C0 and C1 controls other than tab and newline, and variation selectors, stay invisible. `aria-label` on a role-less `div` in `chips.tsx` is ignored by assistive tech (add `role="group"`). Watcher names (`run-header.tsx`) and token breakdowns (`turn-footer.tsx`) live only in hover titles. "△ changed" is read aloud as "white up-pointing triangle" (wrap the glyph in `aria-hidden`). The visible state text duplicates the screen-reader-only status, so browse-mode readers read it twice. After a reconnect each message with attachments mounts its own `role="status"` line at once. Help text should say `y`/`n` act only while the pinned card has focus (`help/shortcuts.tsx`).
- The assertive permission region mounts and fills within one effect tick, which some screen readers (Safari with VoiceOver) miss; a region that stays mounted fixes it. When the sheet opens or closes, the moved announcer region is inserted already holding the current notice, which VoiceOver can announce again (clear `noticeAtom` after a few seconds, or insert the region empty). `announcedAtom` only grows for the life of the page (negligible). `focus-return.ts` leaves its `tabindex` and blur listener in place when `focus()` fails on a hidden region.
- **Column budget** ([`chat/chat-column.css`](../architecture/planner-web-shell.md#chat-home-and-the-chat-panel)). Only Chrome measured it (`web/scripts/chat-layout-test.mjs`); Firefox is unverified, and so is a phone with its on-screen keyboard open. The web CI job runs `bun run test:browser`, including the chat layout check. `suggestion-cards.tsx` still carries `max-h-[33vh]` on `.chat-suggestions`, which `chat-column.css` overrides (`max-height: none` inside the extras strip); delete the class when that file next changes. With the queue strip and attachments both showing, a docked window under about 450px high scrolls the composer by a few pixels.

## Proposals

- A `loom plan verify` warning or a hook that compares a forwarded codex unit's owned files with `git status`: the "succeeded, wrote nothing" failure has now happened in two plans ([mistake](../mistakes/plan-server-delivery.md#a-codex-forward-reported-success-and-wrote-no-files-2026-10-28)).
- Briefs that add tests to an existing file should name its line count; the recurrence across plans suggests a `loom plan verify` check for stages that own a test file at or above 360 lines.

## Planning Guidance Follow-ups

Open points in `planning::guidance`, `plan_server::guidance` and `plan-plugin/` after plan 8 ([architecture](../architecture/planning-document-model.md)). Each names a revisit condition.

## Unverified and unwired

- **Live sessions are unverified.** The four `#[ignore]` tests in `loom/tests/planning_guidance_operator.rs` (`operator_vision|section|metaplan|plan_session_loads_its_skill`) were not run: they need a logged-in Claude Code, tmux and network. Not shown: a real session launched through the bridge loads the rendered `loom-plan` plugin, honours the agent `model`/`effort` frontmatter and reads the session brief through `--append-system-prompt-file`. Run `cd loom && cargo test --test planning_guidance_operator -- --ignored --test-threads=1` after merge.
- **Guidance rendering covers all bridge session kinds.** Authoring reaches render_session_guidance through loom/src/plan_server/guidance/provider.rs:85; summary sessions call it in loom/src/plan_server/summaries/spawner.rs:57, and pressure sessions call it in loom/src/plan_server/pressure/driver/step.rs:209. All three use the renderer and its configuration-block checks. This wiring does not establish a production caller for StartBlocks::block or clear.
- **Authoring graph reachability has a trait-object seam.** The authoring path uses Arc<dyn GuidanceProvider>; a source-graph failure to resolve that call is not evidence that guidance is unwired. Summary and pressure also call render_session_guidance directly through their launch closures (paths above). Use wiring and launch-behaviour tests to establish each path; see [source-graph-known-gaps](source-graph-known-gaps.md).

## Behaviour limits

- Models load once at server start (`GuidanceModels::load` in `install`), so a settings-dialog edit of an authoring key applies after a restart; the spec text implies live editing.
- `PlanGuidance.rendered` grows one entry per session and is never pruned; `remember` re-reads the brief file from disk after `render_session_guidance` wrote it, where the renderer could return the text.
- Render errors carry absolute runtime paths (`plugin.rs`, `provider.rs`, `blocks.rs`) into `BridgeError::Refused`, which reaches the browser through `dispatch.rs`; the same holds for plan 7's `BridgeError::Io`. Log the chain, return `start blocked: <reason>` or a generic text.
- `GuidanceModels::load` never returns `Err` (`read_plan_config` and `UserConfig::load` are infallible), so the warn-and-default branch in `install` is unreachable; its body also copies `SummaryModels::load` line for line.
- `GET /api/session/brief` answers 404 `unknown_session` for a missing `session` parameter; 400 would be accurate.
- `plugin.rs` writes by path (safe against a final-component symlink and a fresh uuid session directory, so not session-exploitable: `sessions/` is writable by no session kind); the dirfd-anchored `safe_fs` helpers `blocks.rs` uses would also close an intermediate-directory race. `blocks.rs` reads `config-block.json` with an unbounded `serde_json::from_reader`, and no test covers its FIFO, non-regular and symlink refusals (a `mkfifo` test would catch a dropped `O_NONBLOCK`).
- A pasted message held behind a background turn (a subagent hand-back or task notification the bridge ignores for confirmation) confirms by its own prompt, late when that turn runs past `delivery_timeout` (10 s); meanwhile it shows Undelivered with Resend, and pressing Resend then sends the text twice. Nothing suppresses Resend while a background turn is running.

## Text and skills

- `common_brief.md` quotes section 03 text that points at "Section 09", "the kinds table" and "(section 01)", which in a user project resolve to the user's files or nothing; add a line listing the document paths and saying the numbers refer to loom's spec.
- `brief.rs` builds the starting-point block by parsing the rendered opening back apart; a shared (step, target, skill) struct used by `opening.rs` and `brief.rs` would end the coupling. `Target` keeps the leading verb (`Target: Write doc/product/...`).
- `skills/metaplan` says "the selected metaplan" (a feature's metaplan openings may name none), describes follow-ups as "for a finished feature" (`/new` starts Plan follow-ups for any feature with a metaplan, done or not), and `skills/refine` says "its metaplans and plans", implying several metaplans (also the brief text at `plan_server/chat/guidance.rs:222`, pinned by `chat/guidance/tests.rs`, and the comment at `web/src/plan/feature/source-href.ts:12`: say "the metaplan"); `skills/vision` describes a first-table rule the parser (`planning/vision.rs`) does not follow; `skills/summaries` hardcodes `~/.loom/plan/<project-key>/drafts/` where the brief names the `LOOM_PLAN_STATE` directory.
- `CLAUDE.md.template`'s delegation rule pushes authoring sessions to hand document writing to subagents that lack the chat's decisions.
- `knowledge_skill_preserves_every_distill_routing_line` uses `skip(13).take(6)`, which passes for any blank line in the window; anchor it on the routing heading or assert each line is non-empty.

## Test hygiene

`user_config/plan_authoring.rs` holds inline default literals where `plan.rs` has `DEFAULT_PLAN_*` constants; `plan_config.rs` keeps an inline `authoring_tests` module beside `mod tests;`; `assets/mod.rs::plan_plugin_manifest_has_expected_name` duplicates `planning/guidance/tests.rs`'s manifest test; `plan_server/guidance/tests/install.rs::IdleHost` duplicates `chat/tests.rs::FakeHost`; four test files each carry a `LOOM_HOME` guard with no shared helper; the `config_api/tests.rs` regeneration note should name the `REGEN_FIXTURE=<path>` switch.

## Proposals for hooks and plan checks

Two mistakes recurred in the plan-8 stages, so each gets a proposal here instead of another paragraph.

- **A PreToolUse hook refusing `run_in_background` on `loom check` and `loom memory resolve`.** An integration-verify main agent ran `loom check integration-verify` in the background again; the relay never saw its `LOOM_RELAY_V1` line and the next call reported `refused: the request never reached the daemon` ([status-broadcast-hardening](../mistakes/status-broadcast-hardening.md#a-ticket-whose-line-never-reached-the-hook-was-leaked-forever-2026-09-18)).
- **A `loom plan verify` check on gates that run only filtered targets.** `guidance-engine` passed with `--test`/vitest filters while integration-verify's full suite found a binary-spawn-guard failure, an `oxfmt --check` failure on a regenerated fixture and a stale sibling vitest ([plan-server-delivery](../mistakes/plan-server-delivery.md)). A stage that adds a `tests/*.rs` target spawning `CARGO_BIN_EXE_loom`, or regenerates `web/src/api/fixtures/config.json`, would need `--test integration` and the full web checks in its criteria.

## Operator test seam

`plan_server::chat::bridge_config` sets `BridgeConfig.loom_bin` from `current_exe()`, which in an in-process integration test is the test executable; `install_with` is `pub(crate)` and `Bridge.config` is private, so `planning_guidance_operator.rs` cannot point the hook binary at `loom` through the server's public API. Revisit when a public override for the bridge's hook binary exists.
