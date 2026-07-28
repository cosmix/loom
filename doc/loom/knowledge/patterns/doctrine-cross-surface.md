# Doctrine Cross Surface

> Pinning multi-surface guidance with equality tests, ambiguity-equals-fail-safe privilege lookups, and token-based shell classification.

## Doctrine Block Cross-Surface Pinning

A **doctrine block** is a fixed chunk of agent guidance that must appear byte-identically on
several surfaces at once — a runtime signal prefix, a static template, and a hook's refusal
message. Loom currently carries two: the subagent no-verify rule and the model playbook.

The failure mode is drift, and it is invisible to ordinary acceptance criteria: greping each
surface for an anchor phrase proves only that a substring exists on each, never that the
surfaces agree. Two independently-worded copies both pass.

**The pattern:**

1. **One authority.** Exactly one surface is canonical. Authority runs plan → the stage that
   could read the plan → every other copy. When two copies disagree, reconcile *toward* the
   authoritative one rather than adopting whichever was found first.
2. **Pin with equality, not greps.** A single test `include_str!`s each static surface, calls
   the generator for each runtime surface, and asserts byte equality against the canonical
   block. See `loom/src/orchestrator/signals/tests_doctrine.rs`.
3. **Exceptions travel with the copied block.** For every rule ask: *which surface is pasted
   verbatim into a subagent prompt?* Any carve-out must survive that paste. A carve-out that
   lives only in the prose explaining the rule never reaches the agent that needs it.
4. **Frame outside the block.** Surface-specific framing (a hook's `BLOCKED:` header line,
   language-specific examples) sits *outside* the pinned block so the block itself stays
   byte-identical everywhere.
5. **Sweep for retired phrasing.** After landing a doctrine, grep the whole guidance surface for
   the wording the doctrine *replaces*, not only for the wording it introduces.

**Known tradeoff:** because the block must stay byte-identical, per-language examples cannot
live inside it. The current no-verify block carries one Rust example, so a blocked Python or Go
subagent reads a `cargo` example. The fix is to append language examples *after* the block as
explicitly surface-local guidance.

## Ambiguity = Fail Safe (Privilege Lookups From State Files)

Any hook or gate that grants a **relaxation** by reading a `.work/` state file must treat more
than one candidate match as ambiguous and refuse. Concretely: resolve by glob, count the
matches, and consult the file **only when exactly one exists**.

`glob | head -1` is not a lookup — it is a silent tie-break, and the tie is attacker-chosen when
filenames carry a sortable prefix. Cross-checking a field *inside* the file (`id:`) does not
help: whoever plants the decoy writes that field too. Only the count is trustworthy.

The matching test obligation: a fail-safe relaxation must be pinned by tests that assert the
**unsafe inputs are refused** (decoy present, wrong stage type, file missing). A test of the
granted direction alone leaves the relaxation free to regress silently.

## Token-Based Shell Command Classification

For a hook that must decide "does this Bash command run a project-wide build/test?", raw
substring matching is wrong (it blocks `rg "cargo test" doc/`) and naive word-splitting is
bypassable. The working shape:

1. Strip embedded content (heredoc bodies, `-m` message text) so a *mention* is not a match.
2. Normalise newlines to ` ; `, then pad separators — `;`, `|`, `(`, `)`, and the pair `&&`.
   Never pad a lone `&`: it would split `2>&1` and reopen a redirection bypass.
3. Split with `set -f` plus array splitting so quotes stay attached to their token.
4. Walk tokens tracking **command position** (line start, or after a separator, or after a
   prefix such as `env`/`time`/`xargs`) and **quote state**; only a runner in command position
   counts.
5. Skip redirection tokens when scanning for a positional argument — otherwise `2>&1` is read
   as a scope-narrowing filter.
6. **Allow unmatched commands.** A false block strands a subagent mid-task with no recourse;
   the gate is a guardrail, not a whitelist.
