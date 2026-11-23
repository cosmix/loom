---
sources:
- loom/src/fs/knowledge/catalog.rs
- loom/src/commands/knowledge/check.rs
- loom/src/fs/knowledge/catalog/issue.rs
- loom/src/fs/knowledge/catalog/evidence.rs
verified: fffbcda55b1c83af0e06059063cab7bf93c6de67
---

# Knowledge Hierarchy

> KB fs

## Module Layout (`fs/knowledge/`)

| File                                     | Owns                                                                                                                                            |
| ---------------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------- |
| `types.rs`                               | `KnowledgeFile`, `KnowledgeTarget`, `KnowledgeLayout`, `INDEX_FILENAME`, the tier-1 alias table                                                 |
| `dir.rs`                                 | `KnowledgeDir` — `initialize`, `append_target`, `replace_section_target`, `layout` detection, index read/write, `refresh_index_if_hierarchical` |
| `index.rs`                               | `scan_topics`, `generate_index`, `write_index`, `MAX_BLURB_CHARS`                                                                               |
| `catalog.rs`                             | `catalog::build` — deterministic chunk list plus `CatalogIssue` diagnostics over the curated tree (see _Audit Rules_ below)                     |
| `catalog/issue.rs`                       | the `CatalogIssue` enum and `is_review_only`                                                                                                    |
| `catalog/baseline.rs`                    | `CheckBaseline` — reads and compares the `loom knowledge check --baseline` file                                                                 |
| `catalog/headings.rs`                    | `HeadingTally` — `DuplicateHeading` and `DuplicateHeadingAcrossFiles`                                                                           |
| `catalog/size.rs`                        | the three size limits and their checks                                                                                                          |
| `fs/knowledge/catalog/evidence.rs`       | `changed_since_verified` — the `EvidenceChanged` check against frontmatter `sources` and `verified`                                             |
| `catalog/source_roots.rs`                | resolving backticked source paths: project root, cargo package source roots, unique path suffix, or basename                                    |
| `catalog/order.rs`                       | deterministic issue ordering                                                                                                                    |
| `catalog/prose.rs`                       | indexing the configured prose roots into the same catalog, with lifecycle derived from the path                                                 |
| `chunker.rs` (+ `chunker/references.rs`) | `chunk_file` / `chunk_sections` — heading-anchored `KnowledgeChunk`s, links, and typed backticked source references                             |
| `frontmatter.rs`                         | leading YAML frontmatter (`id`, `aliases`, `state`, `sources`, `verified`), read by the chunker and written by `loom knowledge annotate`        |
| `splice.rs`                              | `splice_section` — in-place `#{2,6}` heading section replace/append, backing `replace_section_target`                                           |
| `scaffold.rs`                            | tier-2 stub-header detection/healing helpers used when a new topic file is created                                                              |
| `templates.rs`                           | tier-1 and tier-2 file scaffolds, and `scaffold_blurb` (what `GenericBlurb` compares against)                                                   |

The alias table lives in `types.rs`, not the CLI layer:
`commands/knowledge/mod.rs::update`/`replace_section` resolve their `file` argument
through `KnowledgeTarget::parse`, which matches the no-slash case against
`KnowledgeFile::parse`, so the data layer and the CLI cannot drift.

## Layout Predicate

A knowledge directory is **`Hierarchical` iff `INDEX.md` exists**, otherwise `Legacy`
(`dir.rs::layout`). Nothing else is consulted — not topic directories, not content, not links.
Creating `INDEX.md` flips the layout; deleting it downgrades instantly.

## Targets: Tier-1 vs Tier-2

`KnowledgeTarget::parse` splits on the first `/`. No slash → a tier-1 file (resolved through the
alias table: `arch`/`map`/`overview` → `architecture.md`, `lessons` → `mistakes.md`, and so on).
One slash → a tier-2 topic at `<category-dir>/<slug>.md`. A second slash is rejected: topics are
exactly one level deep.

Slugs go through `validation::validate_id` — ASCII alphanumeric plus `-` and `_` only (no dots),
128 chars max, no reserved device names, and **no leading two-digit `NN-` prefix**. A trailing
`.md` is stripped, so `architecture/foo` and `architecture/foo.md` are the same target. The
category directory is created automatically on first write.

## INDEX.md Generation

