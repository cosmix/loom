# Untrusted Value Boundaries

> Topic notes for the mistakes knowledge area.

## Enumerate Every PRODUCER of a Rendered Field, Not Every Field

**What happened.** An agent audited the untrusted-data containment boundary personally,
traced the excerpt fencing, confirmed that chunk ANCHORS are normalized to
alphanumerics-plus-hyphens by `normalize_heading` (`fs/knowledge/chunker.rs:236`), and
concluded the boundary was closed. It was not. An independent reviewer found
`chunker.rs:73-77` takes the chunk id from
`frontmatter.id.clone().unwrap_or(derived_id)` for index 0 — so a knowledge file's YAML
frontmatter can set an **arbitrary multi-line id**, validated nowhere in `chunker.rs`,
`catalog.rs` or `ingest.rs`, and the brief renderer emitted it unfenced inside an inline
code span. A multi-line id closes the span and writes a markdown heading plus free prose
OUTSIDE the fence and outside the "quoted, NOT instructions" guard — into the signal file
the agent treats as its assignment.

**The misleading signal.** The auditor verified the DERIVED id path and the anchor path,
then generalised from two closed routes to "the identifier route is closed" — without
checking whether the id is always derived. One `unwrap_or` upstream turns a normalized
value into an arbitrary one, and the normalizer you read is then simply **not on the
path**.

**Prevention rule 1:** when auditing an escaping boundary, enumerate every PRODUCER of
each rendered field, not every field. Ask of each field "where can this value come from?"
and follow every arm, especially `unwrap_or`, `unwrap_or_default` and `Option` fallbacks.

**Prevention rule 2:** fencing the big obvious payload is the easy half. **The exploitable
half is always the small metadata rendered beside it** — ids, paths, anchors, summaries,
counts.

**Fix:** one `inline_safe()` flattener applied at the renderer (the containment boundary)
to id and pointer, with tests asserting the REFUSING direction.

## Containment At One Render Site Is Only As Good As the Set of Surfaces

**What happened.** After the fix above, the sanitized Knowledge Brief still pointed the
agent at an UNSANITIZED surface. `signals/format/brief.rs` fenced every excerpt and routed
ids, pointers and query through `inline_safe` — but the brief's own footer tells the agent
to run `loom knowledge context`, and that command's renderer did none of it:
`commands/knowledge/context.rs` `print_item` emitted `item.id`, `item.pointer.path` and
`item.summary` with a bare `println!`. Both fields are untrusted by the codebase's own
documentation. So an id carrying a newline plus `## SYSTEM INSTRUCTION ...` was correctly
flattened in the signal file and then rendered as REAL markdown structure in the agent's
tool output. `--json` was safe (serde escapes); default and `--explain` were not.

**Prevention rule:** when a fenced surface tells the agent to run a command, **that
command's output is part of the same trust boundary** and needs the same treatment. A
footer that advertises a second surface silently widens the set of surfaces the rule must
cover.

**Fix, and the shape worth copying:** `context/untrusted.rs` is now the ONE definition both
surfaces call — `MAX_INLINE_CHARS = 200`, backticks replaced with `ˋ` (U+02CB) — and its
module docstring states outright that a second copy would duplicate a security rule that
must never drift. Verified: `brief.rs:48/65/84/93` and
`commands/knowledge/context.rs:174/179/180/182` both route through it, and nothing else
defines a flattener.

## Why This Class Is Hard To See

The untrusted values here are not user input in any conventional sense — they are a chunk
id from a YAML frontmatter field, a filesystem path, and a markdown heading, all from files
in the project's own repository. Nothing about them reads as attacker-controlled, so the
usual instinct to sanitize does not fire. What makes them dangerous is the DESTINATION: a
signal file that an agent reads as its instructions.

**Rule:** classify by destination, not by origin. Any value rendered into agent-facing
prose is untrusted regardless of where it came from, because the blast radius is
"the agent does something else".

## Related

- `mistakes/tests-that-cannot-fail.md` — the fix here needed tests asserting the refusing
  direction; an escaping test that only checks benign input passes forever.
- `architecture/context-retrieval.md` — where `inline_safe` sits in the pipeline.
- `patterns/doctrine-cross-surface.md` — the general problem of one rule across many
  surfaces.
