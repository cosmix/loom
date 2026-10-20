---
---
# Computed Values and Hidden Couplings

> Computed values unread
> retrieval-precision work, all real, all the same underlying shape: a piece
> of logic was locally correct and globally wrong because of something
> outside the function currently being read — a downstream consumer that
> forgot to ask, a second reader of a value that looked purely cosmetic, or a
> threshold whose meaning is a function of a corpus that changed underneath
> it. See [Context Retrieval](../architecture/context-retrieval.md) for the
> subsystem these came from.

## A Field Computed and Carried But Never Published Is Invisible

**What happened:** the exact-rung gate (A.1) needed a way to say "this match
is exact, but only on rarity, not on shape or an explicit backtick" without
weakening the reason list itself. The fix threaded a `confidence_ceiling`
through THREE files: `rank::rungs::RungScore::confidence_ceiling()`
(`rank/rungs.rs:87-90`) computes it, `RankedCandidate::confidence_ceiling`
(`rank.rs:141`) carries it, and `RankedCandidate::confidence()` (`rank.rs:157-163`)
is the one place meant to apply it — returning the weaker of the ceiling and
`Confidence::from_reasons`. `pack::build_chunk_item` and `build_source_item`
were both written to call `candidate.confidence()`, and every ranker-level
test that checked `confidence_ceiling` on a `RankedCandidate` passed. But an
earlier draft of `build_source_item` called `Confidence::from_reasons(&candidate.reasons)`
directly — bypassing the ceiling entirely — and nothing failed, because every
existing test either inspected `RankedCandidate` before packing or built a
`ContextItem` by hand with `confidence_ceiling: None` already baked in
(`context/tests/pack.rs:32,118`). The un-capped `high` label would have shipped
to every consumer of a packed item — the Knowledge Brief, `loom knowledge context
--json`, the eval harness — none of which ever sees a bare `RankedCandidate`.

**Why:** a value computed correctly and carried correctly is still invisible
if nothing checks that the LAST consumer in the chain actually reads it. Each
of the three files was independently reasonable — compute, store, apply — and
each had unit coverage. What no test exercised was the seam between "the
ranker computed it" and "the packer published it", because that seam only
exists at the boundary between two modules neither of which owns the whole
path.

**Prevention — detection rule:** when a value is computed in module A, carried
on a struct through module B, and meant to be APPLIED in module C, a
module-C-only test that hand-builds the struct with the field already set
correctly proves nothing about whether real inputs from A ever reach it. Ask:
"does any test drive A → B → C through the REAL functions, not a fixture that
already has the answer baked in?" If the answer is no, the wiring between A
and C is unverified regardless of how much each module's own tests pass.

**Fix:** `context/tests/pack_source.rs::published_confidence` (`:187-208`) ranks
a real graph through `rank_source`, packs the result through the real `pack`,
and reads back `packed.items[...].confidence` — the only path that actually
proves the cap survives ranker → candidate → packer. Its own doc comment
states the point directly: "a hand-built candidate would only ever test the
last of those three." `a_rare_only_exact_match_is_published_as_medium`
(`:214-224`) is the pinning test.

## A Display Predicate That Is Also a Control Signal