There is no dedicated index-generation verb. `INDEX.md` regenerates automatically —
`KnowledgeDir::refresh_index_if_hierarchical` calls `index::write_index` (which
renders through `generate_index`) after every `loom knowledge update`,
`replace-section` and `annotate`, and `loom knowledge sync` regenerates it on every
run and is the one command that creates it for a flat directory. `write_index`
skips the write when the rendered bytes already match the file on disk.

The generated file opens with `GENERATED_MARKER` (an HTML comment saying not to edit
by hand), a `# Knowledge Index` title, and a header blockquote telling the reader to
read the index first, then only what it points to. Next comes a **Tier 1** table
(`| File | Description | Lines |`) with one row per tier-1 file that exists, then a
**Tier 2** table grouped by `### <category>` with header `| Topic | Blurb | Lines |`
and one row per topic: the slug linked to its topic file, the blurb, and the line
count. There is no Title column — the slug link plus the blurb already carry what it
repeated. The Tier 2 section is omitted entirely when no topics exist.

Blurbs are capped at `MAX_BLURB_CHARS = 80` characters (`index.rs`): a longer blurb
is truncated on a word boundary, trailing punctuation trimmed and an ellipsis
appended, and `loom knowledge annotate --blurb` refuses anything over 80 outright.
Table cells escape `|`.

`scan_topics` is **non-recursive** — it reads each category directory's direct
`*.md` children only, skipping dotfiles and non-`.md` entries. Nested
subdirectories under a category are ignored completely. Title is the first `#`
line, blurb the first `>` line (`extract_title_and_blurb`), falling back to the
slug and an empty string.

Regeneration is idempotent and does a full atomic overwrite, so hand edits to
`INDEX.md` are silently destroyed. Every knowledge write refreshes the index — but
**only once the directory is already hierarchical**. The generated index is capped
at `MAX_INDEX_BYTES = 16_384` bytes (`catalog/size.rs`, roughly 4k tokens for the
first read of every session); exceeding it surfaces as an `OversizedIndex` issue
from `loom knowledge check` rather than being repaired automatically.

## Thresholds

The five size limits ARE enforced — as catalog issues, reported and never
repaired (`catalog/size.rs`, the mechanical form of CLAUDE.md Rule 12):

| Constant                     | Value  | Issue                                                                                                                                                 |
| ---------------------------- | ------ | ----------------------------------------------------------------------------------------------------------------------------------------------------- |
| `MAX_TIER_ONE_SECTION_LINES` | 40     | `OversizedSection` — a tier-1 `##` section over 40 lines, heading line included and trailing blank lines excluded; the headingless preamble is exempt |
| `MAX_TIER_ONE_FILE_LINES`    | 250    | `OversizedFile` — a tier-1 file over 250 lines                                                                                                        |
| `MAX_TIER_TWO_SECTION_LINES` | 80     | `OversizedSection` — a tier-2 `##` section over 80 lines: a section past this is several topics                                                       |
| `MAX_TIER_TWO_FILE_LINES`    | 400    | `OversizedFile` — a tier-2 topic file over 400 lines                                                                                                  |
| `MAX_INDEX_BYTES`            | 16 384 | `OversizedIndex` — a generated `INDEX.md` over 16 384 bytes                                                                                           |

Tier-1 is decided by path depth alone (`is_tier_one`: one path component under the
knowledge root); every file in a category directory is tier-2 and takes the tier-2
pair (`catalog/size.rs`). A tier-2 file that grows past it is split into narrower
topics, not baselined. All five
count toward `loom knowledge check --strict`; a tree that cannot clear them yet records
them with `--write-baseline` (see the baseline section below). The blurb cap (`MAX_BLURB_CHARS = 80`,
`index.rs`) is separate: it truncates in the index and makes `annotate --blurb`
refuse, but raises no catalog issue.

## The `loom knowledge` CLI Surface

`loom knowledge` has ten subcommands, all dispatched from
`cli/dispatch.rs::dispatch_knowledge`: `update`, `replace-section`, `delete-section`, `annotate`
(frontmatter lifecycle state, `--source` evidence paths, the `--verified` revision,
aliases, and the topic blurb), `context`, `eval` (scores retrieval against a
checked-in case file), `telemetry` (summarizes delivery and retrieval events),
`sync`, `check` (`commands/knowledge/check.rs`; see _Audit Rules_ below), and
`bootstrap` (see [knowledge-hierarchy.md](knowledge-hierarchy.md)).

No knowledge-coverage metric exists. `CoverageReport` (`context/coverage/mod.rs`)
measures source-graph parse coverage per file, and there is no `--min-coverage` gate.

## Migration Is Opt-In (a Deliberate Backwards-Compatibility Exception)

