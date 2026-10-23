# Common context: stage `doctrine-and-guards`

Every worker of this stage reads this file first, then its own brief. Where this file and a worker brief disagree, the worker brief wins for that worker's territory.

Workers W1-W7 are Claude subagents. C1 and C2 are codex units (`loom-codex-forwarder`, gpt-5.6-terra): they run in parallel with W1-W6, write only the files their briefs list, and run no build, test or formatter (the main agent compiles after each unit).

## What the stage does

Plan 3 of the token-optimization programme (`doc/plans/optim-token-metaplan.md`, section 3). It removes instructions that are false or contradict each other, deletes one dead code path, and adds four plan guards (a bootstrap-warning fix, two version-2 bookend lints, a stage `model` allowlist, an archive abort). It runs first in the programme; plans 1, 2, 4, 5 and 7 edit some of the same files after it.

## Paths

- The stage's `working_dir` is `loom/`. Rust paths below are relative to `loom/` (`src/...`, `tests/...`). Paths outside the crate are written from the repository root (`loom-hooks/...`, `README.md`, `skills/...`).
- Line numbers come from revision `6fee4ecd`. They drift. Find every edit point by the symbol and the quoted text, then confirm the line before editing. Between `6fee4ecd` and `4cc085b0` the stage's files changed only in `README.md` (one table line), `loom-hooks/spawn-guard.sh` (403 lines, +5 in `resolve_defined_tier`) and `src/orchestrator/signals/format/codex.rs` (the `YOU (opus)` literal is unchanged); every ledger row below is unchanged.
- `cargo fmt` wraps a call whose arguments are wider than 60 columns (rustfmt's `fn_call_width`; the repository has no `rustfmt.toml`), not only a line past 100 columns. When a brief requires a line to stay one line, keep its arguments within 60 columns.

## Rules for every worker

- Never run `git`. Never touch any `.loom/` path.
- You own only the files your brief lists. A compile error or test failure in a file you do not own is not yours: report it with the file, the error and the line, and stop. Never edit another worker's file, and never wait for one. Workers share one crate, so a sibling's half-written file can break your one check; report it the same way.
- Run the one check your brief names, once, at the end. The main agent runs the full gate in a `loom-verifier` round afterwards.
- `tests/doctrine_guard_contracts.rs` holds the stage's frozen contract tests. Never edit it. Your work is done when the contracts that name your code pass.
- `loom/maintainability-baseline.txt` is an exact-match ledger: `cargo test --test maintainability` fails when a recorded file or function grows and when it shrinks. Never edit the ledger; the main agent owns it. Report the final line count of every recorded file or function you changed. Rows that concern this stage: `file src/orchestrator/signals/format/sections.rs 569`, `function src/orchestrator/signals/format/sections.rs format_semi_stable_section 278`, `function ... format_recitation_section 58`, `function ... format_dynamic_section 205`, `file src/orchestrator/signals/tests_cache.rs 630`, `function src/orchestrator/signals/tests_cache.rs test_signal_codex_implementers_section_gated 116`, `file src/plan/schema/validation.rs 1222`, `function src/commands/init/plan_setup.rs initialize_with_plan 192`, `file src/commands/plan/verify.rs 555`, `function src/commands/plan/verify.rs execute 133`, `function src/fs/knowledge/templates.rs default_content 62`, `file tests/integration/plan_verify.rs 1028`, `function tests/plan_environment_contracts.rs plan 64`. A row the brief says stays unchanged must stay unchanged: keep that file or function at exactly its recorded count.
- Size limits for new code: 400 lines per file, 50 lines per function. `cargo fmt` can add lines; leave a margin. A new `#[cfg(test)] mod x;` declaration costs two lines after rustfmt.
- Writing style for every string, comment and doc line you write: short, plain, current truth. No history ("was", "used to", "no longer"), no stock filler words, no contrastive "not X, but Y" constructions. Never mention AI tools or assistants.
- Record a mistake, a surprise or a decision with `loom memory note` / `loom memory decision` as it happens (the subagent preamble has the forms). A knowledge file that contradicts the tree gets `loom memory note "stale-knowledge: <file>#<heading> claims X; the tree does Y (file:line). Correction: <text>"`. Never run `loom knowledge` writes in this stage.

## Files later plans edit after this stage

Plans 1, 2, 4 and 5 rebase on this stage's edits to `loom-hooks/post-tool-use.sh`, `loom-hooks/tests/run-all.sh`, `src/orchestrator/signals/format/sections.rs`, `CLAUDE.md.template`, `loom-hooks/session-start.sh` and `skills/loom-plan-writer/SKILL.md`. Make exactly the edits your brief names in those files and nothing else, so the overview of the plan stays true for them.

## The contract surface (frozen before you start)

`tests/doctrine_guard_contracts.rs` calls these public surfaces. Keep the names and signatures exactly:

- `loom::orchestrator::signals::format_signal_with_metrics(...)` on an integration-verify `Stage`: the content carries `NEVER use 'loom knowledge' in this stage` and none of `loom knowledge update`, `KNOWLEDGE UPDATES REQUIRED`, `## Knowledge Management` (W1).
- `loom plan verify --json PLAN.md` errors with `stage_id` set and messages containing `not an ancestor of knowledge-distill stage` and `not an ancestor of any integration-verify stage` (C1 writes the lints; W5 writes their unit tests).
- `loom::plan::schema::check_knowledge_recommendations(stages: &[StageDefinition], repo_root: Option<&Path>) -> Vec<String>` (W5).
- `loom::yaml::from_str::<loom::plan::LoomMetadata>` rejects a stage `model` outside `haiku`, `sonnet`, `opus`, `fable` with an error containing `invalid model` (C2).
- `loom::fs::memory::archive_run_state(work_dir: &Path, main_root: &Path, plan_id: Option<&str>) -> anyhow::Result<Option<PathBuf>>` returns `Err` naming the archive path when archiving fails while `<work_dir>/memory` holds a `*.md` journal (W6).
