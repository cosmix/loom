# Integration verification: stage `integration-verify`

The stage's acceptance runs the full pre-push gate and the hook suites. This file lists the review focus and the functional checks the acceptance cannot express. Record every result with `loom memory note` (and a `stale-knowledge:` note for each knowledge file the tree contradicts).

## Code review focus

Spawn the reviewers the signal's review procedure names, with these focus areas:

- **Security and data loss:** `deserialize_stage_model` (`loom/src/plan/schema/types_v2.rs`) rejects every value outside `CLAUDE_MODELS`; `archive_run_state` (`loom/src/fs/memory/archive.rs`) returns `Err` while journals exist, and none of its three callers (`commands/init/cleanup.rs`, `commands/clean/mod.rs`, `fs/plan_lifecycle.rs`) deletes the state directory or renames the plan after an `Err`; the hook edits quote every variable and keep `set -u` safe.
- **Architecture:** the bookend lints (`v2_lints/bookends.rs`) resolve stage types with `detect_stage_type` and ancestry with `transitive_dependencies`, and register only in `v2_lints::run`; `check_knowledge_recommendations` moved to `validation/knowledge_recommendations.rs` with both callers passing a root.
- **Doctrine consistency:** every surface now agrees. Integration-verify curates no knowledge (`orchestrator/signals/format/sections.rs`, `cache.rs` IV prefix, `skills/loom-plan-writer/SKILL.md`). Recovery names `loom memory list --stage` (`CLAUDE.md.template`, `loom-hooks/session-start.sh`, the recitation section). Note prefixes `found:`, `gotcha:`, `found/gotcha:` are one class (signal, preamble, `NotePrefix::parse`). `loom review` is described as a review-document generator everywhere. Knowledge distill is never skipped (plan-writer `SKILL.md` and `references/bookend-stages.md`).
- **Test coverage:** each contract in `loom/tests/doctrine_guard_contracts.rs` fails on the wrong implementation its `rejects` names; the new hook test `loom-hooks/tests/learning-validator-stage-journal.sh` is registered in `run-all.sh`; `tests_doctrine_commands.rs` would fail on a misspelled command and skips prose fences; `v2_lint_bookend_tests.rs` covers both lints in both plan versions; `a_section_annotated_non_current_leaves_default_retrieval` (`loom/src/commands/knowledge/tests_annotate.rs`) has a control that retrieval finds the unannotated section.
- **Codex units:** `v2_lints/bookends.rs` and the `deserialize_stage_model` path were written by codex units; review them as closely as the Claude-written code (the four bookend messages are byte-exact with `v2_lint_bookend_tests.rs` and the contracts).

## Functional checks

Build the worktree binary (`cargo build` in `loom/`) and use `bin=$PWD/loom/target/debug/loom` from the repository root for these checks, not the `loom` on `PATH`: the installed binary has none of this plan's changes.

1. **The bootstrap fix on the real tree.** `"$bin" plan verify --json doc/plans/token-optim/<name>` for this plan (the plan file may be named `IN_PROGRESS-PLAN-01-...` once `loom run` starts; use the name present), then read `.warnings.knowledge`: it must be `[]`, because `doc/loom/knowledge/` holds content (the installed binary reported the knowledge-bootstrap warning there). Do not judge by `--strict`: on the merged tree the stage's positive `rg` criteria already pass, so the `baseline` bucket carries warnings ("criterion passes on the untouched tree", `plan/schema/validation/base_tree.rs`) and `--strict` exits 1 for that reason alone. A `knowledge` warning here is a failure of this stage: fix it or dispute it. Record the warning count per bucket and every warning left.
2. **The bookend lints on this programme's plans.** Run `"$bin" plan verify <plan>` on each of `doc/plans/token-optim/*PLAN-0[1-6]-*.md` (plan files only: the directory can also hold `codex-PLAN-*.md` review reports, which carry no METADATA block). Each must report no bookend error (messages containing `knowledge-distill stage` or `integration-verify stage`). A bookend error in another plan is a defect of that plan, not of this stage: record `loom memory note "cross-plan: <plan file> fails <message>"` for its author and continue. The plans are read from this worktree, which holds only committed files: for each of `PLAN-02` to `PLAN-06` that is absent, record `loom memory note "cross-plan: PLAN-0N missing from the worktree; bookend lints not checked"`. At planning time all six conformed (a re-run of the lint rules over each plan's `loom plan verify --json` levels).
3. **The four gate steps acceptance cannot run.** Under "Gate steps outside acceptance" below.

## Gate steps outside acceptance

Acceptance commands run confined: no network, and an empty scratch `CARGO_HOME`, bun cache and `TMPDIR` (`loom/src/verify/criteria/confine.rs:214`, `confine_host.rs:204-215`). These four steps of the repository's pre-push gate and CI need network or a populated cache, so the stage's canonical verifier runs them in the session sandbox (the plan grants every domain), from the repository root after `mkdir -p loom/target/gate-cache/bun loom/target/gate-cache/tmp`:

1. `git ls-files '*.md' | grep -v '^doc/plans/' | grep -v '^loom/tests/fixtures/' | BUN_INSTALL_CACHE_DIR="$PWD/loom/target/gate-cache/bun" TMPDIR="$PWD/loom/target/gate-cache/tmp" xargs bunx markdownlint-cli2` (no `--fix`): pass only on exit 0 with `markdownlint-cli2 v` in the output; without that banner the linter never ran.
2. `(cd loom && cargo audit --db "$PWD/target/gate-cache/advisory-db")`
3. `./scripts/flake-check.sh`
4. `TMPDIR="$PWD/loom/target/gate-cache/tmp" ./scripts/test-pre-commit-partial-staging.sh`

The verifier also runs the full test suite as the pre-push gate does: `env -u GIT_INDEX_FILE -u GIT_DIR -u GIT_WORK_TREE cargo test --all-targets --no-fail-fast` from `loom/`. A markdownlint finding goes to an engineer agent, who fixes it by hand without `--fix`.

The failed-run distill procedure the README now documents (copy `<archive>/memory/*.md` into the new plan's `.loom/work/memory/`) is proven by the unit test `archived_journals_copied_into_a_new_state_dir_are_pending` (`loom/src/commands/memory/handlers/tests_resolve_pending.rs`), which the full suite runs: the archive layout `archive_run_state` writes, copied into a fresh state directory, is listed by `pending_report`, and `strict_should_fail` holds until each entry has a receipt. Confirm in the review that the test drives those functions, not a copy of their logic.
