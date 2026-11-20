# Common rules for every worker of PLAN-stage-exits-and-environment

Read this file and your own brief in full before anything else. The plan is
`doc/plans/PLAN-stage-exits-and-environment.md`; its YAML is authoritative where
a brief and the plan differ.

## What you own

- You write only the files your row of the stage's worker table lists. A file
  you need to change that no row lists is an ownership gap: stop, record it with
  `loom memory note "found: <file> needs <change> for <reason>"`, and name it in
  your report. Never edit another worker's file.
- You never run git, never spawn subagents, never write `.loom/` or
  `doc/loom/knowledge/`, and never use Claude Code auto-memory.
- Verification is the main agent's job. Run at most one narrowly scoped check
  over your own files, once (a `cargo test --lib <your_module>::` filter), and
  skip it when your brief says the crate cannot compile until another worker
  finishes.
- Workers of one wave edit one crate at the same time, so your check builds
  their unfinished files too. A compile error in a file you do not own is not
  yours: do not edit that file, and report each such error with its file:line.
- Leave formatting to the main agent, who runs `cargo fmt --all` once after the
  last wave; still keep every line you write under 100 columns, because the
  maintainability counts below are taken after formatting.

## Anchors

Line numbers in the briefs were read at `b9aaea21` and are advisory. Locate
every edit by symbol (`loom map --outline <file>`, or `rg -n '<symbol>'`) and
read the surrounding code before changing it.

## The maintainability ledger

`loom/maintainability-baseline.txt` records exact line counts for files over 400
lines and functions over 50 lines, and `cargo test --test maintainability`
fails when a recorded entry grows or shrinks, or when a new file or function
crosses a limit. The file is a plan `ratchet_files` entry: any change to it
raises a `TI-ratchet-loom/maintainability-baseline.txt` integrity event that the
stage disputes once, after its final review round.

- A ledgered file or function your brief says stays "net-zero" must keep its
  exact line count: replace lines, do not add them. Run `rustfmt` in your head:
  a line pushed past 100 columns wraps and changes the count.
- A ledgered entry your brief says to "remove" or "lower": refactor as the brief
  says, then edit that one line of the ledger (remove it when the item drops to
  or below its limit, else lower it to the new exact count). Touch no other
  ledger line.
- Every new file stays under 400 lines and every new function under 50.

## Tests and integrity

- Never edit an existing assertion line (`assert!`, `assert_eq!`,
  `assert_ne!`, `assert!(matches!(..))`) in a test file: the test-integrity gate
  raises a `TI-edit` event for it. Add new tests or new assertion lines instead.
  Where your brief says an existing assertion must change, it says so
  explicitly.
- A struct literal that gains a field is not an assertion line; update it.
- Test names given in a brief are exact: acceptance runs some of them by name.

## Style

Match the surrounding code: its error handling (`anyhow` with `context`), its
comment density, its naming. Doc comments state what the code does now. No
TODO, no stub, no `unimplemented!`. User-facing text follows the repo's
writing rules: plain sentences, no marketing words.

## Memory

Record mistakes, non-obvious decisions and surprises the moment they happen:

    loom memory note "mistake: ... Why: ... Prevention: ..."
    loom memory decision "chose X over Y" --context "because Z"
    loom memory note "found/gotcha: ... in <file>:<line>"

## Report

End with: files changed, the check you ran and its result (or why you skipped
it), assumptions you made, and anything unresolved. Nothing else.
