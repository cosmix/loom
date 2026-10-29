# Knowledge Bootstrap

> Digest

## Deterministic Phase

`loom knowledge bootstrap` seeds knowledge for repos not managed by loom plans
(`context/retrieve.rs::NO_KNOWLEDGE_DIR`: `sync`/`context` need `doc/loom/knowledge/` to exist at all). `execute` (`loom/src/commands/knowledge/bootstrap/mod.rs::execute`,
`pub(crate)` — see Visibility below) runs three phases:

1. **Host, no model:** resolve the git root (`resolve_repo_root`, `commands/knowledge/bootstrap/mod.rs::resolve_repo_root`), scaffold the
   knowledge tree (`scaffold`, commands/knowledge/bootstrap/mod.rs::scaffold), rebuild the catalog and source graph
   (`refresh_derived`, commands/knowledge/bootstrap/mod.rs::refresh_derived — the same refresh `loom knowledge sync` runs), partition the
   repo into directory clusters with digests and fan-in hot spots (`clusters.rs`), and print
   coverage plus the work plan (`work_set`, commands/knowledge/bootstrap/mod.rs::work_set).
2. **Semantic, model:** write a work brief under `.loom/work/bootstrap/` (`BOOTSTRAP_DIR`,
   commands/knowledge/bootstrap/mod.rs::BOOTSTRAP_DIR) and launch an interactive foreground Claude session (`launch`, commands/knowledge/bootstrap/mod.rs::launch) sharing
   the driver `pressure` uses. The session explores with `Explore` subagents and `loom map`, and
   writes ONLY through `loom knowledge` — `--disallowedTools Edit,Write,NotebookEdit` as one
   comma-joined value (the flag is variadic; a trailing positional prompt would otherwise be
   swallowed by it).
3. **Host finalization:** `finalize`/`conclude` (`commands/knowledge/bootstrap/mod.rs::finalize`, `::conclude`) refresh the index and catalog,
   report check issues, and write the receipt (see below).

`--structural-only` stops after phase 1. `--dry-run` prints the plan and the exact `claude` argv
(`shell_quote_argv`, `commands/knowledge/bootstrap/mod.rs::shell_quote_argv`) without spawning. The command refuses inside a stage session
(`guard_not_in_stage`, commands/knowledge/bootstrap/mod.rs::guard_not_in_stage): every stage exports `LOOM_STAGE_ID`
(`orchestrator/terminal/native/wrapper.rs::build_wrapper_script`) and child processes inherit it; stages write
knowledge through `loom knowledge update` instead.

## Clusters and Digest

A directory with at most 40 files is one cluster; a larger one splits into child directories plus
a residual cluster of its direct files, and child clusters under 8 files fold back into the
residual. Cluster id is the repo-relative directory (`.` for root). Paths under
`doc/loom/knowledge/` are excluded from facts and clusters — the knowledge tree and receipt are
outputs, not inputs, so `clusters::file_facts` drops every path starting with `KNOWLEDGE_PREFIX`
before digests are taken; without the filter the session's own writes would flip a cluster's
digest and `--refresh` could never report current. Digest is
`crate::context::source_graph::body_hash` (`sha256:<hex>`) over sorted
`"<path>\t<content_hash>\n"` lines from each file's persisted `content_hash`, so nothing is
recomputed. Digests cover tracked and untracked non-ignored files, so a stray untracked file flips
its cluster to changed. Fan-in is a count of cross-file edges into each file (top 3 per cluster,
fan-in descending then path ascending).

## Receipt Semantics

The committed receipt `doc/loom/knowledge/.bootstrap-receipt.json` holds one digest per cluster,
written ONLY when the session touched its completion marker, via `crate::fs::locking::locked_write`
and loaded with `crate::fs::safe_read::read_to_string_bounded` (no-follow, 1 MiB cap). It is
excluded from the `*.md`-only catalog walk (`fs/knowledge/catalog.rs::collect_markdown_files`) so it never enters the index.
Committing it is what makes `--refresh` report current across clones, not just locally.

**Post-session refresh and catalog count are best-effort warnings, not hard failures**
(memory decision, implement-bootstrap): the session already ran by the time `finalize` runs a
cache rebuild, so a failed rebuild must not cost the receipt of an otherwise-completed session.
Pre-session refresh (phase 1) still fails hard — `require_snapshot` bails with
`source graph unavailable: <reason>; nothing was spawned and no receipt was written` before any
brief, spawn, or receipt, deliberately not degrading the way `commands/map.rs` does.

## `--refresh` Behaviour

`--refresh` narrows the work set to clusters whose digest changed since the receipt, any removed
cluster, and any tier-1 file still at its template content (`tier1_gaps`, commands/knowledge/bootstrap/mod.rs::tier1_gaps, compares
trimmed content against `templates::default_content`). With nothing changed, it prints "knowledge
is current" and spawns nothing — true both before and after the knowledge/receipt are committed,
and in a fresh clone.

## Session Contract and a Repeated Visibility Trap

`execute` is `pub(crate)`, not `pub`: `BootstrapArgs` is only `pub(crate)`-reachable (re-exported
from the private `cli/types_memory` module), so a `pub fn` taking it trips `private_interfaces`
under `clippy -D warnings` (precedent: `knowledge::annotate::annotate` is also `pub(crate)`).

`guard_not_in_stage` reuses `commands::hook::target::non_empty_env` for its `LOOM_STAGE_ID` check;
`commands/hook/mod.rs` declares `pub(crate) mod target`, so the helper is reachable from `commands/knowledge`
(the private-module trap is recorded in
[`mistakes/visibility-and-reachability.md`](../mistakes/visibility-and-reachability.md)).

## Related

- `mistakes/visibility-and-reachability.md` — pub(crate) visibility is capped by path.
- `concerns.md` — pressure and bootstrap follow-ups.
