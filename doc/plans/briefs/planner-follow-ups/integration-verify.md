# Integration verify: the follow-ups plan, checked in the running app

Read `common.md` first. You are the last gate, and passing tests do not prove the user's complaint is gone. Check each requirement in the running app, and compare the chat and `/new` screens with their mockups.

## Order

Your session starts in `loom/`. "From the worktree root" means `cd "$(git rev-parse --show-toplevel)"` first.

1. **Build.** From the worktree root: `(cd web && bun run build) && (cd loom && cargo build)`. The binary embeds `web/dist`.
2. **Diagnostic gate and preflight.**
   - Run `loom check integration-verify`. Fix every failure through an engineer of the cheapest capable tier (haiku for a one-line value, sonnet for code, opus only after a failed sonnet attempt), rebuild after each fix (step 1), and re-check only the failed criteria with `--criterion <n>`.
   - Then the browser preflight, from the worktree root: `scripts/plan-screens.sh loom/target/debug/loom "$LOOM_SCRATCH_DIR/preflight" /`. If it fails, file `loom stage block integration-verify "<preflight output>"`.
3. **Journeys.** `scripts/plan-journeys.sh loom/target/debug/loom "$LOOM_SCRATCH_DIR/journeys"`, without `--strict`. List every PENDING id in the walk.
4. **Screens.** `scripts/plan-screens.sh --worktree-doc --themes=ledger,walnut,graphite loom/target/debug/loom "$LOOM_SCRATCH_DIR/screens"` over these routes:
   - `/`, `/new`
   - `/f/feat-planner` (all plans done: the Built state)
   - `/f/feat-planner/product`
   - `/f/feat-planner/engineering/<a plan slug>` (a plan summary: initial level)

   States the script cannot reach are shot with a throwaway Playwright driver under `$LOOM_SCRATCH_DIR`, in one Bash call with its own server start (plan Environment inventory). Where a session cannot start, stub `WebSocket` in `addInitScript` and replay frames from `web/src/plan/api/fixtures/chat-snapshot.json`.
   - The chat in the five Hearth states: empty after New chat, mid-turn with tool use, a permission decision, ending in progress, ended. Serve `doc/plans/mockups/chat/` over loopback and shoot `alt-d-hearth.html` at the same width for the side-by-side comparison.
   - `/new` in the four Threshold states: default, Follow-up with the feature picker open and a feature picked, typed with a document attached, after Send. Shoot `doc/plans/mockups/new/alt-a-threshold.html` the same way.
   - A plan summary opened to the expanded level, then to the source passage; a summary whose adjacent sentences cite one table, opened once.
   - The spine of a feature with a done plan and a pressure-tested plan.
   - Settings: Theme, Reading (with the specimen), Summaries statistics.
   - A selection with the Add to chat control; an opened run with its fold chevrons.
5. **Requirement walk.** Judge each item yourself against the shots and the code. For R1-R13, write one line: "done", "not done" or "could not check here", with the evidence (a shot path, a test or contract name, or a `file:line`).
   - Record each line with `loom memory note "req-walk R<n>: <verdict>: <evidence>"`, citing the gate the plan's "Requirement to gate map" names.
   - An item you cannot show done is a defect to fix now. A visual defect goes to a fresh fable worker for its area (`loom-senior-software-engineer`, model fable, loading `frontend-design:frontend-design` first) under common.md's loop, with the shot paths and the mockup shots. Anything else goes to the cheapest capable engineer.
6. **Final build and gate.** After the last fix, rebuild (step 1) and run the full `loom check integration-verify` once more. That run is the acceptance.
7. **Review.** After the final gate, spawn parallel `loom-code-reviewer` subagents:
   - (a) **Offers and metaplans.** No `RefineSpec`, `refine_spec`, `retro` skill or metaplan round left in code, prompts, skills, spec or help. `Offer::PlanFollowUps` from checklist through wire, opening, web handler, the Built state and `/new`. One metaplan per feature everywhere `is_metaplan` is asked.
   - (b) **Spec check and run review.** The integration-verify signal's product-section block and duty, the sandbox write grant for `doc/product/<f>/`, `## Open Findings` and its one parser, and their tests.
   - (c) **Summaries.** Two levels for plan summaries through format, validation, seal, server body and client; fragment grouping on the server and the client; each passage shown once.
   - (d) **Chat.** No session before the first send; no menu; no idle line; the closing write only after a user turn; the closing prompt never rendered; one roundel per pane.
   - (e) **Accessibility, design and dead code.** Contrast of every theme, reduced motion, 390px without sideways scroll, focus rings, DESIGN.md matching the CSS; no orphaned Aubergine or Pacific tokens, `ChatMenu`, `NoStep` opening, `Followups`, metaplan-round helpers or retro assets.

   Fix or dispute every finding. The review gate refuses completion when the tree changed after its round, so a fix after a round needs step 6 again and another round.
8. **Commit.** The code fixes first, then `web/dist` last as `chore(web): rebuild the planner bundle`.

## What the user checks after the run

Report these as open, never as passed:

- Rebuild summaries, then open a plan summary: initial level, expanded level, source passage with its link.
- Start a real chat with New chat: no tokens are spent before the first message; end it after a conversation and see the designed ending.
- Start a follow-up from the Built state and from `/new`, and see the entry appended under `## Follow-ups` in the feature's `index.md`.
- Run a plan whose `Implements:` names sections and read the integration-verify spec-check decisions in its run review.
