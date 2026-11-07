---
sources:
- loom/src/planning/mod.rs
---
# Planning Document Model Followups

>

## Open Items in `loom::planning`

Reviewer suggestions not applied, with the file that owns each. See [Planning Document Model](../architecture/planning-document-model.md).

**Anchors and parsing**

- `sections.rs`: a punctuation-only heading yields anchor `""`, which `find("")` shadows with the preamble; later duplicates then take `-1`. Skip such headings in `scan_headings` or give them a distinct anchor.
- `sections.rs`: only ATX headings are recognised while `doc/product/feat-planner/section_03.md` says CommonMark; add a test or a note for setext headings.
- `xref.rs`: `at_token_start` tests any occurrence of the path in the text, so a line holding one path both with and without a `src/` prefix yields a duplicate `PathToken`; it also drops a path wrapped in `**`, `_` or written after `key=` (add `*`, `_`, `=`, `{` to the accepted preceding characters). A link with an empty fragment (a link ending in `#`) gets anchor `Some("")` and is reported broken while `find("")` addresses the preamble; filter empty fragments to `None`.
- `pressure.rs` and `vision.rs`: heading matches are exact for "Pressure tests" but case-insensitive for "Open questions" and "Specification sections"; pick one rule.
- `implements.rs`: a substring test finds where metadata starts; use `DocSections.metadata.start_line`.

**Hashing and cost**

- `project.rs`: the xref lookup closure clones a full `DocSections` per anchored reference (the owned return type is pinned by `xref::references`; memoise per target inside `references` if profiling shows it). `derive` recomputes `sections_of` for every document on every refresh, up to three times per plan (with `pressure.rs`); reuse the `ProjectDoc` of documents whose source was not re-read.
- `problems.rs`: `brief_problems` re-runs `collect_inputs` for every unlocked plan on each derive, so each watcher batch re-reads all briefs; `LockedSet::scan` does the same for locked plans, so caching one side changes nothing in order of magnitude.
- `Project::derive` walks `doc/` twice per refresh (`discover` in `project.rs` and inside `LockedSet::scan`), so tree and locked set can disagree for one refresh. Add `pub(crate) LockedSet::scan_tree(root, &DocTree)`.
- `Project::load` caches every Markdown file up to 8 MiB across up to 20,000 files; add a total byte budget that sets `read_error` once exceeded. `discover::list_dir_no_follow` materialises a whole directory before the entry cap applies.

**Paths, locks and symlinks**

- `paths.rs`: `fold` lowercases only; APFS also folds Unicode normalisation, so a non-ASCII slug spelled NFD misses the bundle prefix lock (slugs are ASCII in practice; note it in the module doc or normalise).
- `paths.rs`: `locked_paths()` is built from discovery, which skips dotfiles and editor temporaries, so hidden files in a locked plan's bundle are refused by `classify_write` but absent from the spawn-time deny list.
- `LockedSet::scan` fails entirely on one non-UTF-8 bundle name, one non-UTF-8 `.md` input or an unreadable directory (fail closed per spec), so one file creation disables load and every write decision. Consider recording it as invalid and locking the path. Cost is O(locked plans x inputs) per call, which matters once the hook calls it under a 10 s fail-open timeout.
- `plan::inputs::collect_inputs` follows symlinks (`is_dir`, `read_dir`, `read_to_string`) and `glob::glob` follows a symlinked directory inside the repo; both run unattended from `LockedSet::scan` and `brief_problems` over a session-writable `doc/`. A planted symlink can make them crawl, hang, or put outside entry names into a `broken-brief-reference` message. Reject a pattern whose literal directory prefix contains a symlink before globbing, or list with `list_dir_no_follow`.
- `discover::read_doc_bytes` accepts a leaf symlink reached through a symlinked directory component when it resolves inside `doc/`, unlike a non-symlink leaf and unlike discovery; gate callers on discovery membership or run `symlink_component` on the parent. `fs::safe_read::read_bounded` does not check `nlink` (`open_regular_no_follow` does).
- `runtime.rs`: `check_state` accepts a path with a trailing `/.`, because `file_name()` ignores it and `lstat` on `KEY/.` follows a symlink named `KEY`; reject a state path whose components are not only `RootDir` and `Normal`.
- `project.rs`: `read_error` carries `{error:#}`, which can include `Failed to open dirfd at <abs root>`; the planning server must not forward it raw to the browser.
- `plan::inputs` depends on `planning::discover::MAX_DOC_BYTES` while `planning::briefs` and `planning::paths` depend on `plan::inputs`; a neutral home such as `fs::safe_read` for the shared cap removes the cycle.
- `watch.rs`: an own summary write that pairs as a rename with an unrelated removed file of equal content is not suppressed (extreme edge).
- The lifecycle prefix grammar has copies with slightly different semantics: `commands/usage/discovery.rs` `normalize_plan` hardcodes the three literals, `plan::inputs::bundle_dir` trims repeatedly, `paths.rs` `case_canonical_plan` strips once. Import `fs::plan_lifecycle` everywhere.

