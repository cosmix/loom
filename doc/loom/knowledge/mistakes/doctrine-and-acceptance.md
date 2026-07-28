# Doctrine And Acceptance

> Why a one-phrase grep proves presence but never agreement, and how doctrine drifts across surfaces unnoticed.

## An Acceptance Criterion That Greps One Phrase Proves Presence, Never Agreement (2026-07-28)

**What happened:** the no-verify doctrine ("BLOCK-A") had to appear identically on three
surfaces. At integration-verify time `hooks/subagent-verify-guard.sh` carried a 15-line
*reconstruction* while `orchestrator/signals/cache.rs` and `CLAUDE.md.template` carried the
authoritative 7-line block. **Every acceptance criterion passed**, because they all
`rg -qF` a single anchor phrase that both wordings happened to contain.

**Why:** presence of a substring says nothing about the rest of the text. N greps across N
surfaces still only prove N substrings exist, never that the surfaces agree.

**Prevention:** when the same block must exist verbatim on multiple surfaces, pin it with a
**cross-surface byte-equality test**, not with greps — `include_str!` each static surface,
call the generator for each runtime surface, and assert equality.

**Fix:** `loom/src/orchestrator/signals/tests_doctrine.rs`.

## A Checker That Enumerates Forbidden Strings Must Not Contain Them Contiguously (2026-07-28)

**What happened:** the new cross-surface test lists the *retired* phrasing it greps for, and it
lives in `loom/src/orchestrator/signals/` — exactly the directory a plan criterion scanned for
that phrasing. Both the constant array and a doc comment quoting it matched, so the test tripped
the plan's own acceptance.

**Prevention:** build each forbidden literal with `concat!` so it never appears contiguously in
the source, and keep the forbidden text out of comments too. **Detection:** run the plan's own
greps against your new file before assuming it is inert.

## An Exception Must Live in Every Block That Gets COPIED (2026-07-28)

**What happened:** the integration-verify carve-out was written into the signal-level override,
which is addressed to the *main agent* ("when you spawn a verifier, tell it to run the complete
suite"). But what an integration-verify main agent actually pastes into its verifier subagent's
prompt is the Rule 5 preamble from `CLAUDE.md.template` — which carried the doctrine with **no
exception**. The verifier therefore received "no full build, no full test suite" as its most
rule-shaped instruction.

**Prevention:** for each rule, ask **which surface is pasted verbatim into a subagent prompt**,
and check that the exception survives that paste. A doctrine's exception belongs in every block
that is copied, not only in the prose that explains it.

## After Landing a Doctrine, Grep for the Phrasing It RETIRES (2026-07-28)

**What happened:** four subagents each landed the new doctrine correctly in their own territory,
while the phrasing it *replaces* — "verifies its subtree", "DO write code, run tests",
"test results" — survived a line or two away in `cache.rs`, `format/sections.rs` and
`CLAUDE.md.template`, text nobody was assigned to touch. The enforcement layer shipped while
the guidance layer still instructed the blocked behaviour.

**Why:** acceptance criteria grep for the wording a change *introduces*. Nothing greps for the
wording it *removes*, so contradictions are invisible to the gate.

**Prevention:** after landing a doctrine, sweep the **entire** guidance surface for the retired
phrasing, not just for the new phrasing. A stage's own guidance files can contradict the
doctrine that stage is landing.

## Canonical Text Referenced by Plan Path Is Unreachable From a Worktree (SYSTEMIC, 2026-07-28)

**What happened:** two independent stages were told to copy canonical wording **verbatim from
`doc/plans/`**. Plan files live in the main repo working tree and are frequently uncommitted, so
they are absent from the worktree checkout, and `worktree-file-guard.sh` hard-blocks the
absolute main-repo path on `PreToolUse:Read`. One stage found a readable copy; the other
reconstructed the text from the single phrase pinned by an acceptance criterion, and the two
copies then drifted (see the first lesson on this page).

**Misleading signal:** the signal header claims "plan overview embedded below", but no overview
section is generated.

**Prevention (for plan authors):** any verbatim text a stage must reproduce — canonical wording,
message blocks, doctrine — **must be inlined into the stage description itself**, never
referenced by plan path. **Detection (for executing agents):** if the assignment says "copy
EXACTLY" and the text is not in your signal, you are already blocked — say so rather than
reconstructing.
