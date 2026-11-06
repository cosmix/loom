# Integration verification: planning trace

Read `common.md` first. Work in this order.

1. **Build.** From the worktree root: `(cd web && bun run build) && (cd loom && cargo build)`.
2. **Diagnostic check.** `loom check integration-verify`; read every failure before fixing.
3. **Browser preflight.** `scripts/plan-screens.sh loom/target/debug/loom
   "$LOOM_SCRATCH_DIR/preflight" /`. On failure, `loom stage block integration-verify
   "<preflight output>"`.
4. **Journeys.** `scripts/plan-journeys.sh loom/target/debug/loom "$LOOM_SCRATCH_DIR/journeys"`
   without `--strict`; list every PENDING id in the walk.
5. **Scratch-feature walk** in a scratch git repository under `$LOOM_SCRATCH_DIR` holding DEMO
   (`loom/tests/planning_trace_contracts.rs`) plus a plan with a brief bundle, committed. Start the
   built `loom plan` server against it and drive it with a throwaway Playwright driver (Environment
   inventory in the plan). Record evidence (shot path or response) for each step:
   - `/f/demo/product`: spine rows read Draft; frontier hints list settle hints.
   - Mark settled on section 01; the file holds one Settled line; the row reads Settled. Commit.
   - Edit section 01 in the editor; the page reads revised; Changes since settled opens the
     compare view at the settle commit. The plan implementing it reads that the spec changed.
   - Mark settled on a section with `(My call.)`: the confirmation lists it.
   - `/f/demo/coverage`: the uncovered heading is marked; Uncovered filter; `l` and `h` walk the
     lineage; the focus panel names the node.
   - Import a run review: write a `REVIEW-PLAN-first.md` in the run review format and call the
     import through a unit-level path (a test binary, or a done plan through
     `mark_plan_done_if_all_merged` in a test), then confirm `findings.md` and the Findings page.
     Running the import twice adds nothing.
   - On `/f/demo/findings`: Dismiss with a reason (the ledger line), Fold into entry and Plan this
     (the composer or start frame carries the seed; no real session starts here), the Review the
     feature button (the start frame names `review-feature`).
   - A plan summary sealed in the fixture with a two-level draft citing its briefs (write the
     draft and seal it with `loom plan summary seal --force` from the terminal); edit a brief and
     see the plan page's summary marked out of date.
   - A summary sentence on the product overview or section page shows a chip for the plan
     covering its heading; a chat turn footer (stubbed frames) lists the downstream plan.
6. **Facet walk.** One `loom memory note "req-walk <facet>: ..."` per row of the plan's
   Requirement to gate map, citing the gates and the evidence above.
7. **Fixes.** Each fix is followed by a `web/dist` and binary rebuild. Visual fixes to a fresh
   fable worker with `frontend-design:frontend-design`; other fixes to the cheapest capable tier.
8. **Final check.** The full `loom check integration-verify` against the rebuilt bytes is the
   acceptance.
9. **Final review round.** Parallel `loom-code-reviewer` subagents: (a) Rust trace model, settle
   hashes and ledger writes (symlinks, idempotence, untrusted text); (b) server routes (roles,
   hash checks, error bodies without paths, git invocation); (c) summary store bundle rules;
   (d) web state, pages and accessibility (names, keys, text rendering); (e) skills and spec text
   against common.md. Fix or dispute every finding.
10. **Commits.** Code fixes first, then `web/dist` last as `chore(web)`.