**CLI and tests**

- `commands/plan/check.rs` prints step marks with `{mark:?}`; give `StepMark` an explicit label. `println!` panics on a closed pipe (EPIPE) like every `loom` command; the fix belongs in `main.rs`, not here.
- Not asserted: `ProjectState.sections_listed`, `sections_written` and `metaplans` in `checklist_json_shape` (it pins `plans_by_state` and `locked_files`); `Project::load` with an unreadable Markdown file raising no `missing-title`; exit 2 on a `Project::load` error such as `doc/` nested deeper than 16 levels; a symlinked document in the watcher and in `LockedSet::scan`; sparse-file and `..`-glob cases of `inputs.rs`.
- `doc/product/feat-planner/section_01.md` says the table's step 2 writes the "lowest-numbered missing section" but the code and `table_order_decides_the_first_missing_section` follow the vision table's order.
- Four plans (`PLAN-adopt-ecc-best-practices`, `PLAN-loom-ui-mods`, `PLAN-model-router-hooks`, `PLAN-web-host-graft-followthrough`) have no `Implements:` line, so `loom plan check` reports `implements-missing` and exits 1 on this repository; `Implements: none` under each title clears it (`section_03.md`).

## Summary Store Open Items

Reviewer suggestions on `loom/src/planning/summary/` and `loom plan summary` that no stage implemented; each names its
file. See [Summary Store](../architecture/summary-store.md) for the behavior they qualify. The delivery mistakes of this
work are in [Plan Server Delivery](../mistakes/plan-server-delivery.md).

Robustness:

- `lock.rs` `lock_exclusive` waits with no timeout, so any process holding `summaries.lock` (a sandboxed session
  included) stalls seal, brief and the engine. Consider a bounded wait with a clear error.
- `seal.rs` `write_locked` reads the existing summary with `read_summary(...)?`, while `evaluate` counts an unreadable one
  (symlink, invalid UTF-8, oversize) as missing: a summary file that cannot be read can never be resealed without a manual
  delete. Replace or delete an unreadable file under the lock.
- `seal.rs`: `delete_claim(...)?` after a written summary still fails the seal (exit 2) and strands the claim, while the
  draft removal beside it is warn-only. Make both warn-only. `claims::delete_claim` also fails `NotFound` on a missing
  `claims/` directory although it documents an absent claim as fine (race only).
- `claims.rs` `created_at` is read from a session-writable claim file as an unsanitised string and reaches `status
  --json`. Parse it as RFC 3339 on read or drop it.
- `store.rs`: `summary_path` and `link_from_summary` return an empty path or string for invalid input (fail closed today
  because the `safe_*` calls refuse empty paths and callers validate first); returning `Result` or `Option` removes the
  footgun. `validate_id`'s slug rule is stricter than `plan_slug`, so a plan file with a space in its slug never gets a
  summary and never appears in `status`.
- `seal.rs:93` rebuilds `drafts/<id>.md` while `brief` prints `claims::draft_path` (`paths.drafts`); derive both from one
  helper before `RuntimePaths.drafts` can change.
