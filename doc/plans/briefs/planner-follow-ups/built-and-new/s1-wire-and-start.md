# S1: the Built wire field and the follow-up start (R5, R12 server side)

Tier: sonnet (`loom-software-engineer`). Read `common.md`, then this brief, then
`doc/loom/knowledge/architecture/planner-feature-screens.md` ("Wire and names") and
`architecture/planner-uploads-and-drafts.md` ("The opening in the transcript") before code.
Anchor every edit by symbol; line numbers below are advisory (earlier stages of this plan edited
these files).

## What you build

Every feature checklist on the wire gains `built`: what the feature's done plans left in their
run reviews. The web's Built page (R5) and the `/new` follow-up suggestions (R12) both read it.
And the `plan-follow-ups` start, which the checklist offers only once every plan is done,
becomes startable from `/new` for any feature that has its metaplan.

## The run review parser is follow-ups-core's (do not add another)

`loom::planning::run_review::{review_items, ReviewSection, ReviewItem}`:
`review_items(review: &str, section: ReviewSection) -> Vec<ReviewItem>`,
`ReviewSection::{SpecConformance, OpenQuestions, OpenFindings}`,
`ReviewItem { stage_name: String, stage_id: String, text: String }` (derives `Debug, Clone,
PartialEq, Eq` only, so map it into the wire types below). `fs::plan_review` writes the sections
`## Spec Conformance`, `## Open Questions` and `## Open Findings` (headings as pub consts) and
every bullet with `review_item_line` as ``- **<stage name>** (`<stage-id>`): <text>``. Read both
before code (`rg -n 'pub fn review_items|pub fn review_item_line|pub const' loom/src/planning/run_review.rs loom/src/fs/plan_review`).

## 1. Run review paths: `loom/src/plan_server/review/built.rs`

`fn review_paths(project, slug)` becomes `pub(crate) fn run_review_paths(project: &Project,
slug: &str) -> Vec<PathBuf>`, sorted by path. It accepts a review whose parent is the plan's home,
the home's `archive/`, `plans_dir(None)` (`doc/plans`) or `doc/plans/archive`.
`fs::plan_review::generate` writes `doc/plans/REVIEW-<plan file stem>.md` at the top of
`doc/plans/` even for a feature plan (`generate`, `plans_dir = output_root.join("doc/plans")`),
and `planning/kinds.rs` classifies that file as `DocKind::RunReview { slug }`, so a reader that
looks only in the feature's home never finds it (today's `built_notes` misses every such review).
Update the one caller (`built_inputs`) and the doc comment. `loom/tests/plan_review_contracts.rs`
must stay green unedited.

## 2. The wire field: `loom/src/plan_server/wire/checklist_built.rs` (new)

Declared from `checklist.rs` beside its siblings: `#[path = "checklist_built.rs"] mod built;`
and `pub use built::{BuiltView, CandidateKind, CandidateView, RunReviewView, SpecItemView};`.
Derives as the sibling views (`Debug, Clone, PartialEq, Eq, Serialize, Deserialize`,
`#[serde(deny_unknown_fields)]`; `Default` on `BuiltView`; `CandidateKind`
`#[serde(rename_all = "snake_case")]`). Every field gets a doc comment.

```rust
pub struct BuiltView {
    /// The newest run review of each done plan that has one, in plan-row order.
    pub reviews: Vec<RunReviewView>,
    /// The `## Spec Conformance` items of those reviews: what integration-verify changed in the
    /// spec or the build to make them agree.
    pub spec: Vec<SpecItemView>,
    /// Their open findings, then their open questions: the candidate follow-ups.
    pub candidates: Vec<CandidateView>,
}
pub struct RunReviewView { pub slug: String, pub path: String }
pub struct SpecItemView { pub stage: String, pub stage_id: String, pub text: String, pub review: String }
pub enum CandidateKind { Finding, Question }
pub struct CandidateView {
    pub kind: CandidateKind,
    pub stage: String,
    pub stage_id: String,
    pub text: String,
    pub review: String,
}

