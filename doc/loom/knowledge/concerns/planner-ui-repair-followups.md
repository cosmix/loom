# Planner UI Repair Follow-ups

> Repair

Review suggestions nobody implemented and findings the stages could not close. Each item names its symbol; re-read the code before acting, because a later change may have settled it.

## Codex summary backend

Design and rules: [Summary Backends](../architecture/summary-store.md#summary-backends-claude-and-codex).

- **Never run live.** A stage sandbox has no network and refuses nested `bwrap`, so the sandbox-write behaviour, `check_parent`'s access test and the `O_RDONLY` lock fallback rest on tests and reasoning. Run one real summary job with `plan.summary_backend = "codex"` before relying on it.
- **`loom` on `PATH`.** The job prompt runs a bare `loom plan summary ...`, while hooks use `spec.loom_bin`. A server started from a binary that is not first on `PATH` runs a different `loom` inside the job. Open question: should the spawner prepend the running binary's directory to `PATH`?
- **No in-family model fallback.** `codex_spawner.rs` fails every summary on an account without the chosen model, where pressure falls back (`codex_model_candidates`, `spawn_codex_with_fallback`). The default `gpt-6.1-terra` has no siblings, so only a user-picked model is affected.
- **Guard.** `codex_snapshot` (`guard.rs`) re-implements `config_guard`'s bounded symlink walk with weaker path handling (one walker with a start directory would serve both); the failure text names one instruction file only, though two are guarded, and two tests pin the string.
- **Prompt and config.** `planning/guidance/text/summary_codex.md` tells the job to use the feature name supplied in the assignment, but `render_prompt` supplies ids and the runtime path only. `SummaryModels::load` never returns `Err`, so the `Err` arm in `backend.rs` and its doc sentence are dead.
- **Logs.** `codex-logs/` gets one file per job and is never pruned; the log path shows in the panel only while the job runs, so a failed job keeps just the log's last line.
- **Lifecycle edges.** The registry keeps a job after `wait()` reaped the leader, so `terminate`/`force_later` can signal a reaped group (pgid reuse window); `settle_outcome` reads `stopping` and removes the job under two lock acquisitions; detecting a substituted outcome with `guarded != outcome` is ambiguous when the log tail equals the guard's message; a stop for a session that already left the job map reaches `RealSpawner` and prints a spurious line; `stop` and `stop_all` both SIGTERM and start two kill threads; a job that seals its last id and exits in the 1-2 s before the watcher's batch starts one extra short `codex exec`. A second `TornDown` test would pin its difference from `job_ended`.
- **Duplicated constants.** `CLAIMS_DIR` repeats the private constant in `planning/summary/claims.rs` (export it as `DRAFTS_DIR` is); `lock.rs::write_denied` matches the same errnos as `context::graph_store::is_write_denied`.
- **`safe_fs.rs` seccomp assumption.** A real `EPERM` on an immutable or append-only file sets `OPENAT2_SUPPORT` to 2 for the whole process although the code treats it as seccomp; the portable walk still refuses symlinks, so only the fast path is lost.

## Wire and documents

- `wire/checklist_card.rs`: the title suffix strip ignores word boundaries ("Loom SuperVision" becomes "Loom Super") and does not trim an en dash.
- `planning/guidance/brief.rs`: the guidance brief's written total uses `sections_written.len()`, which can include unlisted section files; the card counts listed rows only.
- `plan_server/files/plan_view.rs::last_change` uses file mtime, so after a clone or checkout every document reads as just changed.
- `DocView` has no dirty field, so "Uncommitted changes" with a warning dot renders on every document, even a clean one (the brief states it unconditionally; it needs a server field).
- The colour index in `plan_server/concept/key.rs` enumerates before `filter_map`, so a dropped member skips a colour.
- `RUN_NOTICE` and `DONE_NOTICE` in `concept/page-heading.tsx` copy `plan_server/files/doc.rs`, and `RUNNING_NOTICE` there lacks "so the app keeps it read-only"; the two can drift.
- `/f/feat-planner/engineering` fails `plan-screens.sh` with console 404s: `concept/cited-pages.ts` prefetches summaries of plans that no longer exist under `doc/plans` while the committed engineering overview still cites them (the page renders one mark and no key).
- An opened run reads Loading until every cited page has arrived, because `loadConceptPages` commits through `Promise.all` (`page-state.ts`); about 2.7 s on a debug server. Committing each page as it arrives would cure it.

## Chat, help and reading

- `chat/start-chat.tsx` should capture the screen's feature at click: the end wait can last 10 minutes and a project-wide session stays mounted across navigation.
- Offer refusals on `/` and `/new` (`chat/offer-flow.ts`) are plain text; a link to the home or `/files` would save the user a guess. `attachHere` and `AddToChat` do not clear `offerErrorAtom` as `discussDoc` does, so a stale refusal stays beside the new chip; the pending chips on a concluded foreign screen are not gated on `canAuthorAtom`.
- `composer-hooks.ts`: `c` sets `chatSheetOpenAtom` at desktop width, where nothing closes it, so narrowing the window later opens the sheet unasked.
- `help/pinned-scene.tsx` re-creates its observer and three timers on every search keystroke (drop the `children` dependency, skip `setMeasured` when unchanged); `chat/use-session-brief.ts` paints one frame with the previous label when a session changes.
- `chat/pressure/pressure-timeline.tsx`: keys repeat if a round ever holds two steps of one kind (server behaviour unconfirmed).
- `foldAll` and the header Shift+Esc path leave fold notes in place while `foldRun` and `useOpenAll` clear them (check against section 05); Open all is a no-op when `page.empty` shows `EmptyStates`; a summary whose source file is gone shows disabled Edit and Discuss with no reason; `tree-entry.tsx` wraps the state onto a second line at about four nesting levels.
- Header: `FeatureProgress` works out the route feature again (reuse `useIdentity().feature`); `path.split('/').at(-1)` is repeated in five places and the pressure strip shows a file name because the wire has no title; `next-step.tsx` mixes `offer.feature` (slug) with `checklist.title`, and its `DocMark` uses the current step's document kind where the spec says the offer's; the compact next-action rules in `header.css` target `OfferButton`'s internal `div > span` structure (a class hook would be sturdier); the `.ps-knob` shadow is a literal `oklch`.

## Untrusted text and tests

- Raw server strings outside the Untrusted rule (`common.md`): `concept-route.tsx`, `marks.tsx`, `run.tsx`, `concept/plan-glyph.tsx`, `files/search-results.tsx`, `files/plan/{stage-row,stage-graph,stages-panel,validation-list}.tsx`, `editor/{validation-panel,edit-body}.tsx`, `diff/diff-page.tsx`, `files/{reading-column,tree}.tsx`, `chat/{connection-banner,turn-footer-plans,chips}.tsx`, `summaries/rebuild-button.tsx`; `concept/empty-states.tsx` renders `page.estimate` raw and a `name` taken from the URL into the title (`visibleControls(name)` would close it).
- Missing format-character tests: `StepItems`, `queue.tsx` section and plan rows, `tree-entry.tsx` wraps, the `sentence-text` link and image branches and the BOM-only fallback, the chip and key-chip paths in `document-key`, and the `step-tracker` label and count spans (today the server sends static labels and integer counts, so no such input reaches them).
- Other test gaps: no test mounts `PressureStrip` in the live header or checks it is omitted when `sample` is set; `dock.test` mocks `ChatRegistrations`, so nothing checks that `connect` mounts the socket and attention hooks; `dashboard-button.tsx` has no test that keeps the last URL when a refresh fails; `next-step.test.tsx` uses fixture labels that differ from production's (`Start the run` against `Start a run`) and checks the label-as-title guard for two kinds of thirteen; `set_owner` during a launch (`bridge/start.rs`) and a recovery with a non-null feature (`tests_recovery.rs`) are untested.
- **Web lint.** `web/.oxlintrc.json` has no react plugin, so a conditional hook passes lint ([the mistake](../mistakes/planner-ui-repair.md#a-conditional-hook-passed-tsc-lint-and-tests)). Enabling `react/rules-of-hooks` raises about 80 warnings under `--deny-warnings`; clean those up, then enable the rule.

## Duplicated helpers and layout details

- Duplicates: `plural` in `feature/steps.ts` and `files/heading-parts.tsx`; `MARK_LABELS` in `feature/step-list.tsx` against `STEP_MARKS`; `CHIP_FILL` and `CHIP_ICON` in `chat/pressure/pressure-plan-card.tsx` against `ui/doc-heading.tsx`; a second `ScreenSync` mounted by `ChatPanel` beside the dock's; `isLive` in `panel-head.tsx` against `isActiveSession`; `feature/` imports `home/feature-mark` while `home/` imports `feature/steps` (move `featureColour` and `plural` to `ui/`); the 1024px `matchMedia` stub exists in `concept-route-support.tsx`, `header/shell-contracts-kit.tsx` (`desktopMedia`) and about ten tests (a shared helper should use `stubGlobal`). `LOCK_BOX` and `LockGlyph` have no importers.
- `loom-plan-name` (`header/name-prompt.tsx`) bypasses `storageKey`; it looks deliberately global across projects, so decide and comment.
- Shots at 1440 and 390: the feature nav's fill stops at y=710 while the dock runs to 900 (`feature.css`); the current step is off-screen in the horizontal nav at 390; in Pacific the current (`--primary`) and running (`--tone-executing`) progress cells are both blue; blocked ghost buttons paint `--muted` on hover in `proposal.css`, `standalone-shelf.css` and `chat-surface.css` (only `.chat-ghost-actions` guards it); a wrapped breadcrumb starts its second line with a chevron on the 390 files-plan route; shelf blocked reasons inherit weight 600; the gauge in `pressure-dialog-parts.tsx` is `text-primary` on a primary 28% tile (likely low contrast in Ledger); the phone Chat button shows the quartered roundel when idle where the panel head shows a chat icon; the app bar track is `minmax(150px, 1fr)` against the stated `minmax(0, 1fr)`, kept because the identity's 150px minimum would overlap the cluster on a 0-minimum track; markdown file-view headings stay weight 600 with rem sizes (the reach of `reading.md` over Markdown is unclear); the rail's live state needs `daemon_running` while home and the step nav show running from plan state.

## Process

- A full `cargo test --no-fail-fast` in the main checkout ended 13:53:44 and the plan's daemon died at 13:53:05 with no shutdown logged. The cause is unconfirmed: an earlier full run stopped at the `maintainability` binary and the daemon outlived it, so the binaries after it are the lead. Ruled out: the commit, an operator `loom stop`, and `plan_handoff_operator`'s `loom stop`. Do not run the full suite in a checkout with a live daemon until a test reaching the real daemon is traced or excluded.
- Codex forwarders ran the wrapper from `loom/` because the guard blocks a `cd` prefix; their units still wrote the right files, and the evidence trailers in the tool-result files confirm unit id and state.
