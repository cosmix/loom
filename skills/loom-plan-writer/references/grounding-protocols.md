# Ground Every Claim — full protocols

Read when: writing any stage that changes a shared type, reuses or mirrors existing code, adds a destructive path, runs code under a new runtime, or depends on a sibling plan. `SKILL.md` Section 1 carries the short checklist; this file holds the full text it points to.

> ⚠️ A plan is a set of CLAIMS about code: "this function does X," "this enum's consumers are Y," "this field is safe to add," "this command type-checks." **Every claim is WRONG until the code confirms it.** The design spine is usually sound — defects hide in UNREAD seams. A file the plan NAMES is a promise to read; a described file is an unread file.

Before any stage description, `acceptance`, `artifacts`, `wiring`, or `wiring_tests` asserts anything about a seam, OPEN that seam and read it to the bottom. Never assert from memory, a sibling repo, a plausible filename, or "it usually works this way." The repo's own incident/runbook docs are PRIMARY — read the one the user has open before encoding an external system's behavior from memory.

**VERIFY-BEFORE-WRITE CHECKLIST — run for every stage:**

```text
□ Every file the stage NAMES, I have OPENED (not inferred from its name).
□ Every symbol the stage CHANGES, I grepped for every importer/consumer across
  the WHOLE repo (BOTH packages in a monorepo) — and followed each edge ONE ring
  out (callers/renderers I did not already think of).
□ Every behavior the stage ASSERTS (a guard enforces X, an error code is
  terminal, a field is safe, a command type-checks) — I read the implementation
  that provides it, including catch-alls and branch ORDER.
□ Every value/behavior the design LEANS ON, I read the line that PRODUCES it (not
  the type/schema/getter that DESCRIBES it) AND confirmed it holds in EACH
  environment that runs the code (prod vs dev, build-time vs unit-test vs e2e,
  container env set, same-origin vs cross-origin). "The symbol is defined" ≠ "it
  holds the right value in the runtime that executes THIS code."
□ Every RULE the plan states about ONE site ("reset this global here," "keep env
  clean for this boot path") — I grepped its structural SIBLINGS (same-shape
  modules, every importer) and applied it to ALL in the same pass, not as a
  one-off note.
□ Every message / limit / line / count / status code / external behavior /
  package dependency is READ from its source, never recalled.
□ No claim rests on memory, a sibling repo, or a plausible name.
□ Every claim about a SIBLING PLAN's surface (upstream symbol, consumer seam,
  file owner) passed the Cross-Plan Contract Protocol — verified against
  committed code or the sibling's stage YAML, never its prose.
```

**HIGH-FREQUENCY TRAPS** (each is a logged, repeated failure):

