# Knowledge Cli Invariants

> Invariants belong in the fs constructor, not the CLI handler; lock ordering for sibling refreshes; update appends.

## A CLI Handler Is Rarely the Only Caller of the Constructor It Guards (2026-07-28)

**What happened:** "new projects start hierarchical" was implemented only in
`commands::knowledge::init()`. But `loom init`, `loom map`, the (now-deleted) `knowledge
bootstrap`, and the implicit init inside `knowledge update` all called `KnowledgeDir::initialize()`
**directly** and bypassed that handler. Every new project was therefore born flat and, at the
time, nagged to run the also-deleted `loom knowledge gc`. The `cli-collapse` later removed both
verbs — the CLI now has only `update`, `context`, `sync` — but the underlying invariant fix below
is unaffected by that collapse.

**Prevention:** when a CLI handler establishes an invariant about on-disk layout, grep for the
underlying filesystem constructor (`rg 'initialize\(\)'`) — the handler is rarely the only
caller. Put the invariant in the constructor.

**Fix:** capture `let fresh = !self.root.exists()` at the top of `initialize()` and write the
index only when fresh — which also preserves the deliberate rule that existing flat directories
are never auto-migrated.

## Writes That Must Refresh a Sibling File Deadlock If Done Inside the Lock (2026-07-28)

**What happened:** `fs/locking.rs` locks a file's **parent directory**, not the file. `INDEX.md`
and every tier-1 file share `doc/loom/knowledge/` as their parent, so regenerating the index
from inside `locked_read_modify_write` requests a second exclusive lock on a directory the same
thread already holds. `flock` is per open file description — it blocks forever.

**Prevention:** any write that must also refresh a sibling file in the same directory has to do
it **after** the locked call returns, never inside the closure. Documented at
`fs/knowledge/dir.rs::refresh_index_if_hierarchical`. (Tier-2 writes lock
`<root>/<category>/`, a different directory, so they do not collide.)

## `loom knowledge update` Appends — a Retry Duplicates the Block (2026-07-28)

**What happened / why:** `update` is append-only by design. The `INDEX.md` refresh that follows a
successful write is deliberately **non-fatal** (it warns to stderr and returns `Ok`) precisely so
that a refresh failure does not make a successful write exit non-zero — an agent's natural retry
would then append the same block twice.

**Prevention:** `update` ALWAYS appends, so never use it to fix an existing section — that is what
`loom knowledge replace-section <file> "<heading>" "<body>"` is for (restored 2026-08-19 after the
CLI collapse had removed it; `cli/types_memory.rs`, `commands/knowledge/mod.rs::replace_section`).
It overwrites the body under the first matching `## <heading>` and appends — announcing that it did
— when no heading matches, so read its output: an appended "correction" means the heading did not
match and the stale text is still in the file. A zero exit code does not prove `INDEX.md` is
current — watch stderr, and finish any batch of knowledge writes with `loom knowledge sync`, which
regenerates the index unconditionally.

## Verify a Prefix-Matching Claim Before "Fixing" It (2026-07-28)

**What happened:** a reviewer flagged `find_oversized_sections` for conflating H3 with H2.
It does not: `"### Foo".starts_with("## ")` is **false** — the third character is `#`, not a
space. Matching on `"## "` is correct for H2-only detection and lets an H2 section span its H3
subsections.

**Prevention:** verify a prefix-matching claim with an actual assertion before changing code to
satisfy it. Pinned by a regression test in `fs/knowledge/tests_gc.rs`.

## `str::lines()` on a Trailing-Newline String Adds No Empty Element (2026-07-28)

`"# T\n\n> B\n\nbody\n".lines().count()` is **5**, not 4. Watch for this off-by-one when
asserting a `TopicEntry.line_count` or any GC line-count metric against a hand-written literal.

## Deleting Public Items in a Foundation Step Breaks the Crate for the Whole Fan-Out (2026-07-28)

**What happened:** a stage's foundation step removed public constants that parallel subagents'
files still referenced, so the crate stopped compiling for the duration of the fan-out. It looks
like a broken worktree; it is not.

**Prevention:** pin the exact new API signatures in **every** subagent prompt and tell each
worker to ignore compiler errors outside its owned files — otherwise a worker "fixes" another
worker's file and work is lost. The main agent is the only one that verifies a green build.
