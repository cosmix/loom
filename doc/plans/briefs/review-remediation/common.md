# Review remediation: rules for every worker

Read this before your own brief. It applies to every worker of
`doc/plans/PLAN-review-remediation.md` (named `IN_PROGRESS-PLAN-review-remediation.md` while the
plan runs).

## What this plan is

Two code reviews of revision `62be2a3d2` produced about 125 findings. Each one was checked against
the tree; the evidence, the fix seam and the regression test for every finding are recorded under
`doc/plans/briefs/review-remediation/validation/`, one file per subsystem, one `## <ID>` section
per finding. Your brief names the sections that concern you. Read those sections; they hold the
detail your brief summarises.

## Anchors

- Line numbers in briefs and validation sections were read at `62be2a3d2`. Other plans may have
  merged since. Locate every seam by symbol (`loom map --find-all <symbol>`,
  `loom map --outline <file>`, `rg`) before editing; treat a line number as a hint.
- When the code no longer matches what your brief describes (a function is gone, a guard already
  exists), stop on that item and report it. Do not adapt the fix by guesswork.

## Boundaries

- Write only the files your brief lists under "Files you own". A compile error in a file you do
  not own is reported to the orchestrator, never fixed in place.
- Contract test files and harness files named in your brief are frozen. Never edit them. A
  contract you believe is wrong is reported to the orchestrator with the reason.
- Never run `git commit`, `git add`, `git stash`, `git checkout` or any `loom stage` command. A
  codex unit runs no `git` command at all and touches no path under `.loom/`.
- Never use `target/debug/loom` in place of the installed `loom`.

## Code rules

- Loom drives projects in many languages on Linux and macOS. A fix must hold for any repository,
  not only this one, and must not assume Rust, React or this repository's layout.
- No backward-compatibility shims and no migration routines: the project is pre-release. When a
  format changes, change every reader and writer in the same change.
- Limits: a file at most 400 lines, a function at most 50, a type at most 300. Put new code in a
  new file when the target file is near the limit. `loom/maintainability-baseline.txt` is a
  ratchet: never add a row and never raise a value. A file already listed there must not grow.
- Remove what your change orphans (an unused helper, a dead test, a stale comment). Make no other
  refactor.
- Errors follow the surrounding module: `anyhow::Result` with `.context(..)` at command and daemon
  boundaries, the module's own typed error where callers match on it. Refuse on an unreadable
  input in a safety check; never treat "could not read" as "nothing there".
- File access to paths an agent session can write goes through the descriptor-anchored helpers in
  `loom/src/fs/safe_fs/`, `loom/src/fs/safe_read.rs` and `loom/src/fs/safe_replace.rs` (no symlink
  following, bounded size, regular files only). Do not add a raw `std::fs::read*` or `File::open`
  on such a path.
- Comments state a non-obvious constraint and nothing else. No mention of any AI system in code,
  comments or test names. Plain wording: no filler, no marketing words.
- Dependencies change only through `cargo add` or `bun add`, and only when your brief says so.

## Tests

- Every fix carries the regression test your brief names. The test must fail on the code as it is
  today and pass on your change; say in your report that you saw both.
- You may compile freely and run the tests of your own module
  (`cargo test --lib <module>::` or `cargo test --test <target>` from `loom/`; in `web/`,
  `node_modules/.bin/vitest run <file>`). Never run the whole suite.
- Tests never touch the real home directory, the network, the operator's `.loom` or a real
  daemon. Use `tempfile::TempDir` and the fixture helpers your brief names. A test that changes a
  process-wide value (an environment variable, the working directory) carries `#[serial]`.
- Do not weaken or delete an existing assertion unless your brief names that test and gives the
  reason. Report every existing test you changed, with the reason, so the orchestrator can account
  for it.
- A codex unit compiles (`cargo check --all-targets` from `loom/`) and does not run tests: its
  sandbox cannot create fixture directories. The orchestrator runs the module tests afterwards.

## Formatting traps

- Rust: `cargo fmt` can lengthen a file; leave a 10% margin under 400 lines. Literal braces inside
  `format!` are doubled. A CLI placeholder inside a rustdoc comment goes in backticks.
- Web: run `node_modules/.bin/oxfmt` and `node_modules/.bin/oxlint --deny-warnings` on the files
  you changed. TypeScript parameter properties are not allowed (`erasableSyntaxOnly`).
- Shell hooks must run under Bash 3.2 (macOS): no `mapfile`, no associative arrays, no `${var,,}`.
  A new script needs `chmod +x`.

## Memory

Record each mistake, decision and surprise the moment it happens:
`loom memory note "..."` or `loom memory decision "..."`. Record a knowledge
section that the tree contradicts as
`loom memory note "stale-knowledge: <file>#<heading> claims X; the tree does Y (file:line). Correction: <text>"`.
Never write to `doc/loom/knowledge/` and never use any other memory store.

## Report back

End with a short report: files changed, the tests you added and their result before and after
your change, every existing test you changed and why, anything you could not do and why, and
anything in the code that contradicted your brief.
