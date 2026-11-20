# Brief: base-provision (provision the impact-tests base checkout)

Tier: `loom-software-engineer` (sonnet). Read `doc/plans/briefs/open-github-issues/common.md` first.

## Owns

`loom/src/verify/impact_tests/base.rs`, `loom/src/verify/impact_tests/base_tests.rs`, `loom/src/verify/impact_tests.rs`, `loom/src/host_verify/child_base.rs` (exists only once the in-progress dispute-guards plan merged).

Frozen, read-only: `loom/src/verify/impact_tests/base_contract_tests.rs`.

## Facts (verified)

- `compare` (`base.rs:155-198`) creates a scratch `BaseCheckout` (`create` 93-99, `check_out` 113-129: `git clone --shared --no-checkout`, `checkout --detach`; tracked files only) only when a selection failed. `at_base` (209-231) maps `Run::Inconclusive` to `Err`, and `stands` (234-244) keeps the failure. A JS package without `node_modules` is Inconclusive (`runs.rs:58-62`, `missing_environment` 249-260).
- The completion process is unconfined (`host_verify/child.rs:235`); its commands run confined. Provision entries: `orchestrator::provision::read_provision_snapshot(work_dir)` (`provision.rs:87`, the `[plan_provision]` snapshot `loom init` writes). Runner: `orchestrator::provision::run_provision_in_stage(entries, worktree, Option<&StageCache>)` (`provision.rs:47`), confined with `NetworkAccess::Provision`, 600 s per entry, stops at the first failure, refuses a missing or escaping directory. Precedent: the dry run provisions a scratch base (`commands/plan/verify/dry_run.rs:92-95`).
- `BaseCriteria` (`base.rs:58-74`) passes no stage cache; neither do the worktree selections (`impact_tests.rs:92-101` `criteria_config`). Add the stage cache to the base run so a provision warms the same cache the run reads (`stage_cache::StageCache::for_stage`-style constructor: find it with `loom map --find-all StageCache`).

## Change

1. `pub(crate) fn provision_with<F>(entries, root, run: F) -> Result<(), String>` exactly as the stage PUBLIC SURFACE states: runs every entry in order through `run`, stops at the first `Err`.
2. `compare`: after the checkout is created, `provision_with(entries, checkout.root(), |e, r| run_provision_in_stage(slice::from_ref(e), r, Some(&cache)))`. `BaseProbe` gains `work_dir` and the stage cache.
3. Judging: a provision `Err(reason)` makes every re-run selection a note `base not runnable: <reason>`; a run still Inconclusive for a missing environment after provisioning is the same note. Build failures and unreadable output at base still stand (`stands`). Update the module docs (1-25), which describe the old rule.
4. `child_base.rs` (if present): provision its persistent base checkout with the same helper before its run.
5. `impact_tests.rs`: pass `work_dir` and the stage cache into `BaseCriteria`/`BaseProbe`; declare the contract module.

## Tests (`base_tests.rs`)

With a fake runner closure: entries run in order and stop at the first failure; a provision failure yields the note; a successful provision lets the base run compare counts as before. Existing tests keep passing (`a_selection_failing_as_badly_at_the_base_is_a_note`, `a_selection_that_does_not_build_at_the_base_either_stays_a_failure`).

## Done

`cargo test --manifest-path loom/Cargo.toml --lib verify::impact_tests::base` passes with the frozen contract.

## Traps

- The note is the operator's decision (plan Decisions item 6); integration-verify's full suite is the backstop. Say so in the module docs.
- `impact-acceptance` owns `failure.rs` and adds `ImpactFailure::with_digest_of(self, full: &[String]) -> Self` (pinned). In `stands`, end the construction with `.with_digest_of(&failed.tests())`; it compiles once that worker's change is on disk.
