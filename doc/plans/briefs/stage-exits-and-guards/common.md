# Stage exits and guards: rules for every worker

Read this before your own brief. It applies to every worker of
`doc/plans/PLAN-stage-exits-and-guards.md` (named `IN_PROGRESS-PLAN-stage-exits-and-guards.md`
while the plan runs).

## What this plan is

A run of the review-remediation plan showed ten defects in how a stage session reaches a
recorded exit and in the guards and doctrine around it. Each defect was traced to its seam in the tree at
`88da0e126`; your brief names the seam, the decision already taken and the proof. The plan's
"Findings" table gives the incident behind each one.

## Anchors

- Line numbers were read at `88da0e126`. Other work has merged since. Locate every seam by symbol
  (`loom map --find-all <symbol>`, `loom map --outline <file>`, `rg`) before editing; a line
  number is a hint.
- When the code no longer matches what your brief describes (a function is gone, a guard already
  exists), stop on that item and report it. Do not adapt the fix by guesswork.
- Every design decision in your brief is settled. Where the brief leaves a name or a helper
  placement open, choose the smallest option that fits the surrounding module and say so in your
  report.

## Boundaries

- Write only the files your brief lists under "Own". A compile error in a file you do not own is
  reported to the orchestrator, never fixed in place.
- Contract test files named in your brief are frozen. Never edit them. A contract you believe is
  wrong is reported to the orchestrator with the reason.
- You run no `git` command at all and touch no path under `.loom/`.
- Never use `target/debug/loom` in place of the installed `loom`.
- You answer from the base tree: `loom map` and `loom knowledge context` do not see other
  workers' edits. No other worker edits a file you own.

## Code rules

- Loom drives projects in many languages on Linux and macOS. A fix must hold for any repository
  and must not assume Rust, this repository's layout or this repository's hooks, unless your
  brief says the file is this repository's own tooling.
- No backward-compatibility shims and no migration routines: the project is pre-release. When a
  format or a sentence changes, change every reader, writer and pinning test in the same change.
- Limits: a file at most 400 lines, a function at most 50, a type at most 300. `cargo fmt` can
  lengthen a file, so leave a 10% margin under 400 and put new code in a new file when the target
  is near the limit. `loom/maintainability-baseline.txt` lists the exact size of every unit over
  a limit: a file or function listed there must not grow, and you add no row.
- Remove what your change orphans (an unused helper, a stale doc comment). Make no other refactor.
- Errors follow the surrounding module: `anyhow::Result` with `.context(..)` at command and
  daemon boundaries, the module's own typed error where callers match on it. A safety check that
  cannot read its input refuses or stays silent as your brief says; it never guesses.
- File access to a path an agent session can write goes through the descriptor-anchored helpers
  in `loom/src/fs/safe_fs/`, `loom/src/fs/safe_read.rs` and `loom/src/fs/safe_replace.rs`.
- Comments state a non-obvious constraint and nothing else. Plain wording. No mention of any
  model vendor or assistant product in code, comments, test names or doctrine text.
- Rust: literal braces inside `format!` are doubled; a CLI placeholder inside a rustdoc comment
  goes in backticks.
- Shell: hooks run under Bash 3.2 on macOS. No `mapfile`, no associative arrays, no `${var,,}`,
  no GNU-only flags. A new script needs mode 0755; say in your report that it needs `chmod +x`.

## Tests

- Every change carries the tests your brief names. A regression test must fail on the code as it
  is today and pass on your change.
- Your proof is a compile: `cargo check --all-targets` from `loom/` for Rust, `bash -n <file>`
  for a shell file. You run no tests: your sandbox cannot create fixture directories. The
  orchestrator runs your module's tests right after you finish.
- Tests never touch the real home directory, the network, the operator's `.loom` or a real
  daemon. Use `tempfile::TempDir` and the fixture helpers your brief names. A test that changes a
  process-wide value (an environment variable, the working directory) carries `#[serial]`.
- Do not weaken or delete an existing assertion unless your brief names that test and gives the
  reason. Report every existing test you changed, with the reason.

## Memory

Record each mistake, decision and surprise the moment it happens:
`loom memory note "..."` or `loom memory decision "..."`. Record a knowledge section the tree
contradicts as
`loom memory note "stale-knowledge: <file>#<heading> claims X; the tree does Y (file:line). Correction: <text>"`.
Never write to `doc/loom/knowledge/` and never use any other memory store.

## Report back

End with a short report: files changed, the tests you added, every existing test you changed and
why, anything you could not do and why, and anything in the code that contradicted your brief.
