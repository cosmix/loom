# B3: overview restatement guard, overview model default, run strip count (#1, #6)

Tier: sonnet (`loom-software-engineer`). Read `../common.md` first.

## Part 1: an overview sentence must not restate the sentence it cites (#1)

Concept mode has three levels. An overview sentence is level 0. It cites document-summary sentences (`#s-xxxxxxxx` links), which are level 1. A document-summary sentence cites source passages, which are level 2. Opening an overview sentence shows the member sentence it cites, so a copied sentence opens to itself.

Measured at planning time: 103 overview-to-member pairs resolve in `doc/loom/summaries/overview/**`. Two are identical, for example `overview/feat-planner/product.md:55` and its cite `section_05.md#s-7c6230dc`. Under the measure below, 18 pairs score at least 0.75. A pair at 0.78 still restates its member ("File mode adds the server's document, tree, save, diff and search routes, with in-process plan validation." against "File mode is the planning app's reader and editor, adding server document, tree, save, diff and search routes plus in-process plan validation.").

**Measure (settled):**

- Tokenise each text into its word set: take the sentence text with links removed (the same text `format/body.rs::sentence_id` hashes), lowercase it, and keep runs of ASCII letters and digits.
- similarity = 2·|A∩B| / (|A|+|B|), the Dice coefficient. Empty sets give 0.
- An overview sentence is rejected at seal when its similarity to **any single** member sentence it cites is **≥ 0.75**.
- Rejection message: `sentence <overview sentence id> restates <member>#<member sentence id>; say what the cited sentences add up to instead of repeating one`.

Where:

- `loom/src/planning/summary/seal/links.rs::sentence_problem` (line ~126) resolves the cited member sentence `s` but does not receive the overview sentence's own text. Thread that text through, add the check after the `Current` match, and keep the function under 50 lines. Put the measure in a new module `loom/src/planning/summary/seal/restate.rs` exposing `pub(super) const RESTATE_THRESHOLD: f64 = 0.75;` and `pub(super) fn restates(overview_text: &str, member_text: &str) -> bool`, with unit tests (identical, 0.78-style restatement, a two-member compression that scores under 0.75 against each member and is not rejected, empty text). `links.rs` calls it as `restate::restates(`. The plan's wiring check greps that call.
- `links::check` runs inside `seal.rs::build_text` for every seal; `force` does not bypass it, and no status, page-model, evaluate or CLI path re-runs it, so stored overviews stay Current until their next rebuild. Keep it that way: do not add the check to `evaluate` or the page model.
- Document summaries are unaffected: they cite only source anchors.
- The frozen contracts (`seal/restate_contract_tests.rs`) build an overview draft that is valid in every other respect; your check must report the restatement as its own `FormatError` whose message contains `restates`.

Tell the model the rule so it does not spend a seal round on it. Add one line to the overview instructions in:

- `plan-plugin/agents/overview.md`, beside "Preserve current sentences word for word" (about line 23). This is the overview agent; `agents/summary.md` is the document agent and does not change.
- `plan-plugin/skills/summaries/SKILL.md`, beside "Preserve sentences it identifies as current word for word" (about line 42).
- `loom/src/planning/guidance/text/summary_codex.md` (the overview part, beside its "Preserve current sentences word for word", about line 40).
- `loom/src/planning/summary/brief.rs::write_rules`, overview branch.

Each line says: a sentence that repeats the sentence it cites is rewritten to say what the cited sentences add up to, even when the brief marks it `[keep]`. `brief.rs` prints every Current overview sentence as `[keep]` (about line 181) and stays as it is: `SealError::Rejected` lists every problem of a draft at once, so the 18 stored restating sentences cost at most one rejected seal per overview on the next rebuild.

Check whether a test pins these texts byte for byte (`rg -n 'word for word' loom/src loom/tests plan-plugin`) and update it.

## Part 2: overview model default back to sonnet (#1)

`loom/src/user_config/plan.rs:13` `DEFAULT_PLAN_OVERVIEW_MODEL: &str = "haiku"` becomes `"sonnet"`. Commit `0102ef95` set it. Run `git show 0102ef95` for every consumer that commit touched. Update each test and doc that states the old default:

