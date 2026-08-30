# Knowledge Write Channel

> Why a distillation stage cannot write knowledge directly, the append-only-is-not-enough gap, and how doctrine baked into signals only reaches the next plan.

## The Distillation Stage Cannot Write Knowledge

**What happened:** the `knowledge-distill` stage of
`PLAN-automatic-knowledge-and-source-graph` could not write a single byte of
`doc/loom/knowledge/`. Six of its nine acceptance criteria were unsatisfiable by
construction. Probed from inside the worktree: `README.md` writable, `doc/` writable,
`doc/loom/` writable, `doc/loom/knowledge/` READ-ONLY. `loom knowledge update` fails with
`Failed to open temp file: ./doc/loom/knowledge/<f>.tmp: Read-only file system (os error 30)`.

**Why — and it is a REGRESSION, not a config typo.** Three facts compose:

1. The plan sandbox carries `deny_write = ["doc/loom/knowledge/**"]`, and
   `doc/loom/knowledge/**` is absent from `allow_write`. That rule is CORRECT for
   implementation stages: agents must not hand-edit knowledge.
2. That rule used to be inert. `sandbox/settings.rs` emitted denies as `Write(path)`, and
   Claude Code's file permission check consults only `Edit(path)` rules — see
   `concerns/sandbox-write-rules-inert.md`. Every earlier distillation stage wrote
   knowledge fine *because the guard did nothing*. Correcting the emitter to `Edit(...)`
   (`settings.rs:287`) made the guard real.
3. The harness now projects permission denies down into the OS sandbox, so the block
   reaches CHILD PROCESSES. That is why `loom knowledge update` gets EROFS rather than a
   tool refusal.

**The load-bearing false assumption:** the doctrine "agents update knowledge through
`loom knowledge ...`, never by editing the files directly" (`README.md`) treats the loom
CLI as a PRIVILEGED writer. It is not. It is an ordinary child of the sandboxed shell and
is denied by exactly the same rule as the agent's own tools. There is no privileged
writer anywhere: the daemon protocol has no knowledge RPC (`daemon/protocol.rs` carries
only stage finalisation and dispute verdicts).

**Prevention:**

1. **Any rule of the form "only tool X may write path P" requires X to run OUTSIDE the
   sandbox that denies P.** If X is a child process of the sandboxed session, the rule
   does not gate X — it disables X. Either give X a privileged channel (a daemon RPC, or
   the spool-and-drain shape in `patterns.md`) or put P in `allow_write` for the stages
   that legitimately use X.
2. **Making an inert guard effective is a behaviour change everywhere the no-op was
   load-bearing.** After fixing one, re-run the stage TYPES that depend on writing the
   newly protected path — not just the tests.
3. **Plan-time detection, mechanical:** intersect every stage's `files:` list with the
   plan sandbox's `deny_write` list. A stage told to modify a path it cannot write is a
   PLAN bug, and it surfaces as a stage failure hours later and one worktree away.
4. A `knowledge-distill` stage MUST have `doc/loom/knowledge/**` in `allow_write` and
   MUST NOT have it in `deny_write`.

**And the escape hatch was shut at the same time:** `loom stage dispute-criteria` reads
`.work/user.token`, which the same generated settings put in `denyRead`. An agent that
correctly diagnoses an impossible criterion has no structured way to say so.

## Append-Only Is Not Enough for a Reduce Step

Distillation step 6 — "remove or update stale knowledge entries" — had no tool for one plan cycle:
`loom knowledge update` appends, and `replace-section`, the only overwrite verb, had been deleted by
the CLI collapse. A distillation stage could ADD knowledge but not CORRECT it, while its own
doctrine required correction, and stale entries actively mislead (a resolved concern left live is
quoted into every later stage's Knowledge Brief).

**Closed 2026-08-19.** `loom knowledge replace-section <file> "<heading>" "<body>"` is restored
(`cli/types_memory.rs`, `cli/dispatch.rs`, `commands/knowledge/mod.rs::replace_section`) over the
`KnowledgeDir::replace_section_target` splice that had survived the collapse in the fs layer. It
overwrites the body under the first `## <heading>` up to the next `##` heading, appends the section
(and says so) when no heading matches, and strips a `## <heading>` line repeated at the top of the
body so a caller carrying `update` habits cannot double the heading. Pass the body WITHOUT its
heading line.

The channel doctrine now has a working shape end to end: file tools stay blocked on
`doc/loom/knowledge/**` inside worktrees (`hooks/worktree-file-guard.sh`) while the CLI writes
through the sandbox grant (`sandbox::config::apply_knowledge_write_grant`); a non-knowledge stage
records a staleness find as a `stale-knowledge:` memory; the knowledge-distill stage applies every
one of them with `replace-section` BEFORE curating anything new (CLAUDE.md Rule 12). Still missing:
a delete verb and a heading rename — see `concerns.md`.

## Doctrine Baked Into Signals Reaches Only the NEXT Plan

The signal for this stage ordered "ALWAYS finish distillation with `loom knowledge
index`" — a verb this very plan deleted. The merged source already says the opposite
(`orchestrator/signals/cache.rs:590`: the index regenerates on every knowledge write and
there is NO index step). Signal prefixes are rendered by the RUNNING daemon binary, so a
plan that rewrites agent doctrine cannot change the signals of its own later stages.

**Rule:** when a stage signal contradicts the merged source it describes, trust the
source, do not run the deleted command, and say so in the finishing report.

## Verify a CLI Surface Against the Source, Never Against `--help`

The `loom` on `PATH` during this plan predated the merge and still advertised all nine
deleted `loom knowledge` verbs, so `loom knowledge --help` "proved" nothing had been
deleted. Check `loom/src/cli/types_memory.rs` instead.

This has teeth beyond confusion: `KnowledgeDir::refresh_index_if_hierarchical`
(`fs/knowledge/dir.rs:264-271`) rewrites `INDEX.md` after EVERY `loom knowledge update`,
and a pre-collapse binary writes the pre-collapse generated-by marker — which names a
deleted verb, silently re-injecting into `doc/loom/knowledge/` the exact string this
plan's acceptance forbids. Run any deleted-verb sweep AFTER the last knowledge write, and
include `INDEX.md` in it.
