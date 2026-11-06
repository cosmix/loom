# Planner Follow Ups Followups

>

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

- `ALL_LIGHT_VARIANTS` and `ALL_DARK_VARIANTS` (`aurora-ui/shared/atoms/theme.ts`) have no consumer, and the light `gray` and `cool` variants (`tokens.css`) are offered by no picker: delete them if nothing selects them.
- Control edges (`--border`, `--hairline`) measure about 1.3-1.4:1 against `--card` in all three themes and the segmented control's chosen fill about 1.1:1, under 1.4.11's 3:1. A project-wide pattern older than this plan; the text colour and weight carry the state.
- `kind-control.css` at 390px: each kind segment has about 98px of content and "Standalone plan" at 13px/600 is about 100px, so it may clip inside the track; check the 390x844 shot.
- Two tone tokens keep hue 325 under Walnut and Graphite ([dashboard](web-dashboard-latent-issues.md#dashboard-settings-themes-and-notifications)).
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

- `fs/plan_review/sections.rs`: a deferred `spec-conformance:` decision appears in both Spec Conformance and Open Findings; the Built view would list it as a spec item and as a finding candidate.
- `orchestrator/signals/generate/product_sections.rs` has no signal-level test of the `Implements: none` case, and no assertion through `assemble_signal`, so removing the `generate.rs` call fails no test (the plan's wiring pattern `product_sections::append_product_sections(` catches it at `loom stage complete`).

## Requirement walk: not checked here

The integration-verify walk of the planner-follow-ups requirements (R1-R13) found every requirement done in code and by contracts, except:

- Knowledge rewrites (R6) were left to knowledge-distill and are applied in this tree.
- Reading-size sizes and the specimen were checked by screenshot; the Settings **statistics block** could not be shot in a stage sandbox (it shows only "Claude Code is not logged in").
- A real summary rebuild (R8) and a real run review with `spec-conformance:` lines (R4) were not run: the stage sandbox has no Claude login and the repository holds no built summaries.
- Journey J2 fails in a stage with "no openable sentence on the product page" because `doc/loom/summaries/` is not in the tree; J3 fails on the known `/tmp` plan-lock 500 (`/api/save`); J4, J5 and J8 need tmux. All run by an operator outside a stage.
- `feat-planner` lists as "no plans yet" in a clone because its `DONE-` plans are gitignored.

## Operator checks after the run

Rebuild the summaries and open a plan summary (opening, detail, source link, fragment at rest); confirm New chat spends no tokens before the first message and a real ending shows the designed card; start a follow-up from the Built page and from `/new` and confirm the entry appended under `## Follow-ups` in `index.md`; run a plan whose `Implements:` names sections and read the spec-conformance lines in its run review; open the dashboard theme picker in Walnut and Graphite; open the Settings statistics block with a Claude login.
