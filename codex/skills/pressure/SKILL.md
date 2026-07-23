---
name: pressure
description: Pressure-test a loom plan and write a review for the plan's author. Trigger when the user invokes $pressure followed by a plan path. The argument after $pressure is the plan file path.
---

The user has provided a plan path as the argument after $pressure. Treat that argument as <PLAN>.

read doc/loom/knowledge. then read <PLAN> (the plan file) and use an agent team to explore the codebase. Your task is to pressure-test the plan: hunt for omissions, wrong assumptions, missing wiring, unhandled edge cases, and any step an executor could not complete from the plan alone. Validate every finding against the actual code before reporting it — cite file:line evidence; drop any finding you cannot ground.

Cover ALL of these dimensions. The first two are the highest-yield failure classes on record.

1. CROSS-PLAN CONTRACTS. List the sibling plans in the plan's directory (PLAN-*, IN_PROGRESS-*, DONE-*) that share files, symbols, or seams with this plan, and read them. For every upstream surface this plan consumes, verify the exact symbol/signature/exporting module against committed code first, else against the sibling's stage YAML (artifacts/wiring/acceptance) — NEVER against the sibling's prose: a capability promised only in a sibling's overview is built by no stage; report it as a blocking gap. For every downstream consumer plan, check that the names/accessors/seams it expects are ones this plan actually provides. Check file ownership: flag any path this plan writes that a sibling stage's files:/artifacts: also owns, and any bare file dropped into a shared module directory instead of a disjoint namespace. Check module placement against lint/import boundaries a sibling plan enforces. Where this plan needs a seam no sibling builds, the fix is an explicit amendment flagged as a blocking dependency — not a conveniently assumed interface.

2. PROSE ⇄ FRONTMATTER COVERAGE. For every capability the plan's prose promises ("ships X", "exposes Y", a public-contract section), grep the plan's own loom YAML: it must appear in exactly one stage's artifacts: and be proven by a wiring/acceptance entry. A prose-only deliverable lets every stage complete green while the promise is never built — report it as top severity. Also check the reverse: YAML that contradicts the prose, and any stage whose acceptance can only be met by editing files missing from that stage's files:.

3. CLAIM GROUNDING. Every symbol, path, signature, config option, library convention (UV/axis/order/defaults — read the installed source), third-party API shape and rate limit, and package/version/peer-graph fact — verify against the installed source, the live registry, or the actual config, never memory. For every specced function, check its inputs actually carry what its stated behavior and output type require. Flag edits anchored by line number instead of symbol.

4. VERIFICATION HONESTY. For each acceptance/wiring gate: would a plausible-WRONG implementation still pass? Does the gate exercise what it claims? (A production build cannot verify a module nothing imports; a bundler neither type-checks nor compiles shader graphs; a grep proves existence, not behavior; a test of an in-memory value proves nothing about the emitted artifact.) Does every code stage run the repo's FULL canonical gate verbatim rather than a scoped subset?

5. EXECUTABILITY & LIFECYCLE. Sandbox allow_write covers every path acceptance commands write (the lockfile by its real name, build output dirs); working_dir is right; anything shipped as an engine/driver/controller has a stage owning its composition-root call site with an executable wiring proof; runtime lifecycle decisions (ownership, scheduling, cancellation, invalidation, dispose, budget scope and ordering) are settled in the plan, not deferred to integration-verify.

Rank findings by severity: blocks execution > silently ships broken > under-specified > polish.

Write your report to a file named codex-<basename of <PLAN>> in the SAME directory as the plan (for example, when <PLAN> is doc/plans/PLAN-foo.md write doc/plans/codex-PLAN-foo.md). Overwrite that file if it already exists. Make every finding concrete: cite files and line numbers, and say what the plan should add or change. Provide a high-level summary here as well.

⚠️ Do NOT edit the plan itself. Your only written output is the review file — the plan's author reads your review and decides what to fold back into the plan.
