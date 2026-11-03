# Planning omissions

## PLAN-portal-token-savings, pressure test of 2026-11-02

**Blocked execution**

- Contracts in `read-gate` and `ask-trial` asserted only behaviour today's code already has (rule 1 warns; an unknown subcommand never runs the fake). Freeze refuses a contract that passes at base, so these needed a control that is red at base.
- The `Ask` arm breaks `dispatch`, an exact 59-line ledger entry, and no standard stage ran `--test maintainability`, so it first failed at integration-verify.
- `docs/test-run.log` matches `*.log` in `.gitignore`, so the fixture would never have reached the commit.
- The briefs told the orchestration skill to grow past its pinned byte cap and claimed the new wording offset it.

**Would have shipped broken**

- The cost model charged a cache read on the request that wrote an item, charged replay on the last request's output, and used a flat 1.25 write weight where `accounting.rs` weights 1-hour writes at 2.0. `total_s2` was never defined.
- `chars` counted serialized attachments, including kinds the model never sees; `rendered` is the model-visible text. Windowed transcripts were reported as startup, and `requests_before` could exceed `n`.
- The worker-brief hook would tell `loom-mechanical-engineer` to invoke a `Skill` tool it lacks. Sidecar types would have read as ledger matches, and team agents' sidecars hold the teammate's name in `agentType`.
- Markdown outline rows would miss the hook's regex without the indent `render_outline` adds at its call site, and the acceptance lines passed on the footer alone. No contract crossed the hook-to-binary seam.
- The Codex lane had no isolation contract, and `--sandbox read-only` still allows shell reads. The child environment kept session identity. SIGINT would orphan a worker in its own process group.

**Wrong assumptions**

- `uncovered_file_warns_and_allows_even_with_deny_on` was expected to need rewriting and a dispute; it runs outside a live stage and passes unchanged.
- `sandbox_instructions` was claimed to appear on engineers; none of 180 engineer transcripts carries it. `spawn-guard.sh` was claimed to be in the size ledger, which covers only Rust files.
- `cache_ignore.rs::check_ignore` was named as the stdin spawning precedent; it only calls `git.run`.

**Under-specified**

- Plan files were not exempt from the read gate, though stages are told to read them. Standard stages ran no fmt, build, doc or hook suites. The distill stage could turn its own acceptance green with `--write-baseline`, and it overlapped feat-planner-13's distill. The plan named no sibling constraints and recorded no baselines.

## PLAN-portal-token-savings, Codex review of 2026-11-02

Most findings repeated the earlier round. These were new.

**Would have shipped broken**

- The `dispatch` fix added a comment line to hold the ledger at 59. The ratchet route, lowering the row through a `dispute-integrity`, was documented in `v2-contracts.md`.
- The Codex lane never received the mode prompt: it was wired only to Claude's `--system-prompt`. Nothing rejected a Codex reply that had used a tool, so a shell read would have passed as a one-call answer.
- `residual_s2` clamped at zero hid over-attribution, and no contract covered a result after the last request.
- Truncated transcripts left the startup medians with no count of how many were dropped.
- `_loom_ledger_append` trims to 300 rows, which contradicted "a repeat is allowed" once an agent passed 300 denied paths.
- Readers had no bound after a zero exit, so a descendant holding the pipes could hang `loom ask` and outlive the answer.

**Under-specified**

- `ask-trial` pinned `dispatch.rs` in `files:` and wiring, which a merged web-host-graft could invalidate.
- The After merge checks compared first-request size and read rows, which the `optim-token` metaplan says never count as a saving alone.

## PLAN-planner-issue-fixes — pressure test, 2026-11-03

Blocks execution:
- `sandbox.allow_unsandboxed_escape: false` would stop the YAML parsing once PLAN-live-test-lane deletes the field (`SandboxConfig` is `deny_unknown_fields`). Removed; the default is false.
- No "Before loom run" note: plan and briefs untracked, plus two modified summary files under `doc/loom/summaries/` that the #18 repro and the shots would not see.
- Integration-verify rebuilt and committed `web/dist` AFTER the final review round; `verify/review/gate.rs::covers_review` refuses completion when a non-`*.md` file changed after the round. Rebuild now precedes the final round.
- `plan-journeys.sh --strict` in integration-verify always fails in a stage (no tmux makes J4/J5/J8 PENDING, and `--strict` fails on PENDING).
- `plan-screens.sh` calls in stages 2 and 3 omitted the required `<loom-binary> <out-dir>` positionals (exit 2).
- Merged-count contract built on `strip/tests.rs`'s fixture, which writes Knowledge stages that base already counts as merged, so the contract would pass at freeze. Now seven Standard stages, one unmerged.
- The sonnet overview default breaks three pins no worker owned and no backend gate ran: `loom/tests/summary_engine_contracts.rs`, `web/src/api/fixtures/config.json` (via `commands::status::web::config_api` tests) and `web/src/components/settings-plan-section.test.ts`.
- V1 had to edit `concept/key-row-extras.tsx` and break `files/doc-heading-view*.test.tsx`, `concept-route.test.tsx`, `summaries/rebuild-question.test.tsx` without owning them; V2's play-icon change breaks `files/doc-heading-view-parts.test.tsx`, `files/tree-entry.test.tsx`, `concept/page-heading.test.tsx`, outside stage 3's files and vitest scope.

