# ask --eval fixtures

`cases.json` holds 24 cases for `loom ask --eval`: six per kind (plan, knowledge,
diff, log). They cover locating a fact, extracting a decision, listing items and
four questions the files cannot answer (`must_contain` is `not found`).

The cases are authored for this trial. They are not mined from transcripts,
because the stage sandbox denies reads of `~/.claude/projects`. The operator
should review the cases before trusting a result.

`docs/` holds frozen copies, so later edits to the originals cannot change the
answers:

| File | Source |
| --- | --- |
| `docs/token-optim-metaplan.md` | `doc/plans/optim-token/index.md` |
| `docs/context-retrieval.md` | `doc/loom/knowledge/architecture/context-retrieval.md` |
| `docs/source-graph.md` | `doc/loom/knowledge/architecture/source-graph.md` |
| `docs/stage-diff.diff` | `git show 954b401c`, author line replaced |
| `docs/test-run.txt` | `cargo test --lib commands::usage::` output, home path replaced |

Case `paths` are relative to this directory. The
`every_case_can_be_answered_from_its_files` unit test in
`loom/src/commands/ask/eval_tests.rs` checks, without a provider, that every
case parses, its paths exist, and each `must_contain` string occurs in its files.
