# chat-server W2: refs, compare, text at a commit, built notes

Working dir `loom/`. You own `src/plan_server/review/{compare.rs, built.rs, git_tests.rs}`. This
is the stage's injection surface: a ref the reviewer types and document paths both reach `git`.
The stage description's PUBLIC SURFACE is authoritative. Frozen contracts:
`tests/plan_review_contracts.rs` and `tests/plan_review_contracts/**`.

## Git calls

Use `crate::git::runner::{run_git, run_git_checked, run_git_checked_bytes}` (they set `LC_ALL=C`
and a timeout and never insert `--`). Rules:

- `resolve_ref`: validate first (empty, more than 256 bytes, leading `-`, any char with
  `is_control()` or `is_whitespace()` gives `Invalid`), then `["rev-parse", "--verify",
  "--quiet", "--end-of-options", &format!("{raw}^{{commit}}")]`; non-zero exit gives `Unknown`;
  stdout must be 40 lowercase hex, else `Unknown`.
- Every other function takes a commit and first checks it is 40 lowercase hex (`bail!`
  otherwise); never pass user text as a ref.
- `text_at`: `check_relpath_syntax(path)` (plan 1; the input is relative to `doc/`, the output
  repo-relative), then `run_git(["cat-file", "blob", &format!("{commit}:{rel}")])` (the form
  `git/merge/checkout_files.rs` uses); a non-zero exit whose stderr says the path does not exist
  gives `Ok(None)`; other failures are errors. Decode UTF-8 lossily.
- `changed_since`: `["diff", "--name-status", "-z", "--no-renames", commit, "--", "doc/"]` parsed
  as NUL-separated pairs (`A`, `M`, `D`, and `T` as changed) plus `["ls-files", "--others",
  "--exclude-standard", "-z", "--", "doc/"]` as added; drop paths under `doc/loom/summaries/`
  (the reviewer compares documents, not summaries). Read raw bytes and reject a non-UTF-8 path
  with an error.
- `commit_exists`: `["cat-file", "-e", &format!("{commit}^{{commit}}")]` exit status.

## built.rs

`parse_generated`: find the line containing `**Generated:** `, take the text after it to the end
of line, parse `%Y-%m-%d %H:%M UTC` with chrono's `NaiveDateTime::parse_from_str` and
`and_utc()`. `built_notes(project, section)`: `section` is relative to `doc/` and passes
`check_relpath_syntax`; take the section number from its file name; `project.implemented_by`
gives the slugs and `project.tree.plans` their state; for each, `PlanState::Running` gives `Running`;
`Done` reads the plan's run review: every document in `project.tree.docs` whose kind is plan 1's
`DocKind::RunReview { slug }` for this slug (directly in `doc/plans/` or under
`doc/plans/archive/`, any spelling `fs/plan_review/mod.rs` `generate` writes, such as
`REVIEW-PLAN-<slug>.md` and `REVIEW-IN_PROGRESS-PLAN-<slug>.md`), read through
`planning::discover::read_doc(&project.root, ..)`; with several, the newest `**Generated:**`
stamp wins; none: nothing. Never rebuild the file name; the section's change date
is `["log", "-1", "--format=%cI", "--", rel]` parsed as RFC 3339, or now when `["status",
"--porcelain", "--", rel]` prints anything; when the change date is after the build date push
`ChangedAfter` with `base_commit` from `["log", "-1", "--format=%H", &format!("--before={}",
built.to_rfc3339()), "--", rel]` (None when empty). Retired and written plans give nothing.

## git_tests.rs

Unit tests on TempDir repositories (git init, local user config, fixed dates through
`GIT_AUTHOR_DATE`/`GIT_COMMITTER_DATE`): rename shows as removed plus added; an untracked
document is added; a non-hex commit is refused before git runs; `text_at` for a path missing at
the commit is `None`; a review file with two `**Generated:**`-looking lines takes the first.
Proof: `cargo test --lib plan_server::review::` and `cargo test --test plan_review_contracts`.

## Required test cases (coverage audit; named tests in `git_tests.rs`)

"rename shows removed plus added"; "untracked document is added"; "summary store paths dropped";
"non-hex commit refused before git runs"; "text_at missing path is none"; "uncommitted section
counts as changed now"; "done plan without review gives nothing"; "section changed before the
build gives nothing"; "retired and written plans give nothing"; "first generated line wins";
"commit_exists false for an unknown commit".

Fix-round tests (git_tests.rs): "review under archive is found"; "REVIEW-IN_PROGRESS- review is
found"; "newest of two reviews wins". The two frozen contracts `marker-after-build-date` and
`marker-finds-in-progress-named-review` pin the archive and prefix cases.