/// What the feature's done plans left in their run reviews.
pub(super) fn built_view(project: &Project, rows: &[PlanRowView]) -> BuiltView;
```

`built_view`, for each row in order whose `project.tree.plan(&row.slug)` state is
`PlanState::Done`:

1. `run_review_paths(project, &row.slug)`; read each with `project.text(path)` (the cached
   source; skip an unreadable one) and keep the newest by
   `crate::plan_server::review::built::parse_generated`, the last path when no stamp parses.
   None: the plan contributes nothing.
2. Push `RunReviewView { slug, path }` (the path as `PlanRowView.path` spells it, `doc/` prefix).
3. `spec`: every `review_items(text, ReviewSection::SpecConformance)` item.
4. `candidates`: every `OpenFindings` item as `Finding`, then every `OpenQuestions` item as
   `Question`. `stage` is the item's `stage_name`, `stage_id` its `stage_id`, `review` the path.

Wire it in `ProjectChecklistView::from` (`checklist.rs`), after `card::card_facts`:
`view.built = built::built_view(project, &view.plan_rows);`. `ChecklistView` gains
`pub built: BuiltView` after `items`; `impl From<&Checklist> for ChecklistView`
(`checklist_rows.rs`) sets `built: BuiltView::default()`. Add
`"built": {"reviews": [], "spec": [], "candidates": []}` after `"items"` to every checklist
object in `loom/src/plan_server/wire/fixtures/checklist.json`, `checklist-message.json` and
`hello.json` (hand-edited: `checklist_fixture_matches_serde_output` and the message fixture
tests only read them).

Unit tests in `loom/src/plan_server/wire/checklist_built_tests.rs`
(`#[cfg(test)] #[path = "checklist_built_tests.rs"] mod tests;` inside `checklist_built.rs`):
the newer of two reviews of one plan wins by its Generated stamp; a feature with no review gives
an empty view; a review in the plan's home and one at the top of `doc/plans/` are both found;
and one round trip: a review whose three sections are written with `fs::plan_review`'s heading
consts and `review_item_line` comes back as the same items on the wire (so the wire reads what
the generator writes). Do not touch `wire/mod.rs`: the contract session adds
`#[cfg(test)] mod built_contract_tests;` there.

## 3. The follow-up start: `loom/src/planning/guidance/start.rs`

The checklist offers Plan follow-ups only in `ReadyAllDone`; `/new` starts it for any feature
with its metaplan, done or not (R12). In `AuthoringStart::from_offer_id`, before the checklist
lookup, route `Offer::PlanFollowUps.id()` (`plan-follow-ups`; `Offer::PlanFollowUps` is a unit
variant) to:

```rust
/// Whether Plan follow-ups can start in `feature`: the feature has its metaplan.
pub fn follow_ups_open(project: &Project, feature: &str) -> bool;

/// Plan follow-ups for a feature that has its metaplan, done or not; the checklist offers it
/// only once every plan is done, so it is not looked up there.
fn follow_up_start(project: &Project, feature: Option<&str>, id: &str)
    -> Result<AuthoringStart, StartError>;
```

`None` gives `StartError::FeatureRequired`; a feature without a metaplan (`feature.metaplans`
empty) or unknown gives `StartError::NotOffered(id)`. Leave `#[cfg(test)] mod tests;` and the
contract scaffold line (`#[cfg(test)] mod follow_up_contract_tests;`) alone. Add one test to
`start/tests/lookup.rs`: a feature with a written plan (not done) starts it; `None` is
`FeatureRequired`.

`loom/src/plan_server/chat/guidance.rs` `offered()` (the fallback provider until
`PlanGuidance` replaces it): return `follow_ups_open` for that id with the selected feature
(false with none), before the checklist scan. One test in `plan_server/chat/guidance/tests.rs`
beside the existing `offered` cases.

A start with an offer id and a first message already gets `"<opening>\n\n<text>"`, the typed
part shown as the user's message (`plan_server/guidance/provider.rs` `authoring_in_feature`);
the web seeds candidates through it, so no server change is needed for seeding.

## 4. The product spec

`summary-fragments` edited `doc/product/feat-planner/section_05.md` before you: anchor by text.

`doc/product/feat-planner/section_05.md`:

- Route table: the `run` row's "Renders" becomes `run state, each plan's state, and a link to
  the Built page once every plan is done`; add after it
  `| /f/<feature>/built | the Built state: done plans with their run reviews, the spec check's outcomes, candidate follow-ups and free text, each starting Plan follow-ups (section 11) | Run |`
  (backticks on the route, column widths re-aligned). If the table still has a
  `/f/<feature>/metaplan[/<file>]` row, it becomes `/f/<feature>/metaplan`, the feature's one
  metaplan (`doc/plans/<feature>/index.md`).
- The landing sentence: a feature whose plans are all done (the checklist's current step is
  ready, all done) opens on `/f/<feature>/built`; the existing three cases follow.

`doc/product/feat-planner/section_08.md`, the `/new` paragraphs: the empty form is one field
with the kind of work as a three-way switch on its top edge (Feature, Standalone plan,
Follow-up); Follow-up opens a picker of the features that have a metaplan, done or not, each
with its state and plan count; the picked feature's open findings and open questions from its
run reviews appear as suggestions under the field; Start for a follow-up starts Plan follow-ups
for that feature with the typed text and opens the feature's metaplan page with the chat; the
Built page's "Start it on New work" arrives with the feature picked. Keep the kickoff and draft
paragraphs that still hold.

## Files

Your row in the stage's worker table. Never edit S2's, S3's, F1's or F2's files, `wire/mod.rs`,
or follow-ups-core's `planning/run_review.rs`.

## Run once at the end

`cargo test --manifest-path loom/Cargo.toml --lib -- planning::guidance::start:: plan_server::wire:: plan_server::chat::guidance:: && cargo test --manifest-path loom/Cargo.toml --test maintainability`

## Report

Files changed; tests run and their result; anything you could not settle.
