# Knowledge Base Drift

> How the knowledge base itself goes stale: plan-authoring notes frozen as architecture facts, `[UPDATED]` duplicates, and invented CLI surface.

The knowledge base is written by agents mid-plan and is not covered by any test. It drifts in
four specific, recognisable ways. All four were found and repaired on 2026-07-30 during a README
rewrite — every one had survived multiple `knowledge-distill` stages.

## Plan-Authoring Notes Frozen as Architecture Facts

**What happened:** `architecture.md` carried a literal
`*** INSERT: check_pending_disputes() + apply_pending_verdicts() HERE ***` marker in the
orchestrator tick sequence, plus sections headed "Dispute Directory Structure (New, Stage 2+)"
and "Plan Versioning (New, Stage 3+)". All three features had shipped. Worse, the note proposed
inserting the adjudicator hooks *after* merge resolution; they actually landed *before* it. An
agent trusting that section would have had both the status and the ordering wrong.

**Why:** A knowledge stage recorded what the plan *intended to build*, in the plan's own
forward-looking voice, instead of what the tree *contains*. Nothing later revisited the tense.

**Prevention:** Treat these as smells and verify against the tree, never trust the text:

| Smell in a knowledge file | Verify with |
| --- | --- |
| `(New, Stage N+)`, `(to be added)`, `Stage 2 replaces this` | `rg <symbol> loom/src/` — does it exist? |
| `*** INSERT ... HERE ***`, `Insertion point for ...` | Read the actual call site |
| `CURRENTLY ENFORCED; X Relaxes It` | Is X shipped? Then the invariant is already narrowed |

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

**Prevention:** After any `loom knowledge update` that corrects an existing section, grep for the
duplicate heading:

```bash
cd doc/loom/knowledge
for f in *.md; do
  d=$(rg -N "^#{2,3} " "$f" | sort | uniq -d)
  [ -n "$d" ] && echo "$f: $d"
done
```

`loom knowledge audit` reports duplicate headers too — read its output, do not just check the
exit code.

**Fix:** Use `loom knowledge replace-section` to overwrite in place rather than `update` to
append. When deleting a superseded section, leave one line saying what was removed and why —
otherwise the next agent re-adds it from the same stale source.

## Invented CLI Surface

**What happened:** `entry-points.md`'s command dispatch table listed `loom hooks`
(`commands/hooks.rs`), `loom sandbox` (`commands/sandbox/`), and `loom verify`
(`commands/check.rs`). None of the three exist — no such commands, no such files. The table also
undercounted the real commands.

**Why:** The table was written from the module layout and from what the commands *ought* to be
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

**What happened:** `patterns.md` § Knowledge Systems described a `.work/facts.toml` cross-stage
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