Silently ships broken:
- Scoped acceptance (module filters, per-dir vitest) missed every consumer above and the integration targets pinning the paste path and scheduler; stages now run whole suites.
- #16 had no gate at all; B2's three regression tests now have fixed names checked by after_stage.
- Restatement contract tested only identity (an equality check passes) and a bare `Rejected` match (already true at base for a draft missing citations).
- "SummaryStatusRow shows only when none runs" would hide the rebuild question it holds.
- "Disabled reason inline" in the shared `DisabledReason` would restyle about 40 consumers; now an opt-in prop.
- Deleting `@keyframes activity-glyph-running` breaks `activity-glyphs.test.tsx`'s per-kind keyframe test.
- V1's starting values allowed turning Source/Summary/Edit links into buttons, which the browser journeys click by role; no stage owns `web/scripts/**`.

Under-specified / wrong claims:
- Contract seams: C1's contract named a FakeSocket seam panel-a11y.test.tsx never uses; ItemView needs `turn`; ActivityPill needs the dashboard-button and summary-feed mocks; the changed-since kit hard-codes at-commit/doc texts; the Fold-all contract pointed at a renderHook test instead of the renderAt seam.
- B3 put the overview rule in `agents/summary.md` (the document agent) instead of `agents/overview.md`.
- O1's brief left "Mark all seen" text capture unsettled and placed ChangeRows/Limits/RefForm in the wrong file; the product spec section_10 would contradict the new behaviour with no owner.
- B1's seam choice left open; `transcript/mod.rs` and `records.rs` are at 389/388 lines; the 100,000-char cap runs before parse.
- The removed RunStrip also showed a stopped daemon's plan; the brief did not say a stopped plan now shows no row.
- `beginChat` claimed to serve offers through `offer-flow.ts`; offers use their own `endCurrentChat`.
- Stale line anchors across V1/V2/O1/B2; the ledger row count (135, now 143).
- Knowledge: `conventions/plan-web.md` claimed a frame-feeding `addInitScript` wrapper in `plan-journeys-kit.mjs` that does not exist; `concerns/session-tmpdir-deny.md` described a deny the tree removed. Both corrected.

## PLAN-planner-issue-fixes — second review (codex), 2026-11-03

Blocks execution:
- Integration-verify ran its one `loom check` before building, so the dist freshness criterion compared a fresh build with the dist the web stages restored, and the brief both expected that failure and demanded zero failures. Now: build first, a diagnostic check, a rebuild after every fix, and a final full check on the committed bytes before the final review round.
- A missing Chromium was "recorded and reported", which contradicted mandatory shots and O2's #18 reproduction. Each web stage and integration-verify now run a browser preflight (`plan-screens.sh ... /`) and block on failure. The plan also states it is Linux-only: on macOS loopback binding needs `allow_local_binding: true`.

Silently ships broken:
- V2's #14 rule hid `SummaryStatusRow` (and with it `SummaryPanel`'s queue, failures and `start_error`) while a rebuild runs, exactly when B2 promises the panel explains a waiting press. The running row is now the disclosure for the same detail.
- Removing `RunStrip` dropped a stopped run's plan, merged count, attention count and dashboard link entirely; the plan had called that "nothing is in progress". A stopped run now keeps one footer control line that lights no activity.
- O1 made stored seen text the baseline but left `ComparisonList`'s global commit availability, which hides every diff once the checkpoint commit is gone. Availability is now per document; new frozen contract `seen-text-outlives-a-missing-commit`.
- O1 removed the checkpoint identity check without replacing its stale-response guard; a diff fetch resolving after Mark all seen could overwrite the newer baseline. Added a reset generation, `{hash, text}` pairing, and cache invalidation on a failed Mark-all-seen fetch.
- #16: question approval (`Via::Approved`) maps to `background`, and `pressed_or_background` needs every id pressed. Both were missed as candidate causes of the all-background ledger. The attribution rule is now settled (any pressed or approved id makes the session `button`), and the ledger line is read back through `cost::read_log`.

Under-specified / wrong claims:
- `files` and worker tables were repository-relative under a `working_dir`; retrieval and `stage_touches_dir` join them to `working_dir` (`web/web/src/...`). Normalized to working_dir-relative; recorded in `mistakes/plan-authoring.md`.
- reading-and-summaries may change Rust but ran no rustdoc; added, and its Rust tests widened to `plan_server::`.
- Knowledge `summary-store.md` claimed the ledger trigger is Button when any id was pressed; the code requires all. Corrected to the code; knowledge-distill rewrites it after B2.
