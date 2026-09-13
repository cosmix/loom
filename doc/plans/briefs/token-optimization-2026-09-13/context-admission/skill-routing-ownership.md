# Context admission: skill routing and ownership diagnostics

## Lane and boundary

Use the Terra implementation lane. This worker changes only the files listed below. Do not run git or proof commands; the stage orchestrator runs the listed commands after all workers finish.

## Owned files

- `loom-hooks/skill-trigger.sh`
- `loom/src/orchestrator/signals/format/skills.rs`
- `loom/tests/integration/hooks_skill_trigger.rs`
- `loom/src/plan/schema/structural_checks.rs`
- `loom/src/plan/schema/validation.rs`

This worker does not own the read ledger, polling guard, delivery records, worker-brief hook, or plan parser. Do not edit them.

## Grounded seams

`loom-hooks/skill-trigger.sh` is the UserPromptSubmit path. `main` calls `_tokens`, `_score_keywords`, `_add_project_matches`, `_rank`, then `_render`. `_score_keywords` gives an exact skill-name or multi-word keyword weight 2 and a generic keyword weight 1. `main` snapshots that lexical score as `keyword_scores` before `_add_project_matches`; `_render_one` currently turns any `keyword_scores >= MIN_SCORE` into the Codex instruction to read a full `SKILL.md`. Repository-type matches are intentionally only a tie-breaker.

`loom/src/orchestrator/signals/format/skills.rs::format_skill_recommendations` partitions detected project triggers from advisory matches. It then currently builds one `ordered` list and feeds it to `combined_loader_line`, which makes an advisory catalogued skill look mandatory. The direct caller is `format::sections::format_semi_stable_section`; its output reaches normal signal generation and recovery formatting.

`loom/src/plan/schema/structural_checks.rs::check_overlapping_files_without_dependency` checks only stage-level `files` patterns across the dependency DAG. `validation.rs::validate_structural_preflight` collects its warnings; its direct command callers are plan setup and `loom plan verify`. The existing plan worker-ownership table is free-form stage description text, so there is no structured worker schema to extend safely without widening parser and serialization blast radius.

## Implementation contract

1. Keep direct, high-confidence lexical matches useful: an exact skill-name or multi-word keyword may still direct Codex to read that skill's `SKILL.md` in full.
2. A pile of generic one-point keywords must not become a mandatory skill load merely by meeting the numeric threshold. It remains an advisory, including when project discovery identifies a repository type. Preserve project type as a tie-breaker, never a qualifier by itself.
3. Preserve the existing bounded ranking, unavailable-skill filtering, and rendered match provenance. Do not silently suppress a strong direct match.
4. In `format_skill_recommendations`, pass only detected/mandatory matches to the combined catalogued loader. Leave advisory rows visible and explicitly conditional. Do not change the individual advisory table format unless tests require a truthful wording correction.
5. No new cross-worker API is needed. Keep any helper private to its current file and under the repository function/file limits.

6. Add **new** private worker-table diagnostics in `structural_checks.rs`, called from the existing `validate_structural_preflight`. The chosen input contract is the existing Markdown `| Worker | Files owned |` table in a stage description: parse only a clearly headed table and only its write-owner column; if the table is absent or malformed, make no ownership claim. Emit advisory warnings when two distinct worker rows claim the same normalized path or when a claimed write path is outside that stage's declared `files`. Ignore read-only columns and do not add an optional structured plan field. Diagnostics must name stage, both worker labels, and path; ordinary stage DAG overlap behavior remains unchanged.

## Tests to add or adjust

Extend the existing fake-home integration harness in `loom/tests/integration/hooks_skill_trigger.rs`.

- Fixture a prompt whose generic words include `context` and `pointer`, with project discovery returning a Go-like type. Assert a matched Go skill, if rendered, is conditional and does not say `read ... SKILL.md in full`.
- Fixture an exact Go skill-name or a matching multi-word keyword. Assert the same Codex surface does say to read its `SKILL.md` in full.
- Keep the existing tie-breaker/no-keyword behavior explicit.

Keep or update the unit tests in `format/skills.rs` to prove: a detected plus advisory pair never emits a combined loader containing the advisory skill; two detected catalogued skills still produce the combined loader; advisory-only matches do not produce it; core exclusions remain intact.

In `structural_checks.rs` tests, add a valid table with two workers claiming one path, a worker path outside declared files, distinct owned paths, a malformed/absent table, and the existing transitive-stage dependency case. These are warnings, not schema errors.

## Orchestrator proof commands

```sh
cargo test --manifest-path loom/Cargo.toml --test integration hooks_skill_trigger
cargo test --manifest-path loom/Cargo.toml orchestrator::signals::format::skills --lib
cargo test --manifest-path loom/Cargo.toml plan::schema::structural_checks --lib
```

## Acceptance evidence

The hook output makes a mandatory-load claim only from high-confidence lexical evidence, signal output never converts an advisory catalogued match into an all-at-once load, and worker-table diagnostics expose conflicting write ownership without expanding the plan schema. The tests exercise both the historical generic collision and the preserved strong-match path.
