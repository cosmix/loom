# Knowledge Bootstrap

> Deterministic phase, cluster digest, receipt semantics, session contract

## Deterministic Phase

`loom knowledge bootstrap` restores the command deleted on 2026-08-18 by `36268adc`, for repos not
managed by loom plans (`context/retrieve.rs:84,151` needs `doc/loom/knowledge/` to exist before
`sync`/`context` work at all). `execute` (`loom/src/commands/knowledge/bootstrap/mod.rs:67`,
`pub(crate)` — see Visibility below) runs three phases:

1. **Host, no model:** resolve the git root (`resolve_repo_root`, mod.rs:142), scaffold the
   knowledge tree (`scaffold`, mod.rs:214), rebuild the catalog and source graph
   (`refresh_derived`, mod.rs:229 — the same refresh `loom knowledge sync` runs), partition the
   repo into directory clusters with digests and fan-in hot spots (`clusters.rs`), and print
   coverage plus the work plan (`work_set`, mod.rs:254).
2. **Semantic, model:** write a work brief under `.loom/work/bootstrap/` (`BOOTSTRAP_DIR`,
   mod.rs:45) and launch an interactive foreground Claude session (`launch`, mod.rs:277) sharing
   the driver `pressure` uses. The session explores with `Explore` subagents and `loom map`, and
   writes ONLY through `loom knowledge` — `--disallowedTools Edit,Write,NotebookEdit` as one
   comma-joined value (the flag is variadic; a trailing positional prompt would otherwise be
   swallowed by it).
3. **Host finalization:** `finalize`/`conclude` (mod.rs:346,360) refresh the index and catalog,
   report check issues, and write the receipt (see below).

`--structural-only` stops after phase 1. `--dry-run` prints the plan and the exact `claude` argv
(`shell_quote_argv`, mod.rs:309) without spawning. The command refuses inside a stage session
(`guard_not_in_stage`, mod.rs:133): every stage exports `LOOM_STAGE_ID`
(`orchestrator/terminal/native/wrapper.rs:340`) and child processes inherit it; stages write
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
excluded from the `*.md`-only catalog walk (`catalog.rs:254`) so it never enters the index.
Committing it is what makes `--refresh` report current across clones, not just locally.

**Post-session refresh and catalog count are best-effort warnings, not hard failures**
(memory decision, implement-bootstrap): the session already ran by the time `finalize` runs a
cache rebuild, so a failed rebuild must not cost the receipt of an otherwise-completed session.
Pre-session refresh (phase 1) still fails hard — `require_snapshot` bails with
`source graph unavailable: <reason>; nothing was spawned and no receipt was written` before any
brief, spawn, or receipt, deliberately not degrading the way `commands/map.rs` does.

## `--refresh` Behaviour

`--refresh` narrows the work set to clusters whose digest changed since the receipt, any removed
cluster, and any tier-1 file still at its template content (`tier1_gaps`, mod.rs:241, compares
trimmed content against `templates::default_content`). With nothing changed, it prints "knowledge
is current" and spawns nothing — true both before and after the knowledge/receipt are committed,
and in a fresh clone.

## Session Contract and a Repeated Visibility Trap

`execute` is `pub(crate)`, not `pub`: `BootstrapArgs` is only `pub(crate)`-reachable (re-exported
from the private `cli/types_memory` module), so a `pub fn` taking it trips `private_interfaces`
under `clippy -D warnings` (precedent: `knowledge::annotate::annotate` is also `pub(crate)`).

`guard_not_in_stage` inlines its own `LOOM_STAGE_ID` check rather than calling
`commands::hook::target::non_empty_env`, because `commands/hook/mod.rs:14` declares `mod target;`
private — the same `pub(crate)`-item-in-a-private-module trap as
[`mistakes/visibility-and-reachability.md`](../mistakes/visibility-and-reachability.md). Widening
that module (`pub(crate) mod target` or a re-export) to de-duplicate the check is an open,
low-priority follow-up (`concerns.md`).

## Related

- `mistakes/visibility-and-reachability.md` — pub(crate) visibility is capped by path.
- `concerns.md` — the dry-run argv quoting gap and the guard-reuse follow-up.
