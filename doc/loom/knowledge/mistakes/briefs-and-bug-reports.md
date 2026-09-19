# Briefs And Bug Reports

> Stage bug reports; guard flags in briefs

## A Bug Report From a Loom Stage Is About Loom the Product, Not About a Project on This Machine (2026-09-18)

**What happened:** the user relayed a `knowledge-distill` stage's report of a relay-ticket deadlock. The investigation searched `~/.claude/projects` for the error string and started reading transcripts belonging to another project's worktree. The user stopped it: the reporting instances ran in other projects that use loom as a product, and those projects are out of reach.
**Why:** the report was treated as a local incident to reconstruct from logs, when it was a defect report against loom's source.
**Prevention:** a report quoted from a stage session is a symptom description. Diagnose it from loom's own source, tests, and knowledge in this repo; never go looking for the reporter's transcripts, worktrees, or state, and never read another project's directory under `~/.claude/projects`. Ask the user for more detail from the report if the source leaves it ambiguous.
**Fix:** drop anything learned from the other project's files and reason from the code path the quoted error names.

## A Brief That Invents a New Guard Flag Can Widen an Existing One (2026-09-18)

**What happened:** the brief for the relay hook's new ` memory ` fast-path arm told the worker to add a `NOTIFY` flag (relay line OR persisted-output marker) and gate every diagnostic on it. The worker did, moving `say()` off `HAS_LINE`. `say()` had been on `HAS_LINE` alone on purpose ("a large output that was merely persisted to a file is too common to comment on"), so any large Bash output could now emit relay diagnostics. Caught in the orchestrator's diff review; reverted in a fix pass.
**Why:** the brief specified a mechanism without first checking whether the existing guard already gave the required silence. It did: a payload admitted only by the new arm has `HAS_LINE=0`.
**Prevention:** before a brief introduces a new condition variable next to an existing one, state in the brief what the existing one already covers and why it is insufficient. If that sentence cannot be written, the new variable is not needed. A worker's "I changed behaviour X, flagging in case" note is a review item, never a footnote.
