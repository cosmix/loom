# Knowledge Base Drift

> How the knowledge base goes stale: frozen notes, drift

The knowledge base is written by agents mid-plan and is not covered by any test. It drifts in
four specific, recognisable ways. All four were found and repaired on 2026-07-30 during a README
rewrite — every one had survived multiple `knowledge-distill` stages.

## Plan-Authoring Notes Frozen as Architecture Facts

**What happened:** `architecture.md` carried a literal
`*** INSERT: check_pending_disputes() + apply_pending_verdicts() HERE ***` marker in the
orchestrator tick sequence, plus sections headed "Dispute Directory Structure (New, Stage 2+)"
and "Plan Versioning (New, Stage 3+)". All three features had shipped. Worse, the note proposed
inserting the adjudicator hooks _after_ merge resolution; they actually landed _before_ it. An
agent trusting that section would have had both the status and the ordering wrong.

**Why:** A knowledge stage recorded what the plan _intended to build_, in the plan's own
forward-looking voice, instead of what the tree _contains_. Nothing later revisited the tense.

**Prevention:** Treat these as smells and verify against the tree, never trust the text:

| Smell in a knowledge file                                   | Verify with                                          |
| ----------------------------------------------------------- | ---------------------------------------------------- |
| `(New, Stage N+)`, `(to be added)`, `Stage 2 replaces this` | `rg <symbol> loom/src/` — does it exist?             |
| `*** INSERT ... HERE ***`, `Insertion point for ...`        | Read the actual call site                            |
| `CURRENTLY ENFORCED; X Relaxes It`                          | Is X shipped? Then the invariant is already narrowed |

**Fix:** Record shipped state in the present tense with a file reference. When a section
documents something that landed differently than planned, say so explicitly — the correction is
the valuable part, because the wrong version is what a reader already half-remembers.

## `[UPDATED]` Sections That Never Replaced the Original

**What happened:** `architecture.md`, `entry-points.md`, and `patterns.md` each carried two
sections with the same title, one plain and one suffixed `[UPDATED]`. In every case the plain one
was stale and the `[UPDATED]` one was current. The stale copies claimed `truths` was a
goal-backward verification layer — it was removed from goal-backward and merged into `acceptance`.
`signal-generation.md` had the same disease: two `append_*` helper tables, the first missing
`append_anti_slop_guidance()` and `append_adversarial_review()`.

**Why:** `loom knowledge update` **appends**. An agent correcting a section adds a new one; the
wrong text stays directly above the right text, and a reader scanning top-down hits the wrong one
first.

**Prevention:** After any `loom knowledge update` that corrects an existing section, check for the
duplicate heading — tier-2 topics included:

```bash
cd doc/loom/knowledge
for f in *.md */*.md; do
  d=$(rg -N "^#{2,3} " "$f" | sort | uniq -d)
  [ -n "$d" ] && echo "$f: $d"
done
```

There is no `loom knowledge audit` command; this loop is the check.

**Fix:** Correct the section IN PLACE with
`loom knowledge replace-section <file> "<heading>" "<body>"` — restored 2026-08-19, after the CLI
collapse had removed it, precisely so distillation can retire a stale claim instead of layering
over it. Pass the body WITHOUT its `##` heading line. It cannot rename a heading (see
concerns.md § `loom knowledge` Cannot Rename a Section Heading) and there is still no delete verb.
When a superseded claim disappears, say in the replacement what was wrong and why — otherwise the
next agent re-adds it from the same stale source.

## Invented CLI Surface

**What happened:** `entry-points.md`'s command dispatch table listed three invented commands:
`loom hooks` — there is no `commands/hooks.rs` and no such command, `loom sandbox` — the
`commands/sandbox/` directory was invented too, and `loom verify` — invented and attributed to
`commands/check.rs`, which does not exist under that name (the real file is `commands/verify.rs`,
and it implements `loom check`, not a `loom verify`). None of the three commands exist. The table
also undercounted the real commands.

**Why:** The table was written from the module layout and from what the commands _ought_ to be
called, not from `cli/dispatch.rs`. `commands/verify.rs` does exist, which makes `loom verify`
feel real — but it is the implementation behind `loom check`.