- `user_config/plan/tests.rs`, `user_config/plan_keys_tests.rs`, `user_config/tests.rs`, `planning/summary/config/tests.rs`, `planning/guidance/models/tests.rs`, `tests/planning_summary_cli_contracts.rs`.
- `loom/tests/summary_engine_contracts.rs` `summary_models_default_to_haiku_high` (about lines 26-30): the overview assertions become sonnet and the test name says so; the document assertions stay haiku.
- `web/src/api/fixtures/config.json`, the `plan.overview_model` entry (about lines 571-590): its `default`, `user.value`, `project.value` and `effective.value` become `"sonnet"`. `commands::status::web::config_api::tests::the_config_fixture_matches_a_real_payload` compares this fixture with a real payload as parsed JSON, so a hand edit of those four values is enough; keep the file's formatting.
- `web/src/components/settings-plan-section.test.ts` (about line 44): `["plan.overview_model", "haiku"]` becomes `"sonnet"`.
- `plan-plugin/agents/overview.md` frontmatter `model: haiku` becomes `model: sonnet`, as it was before `0102ef95`. `planning/guidance/plugin.rs` rewrites the model from config when it renders the agent, so this is for consistency only.
- `README.md` (the `plan.overview_model` row, about line 660).
- `doc/product/feat-planner/section_04.md`, and `section_09.md` lines ~460-462 and line ~27 ("haiku, high" for the background summary).

`document_summary_model` stays haiku. Editing `section_04.md` and `section_09.md` makes their stored summaries and the feat-planner product overview stale in the planner; that is expected and is not a regression.

## Part 3: run strip merged count (#6)

The planner shows "plan · 0 of 6 stages merged" for a plan whose stages all merged. `loom/src/plan_server/run/strip.rs::build_strip` sets `merged = data.merge.merged.len()`. That count comes from `collect_merge_summary` (`status/data/collector_snapshot.rs`), which counts a stage only when `git merge-base --is-ancestor` proves its `completed_commit` reaches a merge point that falls back to `"main"`. A plan merged into another base branch, a squash merge, or a cleaned-up branch therefore counts 0. The dashboard counts the stage's own `merged` flag (`web/src/lib/notify.ts:76`).

Fix: `merged` counts the stages whose `merged` flag is true (`data.stages`, `StageSummary.merged`). `total` stays as it is. Leave `collect_merge_summary` alone: other consumers use it. `git/merge/status.rs::check_merge_state` deliberately maps a merged non-Knowledge stage with no verifiable commit to `MergeState::Unknown` (the phantom-merge guard) and counts a merged Knowledge stage from its flag. Do not change that guard: the strip is a display count that matches the dashboard. The existing `strip/tests.rs` fixture writes Knowledge stages, which is why its test already sees merges; the frozen contract uses Standard stages, one of them unmerged.

## Contracts (frozen before you start; make them pass)

- `loom/src/planning/summary/seal/restate_contract_tests.rs`, tests `planning::summary::seal::restate_contract_tests::overview_restating_its_member_is_rejected` and `planning::summary::seal::restate_contract_tests::overview_near_restatement_is_rejected` (the 0.78 pair above).
- `loom/src/user_config/overview_default_contract_tests.rs`, test `user_config::overview_default_contract_tests::overview_model_defaults_to_sonnet`.
- `loom/src/plan_server/run/strip/merged_contract_tests.rs`, test `plan_server::run::strip::merged_contract_tests::all_merged_stages_count_as_merged`.

## Files you own

`loom/src/planning/summary/**` (except frozen contract files), `loom/src/planning/guidance/text/summary_codex.md`, `loom/src/planning/guidance/models/tests.rs`, `plan-plugin/skills/summaries/SKILL.md`, `plan-plugin/agents/overview.md`, `loom/src/user_config/plan.rs`, `loom/src/user_config/plan/**`, `loom/src/user_config/plan_keys_tests.rs`, `loom/src/user_config/tests.rs`, `loom/tests/planning_summary_cli_contracts.rs`, `loom/tests/summary_engine_contracts.rs`, `web/src/api/fixtures/config.json`, `web/src/components/settings-plan-section.test.ts`, `loom/src/plan_server/run/strip.rs`, `loom/src/plan_server/run/strip/tests.rs`, `README.md`, `doc/product/feat-planner/section_04.md`, `doc/product/feat-planner/section_09.md`.

## Proof

Run each once at the end:

- `cargo test --lib planning::summary::`
- `cargo test --lib user_config::`
- `cargo test --lib plan_server::run::`
- `cargo test --lib commands::status::web::config_api::`
- `cargo test --test planning_summary_cli_contracts`
- `cargo test --test summary_engine_contracts`
- `cd ../web && node_modules/.bin/vitest run src/components/settings-plan-section.test.ts`