1. **Widen an enum / union / shared type / required field** → run the Blast Radius Protocol. The single most-repeated failure.
2. **"Behavior-preserving refactor"** → prove it PER CALL SITE by reading the EFFECTIVE check (in-handler re-reads, defensive fallbacks, stored-vs-derived values), not the nominal guard. A guard census is mandatory.
3. **"The single funnel / the one place X happens"** → grep the callers of the LEAF PRIMITIVE the funnel wraps, NOT the funnel's own callers. A direct call to the primitive is invisible to a funnel-caller grep. Hook at the primitive.
4. **Reuse a "generic" seam** → read its constructor / closed-over config (and an "atomic" helper's contention granularity — one global lock vs per-key) before building on it. A shared method can bake in caller-specific config. Full checks: Reuse & Precedent Protocol below.
5. **Edit target from a filename** → NEVER. Grep the actual predicate and follow the flow. A pure re-export (`export type * from …`) has nothing to edit — naming it as an editable target is a no-op (logged 3×).
6. **A persisted state flag** → trace what SETS and what CLEARS it across EVERY transition (role removed, resource disabled, early-return success). If the natural recovery event doesn't reset it, you built a one-way latch.
7. **"Out of scope / follow-up"** → DECOMPOSE it. On a shared/generic surface a half-cut is a correctness hole, not a clean defer. A thing you noticed in passing is not a thing you handled.
8. **Line numbers as edit anchors** → anchor every edit by SYMBOL + a short snippet; line numbers are advisory (they drifted in every logged pressure pass). Sequential stages editing one file make later stages' absolute anchors guaranteed-stale — say so in the plan. Re-read any deletion RANGE edge-to-edge (a one-line overshoot deletes a keep-line), and when an edit SUPERSEDES existing code, say what to DELETE — leaving the old branch double-writes.
9. **"Typechecks" ≠ "bound"** for vendored / FFI / wasm bindings (a curated `.d.ts` over-declares) → any API path unexercised at runtime anywhere in the repo must be SPIKED — EVERY novel path the plan rests on, not just the scariest one — and each prescribed call form must cite an in-repo runtime precedent (file:line).
10. **Dependency / toolchain / provider facts from memory** → NEVER encode "package X ships types," "version line Y is current," or "the provider's API returns Z" from memory. Curl the registry (`registry.npmjs.org/<pkg>/<ver>` → `types`/`exports`/`peerDependencies`), read the installed source, and re-check third-party API docs (schema AND rate limits) AT PLANNING TIME. Verify the peer graph CLOSES before pinning a version set — "latest of each" is not a compatible set (logged: a plan forbade `@types/three` on a false memory; another paired Vite 6 with a plugin that peers `vite@^8`).
11. **Library conventions assumed (UV/axis/order/defaults)** → read the INSTALLED library source for any convention a design leans on, and test at LANDMARKS (poles, origin, antimeridian, a known-answer point) — range and algebraic-invariance checks pass with a flipped convention (logged: an upside-down globe behind a green UV-range test).
12. **A signature is not a mechanism** → for every function the plan specs, work BACKWARD from the promised behavior: the INPUTS must carry what the behavior and OUTPUT type require (an id-set filter that never receives the id set; a batch stamped with a version it was never passed). Give every cross-format field ("timestamp: number") a defined per-source decoder, and every network/persisted payload an explicit runtime parser — a static type validates nothing at runtime.
13. **Runtime lifecycle deferred to "integration will catch it"** → ownership, completion-based scheduling (not overlapping timers), cancellation, retry taxonomy, invalidation/generation guards, idempotent `dispose()`, and a resource budget's SCOPE (global vs per-X) and ORDERING (evict/reserve BEFORE allocate) are plan-level design decisions. Settle them in the plan; an unsettled lifecycle ships an unowned leak or a stale-data race.

## Blast Radius Protocol (enum / union / shared type / required field)

Widening a type is NEVER "just the type." Run all six:

1. Grep EVERY importer of BOTH the schema AND the inferred type, across every package. A name in `shared/` is a cross-subsystem contract.
2. Enumerate EVERY exhaustive consumer: switches, ternaries, `Record<K,…>` literals, `Partial<Record>`, hand-written unions, boolean `===`/`!==` chains, display/label maps, validators, serializers, public DTO mappers, and MOCKS (a mock is a parallel implementation of the same contract — it changes in lockstep).
3. Classify each consumer COMPILER-CAUGHT vs SILENT — verify the ACTUAL compiler flags. SILENT misses ship bugs: value-returning switch/ternary with no `default`, `Record<string,…>` + `?? fallback`, an `as`-cast `Record<Enum,…>` (an exhaustiveness LIE — back it with a runtime assertion), boolean comparison chains, primitive-typed params. Assign every SILENT consumer an explicit edit task.
4. Trace any NEW default value end-to-end through validation AND render/consume paths — your own new default is the first thing to break (unsaveable, or renders as a raw enum string).
5. "Additive ⇒ non-breaking" is FALSE for a required field in an inferred-type contract — grep every typed literal/builder (prod + tests + fixtures + mocks) before calling it safe.
6. For a RESULT/response schema, decide inclusion EXPLICITLY ("does it parse" ≠ "should it be allowed") — a permissive widened union can leak an internal variant into public DTOs, webhooks, or stats.
7. Grep the tree for the LITERAL member strings, not just the type name — dev HUDs, label maps, op-name arrays, and prose keep private copies a type-consumer grep never sees. A new member of any op/kind set needs its label everywhere members are labelled.
8. Sweep the WHOLE doc tree: product docs, README, and in-app Help/About (which are CODE — assign them to the UI stage, not the docs stage) for pinned counts ("exactly 56 entries"), verbatim union restatements, UX-law contracts, and explicit deferrals ("not in v1: X") the change breaks or satisfies. This bit three of four logged plans.

## Reuse & Precedent Protocol ("reuse X" is a claim with five checks)

"Reuse X from file A" / "model new path N on existing path Y" failed more pressure passes than any claim except type-widening. Before prescribing reuse:

1. **Importable.** X is actually `export`ed — grep the export, not the definition (a private helper means "replicate," not "import"). If the plan ADDS the export, export the TRANSITIVE CLOSURE of X's own private callees too — exporting `f` that calls three file-local helpers does not compile for the importer.
2. **Reachable.** The stage whose work needs the edit can actually touch the file — "reuse a helper from holes.ts" is dead if another stage's `files:` owns holes.ts. Put the export in the stage that owns the file, ordered FIRST in the DAG.
3. **Embedded assumptions hold.** Read X for caller-specific copy ("Choose screw…"), closed-over config, invariants it does NOT enforce (an unchecked fuse the caller depends on), and calibrated constants (a threshold tuned to one topology is un-reusable for another). If any fails for the new use, the task is "extract + adapt" with the delta specified — not "reuse."
4. **Failure path read.** "Mirrors existing behavior" requires reading Y's FAILURE path, not just its happy path — the flow you cite may THROW where your plan promises auto-fallback.
5. **Full-body diff for paralleled flows.** A new path "like existing atom/handler Y" must enumerate EVERY side-effect of Y (rollback staging, commit-after-success ordering, history reset, disposal, revision bumps) and state which the new path replicates, which it omits, and WHY. And pick the SAFE template — check concerns.md/mistakes.md for known defects in the sibling you copy; the nearest complex sibling may carry the bug.

## Wireability Protocol (a true fact ≠ a landable edit)

A verified fact is half the check; the other half is whether the codebase's actual SHAPE lets the executor use it. Before the plan prescribes an edit ("reuse X in B," "thread `signal` through M," "act on every response"), answer three carrier questions:

1. **Import direction / cycle.** When a plan says "reuse X from file A in B," read the EXISTING A↔B import edge FIRST. If B already imports A (or the reverse of what you need), the back-import is a cycle. A shared value goes in a LEAF module both import — never back-imported into a module the other already depends on.
2. **Signature reaches what you pass — trace OUT, not just in.** Before "wrap / pass / thread X through method M," read M's real signature AND every hop it delegates to. "The API can be aborted / takes the param" is a fact to VERIFY at the signature, not assume. Then trace OUTWARD to every CALLER that can short-circuit before your shared handler (an early `throw`/`return` in the caller skips a "first statement in the method" fix). "Act on every X" means every caller, not just the branches inside one function. Trace UPSTREAM too: a widened consumer guard is dead if no producer ever EMITS the event — follow the whole intent pipeline (input → resolver → dispatch → buffer → consumer) and place the edit at the right point. When a listener buffers last-writer-wins for deferred processing, validity/ownership guards belong at the PRODUCE site, not the consume site. And if the flow opens an async window (a busy flag other writes honor), state what happens to writes issued DURING it — "it lags" and "it's dropped" are different bugs.
3. **Survives the lifecycle that runs it.** Middleware/effect ORDER (a guard before the handler you patched returns a different code), framework lifecycles (double-mount, listener cleanup), live runtime toggles, and manifest/CI wiring (deps declared, command actually selected — `SKILL.md` Section 8). An edit correct in isolation can be dead or double-firing once the surrounding lifecycle runs it.
4. **UI affordance: renders ≠ works.** For every control/warning/label the plan promises, verify three edges: the control's onChange traces to a helper that ACCEPTS the new input (not one that early-returns `previous` unchanged); the triggering state has a concrete render path that DISPLAYS it (a warning-severity issue with no rendering surface never shows); and the displayed data is actually RETURNED by some resolver — "surface X" must name the field/channel that carries X end-to-end. A formatter with first-match/fallback logic over a field you widened needs its own edit + test.

## Destructive-path Protocol (clear / reset / mode-switch / teardown)

A destructive operation speced by naming a few atoms/fields ships data loss. For every clear/reset/switch/dispose the plan introduces:

1. **Trace from the RENDER root, not the logical root.** Enumerate every piece of derived state keyed off a DIFFERENT root than the thing being cleared (mesh/cache/session handles/lookup tables). Clearing "the document" while the viewport reads "the mesh" leaves stale render state on screen.
2. **Enumerate EVERY path that reaches the destructive action.** A confirm-guard on two of three routes is a hole — the third silently destroys. Include indirect routes (a toggle that clears BEFORE the guarded action ever runs exempts itself from the guard).
3. **Enumerate every replay/recovery channel.** Undo/redo stacks, crash-recovery sessions, and queued jobs can RESURRECT what you cleared — a live recovery session is a standing instruction to rebuild the discarded thing. Reset them in the same operation.
4. **Guards live at the MUTATION, not the affordances.** Hotkey/UI gating misses the pointer/store/replay paths that reach the same primitive; a guard at the mutating primitive is the backstop (same spirit as trap #3).
5. **Wiring a dead path makes its feeders live.** When the plan connects a previously-orphaned hook (recovery adopt path, callback, listener), re-audit every EXISTING eager write that feeds it — code harmless while the consumer was orphaned becomes a live correctness seam the moment it is wired.

## Running existing code under a NEW runtime / resolver / bundler

For test-infra and migration plans, the ZEROTH claim is **"does it even import and resolve under the planned config?"** — verify empirically with ONE probe import before designing any stage:

- Enumerate every module-top-level reference to the OLD runtime's globals/builtins in the SHARED import graph (a rate-limit/IP middleware every route imports is exactly where a top-level runtime-global access hides).
- A fresh loom worktree ships NO gitignored deps (`node_modules`) — read `.gitignore` + existing lockfiles for the repo's REAL locking convention before any stage depends on them.
- Before writing a build/test command into `acceptance`, confirm it EXISTS and does what you think (does `build` type-check, or only bundle?). Read the actual `package.json` scripts / Makefile / cargo aliases.
- Apply any gotcha you cite to the plan's OWN mechanics and to EVERY case family touching the same resolver.

## JS/TS projects: provision worktree dependencies first

A fresh worktree has no `node_modules` (ignored files are not checked out), so node module
resolution walks up into the MAIN repo's `node_modules`, and any in-session test run then writes
its caches there (vite: `node_modules/.vite-temp`) — denied by the sandbox with EROFS, and it
would corrupt state shared across parallel stages if allowed. Any stage that runs JS/TS tests
in-session must make its FIRST task an explicit dependency install in the worktree
(`bun install`). The `setup:` field does not cover this: it only prefixes acceptance commands, so
it never runs as part of the session's own task work — and an acceptance command is NOT reliably a
host-side command either. It runs wherever it is invoked from: the daemon verifies on the host, and
`loom stage complete` runs the same list from inside the sandboxed session (`SKILL.md` Section 8).

## Cross-Plan Contract Protocol (sibling plans in doc/plans/)

When the plan is part of a multi-plan program (sibling `PLAN-*` / `IN_PROGRESS-*` / `DONE-*` files sharing one tree), cross-plan claims are a top logged failure class: plans modelled their siblings from their own assumptions instead of the siblings' real text and committed code. Before writing any claim that touches another plan:

1. **Enumerate the siblings.** List every plan that owns files, symbols, or seams this plan touches — upstream (you consume it), downstream (it consumes you), and neighbors sharing a module directory. Read them.
2. **Committed code beats plan prose; stage YAML beats overview prose.** Verify an upstream surface in this order: committed code (if the sibling merged) → the sibling's stage `artifacts:`/`wiring:`/`acceptance:` → nothing else. A capability named only in a sibling's overview prose is built by NO stage — treat it as MISSING and record a required amendment. (Logged: `createTileUploader` was "load-bearing" prose in one plan, appeared in no stage's artifacts, the plan closed green without it, and the consumer plan stalled at zero code.)
3. **Cite real symbols, never plausible ones.** Every upstream type/function this plan names must be quoted from the verified surface with its exact name, signature, and exporting module (`CityRecord` not `City`; `bindingFor(id)` not "returns a handle"; `wDayMaster` not `wDay`). Prose paraphrases drift; a wiring grep is satisfiable by the wrong symbol.
4. **Contract line + first-stage fail-fast.** Every cross-plan dependency gets a contract line in the plan — exporting module, symbol, signature — plus a one-line grep against COMMITTED CODE proving it (e.g. `rg -n "export function createTileUploader" src/renderer/tiles`) in the FIRST dependent stage's acceptance. A plan should fail on its first stage when its premise is false, not four stages in at integration-verify. Never grep sibling plan files from acceptance (they get renamed/archived) — grep the code.
5. **Disjoint file ownership.** Before claiming any path, check every sibling's `files:`/`artifacts:` for it. Never land bare files in a shared module directory a sibling owns — carve a disjoint namespace (`src/data/weather/`, not a second `src/data/types.ts`).
6. **Read the consumer's real contract.** When this plan hands work to a downstream engine (or consumes one), pin that engine's ACTUAL public seam from its plan + code: cache keys, injection points, and the variance axes its consumers need (content version, time, params). If the needed seam does not exist, state the required amendment to the other plan as an explicit BLOCKING dependency — never silently assume a convenient interface.
7. **The owner reconciles names.** The plan that OWNS a shared contract reads its consumers and either matches their expected names/accessors or exports a reconciling alias/helper (`City = CityRecord`, `getCityById`) — don't leave each consumer to paper over the gap independently.
8. **Honor enforced boundaries.** Read the merged lint/import-boundary config before choosing module homes and wiring seams — a file placement that violates a sibling-enforced zone fails this plan's own gates. Cross-zone construction/registration belongs to the composition root, in a stage whose `files:` includes it.
9. **Re-base on current code.** Names, paths, and seams move between plan-writing and execution. Anything the knowledge files mark SUPERSEDED must not appear in a stage's instructions; re-verify every "edit the call in X" against today's tree before the plan ships.
10. **Discovery must amend the graph.** When knowledge-bootstrap or a verify stage is told to check a cross-plan premise, pair the check with a named remediation ("if absent: build it under this stage, in the owning module's territory") and re-point dependents — a blocker entry that leaves the graph unchanged is a stall, not a fix.