**Prevention:** The CLI surface has exactly one source of truth: the `Commands` enum in
`cli/types.rs` and its arms in `cli/dispatch.rs`. Confirm against `loom --help` before writing a
command name into a knowledge file. A module named `commands/foo.rs` does **not** imply a
`loom foo` command.

**Fix:** When correcting an invented command, record that it does not exist and where the real
functionality lives. A bare deletion means the next agent invents it again from the same module
name.

## Features Documented That Were Never Built

**What happened:** `patterns.md` § Knowledge Systems described a `.loom/work/facts.toml` cross-stage
KV store, a `loom memory promote` command, and `<!-- .loom-protected -->` file markers. All three
are absent from the codebase. The real cross-stage KV is `loom stage output`; the real
memory→knowledge promotion path is the `knowledge-distill` stage.

**Why:** Most likely a design sketch recorded as though implemented — the same tense failure as
above, but for features that were dropped rather than deferred.

**Prevention:** Every capability claim in a knowledge file should be greppable. Before writing
"system X does Y", run the grep that would prove it. A one-line `rg` is cheaper than the hour an
agent later spends looking for a file that was never written.

**Fix:** Name the non-existent thing explicitly in the correction. "There is no
`loom memory promote`" is more durable than silently describing the right mechanism, because it
inoculates against the stale copy that may still exist elsewhere.

## Skill Documentation Freshness

**Mistake:** Skill files referenced old schema state after fields were added/removed.
**Fix:** Update skill files and feature code together when changing schemas.

## Repair Must Propagate Skill-Index Failures (Resolved 2026-08-08)

**Mistake:** Hook repair rebuilt the skill index but discarded a rebuild error, so the action could be counted as fixed without producing a usable index.
**Prevention:** A composite repair step must return the first failed sub-operation; never increment the repaired count after a required side effect fails.
**Fix:** `fix_hooks_with` now returns the skill-index rebuild result, and `hook_repair_propagates_skill_index_write_failure` pins the failure path.

## Verifying "Dead Schema" Claims Before Writing Code (2026-06-15)

**What happened:** Plan PLAN-anti-slop-thoroughness described `before_stage` as "dormant / parsed-but-never-run." Stage 3 Subagent 1 was tasked to wire it. It verified the claim against `stage_executor.rs:219-256` and found `before_stage` was already fully wired — runs pre-spawn, blocks session on failure. The task was a no-op.

**Misleading signal:** Plan descriptions are written at planning time and can go stale as other stages implement things. A plan claiming a field is "dead" is as reliable as code comments — it describes intent at authoring time, not current reality.

**Prevention:** Before implementing "wire X" or "add execution of Y," run `rg "before_stage\|after_stage\|<field>" loom/src/` to verify the current execution path. Check `stage_executor.rs` (pre-spawn), `complete.rs` (post-acceptance), `generate.rs` (signal), `plan_setup.rs` (copy). Only skip after confirming absence, not trusting the plan text.

**Fix:** Skipped the no-op task; verified the actual dormant field (code_review) and wired it instead.

## Spooled Knowledge Goes Stale Before It Is Applied

**What happened:** A `knowledge-distill` stage could not write `doc/loom/knowledge/**`
(the plan sandbox denied it), so it spooled final curated prose to
`doc/loom/PENDING-KNOWLEDGE-*.md` for an operator to apply later. By the time it was
applied, several of its load-bearing claims were false: it asserted
`loom knowledge replace-section` had been deleted (it was restored in the working
tree), that six `KnowledgeDir` methods were production-dead (four — restoring the CLI
verb revived `read_target` and `replace_section_target`), and that
`fs/knowledge/summary.rs` was an open concern — that file was removed from the tree.
A sibling tier-1 table in `entry-points.md` named three files that no longer existed.

**Why:** Spooled prose is a snapshot of one revision, but it is applied against
another. The gap between authoring and application is unbounded — and the very
staleness the distillation exists to remove is what accumulates inside it while it
waits. Worse, it reads as authoritative: "already curated, it is not notes."

