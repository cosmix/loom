# Plan startup inputs

`loom run` creates stage worktrees from the base branch, so every file the plan
depends on must exist and be committed there before the first stage starts.

## What counts as an input

- the plan file;
- the conventional bundle `doc/plans/briefs/<plan-slug>/`;
- every `doc/plans/briefs/...` path, directory or glob named in the plan or in
  any `.md` input, followed recursively.

## What `loom run` refuses

`loom run` lists every problem at once under `Cannot start loom run`:

- an invalid reference: a glob that does not compile, a path that traverses
  parent directories, or a symlinked path. Each line names the token as written
  and the file it came from;
- a missing input file;
- an input that is not committed, has uncommitted changes, or whose working copy
  differs from the committed file.

## What `loom plan verify` reports

`loom plan verify` runs the same discovery on the verified plan and reports the
same problems:

- invalid references and missing inputs are errors, so verify fails without
  `--strict`;
- inputs that exist but are not yet committed are one note, not a warning, so
  `--strict` is unaffected. Plans are verified before they are committed;
  `loom run` still refuses until they are.

Verify skips these checks when the plan is not inside a git repository, and
skips the uncommitted note when git cannot answer.