This project otherwise forbids compatibility shims and migration routines. The
knowledge layout is the documented exception: `KnowledgeDir::initialize()` captures
`let fresh = !root.exists()` and writes `INDEX.md` **only for a directory it just
created**. An existing flat knowledge base is never migrated as a side effect, and
`KnowledgeLayout` keeps its `Legacy` arm indefinitely.

The reason is that a knowledge base is **user-curated prose, not code**. Silently
restructuring thousands of lines of someone's writing as a side effect of an
unrelated command is destructive and unreviewable; a wrong migration cannot be
recovered by re-running a build. Upgrading is therefore explicit:
`loom knowledge sync` is the one command that migrates. On a `Legacy` directory it
writes the first `INDEX.md` (`upgrade_flat_layout`, a hard failure if that write
fails); on an already-hierarchical one it regenerates the index best-effort before
rebuilding the derived catalog. `update`, `replace-section`, `annotate` and every
retrieval path leave a flat directory flat. Writing an `INDEX.md` by hand also opts
a directory in, since the layout predicate checks only that the file exists.

## Locking

`fs/locking.rs` locks a file's **parent directory**, not the file. `INDEX.md` and all tier-1
files share the knowledge root, so an index refresh must run **after** `locked_read_modify_write`
returns — calling it inside the closure self-deadlocks on the same thread, because `flock` is
per open file description. Tier-2 writes lock `<root>/<category>/` and therefore never collide
with an index write. Refresh failures warn to stderr and return `Ok`, so that a successful
content write is never retried into a double append.

## Baselines, `delete-section`, `annotate --section` and `memory pending --group`

- **`loom knowledge check --baseline <file>`** makes `--strict` fail only on structural issues the file does not
  record (a missing file is an empty baseline), so a tree can adopt the size limits without a flag day.
  `--write-baseline <file>` records every current structural issue and exits 0; it refuses `--strict`,
  `--strict-evidence`, `--json` and `--baseline` in the same call (`commands/knowledge/check.rs`). One line per
  issue, `<kind> <file> [<detail>]` (`fs/knowledge/catalog/baseline.rs`, key in `catalog/order.rs::baseline_key`);
  `#` comments and blank lines are ignored and a repeated entry is rejected. The key DROPS the measured counts
  (lines, occurrences, bytes) so a recorded issue stays recorded while its size moves, and the detail is the
  heading for `DuplicateHeading` and `OversizedSection`, empty for `OversizedFile` and `OversizedIndex`, the
  payload otherwise. Drift is one-directional: a recorded issue that no longer occurs only prints "baseline can be
  tightened" and never fails.
- **`delete-section <file> <heading>`** removes a `#{2,6}` section with its nested subsections and errors when no
  heading matches. It closes the gap that no command could remove or rename a heading: a rename is
  `delete-section` then `update`.
- **`annotate --section <heading> --state <state>`** writes a `<!-- state: ... -->` marker under the heading; only
  `##` headings are accepted (`fs/knowledge/splice.rs`).
- **`loom memory pending --group`** prints five groups (corrections, mistakes, decisions, suggestions, other); corrections are
  sorted by target file then heading with the target in its own column, so a distiller works file by file, and
  `--json --group` carries the same structure. `--strict` keeps its meaning. `loom memory note` rejects a
  `mistake:` note without `Prevention:` and a `stale-knowledge:` note that does not parse as
  `<file>#<heading> ... Correction: ...`, printing the expected shape and writing nothing
  (`commands/memory/handlers/prefix.rs`).
- **A backticked span that looks like a source path is checked** (`fs/knowledge/chunker/references.rs::classify_reference`,
  `classify_reference`); a sentence quoting a path that does not exist reports `MissingSourceRef`, and
  `annotate --clear-sources` cannot clear it because it comes from the body, not the frontmatter. Reword the
  sentence so it carries an example marker (`<`, "example") or says "does not exist".

## Audit Rules — the Catalog Issue Kinds

`fs::knowledge::catalog::build` walks every curated markdown file under the
knowledge root (recursing into category directories, skipping `INDEX.md` and
dotfiles) and reports eleven `CatalogIssue` kinds (`catalog/issue.rs`), sorted
deterministically by `catalog/order.rs`:

- **`DuplicateHeading`** — the same normalized H2+ anchor occurs more than once in
  one file.
- **`DuplicateHeadingAcrossFiles`** — the same heading appears in several files
  (`note: heading "X" appears in 2 files`); review-only.
