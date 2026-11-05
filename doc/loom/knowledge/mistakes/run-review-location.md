# Run Review Location

> Feature run reviews written to doc/plans/ top, read only from the plan home

## A feature plan's run review is written where nothing looks for it

**What happened**: `fs::plan_review::generate` (and `fs/plan_lifecycle/commit.rs`) write `doc/plans/REVIEW-<plan stem>.md` at the top of `doc/plans/` for every plan, feature plans included, while `kinds::classify` gives `DocKind::RunReview` only to a review in the plan's own home or its `archive/`, and `plan_server/review/built.rs` searches only there. A feature plan's run review is never found by the planner.
**Why**: The run review predates feature directories under `doc/plans/<f>/`; the writer kept the old flat location when the readers moved to the plan's home.
**Prevention**: When a document's home moves, `rg` every writer of the old path and every reader of the new one, and add a test that writes through the real generator and reads through the real classifier.
**Fix**: Pending: doc/plans/PLAN-planner-follow-ups.md, stage built-and-new (S1 widens the search to the top of `doc/plans/`).
