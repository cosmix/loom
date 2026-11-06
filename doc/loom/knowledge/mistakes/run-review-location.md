# Run Review Location

>

## A feature plan's run review is written where nothing looks for it

**What happened**: `fs::plan_review::generate` (and `fs/plan_lifecycle/commit.rs`) write `doc/plans/REVIEW-<plan stem>.md` at the top of `doc/plans/` for every plan, feature plans included, while `kinds::classify` gives `DocKind::RunReview` only to a review in the plan's own home or its `archive/`, and `plan_server/review/built.rs` searches only there. A feature plan's run review is never found by the planner.
**Why**: The run review predates feature directories under `doc/plans/<f>/`; the writer kept the old flat location when the readers moved to the plan's home.
**Prevention**: When a document's home moves, `rg` every writer of the old path and every reader of the new one, and add a test that writes through the real generator and reads through the real classifier.
**Fix**: The readers accept the writer's location: `kinds::classify` gives `RunReview` in a plan home, its `archive/`, the top of `doc/plans/` and `doc/plans/archive/`, and `run_review_paths` (`plan_server/review/built.rs`) searches the same four places, so the Built page reads each done plan's newest review ([run reviews](../architecture/planner-feature-screens.md#run-reviews)). The writer was left where it is.