- `commands/plan/summary.rs` `root()` runs `output.trim()` on the repository path; `run_git_checked` already trims
  (`git/runner.rs:193`), so only `run_git_records` (untrimmed, `pub(crate)`) would preserve a path with edge whitespace.
- `fs/work_dir/config_sections/plan_config.rs` `read_plan_config` drops the whole `[planning]` table without a log line on
  one bad key; a `warn` would help.
- `SummaryModels::load` returns `Result` but never errors and its only caller is `tests/planning_summary_cli_contracts.rs`;
  the signature is pinned by `feat-planner/briefs/feat-planner-04-summary-store/common.md`; `planning/guidance/models.rs` calls
  `resolve`, so no production code needs the `Result` and it can become infallible with the next change to that brief.
- `loom-hooks/worktree-file-guard.sh:193` `allow_loom_scratch` is confined to the validated scratch directory, but its
  comment omits `validate_session_dir`'s 0700 mode check, `LOOM_SCRATCH_DIR` is not tied to the scratch root, and file
  tools can now create `.req` relay tickets directly (the relay verifies them). Say so in the comment.

Untested branches: the `Changed` outcome and `WRITE_ATTEMPTS` exhaustion of `write_locked`; `read_claim` on an absent
file; a failing rewrite or deletion in `engine.rs` (the batch must still complete); `delete_summary` with a symlinked
parent or orphan; `validate_id` with control characters, newline, space, `product/section_` and `product/`; a
`members_hash` over a reordered member (sorting before hashing would survive); a claimed summary with no file (evaluates
`missing`, not `rebuilding`; no unit test pins it); `runtime::create_runtime_dir` refusing a symlinked `plan/` or runtime
directory and chmod-ing an existing one; the `brief` and `release` CLI arms at binary level; `relay_verdict`'s wait notice
and `end_turn=false` in `commands/stage/adjudicate.rs`. In tests, `summary_cli_uses_session_state` fails as uid 0, the
`#[serial]` tests that mutate `LOOM_HOME` race un-annotated tests in the same binary that read the environment, and
`seal_rejects_duplicate_sentence` matches `'3'` and `'5'` loosely enough for a hex id to satisfy it.

Open question for the operator: the "incomplete" row of `doc/product/feat-planner/section_04.md` ("What out of date means") still
lists a removed member summary as a trigger, but with seal requiring every member cited a removal stales the citing
overview sentences instead, so that route is reachable only by a hand edit. Confirm the stale direction and amend the row.

Also open: `loom/maintainability-baseline.txt:42` could not be confirmed as a decrease from the range alone; check `git diff`
for the rows of `dispatch`, `verify_artifacts`, `verify_regression_test` and `work_dir.rs`.

## Concept mode server gaps

