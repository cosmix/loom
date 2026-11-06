# Planner Design Brainstorm

> Stubs

## Stubs were mistaken for summaries in a planner redesign proposal

**What happened**: Brainstorming a planning graph, an agent proposed letting the vision's section paragraphs and metaplan entries (stubs) replace the generated summaries and overviews, deleting most of the summary engine.
**Why**: Stubs and summaries look alike (short prose above a long document), and the agent did not ask what each is for.
**Prevention**: Stubs exist BEFORE the detailed document and scope it; summaries are generated AFTER it is finalized, to help a human review, understand and revise long documents (plans with many briefs above all). Never merge the two or propose one in place of the other.
**Fix**: Corrected by the user on 2026-11-05; the graph proposal keeps both node kinds.

## Plan briefs were left out of the summaries

**What happened**: The feat-planner vision ruled that briefs get no summaries of their own and are reached only through their plan's summary and file mode, marked (my call).
**Why**: Briefs were treated as supporting files; they hold much of a plan's reviewable content, so a reviewer of a plan summary misses it.
**Prevention**: Anything a reviewer has to approve before a run, briefs included, needs a summary path.
**Fix**: Pending: the user named it a mistake on 2026-11-05; a redesign must summarize briefs.

## A plan's new line grammar collided with existing documents

**What happened**: The first draft of PLAN-planning-trace named the settle mark `Status: settled ...` and reported any other `Status:` line as a problem; the feat-planner vision already opens with `Status: agreed vision (2026-10-26).`, so every load would have reported it. The same draft also had the trace read brief bundles with `briefs::plan_briefs`, which reads the disk for the run preflight, not the loaded project.
**Why**: New line grammars and helper reuse were designed from the brainstorm and the explorers' summaries without grepping `doc/` for the chosen prefixes or reading the helper's signature.
**Prevention**: Before a plan introduces a line prefix in documents, `rg '^<prefix>' doc` for collisions; before naming a helper for reuse, read its signature and inputs (project vs disk).
**Fix**: Renamed the mark to `Settled:` and added `briefs::bundle_files` over the loaded project, shared by the trace and the summary store.

## Trace mockups showed every concept at once and were checked only in Chrome

**What happened**: The first planning-trace mockups put every trace concept on the feature page at the same time: settle diamonds in a new spine gutter, five frontier hints, three rows of heading pills, a stub panel, and edge chips floating in a right margin. They were verified only in headless Chrome. In the user's Firefox the spine overflowed its column, ghost chips leaked, and nothing aligned. The user judged the result broken, confusing, overly complex and ugly.
**Why**: The brief listed every element of common.md 'The browser' and asked for all of them to be drawn, so the designer added surfaces instead of designing an experience. Visual verification used a different browser from the user's.
**Prevention**: A mockup brief states the user task each screen serves and a budget of new elements per surface (show state only where it needs attention; detail on demand). Verify in the user's browser (Firefox here) at real window sizes, and have the orchestrator read the screenshots before showing the user.
**Fix**: Pending: a redesign brief with a simplicity budget, verified in Firefox.

## The second trace mockup round was bland because the brief asked for plain

**What happened**: After the first round's clutter, the redesign brief asked for 'existing page plus small additions', 'one quiet line', 'a plain table' and 'a plain list'. The designer produced aligned but bland screens with chrome-sized type for content, a settle action nobody could find, a collapsed Scope line in small type, a spine row carrying four marks, and no visible way to keep working on a document. The user judged it ugly and uninspiring.
**Why**: The orchestrator overcorrected in the brief and gave no quality bar, so 'plain' was followed literally. A design skill does not rescue a brief that asks for plain.
**Prevention**: A design brief names an approved quality bar (here the Hearth and Threshold mockups), states minimum content type sizes, describes each screen's user task and hierarchy, and limits clutter by rules (one state glyph per row) rather than by asking for plainness. Show the editing loop (chat open) when the screens exist to support it.
**Fix**: Round 3 briefed with those rules.

## Spec sections invented product features the vision never asked for

**What happened**: Writing the feat-planner spec sections from the vision, an agent turned one sentence about a returning author ('revise the spec or add plans in light of what was built') into two done-state offers, 'Compare spec with build' (with a new loom-plan:retro skill) and 'New metaplan round', and later plans implemented both. The user never asked for either and rejected the first.
**Why**: A user description in the vision was read as a feature request, and spec sections were treated as license to add UI and skills.
**Prevention**: A spec section adds a user-facing action, button or skill only when the vision names it; a user story the vision describes gets an open question in the section, not an invented feature. Mark any such addition '(My call.)' and raise it with the user before plans build it.
**Fix**: The plan planner-follow-ups removed both offers and the `loom-plan:retro` skill. Spec and build are aligned inside each run by integration-verify's spec-conformance duty ([the Product Sections block](../architecture/signal-generation.md#product-sections-block-integration-verify-only)); follow-up work is the Built page's single `Plan follow-ups` offer, which appends entries under `## Follow-ups` in the feature's `index.md` ([Built page](../architecture/planner-feature-screens.md#built-page)).

## A feature plan's run review is written where nothing looks for it

**What happened**: `fs::plan_review::generate` (and `fs/plan_lifecycle/commit.rs`) write `doc/plans/REVIEW-<plan stem>.md` at the top of `doc/plans/` for every plan, feature plans included, while `kinds::classify` gives `DocKind::RunReview` only to a review in the plan's own home or its `archive/`, and `plan_server/review/built.rs` searches only there. A feature plan's run review is never found by the planner.
**Why**: The run review predates feature directories under `doc/plans/<f>/`; the writer kept the old flat location when the readers moved to the plan's home.
**Prevention**: When a document's home moves, `rg` every writer of the old path and every reader of the new one, and add a test that writes through the real generator and reads through the real classifier.
**Fix**: The readers accept the writer's location: `kinds::classify` gives `RunReview` in a plan home, its `archive/`, the top of `doc/plans/` and `doc/plans/archive/`, and `run_review_paths` (`plan_server/review/built.rs`) searches the same four places, so the Built page reads each done plan's newest review ([run reviews](../architecture/planner-feature-screens.md#run-reviews)). The writer was left where it is.

## Built page shipped with invented actions and no navigation

**What happened**: The feature-name header opened `/f/<feature>/built`, a page reachable from nowhere else, almost unstyled, offering 'Run review' (opens a review document, runs nothing), an empty 'spec check' section, 'candidate follow-ups' listing source-code files, and a 'Plan this' button. The spine also showed a lowercase 'product vision' row (the next-step sentence falling back to a raw checklist item). The user rejected all of it: never requested, never planned.
**Why**: Spec section 05/11 described a done-state page, and implementing agents built every listed element without checking it against the vision or judging usability.
**Prevention**: Every screen needs a named user task from the vision and a navigation entry the user can find; a label must describe what the control does; a section with no content is not rendered. Review UI against a screenshot in a real browser, not only against the spec text.
**Fix**: The Built page, its route and its wire (spec check, run review list) were removed; the feature-name link opens the product page, else the plans overview. Follow-up candidates stay as suggestion pills on New work.