**What happened:** `ContextPack::degraded` (A.11) was introduced to flag one
specific reachable failure — a semantic revision naming a layer that neither
the base nor any overlay could back — and rendered as a `DEGRADED:` banner on
the brief's revision line. A first version of `degraded_reason`
(`context/retrieve/graph.rs:116-125`) treated ANY missing base file as
degraded. That is wrong on its own terms — a dirty working tree never
publishes a base at all (bases are immutable and revision-keyed; see
`context/graph_store/mod.rs`'s module doc), so "no base for HEAD, served from the local
overlay" is the ordinary steady state of any checkout someone is actively
working in — but the consequence went well past a wrong banner:
`reconcile_graph::spawn_if_needed` (`commands/hook/reconcile_graph.rs:210-211`)
fires a DETACHED, unbounded, full-repository tree-sitter rebuild whenever
`pack.semantic_freshness.stale || pack.degraded.is_some()`. Every prompt in
every working checkout was therefore both mislabeled AND starting a
background rebuild, throttled only by the reconcile debounce lock.

**Misleading signal:** the change that widened the predicate looked purely
cosmetic — it only touched which banner string got chosen. Nothing at the
call site hinted that the same `Option<String>` was also read as a boolean
trigger three modules away, by a function with no textual relationship to
`degraded_reason` beyond the field name.

**Why:** an `Option<String>` (or any "reason" field) reads as documentation —
something a human looks at — which is exactly the framing that hides a
second, machine reader deciding something expensive from the same value.
Nothing in the type signature of `degraded_reason` says "this also gates a
background job"; that fact lives three files away, in `spawn_if_needed`'s own
trigger condition.

**Prevention — detection rule:** before widening or narrowing ANY predicate
that renders as a display string (a banner, a warning label, a status field),
`rg` every reader of the struct field it populates — not just the renderer
you were looking at. A field is a control signal the moment more than one
function reads it, whether or not its name or type suggests that. When you
find a second reader, read its trigger condition (`stale || degraded`, in this
case) before changing what makes the first one fire.

**Fix:** `degraded_reason` is now a two-part honest test:
`base_revision` non-empty (a base was actually found, even a genuinely empty
zero-file one — a docs-only repository is a real, non-degraded case) OR
`graph.files` non-empty (an overlay alone answered the query with no base
present). Only when NEITHER holds — nothing anywhere can answer this revision
— does `degraded_reason` return `Some`. Its own doc comment
(`context/retrieve/graph.rs:100-115`) now states the reconcile-trigger
consequence explicitly, so the next person widening this predicate sees it is
not merely a display choice.

## Corpus Composition Changes What "Ubiquitous" Means

**What happened:** query stopwording (A.2) drops any term whose document
frequency exceeds `corpus_size * stop_df_ratio` (default `0.10`) — a rule
designed to return nothing for a query of pure stopwords, and it worked, right
up until A.15 indexed the project's own prose into the same corpus the rule
measures. Measured directly on this repository: over the curated-only corpus
(658 documents, floor 65.8), the query "worktree claude code sandbox settings
rules sessions" kept `settings`=57, `rules`=48, `sessions`=65 as surviving
terms and returned a normal pack. After prose indexing (904 documents, floor
90.4), the SAME three terms measured 105, 93 and 102 — prose is loom's own
design documentation, written in the exact vocabulary its own questions are
asked in — so every term in an ordinary, well-formed question exceeded the
floor simultaneously and the pack came back EMPTY.

**Why:** `stop_df_ratio` is a RATIO, and a ratio's meaning is inseparable from
what is being divided. The rule was tuned and tested against one corpus
(curated knowledge only) and silently became a different, stricter rule the
moment the corpus it measures grew to include a second, larger, thematically
overlapping document set. Nothing about the stopwording CODE changed; only
its input population did, and that was enough to break it. The failure mode
generalizes past this one rule: any threshold expressed as `X% of the corpus`
is a function of the corpus, not a fixed property of the terms it judges — and
a change elsewhere in the system that grows or reshapes the corpus changes the
rule's effective behavior with zero changes to the rule's own code.

**Prevention — detection rule:** before adding new documents, files, or
records to any corpus/population a ratio-based rule already measures against,
re-run that rule's real regression cases over the ENLARGED population, not
just over synthetic fixtures sized like the old one. A rule that "still looks
right" on a small hand-built test corpus can silently invert on the real one
once its denominator changes shape. When a fix for this class of bug is
proposed, prefer widening what the rule CONSIDERS (a rescue floor, an
exemption class) over shrinking the population the rule measures — a second,
narrower population invites the same class of bug the next tuning pass any
OTHER caller of that population makes (see `ExactGate::is_rare` below).

**Fix:** a rescue floor (`rank/corpus/stopwords.rs::rescue_rarest`) resurrects
up to `RESCUE_LIMIT = 3` of the rarest dropped terms ONLY when stopwording
would otherwise empty the whole query, itself capped by a looser
`stop_rescue_max_ratio` (default `0.25`) so a term that is truly ubiquitous
under any reading (`the` at 90%) is never resurrected. The alternative that
was considered and rejected — computing document frequencies over curated
documents only, excluding prose — was rejected specifically because
`lexical::ExactGate::is_rare` (`context/lexical/evidence.rs:185-191`) reads
the SAME document-frequency map from the opposite end, to decide whether a
name is corpus-rare enough to claim an exact-match rung on its own. Splitting
the map in two would let one term be simultaneously "too common to score" and
"rare enough to claim an exact symbol match" — a contradiction with no
principled resolution. One shared map, plus a guarantee that the surviving
set is never empty when anything is rescuable, keeps both rules honest
against the same reality.

## A Merged Lane-Provenance Value Gated Both Display and a Control's Visibility (2026-09-12)

**What happened:** `LaneSlot` (`web/src/components/settings-control.tsx`) folds a
write's in-flight status into the lane's displayed provenance — `p = status.phase ===
"error" ? "error" : status.phase === "pending" ? "pending" : state.provenance` — so the
UI can show one merged badge. The same merged value also gated the clear (`✕`) button's
visibility. A rejected write to an already-set key therefore hid the clear button: the
merged value read `"error"`, not `"set"`, even though the underlying config entry was
still set and clearable. The brief's own pseudo-code had the same shape, and the
component it replaced (the old `SettingRow`) gated the same button on
`provenanceAt(...) === "set"` alone — a single, unmerged source.

**Why:** a value built for one reader (the badge) looks safe to reuse for a second
reader (the button) once both need "the same kind of state" — but a display merge is
lossy by design, and the button needed the fact the merge discarded.

**Prevention — detection rule:** before gating a control's behaviour on a value that
also drives a display string or badge, ask whether that value is a MERGE of more than
one underlying signal. If it is, `rg` every reader of the pinned model state
(`state.provenance` here) and gate behaviour on that unmerged state directly, never on
the display value derived from it. Compare against the component being replaced: what
did IT gate the same behaviour on?

**Fix:** the clear button now gates on `state.provenance === "set" && !pending`,
reading the model's own provenance instead of the merged display value.

## `ensure_loom_hooks_local` Leaves Emptied Event Arrays Behind — Callers Must Order Around It (2026-09-13)

**What happened:** `fs/permissions/settings.rs::ensure_loom_hooks_local_inner` can leave a hook
event's array present but empty after rewriting hooks, rather than removing the key. `loom repair`
strips loom-written keys BEFORE rewriting hooks specifically to avoid layering a stale empty array
under the new one.

**Why:** the function's job is installing/updating loom's own hook entries; it was never written to
clean up an empty array a previous edit left behind, so a caller that rewrites hooks without first
stripping the old keys can end up with both an empty array and the new entries.

**Prevention:** when a caller needs both "remove loom-written keys" and "rewrite hooks" against the
same settings file, the strip must run first — this function does not do it for you.

## Daemon Start Reverts Any Plan Edit Made After an Amendment (2026-09-13)

**What happened:** the token-optimization plan lost its knowledge-bootstrap revalidation, committed in `ecb9eb06` at 03:28. Earlier, around 03:15, that stage had run `loom stage amend`, which saved snapshot `.loom/work/plan_versions/1.md`. At 12:10:18 a reinstall (`dev-install.sh` runs `pkill -x loom`) and `loom run` started a fresh daemon. `Orchestrator::new` (`orchestrator/core/orchestrator.rs:173`) calls `verify_plan_versions_consistency`, and its Case 2 (`plan/amendment.rs:805-826` at the time) found the live plan unequal to that snapshot and overwrote the whole file with it. The DONE rename later carried the stale text. The revalidated version survives only in commit `ecb9eb06`, as the plan file committed there.

**Why:** the catch-up compared whole files and restored the snapshot on any difference, so it could not tell an amendment that never reached the file from a prose edit made after one. Case 3 of the same function, the stage-file catch-up, already compared only the amended field.

**Prevention:** a catch-up that repairs a file from a snapshot compares only what the snapshot is authoritative for (here, the amended criterion) and splices that into the live content; it never replaces the whole file. Detection: a tracked plan file that shows as modified right after a daemon start, with a diff that reverts a recent commit.

**Fix (2026-09-14):** Case 2 calls `reconcile_amended_field` (`plan/amendment_catch_up.rs`), which compares and splices only the amended field and never writes a plan it cannot parse; see [Merge and Recovery Edge Cases](../concerns/merge-and-recovery-edge-cases.md).

## loom update refreshed assets into the default roots

**What happened:** An install run with `LOOM_CLAUDECODE_INSTALL_DIR` / `LOOM_CODEX_INSTALL_DIR` exported placed assets in the custom roots, but a later `loom update` from a shell without the variables refreshed `~/.claude` and `~/.codex` instead.
**Why:** `default_paths` resolved roots from the environment alone and nothing persisted the roots an install chose; the knowledge entry claimed the flagless self-update path "retains configured roots", which held only while the variables stayed exported.
**Prevention:** A setting that picks where a later automatic step writes must be persisted by the step that chose it, not re-derived from the caller's environment.
**Fix:** each flagless `install-assets` recorded its roots in `~/.config/loom/install-roots.toml`, read by `default_paths` between the env override and the default. The whole custom-root feature, record included, was later removed; see "A relocated Claude root never reached stage sessions" below.

## StageSummary Is a Wire Type: Status Clients Recompute Attention From Deserialized Data

**What happened:** a brief told an implementer to add merge-resolver facts to `StageSummary` with `#[serde(skip)]` so the dashboard wire would not change. Every daemon-sourced frame would then have shown the default (no live resolver, 0 attempts), because the facts exist only in the daemon's copy.

**Why:** `StatusData` is built in the daemon (`daemon/server/status.rs`), sent as JSON in `Response::StatusUpdate`, and deserialized by both the web broadcaster (`commands/status/web/broadcast.rs` `classify_response`) and the live TUI (`commands/status/ui/tui/app.rs`). Each client calls `attention_entries` on the deserialized stages, so any input to that function must survive serde. Tests that build attention from in-memory stages cannot see the loss.

**Prevention:** a field that `attention_entries` or any renderer reads must be serialized. Use `#[serde(default, skip_serializing_if = "Option::is_none")]` for state-specific facts. The browser's `stageSummarySchema` (`web/src/api/schema.ts`) is `.strict()`, so either add the key there or strip it in `collect_snapshot` before the browser snapshot. Pin it with a test that round-trips through `serde_json` before computing attention.

**Fix:** `merge_resolver_session` and `merge_resolver_attempts` are optional keys, present only on MergeConflict/MergeBlocked stages. They cross the daemon socket, and `without_merge_resolver_facts` (`commands/status/web/model.rs`) strips them from the browser snapshot once attention is built. `the_running_resolver_note_survives_the_daemon_wire` pins the round trip.

## Operator Guidance Keyed on Status Alone Told Operators to Do the Daemon's Work

**What happened:** every `loom status` view (text, TUI, web "what to do") told the operator to run `loom stage merge` for MergeConflict and MergeBlocked while the daemon's merge resolver was already working on them. It said `loom stage retry` for crashes the daemon auto-retries, `loom stage resume` (hook-internal) for a pending question, and a bare `loom stage human-review` beside its three flagged forms, which rendered as four identical truncated lines. Escalation reasons named `loom stage merge` and `retry` for NeedsHumanReview stages, where both commands refuse.

**Why:** the hint came from a fixed status→command table written before the resolver and auto-retry existed. It saw only the status and the id, and nothing checked a suggested command against that command's own state gate.

**Prevention:** guidance for a state must say whether loom is handling it (`automatic`) and read the facts that decide that (a live resolver session, retry eligibility). A command named in a reason or hint must accept the state the stage will be in when the operator reads it; check the command's gate (`require_merge_state`, `skip_retry` retryable set, `human_review` status check) before naming it.

**Fix:** `AttentionEntry` carries `command`, `note` and `automatic` (architecture/status-data-model.md, Attention Rendering). Merge escalations name `human-review --force-complete` through `manual_merge_steps`.

## `loom stage retry --context` Never Delivered the Operator's Text (2026-10-14)

**What happened:** an operator answered a blocked troofoo stage twice with `loom stage retry local-runtime --context "..."`; both retried sessions started with only the three generic recovery actions, re-ran the same check and blocked again on the same need.

**Why:** `--context` only forced the recovery reason to Manual (`skip_retry.rs`). `RecoverySignalContent::for_manual` took no context, and the text was stored in `close_reason` after the signal was written, where the recovery formatter never reads.

**Prevention:** a flag whose purpose is to hand text to an agent needs a test that reads the delivered artifact, the written signal, for that text.

**Fix:** `commands/stage/recover.rs` `with_operator_answer` inserts the answer verbatim as the first recovery action before `generate_recovery_signal`, with a test on the written file.

## A Global Read Guard and the Skill Catalog Collided Across Stages

**What happened:** the guard-hooks stage's `read-guard.sh` (warn above 400 lines, deny a repeated
full read) merged one commit before the skills-catalog stage, whose loader tells the model to read
an oversized catalogued `SKILL.md` in full. Neither stage's own tests could see the other's change;
integration-verify found that the guard would argue the model into partial loads and could block a
popular oversized skill outright.