Each item is a known limit of `loom/src/plan_server/concept/`, none of them reachable by a normal rebuild; fix them with the next change that touches the same files. Design in [Plan Server](../architecture/plan-server.md#page-route-and-page-model).

- **Hash and evaluation come from different reads.** `page.rs::summary_file_hash` reads the summary file per request while the blocks come from the last evaluation, and `delta.rs::snapshot` reads each file again after `evaluate`. A reseal between the two reads leaves a `file_hash` that describes another build than the rendered text, and the next batch then compares equal and names no page. Derive the hash from the evaluation's own read.
- **An engine failure with readable claims names no pages.** `ConceptState::apply` publishes only the batch's `sources` (`publish(ctx, batch, None)`), so a title, plan-state or pressure change in that batch is never named afterwards, because the next snapshot diff starts from the old state. Naming the batch's touched pages (`delta.rs` `summary_id_for` and `page_key_of`) in that branch closes it. The trigger is a transient race between two claims reads.
- **The claims-unreadable fallback is silent to subscribers.** It stores the claims-free evaluation and publishes the delta, but calls no `subscribe` listener and does not advance the engine's own previous-state map, so the summary scheduler sees no invalidations from those batches.
- **`no_plan` and `implemented_by` disagree.** `key.rs` computes `no_plan` from `project.implemented_by`, which counts a plan with no concept page; `links.rs::implemented_by` leaves such a plan out. A section implemented only by an unlisted archived plan shows no "no plan" marker and an empty "Implemented by" list.
- **The delta carries no reason.** `page-state.ts::dashOutOfDate` always applies `source_changed`, including for a `current` to `rebuilding` change that `delta()` also lists in `out_of_date`; the refetch corrects the reason. `ConceptChanged` could carry it.
- **Colour follows list position.** `key.rs` takes `index % 8 + 1` from the position in the full scope list, so a skipped id shifts later chips against plan 4's order.
- **A rebuild refetches the whole scope.** During a full rebuild every member delta names the overview, so every loaded scope page refetches per batch; acceptable at session scale, revisit when many summary pages stay loaded.
- **Presence does not know concept pages.** Section 05 asks the presence list to show the page a browser reads; `reading` is never sent from `web/src/plan`, and `conceptPageLabel` has only the eyebrow as caller. Wiring needs plan 2's `reading` message and a label from `conceptPageLabel`.

## Concept mode web gaps

Items in `web/src/plan/concept/` and `web/src/plan/api/` that review left open.

- **Accessibility.** The sentence button's accessible name excludes its marks and their out-of-date state, and marks have `tabIndex` -1, so a keyboard-only reader never sees the tooltip; browse mode can swallow `k`, `f`, `d`, `g` and the arrows, so the keys need a pass with a screen reader. Overviews have no `h1` (level 1 maps to `h2`, the column label is a `p`) and a summary page repeats its title as `h1` and `h2`. The rebuilt-summary link in `run.tsx` has `tabIndex` -1 and only `d` reaches it, which opens the run's first document. Each `FoldCap` (`fold-handle.tsx`) is a 1.3em circle (`.concept-run-cap` in `concept.css`); whether that meets the 24px target of WCAG 2.2 SC 2.5.8 at the reading size is unchecked.
- **Rendering.** The page header (`page-heading.tsx` on the shared `ui/doc-heading.tsx`) passes titles, paths and chips through `Untrusted`, but `passage.tsx`, `fold-note.tsx` and `marks.tsx` render document text and paths raw, although section 12 "Rendering" wants Unicode format characters shown as visible markers in an LTR-isolated block; the helpers are `Untrusted` and `visibleControls` in `plan/chat/visible-controls.ts`. The chip label in `document-key.tsx` is set in monospace although a product label is a section number. The nested `TooltipProvider` in `reading-column.tsx` takes the wrapper's 0 ms default while the shell's is 250 ms, so marks and chips show tooltips at different speeds. `page-heading.tsx` (`implementsChips`) renders a bare "Implements:" chip for an `ImplementsView::Sections` view with no links.
- **Provenance.** `data-passage` and `data-hash` in `passage.tsx` sit on the wrapper that also holds the head link and the "Loading" or "could not be loaded" text, so selecting that chrome and using Add to chat creates a source attachment with the passage's path and anchor, a null hash and UI text as content (`selection.ts`). Scope `data-passage` to the rendered Markdown body. `empty-states.tsx::sourceOf` assumes `doc/plans/PLAN-<name>.md`, so the not-found "Open in file mode" link misses plans under `IN_PROGRESS-`, `DONE-` or `doc/plans/archive`.
- **Fold notes name the anchor slug.** `SourceAnchorRef.title` is filled only by hand-built runs, so a note reads "Section port-adapter was removed" instead of the heading. Revisit when the wire `SourceAnchor` carries a title or the column keeps a synchronous title map from loaded passages.
- **Cited pages.** A cited summary whose page key is not a concept page key is never fetched (`pagePath` is null), so its run reads "Loading" for good. `kept` in `passage-doc.ts` is not cleared on failure, so a deleted file that reappears shows its pre-deletion text during the next refetch's loading state (`setKept(null)` on failure closes it).
- **Efficiency and structure.** `ClosedSentence` in `sentence.tsx` is exactly 50 lines, at the function limit. `fileName` is defined in `open-state.ts` and `passage.tsx` and inline in `fold-note.tsx`; `outOfDateText` (`summary-text.ts`) and `reasonText` (`marks.tsx`) switch over the same five reasons; `selection.ts::overLimit` repeats the byte check of `attachments/add-to-chat.tsx` with a second `TextEncoder`. `columnOrder` has no unit test for the parent it assigns under adjacent and nested runs; only `reading-column-focus.test.tsx` covers it.

## Outside concept mode

- `loom/src/verify/criteria/confine_host_tool_tests.rs` holds the fifth private ETXTBSY retry loop (the others are `wrapper/tests.rs`, `self_update/tests.rs`, `quota/codex.rs`, `claude/session.rs`). Its copy-then-probe contract differs from the spawn-retry wrappers; hoist one shared `#[cfg(test)]` retry over `std::io::Result` when a sixth appears.
- Proposed guard: whole-crate formatting by a worker in a shared checkout recurs (`cargo fmt -- <file>` reformats every crate file, including another agent's uncommitted test file). A subagent hook that rejects `cargo fmt` without `--check` would end it; the prevention rule lives in [rustfmt reformats more than the file it was given](../mistakes/typed-config-values-process.md#rustfmt-reformats-more-than-the-file-it-was-given).

## Summary Rebuild Open Items

Known limits of `loom/src/plan_server/summaries/` and `web/src/plan/summaries/`, none of which a normal rebuild reaches. Scheduler and ledger limits are in [Summary Store](../architecture/summary-store.md#rebuild-scheduler).

- **Work on the bridge's flushing thread.** `attribution.rs` `on_subagent_stopped` parses whole transcripts and `start_session` renders guidance on the thread that drives the bridge, which asks listeners to return quickly. Fine at current sizes; revisit when transcripts reach tens of MB.
- **A panic in `start_summary` strands the scheduler.** The `(starting)` placeholder stays, `check_alive` filters it, the stall timer stops `(starting)`, and the tick thread has no `catch_unwind`, so one panic ends stall detection silently.
- **`check_alive` assumes `Ended` arrives within one 5 s tick.** A stalled flushing thread makes outcome `Closed` requeue ids that should fail with the real reason; stash the outcome instead of relying on timing.
- **Narrow races.** An `Ended` handled between the state-guard drop and `summary_started` leaves a `Role::Summary` that is never removed, so that transcript is never costed. `spawner.rs` does not refuse a start while `ctx.is_stopping()`, so a Button press can start a session the bridge drain then final-writes. `claims.rs` creates a claim file and then writes it, so a concurrent `release_terminal` can read the empty file as a session-less claim and delete a live one; write a temp file and link it into place.
- **Facade-held rules sit outside the pure scheduler's tests.** The pressure-session suspension and the two-listings claims debounce live in the facade; plan 11's pressure run will hold a second suspension beside the scheduler's.
- **Route gates.** `routes.rs` `writable()` grants free authoring to a minted (cookie-less) browser id that the gate refuses on authoring routes, and the release error can name the absolute runtime directory through the anyhow chain (log it and return a generic 500 as `files/save.rs` does).
- **Pages keep stale progress.** `building` and `estimate` are global on every concept page, but `ConceptChanged` names only the running ids' pages, so other open pages keep old progress until they refetch (`concept/page.rs`).
- **Panel and popover.** The rendered layout at 390 px and 1280 px, light and dark, was never measured (headless Chrome cannot start in the stage sandbox); only the built classes were reviewed. Open items: the compact `2/12` trigger text is not in its accessible name (WCAG 2.5.3) and the blocked reason renders twice at 390 px because `DisabledReason` has no describedby prop; Enter on the focused trigger closes a popover that a question auto-opened; auto-open places a non-modal popover over an open modal; with ten or more ids the answer buttons fall below the fold at 390x667 because the question renders after the panel; the popover is `calc(100vw-2rem)` wide between `sm` and `lg`; answer buttons give no pending feedback while a request is in flight; `turnCostsAtom` fills from live frames only, so after a reload a row shows nothing instead of `tokens unknown`; `authBlockReasonAtom` is null before authoring info loads, so `RebuildButton` shows enabled in that window.
- **Test gaps.** Route tests return early when loopback cannot bind and pass silently there; `RealSpawner::stop`'s `release_claims` branch, `SubagentStopped` to `record_costs` to the `SummaryTurnCost` broadcast, and `after_recovery` with a first-plan-sized queue have no facade-level test.
- **Dead or test-only surface.** `SchedulerView.declined`, `Estimate.per_summary` and the Background and Refinement arm of `Rebuilds::enqueue` have no production reader or caller; `Env.bridge` and `new_for_test` are production-compiled and only tests use them; `authoring_state_idle` and `claims_changed` take unused parameters; attribution parses the model twice with a weaker filter than `AgentState`. `scheduler.rs` sits at 396 lines, so the next addition goes in a submodule.
- **Smaller items.** `summaries/mod.rs` subscribes `follow_bridge` after `after_recovery`, so when recovery is already done the `on_recovered` callback runs synchronously in `install` and can start a session before `Rebuilds` hears bridge events; swap the order. `routes.rs` `writable()` builds a full panel (quota file reads, totals) only to read `read_only`; `on_update` deep-clones the `Evaluation` that `ConceptState::latest()` holds as an `Arc`. `fixtures/summary-panel.json` uses ids and a session id that do not match real shapes (`plans/<slug>`, UUID session ids). `tests_alive.rs` repeats a subset of `plan_session::bridge::fake_host::FakeHost` (expose it `pub(crate)` under `cfg(test)` when more tests need a host) and `tests_bridge.rs` `Rig::new` builds its bridge over the real `TmuxHost`. `ApprovalNotice` repeats `RebuildQuestion`'s count and estimate text; the ids list's `tabIndex=0` adds a tab stop when nothing overflows; some `role=alert` errors lack `text-destructive`, and the panel's sections create up to seven region landmarks in one dialog.

## Planning Trace Limits

**Not checked in a stage sandbox** (the integration walk ran against the built binary in a scratch repository and could not reach these; run them outside a stage):

- Journeys J2 (needs sealed summaries in `doc/loom/summaries`), J3 and every server write (settle, findings status, save) fail on the read-only `/tmp` plan lock (`fs/locking.rs`); J4, J5 and J8 need tmux, which cannot create a socket. The walk covered a hand-written Settled line, hand-written `REVIEW-` files and ledger and stubbed frames instead.
- Still open: a real Review the feature session, a real summary rebuild over a brief bundle, a real plan completion importing into the ledger, a folded finding reaching a later plan's revisit mark (unit-tested, not driven), and `trace_changed` refetching without a reload (evidence is `trace_changed_follows_a_heading_edit` and the web refetch tests, not the walk).
- `POST /api/summaries/auto` answers 403 when a chat start claims authoring, the browser posts the auto-rebuild setting on the holder transition, and the session fails to start and frees authoring before the post lands. Seen only when a session start fails within moments of the claim (tmux denied in a stage sandbox). A client settle timer narrows the window and cannot close it, and a server 200 would break the pinned assertion in `summaries/state.test.tsx` ("resends the current auto-rebuild preference after a reconnect hello").

**Server and model limits:**

- `settle_route.rs`'s stale check covers only the document's own file: a plan whose brief bundle or upstream section changed inside the watcher lag gets a line built from the old hashes and reads `revised` or `upstream_changed` after the refresh. A plan with validation errors answers 422, which the route table lists.
- `settle::clear` drops every column-0 `Settled:` line before hashing, so editing a prose line that begins `Settled:` never makes the document `revised` (the plan check still reports `settled-bad` or `settled-duplicate`); excluding only parsed marks from the hash would close it.
- `version_route.rs` runs `git log` with the user's configuration: with `log.showSignature=true` and signed commits, gpg lines precede the id, `is_commit_id` fails and the route answers `null` (pass `--no-show-signature`); an I/O error from `symlink_component` answers 400 "crosses a symlink" instead of 500.
- `ReviewFeature` has a null wire `target` (`planning/checklist/offers.rs`) while the brief names `doc/plans/<f>/findings.md`; no consumer reads the target for this kind.
- `engine.rs` `with_source` rewrites only the plan's own `links` and `cites` keys on a move between homes; brief links, brief `cites` keys and `bundle` keys keep the old `briefs/<slug>/` path, so a moved plan with briefs reads stale and is rebuilt. An unreadable `.md` brief is exempt from must-cite while the rebuild prompts say "every Markdown brief file".
- `build/docs.rs` `finish` is exactly 50 lines, the function limit; the next edit there splits it. `planning/summary/brief.rs` ends a rule line with a trailing contrast ("in several sentences, not one") that the style rules forbid, and `brief/tests.rs` pins the text.
- `findings/tests.rs`: both symlink tests pass even without `refuse_symlinks` (`import.rs`), because `O_NOFOLLOW` in `replace_no_follow` and the no-follow plan read fail first; a linked directory that does hold the plan would pin the explicit check.
- The route tests lack a negative case (an untouched second feature receives no `trace_changed` frame) and the web settle tests lack the 422 invalid answer and `locked={false}` overriding the file-name rule; `visionRowText` (`trace/stub-text.ts`) matches the first numbered row of any table in the vision, so a numbered table above the sections table gives the wrong stub.
- `schema-trace.ts` `unresolvedSchema` uses `z.object` where the rest of the mirror uses `z.strictObject`; `skills/loom-plan-writer/SKILL.md` separates the stage-order block from the bullets that explain it with the `Implements` forms paragraph.

**Browser limits:**

- A finding's text enters a session as the user's own first message (the seed is spec-mandated), so an instruction-like finding is treated as a user instruction.
- The ledger hash is read after the trace, so a ledger edit between the two reads leaves the hash newer than the cards drawn and the 409 guard does not fire for that edit; `trace_changed` follows and reloads, and the window is small.
- Accessibility and cost nits on the trace pages: `findings-page.tsx` puts `aria-label` on a role-less `<p>` (About) and `coverage-page.tsx` sets `aria-label` beside an `h3` with the same text (use `role=group` or `aria-labelledby`); several "Affected documents" lists in one turn footer and every card's "About" group share an accessible name; Confirm dismissal shows no busy state while the POST is in flight; `coverage-page.tsx` calls `model.cards.indexOf` per card per render; `findings-cards.tsx` uses the literal 200 where `REASON_MAX` exists and draws the no-offer reason on every open card; `trace.css` should use `:focus:not(:focus-visible)` for its `outline: none` rule; the settle control draws its own inline reason for both Mark settled and Unsettle while working or blocked; `help/maps-help.tsx` `Definitions` nearly duplicates `help/legend.tsx` `LegendGroup`; `trace.css` line 3 passes 100 columns (oxfmt does not check CSS comments); `turn-footer-impact.test.tsx` checks absence before the trace has loaded and `settle-flow.test.tsx`'s state-label `findByText` resolves immediately; `STARTED_PLAN` in `findings-model.ts` duplicates `LOCKED_NAME` in `settle-control.tsx`, and reading `docOfPath(trace, entry.plan)?.progress` would remove it.
- `trace.contracts.test.tsx` "Mark settled" (`findByRole`, the default 1 s timeout) and `editor/live-preview.test.ts` failed once each in the 366-file vitest run of the completion check and pass alone: load-sensitive tests, not defects of the stages that ran them.

**Hook proposals for recurrences:** subagents ran `git mv` against a brief that forbids git writes ([again](../mistakes/planner-follow-ups-delivery.md#small-slips-that-recurred-in-this-plan)); a PreToolUse check refusing `git mv` and `git rm` in a subagent Bash call would stop it. The backgrounded `loom check` proposal is in [planning-guidance-followups](planning-guidance-followups.md).