**Prevention:** Treat a spool file as CLAIMS, not as content. Before applying any of
it, re-verify every factual assertion against the source tree — file existence with
`test -f`, verb existence against `cli/types_*.rs` and `cli/dispatch.rs` (never against
`--help`, which reflects the INSTALLED binary, not the working tree), and
"no callers" claims with `rg` filtered of tests. Verify against the SOURCE, because an
installed binary and a working tree routinely disagree.

**Fix:** Apply spooled prose through an orchestrator that verifies each claim and
hands workers the corrected facts, rather than pointing workers at the spool file and
telling them it is final. An acceptance grep written into the spool goes stale with it:
here the criterion forbade every mention of `replace-section`, which after the restore
would have forced workers to write something false in order to pass. When acceptance
and ground truth disagree, fix the criterion — never the prose.

## Correcting a Quoted Error Message Can Leave a Half-True Claim (2026-09-15)

**What happened:** a `.work` → `.loom/work` sweep brief said to replace a quoted CLI error with the
string the source emits now. In `concerns/knowledge-cli-gaps.md` that put the current message under
the old sentence about `loom memory note` without re-checking whether the command still fails that
way. In `concerns/automatic-knowledge-source-graph-followups.md` the sweep respelled a quoted
message into `.loom/work directory does not exist`, a string `loom/src` does not contain.
**Why:** a quote of observed output records past behavior. Changing the string without re-checking
the claim around it rewrites evidence and leaves the claim as wrong as before.
**Prevention:** in a path or spelling sweep, keep quoted output verbatim. When the behavior the
quote documents has changed, correct the claim itself after checking the code, or record a
`stale-knowledge` note.
**Fix:** both quotes were restored verbatim. The two concerns still need a re-check against current
code.

## Preserve unrelated prose during section corrections

**What happened:** An install-root documentation edit reflowed and lightly rewrote unrelated paragraphs while replacing sections with stale installer claims.

**Why:** The replacement body was reconstructed instead of retaining the original section verbatim outside the corrected sentences.

**Prevention:** Build replacement bodies from the existing section and replace only the verified stale text; preserve wrapping, subheadings, lists, and unrelated wording.

**Fix:** Restore the original section bodies with only the targeted installer corrections retained.

## Pass replacement prose as literal process arguments

**What happened:** A knowledge correction command accidentally subjected Markdown backticks to shell substitution; the attempted lookups failed and the affected section was restored from its saved original.

**Why:** Replacement prose crossed a shell quoting boundary while constructing the CLI command.

**Prevention:** Invoke knowledge writes with subprocess argument arrays, or use literal quoted input; never interpolate Markdown into shell code.

**Fix:** Replaced the section through safely quoted input and verified the final diff retained only the intended corrections.

## A Large Distillation Trips the INDEX Byte Budget and the Tier-1 Line Cap (2026-09-25)

**What happened:** the verification-v2 distillation added five tier-2 topics and a summary to each of five tier-1 files.
`loom knowledge check --strict --baseline` then failed on `INDEX.md` (16,788 bytes against a 16,384-byte budget), on four
tier-1 files at 251 to 258 lines (cap 250, `fs/knowledge/catalog/size.rs`), on a tier-2 section of 88 lines (cap 80), and on
dangling references: a backticked file name that matches no path in the repository counts as a source reference, and a relative link inside a tier-2 file must start with `../`.

**Prevention:** budget before writing. Each tier-2 topic costs about 110 bytes of `INDEX.md`; when the index is within a few
hundred bytes of the budget, shorten other blurbs (`loom knowledge annotate <topic> --blurb`, also accepts a tier-1 file
name) instead of skipping the topic. A tier-1 summary is 3 to 5 lines with a link; when a tier-1 file sits at its cap,
move an existing section's detail into its tier-2 topic first. Split a tier-2 section at a natural paragraph before it
reaches 80 lines. Write prose without file-name-shaped tokens that do not exist. Feed long bodies to `loom knowledge update
<topic> -` from a scratch file rather than a heredoc: `loom-control-complete.sh` blocks a Bash command whose text names
`loom` and the stage-completion path.