**Why:** a global Read-class hook and a large-file read path were built in parallel stages, each
tested only against its own code.

**Prevention:** drive a new large-file read path against a real oversized example under every
global Read-class hook before trusting the build.

**Fix:** `loom-hooks/_read_discipline.sh` rule 0 exempts `SKILL.md` under both skill roots from
every line-count rule ([Skill Catalog](../architecture/skill-catalog.md#why-most-skills-live-outside-claudeskills)).

## A relocated Claude root never reached stage sessions

**What happened:** an operator installed loom with a custom Claude asset root (`LOOM_CLAUDECODE_INSTALL_DIR`, recorded in `~/.config/loom/install-roots.toml`) and left Claude Code on its default config dir (`CLAUDE_CONFIG_DIR` unset). A v2 standard stage (`doc-root-install`) finished its work with green criteria, then blocked: "subagent-stop.sh records rounds only for agent_type loom-code-reviewer; this harness has no loom-* agents". No review round could ever be recorded.
**Why:** Claude Code discovers agents, skills and the global `CLAUDE.md` only under its config dir (`CLAUDE_CONFIG_DIR`, else `~/.claude`), the project's `.claude/`, plugins and per-launch flags. A custom root that is not Claude Code's config dir is therefore read by no Claude Code session, the operator's included; Codex likewise reads only `CODEX_HOME`, else `~/.codex`. The custom-root work scoped itself to install destinations ("runtime discovery relocation excluded") and later made loom's own readers follow the root; its tests checked where assets were written and how loom resolves them, never whether a launched session sees them. Hooks hid the gap: `loom init` registers them in the repository's `.claude/settings.json` and the session capsule passes them, both by absolute path, so the install looked healthy until the review gate needed `loom-code-reviewer`. An operator who did point `CLAUDE_CONFIG_DIR` at the root was defeated too, because the stage session wrapper runs under `env -i` without it.
**Prevention:** a feature that moves where an asset lives must list every consumer of that asset, the external runtime included, and prove each one finds it at the new place with a launch-level test.
**Fix:** the custom-root feature was removed for Claude and Codex alike: the env vars, the `install-roots.toml` record, `install-assets --claude-dir/--codex-dir`, `AgentRoots` and the shell hooks' own-location root. Loom installs only into `~/.claude` and `~/.codex`. A plugin is the route if assets outside `~/.claude` come back ("Loom Assets Outside ~/.claude Need a Plugin" in [guidance-channels-and-plugin-scope](../conventions/guidance-channels-and-plugin-scope.md)). An operator who used a custom root keeps hook entries pointing at it in each repository's `.claude/settings.json`: `is_loom_hook` (`fs/permissions/hooks.rs`) matches only `~/.claude/hooks/loom/` spellings, so `loom init` and `loom repair` leave them beside the new entries and they must be deleted by hand.
