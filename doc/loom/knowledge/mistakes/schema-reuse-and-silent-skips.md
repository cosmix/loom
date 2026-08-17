# Schema Reuse And Silent Skips

> Topic notes for the mistakes knowledge area.

## `deny_unknown_fields` on a Type With TWO Deserialization Sources Broke the Second One (2026-08-17)

**What happened:** every `.work/stages/*.md` file failed to parse. `loom run` printed one
`Warning: Could not parse ... Failed to parse StageDefinition from frontmatter` per stage
and carried on. `StageDefinition` is `#[serde(deny_unknown_fields)]` — added in
`2d5a4679` to reject typo'd keys in PLAN YAML, which is a good guarantee for that caller.
But the same type was also being deserialized from a second, entirely different source:
stage files, whose frontmatter is a full serialized `Stage` carrying runtime-only keys
(`status`, `merged`, `fix_attempts`, `resolved_base`, `session`, `worktree`, …). Strictness
that is correct for the plan is fatal for the stage file, and nobody noticed the type had
two readers.

**The stale comment was the tell.** `fs/stage_loading.rs`'s module doc still asserted the
premise the attribute had invalidated — "serde ignores the runtime-only keys (`status`,
`created_at`, `merged`, …)". A doc comment stating a serde behaviour is load-bearing
documentation: when an attribute changes that behaviour, the comment becomes a false claim
that reads as reassurance.

**Prevention:** before adding `deny_unknown_fields` (or any strictness attribute) to a
type, enumerate every `from_*`/`parse` call site that deserializes INTO it. If there is
more than one source and they carry different key sets, the attribute belongs on a
per-source type, not the shared one. The fix here parses the frontmatter as the type the
file actually holds (`Stage`) and projects down via `definition_from_stage`, so
`StageDefinition` keeps its plan-YAML guarantee untouched.

**Keep the projection compiler-enforced.** `definition_from_stage` is a struct literal with
NO `..Default::default()`, so adding a field to `StageDefinition` fails the build until the
projection is updated. This is deliberate: an earlier hand-rolled partial struct silently
dropped `stage_type`, `auto_merge`, `sandbox`, `context_budget` and
`before_stage`/`after_stage` on every daemon restart. Never reintroduce a spread or a
hand-maintained key allowlist here — both fail silently, which is the whole bug.

## Warn-and-Continue Turns Total Failure Into an Empty Result

`load_stages_from_work_dir` handled a parse error by `eprintln!`-ing a warning and
`continue`-ing to the next file. With every file failing, it returned `Ok(vec![])` — a
successful empty load. Its production caller is the documented recovery path in
`plan/graph/loader.rs` ("Stage files in .work/stages/ can be used instead of the plan
file"), so that fallback could never have worked; it would have produced an empty graph
rather than an error.

**Prevention:** a per-item skip is only safe when *some* items are expected to succeed. When
a loop skips EVERY item, that is a total failure wearing a success type. Either fail loudly
when the skip count equals the item count, or make the caller distinguish "no stage files"
from "no stage file parsed". A warning printed once per file is not a substitute — it
scrolls past in daemon output and nothing downstream can act on it.

**Test fixtures encode assumptions too.** Five pre-existing tests hand-wrote minimal,
plan-shaped frontmatter and asserted it parsed — the exact assumption the bug was made of.
They passed for as long as the bug existed. When a fix makes old fixtures fail, check
whether the fixture or the code was wrong before "repairing" the fixture; here the fixtures
were, and were rebuilt to serialize a real `Stage`.
