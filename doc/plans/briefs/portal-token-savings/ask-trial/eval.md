# loom ask --eval fixtures and self-check

The core worker has finished and published `eval::parse_cases` and `EvalCase` in `loom/src/commands/ask/eval.rs`. You own the fixtures and their self-check test. Do not edit `eval.rs` except to add the line below; report any change it needs.

## Frozen fixtures: `loom/tests/fixtures/ask_eval/`

Copy these real documents. They stay frozen here so later edits to the originals cannot change the answers.

| Fixture | Source |
| --- | --- |
| `docs/token-optim-metaplan.md` | `doc/plans/optim-token/index.md` |
| `docs/context-retrieval.md` | `doc/loom/knowledge/architecture/context-retrieval.md` |
| `docs/source-graph.md` | `doc/loom/knowledge/architecture/source-graph.md` |
| `docs/stage-diff.diff` | `git show 954b401c` |
| `docs/test-run.txt` | A captured `cargo test --manifest-path loom/Cargo.toml --lib commands::usage:: 2>&1` run, trimmed to at most 2,000 lines |

`.gitignore` ignores `*.log`, so a `.log` fixture never reaches the commit. For the metaplan, copy it from the committed `doc/plans/optim-token/index.md` at the stage's base; if it moved to `archive/`, use that copy.

Check that none of the five fixtures, including `docs/token-optim-metaplan.md`, carries a secret, an absolute home path or the user's name. Replace `/home/<name>` with `/home/user` and `-home-<name>-` with `-home-user-`. Then `rg -n 'dkaponis|/home/' loom/tests/fixtures/ask_eval` must print nothing except `/home/user`. Run the self-check after the scrub, and use none of those strings in a case.

## `cases.json`

Write 24 cases, 6 per document kind (plan, knowledge, diff, log), covering:

- Locating a fact ("Which file pins the stable prefix byte cap?").
- Extracting a decision ("What does the metaplan say about the 800,000-token ceilings?").
- Listing ("Which functions does this diff change?").
- A question the file cannot answer, with `must_contain: ["not found"]`. Include at least 3.

Every `must_contain` value is a short literal the correct answer must include, copied from the source: a symbol, path or number. Avoid prose phrases, because a correct answer can word those differently. Use `must_not_contain` for a plausible wrong answer where one exists.

These cases are authored for this trial rather than mined from transcripts, because the stage sandbox denies reads of `~/.claude/projects`. Add a `README.md` in the fixture directory saying so, and saying the operator should review the cases before trusting a result.

## Self-check test

In `loom/src/commands/ask/eval_tests.rs`, add a test that reads `cases.json` under `env!("CARGO_MANIFEST_DIR")` and needs no provider: every case parses through `parse_cases`, each listed path exists, every `must_contain` string except `not found` occurs in that case's files ignoring ASCII case, and there are 24 cases. This catches a case that no correct answer could pass.

Also test duplicate-id rejection. Declare the test module by adding `#[cfg(test)] mod eval_tests;` to `eval.rs` yourself: core has returned.

## Check

Run `cargo test --manifest-path loom/Cargo.toml --lib commands::ask::` once. Report the result to the orchestrator.