- **`GenericBlurb`** — a topic's first `>` line still equals
  `templates::scaffold_blurb` for its category.
- **`BrokenLink`** — a markdown link target does not resolve to a real file, by
  lexical path resolution (`.` and `..` folded relative to the linking file,
  `contained_link_target`). An absolute target, or one that folds outside the
  knowledge root, is reported as broken without being probed on disk.
- **`MissingSourceRef`** — a backticked source path classified live does not
  resolve (see _Reference classification_ below).
- **`EvidenceChanged`** — a file's frontmatter declares `sources` and a `verified`
  revision, and `git diff --name-only <verified>..HEAD -- <sources>` lists one of
  them (`fs/knowledge/catalog/evidence.rs`). `EvidenceCollector` defers the git
  work until every file is parsed, so files declaring the same `verified` and
  sources share one bounded probe (`MAX_GIT_OUTPUT_BYTES`, 1 MiB). A git failure
  reports the next kind.
- **`EvidenceUnavailable`** — declared evidence could not be assessed, with a
  reason: `missing_revision`, `invalid_revision`, `missing_repository`,
  `git_unavailable`, `command_failed` or `resource_limit` (`catalog/issue.rs`).
- **`UnverifiableReference`** — a backticked path classified example, runtime,
  external or historical that does not resolve; a note, since such a path is not
  expected to exist.
- **`OversizedSection`**, **`OversizedFile`**, **`OversizedIndex`** — the size
  limits under _Thresholds_ below (tier-1 AND tier-2).

`EvidenceChanged`, `EvidenceUnavailable`, `UnverifiableReference` and `DuplicateHeadingAcrossFiles` are review-only
(`CatalogIssue::is_review_only`): they print as `review:` / `note:` lines, land in
the JSON `review` array instead of `issues`, and never count toward `--strict`.
`--strict-evidence` (`commands/knowledge/check.rs::CheckOptions`) additionally counts
`EvidenceChanged` and `EvidenceUnavailable`, and fails on a missing knowledge root.
Stage gates use `--strict` alone: most pages were never assessed against their
sources, so a corpus-wide `--strict-evidence` gate fails on unreviewed drift, not
on a defect. Changed evidence stays a review signal until someone re-reads the
sources and runs `loom knowledge annotate <target> --verified HEAD`.

## Reference Classification and the Check Surfaces

**Reference classification** (`chunker/references.rs::classify_reference`, applied
to each backticked span ending in a source extension — rs, tsx, ts, py, go, sh, md,
toml, yaml, yml — outside fenced blocks, using the text around it): _runtime_ when
the path starts with `.loom/`, `.loom/work/`, `target/`, `node_modules/`, `~`, `/tmp` or
`$`, or contains an angle bracket; else _example_ when the path or its sentence
carries a placeholder marker (foo, bar, baz, an angle bracket, an ellipsis,
path/to, slug, the words example or placeholder, and a few more); else
_historical_ when the sentence says the path does not exist, no longer, was
removed or deleted, was renamed, used to, or "there is no" directly before it;
else _external_ on markers such as upstream or another project; otherwise _live_.
A live path must exist at the project root, under a cargo package source root, as
the unique project file with that path suffix, or — bare basename — as any file
with that name (`catalog/source_roots.rs::repository_source_path_exists`). A live
path containing a slash whose first component names nothing in this project
becomes an external note instead of a `MissingSourceRef`
(`push_source_ref_issue`).

`catalog::build` never repairs anything (`context/ingest.rs` states it as a hard
constraint) and reports on the curated tree only — chunks indexed from the
configured prose roots contribute no issues.

**Surfaces.** `loom knowledge check` (`commands/knowledge/check.rs`) resolves only
the knowledge root — never the context store, so it writes nothing and is safe as a
stage acceptance criterion — and prints one line per issue. It exits 0 unless
`--strict` is set and at least one non-review issue exists, in which case it exits
1 after printing. `--json` prints `root`, `issues`, `review` and `count`, where
`count` is the strict count. `loom knowledge sync` also runs the build and prints
the issue count, but gates nothing.

There is no per-link form requirement — a human title pointing to a topic path is
simply the house style (see `patterns.md`), not something an audit enforces.

Evidence freshness probes run Git through the bounded process runner and `OwnedProcess` (`loom/src/process/owned.rs`). Its deadline-based `Child::try_wait` polling works without a SIGCHLD notification socket, including in restricted sandboxes; timeout retirement still kills and reaps the owned group.

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
