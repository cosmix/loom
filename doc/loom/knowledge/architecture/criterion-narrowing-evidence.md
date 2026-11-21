# Criterion Narrowing Evidence

> base-run evidence, leaves_out narrowing rules, hand-offs

## Criterion Narrowing, Base Evidence and Hand-Offs

An adjudicator may delete or narrow an acceptance criterion that cannot pass whatever the implementation, but only with evidence loom produced, and what it leaves out is handed to a later check. The stage agent never narrows its own checks.

### Base run

`loom check <stage> --criterion <n> --at-base` (a session relays it; `commands/verify.rs` refuses it without a session, `AT_BASE_NEEDS_SESSION`) runs the criterion confined in a checkout of the stage's base commit, with the stage's build cache (`host_verify/child_base.rs`). The result is a `BaseEvidence` in `reviews/<stage>/base-evidence.json` (`verify/review/handoffs.rs`): `index`, `command`, `base` (the commit it ran at), `passed`, `executed`, `failed`, `build_failed`, `timed_out`. A later run of the same index and command replaces the earlier one.

The checkout lives in the stage cache (`stage_cache/base_checkout.rs`), so a second base run is warm:

- Path `<cache base>/loom/stage-base/<stage cache name>/<base>`, mode 0700, opened no-follow; the build cache is its sibling `cache-<base 12 hex>`, not nested under `<base>/`.
- Filled and judged clean with `GIT_DIR=<repo common dir>`, `GIT_WORK_TREE=<checkout>` and a private `GIT_INDEX_FILE`; git is never discovered inside the checkout, whose `.git` an agent could plant.
- Ignored build output is kept on reuse (no `git clean -fdx`); `ensure_base_dirs` prunes the checkouts and caches of every other base when one is made; `stage_cache/cleanup.rs::remove_stage_cache` runs both removals and reports the first error.
- Impact-test selections run with per-command scratch caches, not the stage cache (`verify/impact_tests.rs::criteria_config`), so a base provision does not warm what an impact base run reads.

### Verdict rules

`plan_patch.leaves_out: [{index, tests, runs_at, reason}]` accompanies a `delete` or a narrowing `replace` of `acceptance` (`orchestrator/adjudication/verdict_narrowing.rs::check_narrowing`, run at recording by `validate_in` only when `plan_patch.field` is `acceptance`, and again at apply time). `runs_at` is `integration-verify`, `ci` or `nowhere`.

- A `delete` needs a `leaves_out` entry; an entry needs a patch that amends its index.
- `check_run` demands base evidence for the criterion that failed with tests executed. A criterion that passes at the base, does not build there, executes zero tests, or fails fewer tests than the entry lists, is the stage's own failure: the judge is told to answer `needs-more-evidence` (or, for a frozen contract, to have the agent file `dispute-contract` with a replacement).
- `at_current_base` keeps only runs whose `base` equals the stage's base read now (`locate_stage_site` and `stage_base`), at record time and again at apply time; an unreadable base refuses the narrowing.
- A `replace` equal to the current entry is refused (`same_entry` compares the value parsed as an `AcceptanceCriterion`).
- A refusal at apply time routes to `apply_needs_more_evidence` with `EVIDENCE_ACTION` and amends nothing (`apply_accept.rs`).

Limits that stay: adapters report counts, not test names, so a base run that fails one pre-existing test lets a `leaves_out` entry name the stage's own new test; a `delete` is checked by failed count against the tests listed, not against the number executed; a narrowing the judge does not declare in `leaves_out` is not detected mechanically, only listed.

### Hand-offs

Each accepted entry becomes a `Handoff` (`stage`, `field`, `index`, `tests`, `runs_at`, `reason`, `dispute`, `verified`) in `reviews/<stage>/handoffs.json`. `verified` is false for a timed-out or unparsed base run and for a deleted contract. They are computed against the stage's list before the amendments and recorded after (`apply_accept::narrowing_handoffs`, `apply_contract_rulings::amend`); `record_handoffs` dedups on replay and `forget_handoffs` undoes them when the amendment does not land. Readers: the integration-verify signal (`orchestrator/signals/handoffs.rs`), the plan completion summary (`commands/status/render/summary.rs`) and the status TUI (`commands/status/ui/tui/handoffs.rs`), which list every hand-off and every amendment of the plan.

A deleted contract is handed off the same way: with no confined formulation of a contract, the adjudicator deletes it and the deletion is recorded.
