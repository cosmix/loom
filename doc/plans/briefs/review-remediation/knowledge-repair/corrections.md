# Knowledge repair: corrections work list

This file is the complete work list of the `knowledge-repair` stage (`doc/plans/PLAN-review-remediation.md`). The stage corrects the curated knowledge under `doc/loom/knowledge/` so it states how the tree works at commit `62be2a3d2`, and it brings `loom knowledge check --strict` to green apart from the `INDEX.md` byte budget, which a code change elsewhere in the plan handles. Every correction goes through the `loom knowledge` CLI (`replace-section`, `delete-section`, `update`, `annotate`, `sync`, `check`); never edit a file under `doc/loom/knowledge/` with a file tool. Body files live outside that directory (`$TMPDIR/.distill-body-<n>.md`, written with the Write tool) and are fed to the CLI on stdin.

Order: items 1 to 27 (stale corrections, all independent), then items 28 to 34 (content moves and link repoints; do 31 to 33 after 30), then item 35 (history note), then the closing steps at the end of this file: `loom knowledge sync`, `loom knowledge check --strict`, and a regenerated `doc/loom/knowledge/check-baseline.txt`. Run every command from the repository root.

Precondition: `git status --short doc/loom/knowledge` shows no deleted file. If it shows deleted files, run `git checkout -- doc/loom/knowledge` before item 1.

Rules for each item:

- The heading in the item title is the exact text from the file. Pass it unchanged; when it does not match, `replace-section` appends a duplicate section instead of replacing (the CLI says so in its output line: read it, and expect `Replaced`, `Deleted` or `Appended` as named in the item).
- `replace-section` on a `##` heading replaces its nested `###` subsections too, and on a `###` heading it replaces only that subsection.
- Each item lists the line count of the file after the item, measured by running this list in order on a clean clone of `62be2a3d2`. Confirm with `wc -l`. A tier-1 file stays at or under 250 lines and a tier-2 file at or under 400.
- A fact that could not be verified is marked in the item; leave that text as it stands and report it with `loom memory note`.
- Evidence line numbers are as of `62be2a3d2`. When a cited line has drifted, re-find it with `rg` before correcting; do not copy a stale line number into the knowledge file.

## Part A: stale corrections (items 1 to 27)

### 1. stack.md#Core Stack

Stale claim: The Language bullet says Rust is "~380K lines under `loom/src`".
Tree today: `git ls-files "loom/src/*.rs"` lists 3,752 files and 721,598 lines at `62be2a3d2`.
Action:

```sh
loom knowledge replace-section stack 'Core Stack' - < "$TMPDIR/.distill-body-1.md"
```
Lines: `stack.md` 116 -> 116 after this item.

New body (write to `$TMPDIR/.distill-body-1.md`, without the fence lines):

````markdown
- **Language:** Rust (~720K lines under `loom/src`, about 3,750 `.rs` files)
- **Concurrency:** std threads only. The daemon is thread-based: a Unix socket accept loop, a client worker pool, and the orchestrator, log-tailer, status-broadcaster, and quota-poller threads, each joined with a 5 s timeout on shutdown (`loom/src/daemon/server/lifecycle.rs`). `loom/Cargo.toml` has no tokio and no async runtime.
- **CLI Framework:** clap with `#[derive(Parser)]`
- **Serialization:** serde; YAML through `loom::yaml` (serde-saphyr), JSON through serde_json, toml
- **Error Handling:** `anyhow` at application/orchestration boundaries with context chaining; typed
  domain errors where callers branch on outcomes
````

### 2. stack.md#Tree-sitter Source Extraction

Stale claim: The core pack row pins `tree-sitter =0.27.0` and the Wave B row pins `tree-sitter-php =0.24.2`; the closing paragraph narrates a 0.26 to 0.27 upgrade ("was" wording outside the mistakes files).
Tree today: `loom/Cargo.toml:38` pins `tree-sitter = "=0.27.2"` and `:50` pins `tree-sitter-php = "=0.25.1"`; the other eleven pins in the section match `Cargo.toml:39-52`. `context/extract/treesitter/collect.rs:138` calls `matched.captures()`.
Action:

```sh
loom knowledge replace-section stack 'Tree-sitter Source Extraction' - < "$TMPDIR/.distill-body-2.md"
```
Lines: `stack.md` 116 -> 116 after this item.

New body (write to `$TMPDIR/.distill-body-2.md`, without the fence lines):

````markdown
Thirteen optional dependencies behind three default-on cargo features (`loom/Cargo.toml`), all exact-pinned
with `=`:

| Pack (feature)                 | Crates                                                                                                                                                                                                                          |
| ------------------------------ | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Core (`source-graph`)          | `tree-sitter =0.27.2`, `tree-sitter-rust =0.24.2`, `tree-sitter-typescript =0.23.2` (TypeScript and TSX), `tree-sitter-python =0.25.0`, `tree-sitter-go =0.25.0`, `tree-sitter-javascript =0.25.0`, `streaming-iterator =0.1.9` |
| Wave B (`source-graph-wave-b`) | `tree-sitter-java =0.23.5`, `tree-sitter-c-sharp =0.23.5`, `tree-sitter-ruby =0.23.1`, `tree-sitter-php =0.25.1`                                                                                                                |
| Wave C (`source-graph-wave-c`) | `tree-sitter-c =0.24.2`, `tree-sitter-cpp =0.23.4`                                                                                                                                                                              |

`default = ["source-graph", "source-graph-wave-b", "source-graph-wave-c"]`; each wave feature implies
`source-graph`. `streaming-iterator` is required, not incidental: `QueryCursor::matches` returns a
`StreamingIterator`. Every grammar comes from `github.com/tree-sitter`; Kotlin and Swift grammars are not
maintained there, so those dialects are not supported, and shell and configuration languages (proposal
wave D) are out of scope.

**The per-pack capability-gap rule.** The pack, not the crate, is the unit of feature gating. A pack that is not
compiled does not fail the build or hide its dialects: `extractor_for` returns `Lookup::Gap` and the files get
file-level `LexicalOnly` nodes with the detail `grammar pack {feature} not compiled ({dialect})`, which the coverage
report and census list as `gaps`. Collapsing every grammar into one feature would let a host drop half the grammars
and leave the registry inconsistent, and a feature per crate (what `cargo add --optional` generates) would expose
that same inconsistency; packs group grammars that ship together. `GrammarPack::feature()` names the feature and
`compiled()` tests it. `--no-default-features` remains the degraded mode that builds with no C toolchain and yields
lexical nodes only. The `[features]` table is the one hand-edited block of `Cargo.toml`: it has no `cargo` command;
dependencies themselves are added with `cargo add <crate>@=<version> --optional`.

The three packs cost binary size and build time: the release binary grew from 37.7 MiB (core grammars only) to 52.0 MiB
(+37.9%) with both waves, and the grammar build scripts that compile `parser.c` take 0.5 to 1.7 s each (cpp and c-sharp
1.7 s, ruby and php 1.4 s, javascript 0.8 s, java 0.6 s, c 0.5 s).

Exact pins matter because `ExtractorIdentity.grammar_version` carries the _grammar_ crate version into the cache
identity: a floating pin would silently invalidate or, worse, silently reuse cached extractions. A core-only
`tree-sitter` bump does not change it. Upgrading the core crate is not free: the harness reads each match through the
`QueryMatch::captures` method (`context/extract/treesitter/collect.rs:138`). See
[Source Graph](architecture/source-graph.md#the-extractor-trait).
````

### 3. architecture/source-graph.md#The Extractor Trait

Stale claim: The dialect table lists `tree-sitter-php 0.24.2`.
Tree today: `loom/Cargo.toml:50` pins `tree-sitter-php = "=0.25.1"`. The other table rows (`rust 0.24.2`, `c 0.24.2`, and the rest) match `Cargo.toml:39-52`. Only the one cell changes; the section has no other version string.
Action:

```sh
loom knowledge replace-section architecture/source-graph 'The Extractor Trait' - < "$TMPDIR/.distill-body-3.md"
```
Lines: `architecture/source-graph.md` 372 -> 372 after this item.

New body (write to `$TMPDIR/.distill-body-3.md`, without the fence lines):

````markdown
`context/extract/mod.rs` — bytes in, `FileExtraction` out. Each dialect has one extractor
over a pinned grammar and a tree-sitter query embedded in its module.

```rust
pub trait SourceGraphExtractor {
    fn dialect(&self) -> &'static DialectSpec;
    fn capabilities(&self) -> Capabilities;
    fn cache_identity(&self) -> ExtractorIdentity;
    fn extract(&self, path: &Path, bytes: &[u8]) -> Result<FileExtraction>;
}
```

**Dialect lookup, not `supports()`.** `context/extract/dialect.rs` (always compiled, no `cfg`)
holds `DIALECTS: &[DialectSpec]`, `dialect_for_path` (lowercase extension) and `dialect_by_id`.
A `DialectSpec` carries `id` (equals `NodeLanguage::as_str()`), `language`, `family`,
`extensions`, `grammar` (crate and version), `pack: GrammarPack`, `self_receivers` and
`bare_calls_reach_members`. Every extension appears in exactly one row, pinned by a unit test.
`extractor_for(extractors, path)` is the one path to an extractor and returns
`Lookup::{Extractor, Gap, Unknown}`; `extract_file`, `parser_version_matches`,
`layer_is_current`, the `worktree_graph` scan and `verify/goal_backward` all call it.

| id                   | family     | extensions                | grammar                         | pack  |
| -------------------- | ---------- | ------------------------- | ------------------------------- | ----- |
| `rust`               | rust       | `rs`                      | `tree-sitter-rust 0.24.2`       | Core  |
| `typescript` / `tsx` | ecmascript | `ts,mts,cts` / `tsx`      | `tree-sitter-typescript 0.23.2` | Core  |
| `javascript`         | ecmascript | `js,mjs,cjs,jsx`          | `tree-sitter-javascript 0.25.0` | Core  |
| `python`             | python     | `py,pyi`                  | `tree-sitter-python 0.25.0`     | Core  |
| `go`                 | go         | `go`                      | `tree-sitter-go 0.25.0`         | Core  |
| `java`               | java       | `java`                    | `tree-sitter-java 0.23.5`       | WaveB |
| `csharp`             | csharp     | `cs`                      | `tree-sitter-c-sharp 0.23.5`    | WaveB |
| `ruby`               | ruby       | `rb,rake,gemspec`         | `tree-sitter-ruby 0.23.1`       | WaveB |
| `php`                | php        | `php`                     | `tree-sitter-php 0.25.1`        | WaveB |
| `c`                  | c          | `c`                       | `tree-sitter-c 0.24.2`          | WaveC |
| `cpp`                | c          | `cc,cpp,cxx,hh,hpp,hxx,h` | `tree-sitter-cpp 0.23.4`        | WaveC |

Resolution never binds across **families**: `ecmascript` joins TypeScript, TSX and JavaScript,
`c` joins C and C++, every other dialect is its own family. `.h` belongs to `cpp` because the C++
grammar accepts nearly every C header while the C grammar rejects C++ headers outright; the
standard `#ifdef __cplusplus / extern "C" {` guard still makes a header a whole-file `ParseError`
([Source Graph Known Gaps](../concerns/source-graph-known-gaps.md)). `NodeLanguage` has a unit
variant per dialect with an explicit `#[serde(rename = "<id>")]`, plus `Other(String)`.
`crate::language::DetectedLanguage` (the stage and skill language, 11 variants) takes its extension mapping
from `dialect_for_path`: TSX folds into TypeScript and `Other(_)` maps to no language.

**Gap vs Unknown.** `Lookup::Gap` is a dialect loom knows whose extractor is absent: the file
still gets a file-level node with `FileCoverage::LexicalOnly` and a named detail
(`grammar pack {feature} not compiled ({dialect})` or `no extractor registered for dialect
{dialect}`). `Unknown` (no dialect for the extension) keeps the lexical fallback. Both stamp
`LEXICAL_PARSER_VERSION`, and every currency check treats them alike, so a base holding such a
file is `Reused` on the next `ensure_snapshot`. `Capabilities { declarations, imports,
import_bindings, calls, receivers, references }` feeds the coverage report; the flags are
informational and never gate behaviour.

What an extractor promises: every node it emits is a real declaration in the bytes it was
handed, and every edge carries honest provenance. It does **not** promise an exhaustive call
graph. Extraction is per-file, so a cross-file call is unresolved or ambiguous until
`context::resolve` ([Source Graph Resolution](source-graph-resolution.md)) binds it. A syntax
error is data (`FileCoverage::ParseError`), never an `Err`.

**One shared harness.** The tree-sitter walk lives in `context/extract/treesitter/` (`mod.rs`
`run_query`, `build.rs`, `collect.rs`, `binding.rs`, `ids.rs`), parameterized by the per-language
`QueryHarness` trait. A language module supplies a grammar, a query using the capture protocol
(`@definition.<kind>`, `@name`, `@import.path`, `@import.statement`, `@call.name`,
`@call.receiver`, `@reference.name`, `@definition.qualifier`) and `kind_for_capture`. Identity-by-
default hooks cover dialect quirks: `import_bindings`, `import_spec` (Ruby `require_relative` gives
`./x`), `definition_name` (PHP `A\B` gives `A.B`), `self_receivers` (defaults from the dialect row)
and `top_level_self` (Ruby only). A `@definition.*` match with no `@name` counts toward
`FileCoverage::Partial`. Centralizing makes the honesty constraint (provenance, the 0.5 ceiling)
structural instead of a rule twelve `extract()` implementations each have to remember. The compiled
`Query` is cached per `(NodeLanguage string, query text)` for the process.
````

### 4. concerns.md#Layering Violations

Stale claim: The list names `daemon/server/status.rs`, `daemon/protocol.rs`, `orchestrator/core/inbox_drain/merge_resolved.rs` and `orchestrator/core/merge_handler/resolver_exit.rs` as importing `commands`; it omits `orchestrator/skip.rs`.
Tree today: `daemon/server/status.rs:30` imports `crate::status::data`, `daemon/protocol.rs:260` and `orchestrator/core/inbox_drain/merge_resolved.rs:63` no longer import `commands`, and `resolver_exit.rs:174` imports `crate::verify::completion::resolved`. Still present: `daemon/server/client.rs:89`, `orchestrator/core/coherence.rs:41`, `orchestrator/skip.rs:45`, `git/cleanup/orphans.rs:10`. `models/stage/sandbox_config.rs:161` imports `crate::fs::permissions::state_root` in production code, so `fs` joins the models edges. The "Layering Violations" heading is a `###` under "Architecture Concerns"; the file heading to pass is the `###` one.
Action:

```sh
loom knowledge replace-section concerns 'Layering Violations' - < "$TMPDIR/.distill-body-4.md"
```
Lines: `concerns.md` 250 -> 250 after this item.

New body (write to `$TMPDIR/.distill-body-4.md`, without the fence lines):

````markdown
Lower layers import from higher ones. Each edge was checked against the tree; the full list, with the module cycles, is in [Core Abstractions](architecture/core-abstractions.md#layering-violations-known-issues).

- daemon calls commands: `loom/src/daemon/server/client.rs::authorize_preface` (`commands::stage::admin_proof::verify_and_consume_admin_proof`)
- orchestrator calls commands: `loom/src/orchestrator/core/coherence.rs::begin_scheduling_pass` (`commands::stage::complete::strip_privileged_env_for_runtime`) and `loom/src/orchestrator/skip.rs::skip_stage` (`commands::stage::native_gate::ensure_no_native_workers`)
- git imports orchestrator: `loom/src/git/cleanup/orphans.rs` (`orchestrator::merge_lifecycle::{CleanupOutcome, MergeLifecycle}`)
- models imports plan, verify, git, assets and fs: `models/stage/{types,checks,methods}.rs` and `models/dispute_criterion.rs` (`plan`), `models/dispute.rs` (`verify`), `models/stage/merge_block.rs` (`git`), `models/forward_receipt/transcript/argv.rs` (`assets`), `models/stage/sandbox_config.rs` (`fs::permissions`)
````

### 5. architecture/core-abstractions.md#Layering Violations (Known Issues)

Stale claim: The list names four violations ("daemon imports commands (mark_plan_done_if_all_merged)", "orchestrator imports commands (check_merge_state)", "git/worktree imports orchestrator (hook config)", "models imports plan/schema") with fixes for them, and the subsection "The newer modules are clean" says `telemetry` is a leaf and `context` imports only `context`, `fs`, `language`, `models` and `git`.
Tree today: `rg -n "mark_plan_done_if_all_merged" loom/src` finds nothing, `check_merge_state` lives in `git/merge/status.rs:62` (a downward home), and `rg "crate::orchestrator" loom/src/git/worktree` finds nothing, so three of the four named violations are gone; the fourth (models importing plan) remains and widened. The real edges are the ones in the new body; each `use` line was found with `rg -n "crate::commands" loom/src/{daemon,orchestrator,handoff,subagent_lifecycle,stage_service}` and the related searches (`validation/runtime.md` section CR-32 lists the same edges; the tree adds `orchestrator/skip.rs:45` and `models/stage/sandbox_config.rs:161`, which that section misses or calls a doc link). `telemetry/mod.rs:15-18,163` imports `fs`, `relay` and `git`. `context/worktree_graph.rs:27` imports `verify`. `replace-section` on a `##` heading replaces its nested `###` subsections too (checked on a scratch copy), so this body carries the renamed subsection. Do the rename here only; no other file links to the old subsection anchor.
Action:

```sh
loom knowledge replace-section architecture/core-abstractions 'Layering Violations (Known Issues)' - < "$TMPDIR/.distill-body-5.md"
```
Lines: `architecture/core-abstractions.md` 135 -> 129 after this item.

New body (write to `$TMPDIR/.distill-body-5.md`, without the fence lines):

````markdown
Correct dependency direction: commands/ -> orchestrator/ -> models/ (top), daemon/ / git/ / plan/ (middle), fs/ (bottom).

Upward edges in production code (test modules excluded), each checked against the tree:

- **daemon -> commands:** `daemon/server/client.rs:89` (`commands::stage::admin_proof::verify_and_consume_admin_proof`).
- **orchestrator -> commands:** `orchestrator/core/coherence.rs:41` (`commands::stage::complete::strip_privileged_env_for_runtime`) and `orchestrator/skip.rs:45` (`commands::stage::native_gate::ensure_no_native_workers`).
- **handoff -> commands:** `handoff/completion/attest.rs:14` (`commands::stage::admin_hmac::{constant_time_eq, hmac_sha256}`; the other user is `commands/stage/admin_proof.rs:6`).
- **subagent_lifecycle -> commands:** `subagent_lifecycle/claude.rs:8` imports `StartedAgentTypeIndex` through `commands::subagents::ledger`, which only re-exports `provider_data::ledger`.
- **git -> orchestrator:** `git/cleanup/orphans.rs:10` (`orchestrator::merge_lifecycle::{CleanupOutcome, MergeLifecycle}`).
- **process -> daemon:** `process/sandbox_probe/needs.rs:8` (`daemon::{socket_path_fits, SUN_PATH_MAX}`, defined in `daemon/socket.rs`). `daemon` imports `process`, so `process` is the lower layer.
- **models -> plan, verify, git, assets, fs:** `plan` at `models/dispute_criterion.rs:7` and `models/stage/{checks,types,methods}.rs`, `verify` at `models/dispute.rs:15-18`, `git` at `models/stage/merge_block.rs:8`, `assets` at `models/forward_receipt/transcript/argv.rs:24`, `fs::permissions` at `models/stage/sandbox_config.rs:161`.
- **stage_service -> daemon:** `ContractRunReport` is defined in `stage_service/mod.rs:43` and re-exported by `daemon/protocol.rs:93`; `stage_service/contracts_choices.rs:15`, `verify/contracts/*` and `host_verify/*` import it through `crate::daemon`. The edge `daemon -> stage_service` (`daemon/server/contracts.rs:18`) is a downward call.
- **handoff <-> orchestrator (cycle):** `handoff/mod.rs:28` re-exports `orchestrator::continuation` (the continuation code lives in `orchestrator` because of its spawner and signal dependencies) and `handoff/session_content.rs:21` imports `orchestrator::monitor::stage_context_tokens`; `orchestrator/continuation/context.rs:9-10`, `orchestrator/core/stage_handoff.rs:3`, `orchestrator/core/inbox_drain/complete.rs:13` and `orchestrator/monitor/completion_blockers.rs:6` import `crate::handoff`.

### Edges of `context`, `telemetry` and `process`

Checked with `rg -o 'crate::[a-z_]+' loom/src/<module>`, test files excluded:

- **`context`** imports `crate::context`, `fs`, `git`, `models`, `utils`, `yaml`, `testrun` and one `verify` edge (`context/worktree_graph.rs:27`, `verify::review::fingerprint::checkout_git`). It has no edge to `orchestrator`, `commands` or `daemon`. The `git` edges (`context/refresh/source_graph/generation.rs::working_tree`, `layer.rs`, `worktree_graph.rs`, `census/mod.rs`) list tracked files and judge tree cleanliness, and they point downward.
- **`telemetry`** imports `crate::fs`, `crate::git` and `crate::relay`, never a layer above them. The orchestrator calls into it (`orchestrator/core/stage_telemetry.rs`), never the reverse.
- **`process`** holds the env allowlist and is consumed by both `verify` and the terminal spawner without importing either. Its one upward edge is the `daemon` import listed above.

The rule to preserve: the orchestrator calls into `context`, never the other way around. A `use crate::orchestrator` appearing under `loom/src/context/` outside test files is a regression; `rg 'crate::orchestrator' loom/src/context` finds it.

(A match for `crate::external` under `context/extract/rust/tests.rs::FIXTURE` is inside a golden test fixture string, not a real import.)
````

### 6. concerns.md#Automatic-Knowledge Plan Followups

Stale claim: "Four open items" lists "an unbounded whole-file read ahead of the extraction size cap".
Tree today: `loom/src/context/source_graph/read.rs:15-40` (`SourceRead::read`) hashes and counts every byte through a 16 KiB buffer and retains at most `MAX_EXTRACTED_FILE_BYTES`. Still open and re-checked: `KnowledgeDir::append`, `read_index` and `replace_section` (`loom/src/fs/knowledge/dir.rs:127,162,138`) have no non-test caller; `context/delivery.rs:67` (`plan_key_from`) files a blank id under `default` while `orchestrator/merge_lifecycle.rs:344` (`plan_id`) returns the raw config id; `loom/src/fs/permissions/constants.rs:327` (`LOOM_PERMISSIONS_WORKTREE`) is referenced only from its re-export and `fs/permissions/tests/constants_tests.rs`. The topic file keeps the bounded-read paragraph as an open item; item 7 fixes that.
Action:

```sh
loom knowledge replace-section concerns 'Automatic-Knowledge Plan Followups' - < "$TMPDIR/.distill-body-6.md"
```
Lines: `concerns.md` 250 -> 250 after this item.

New body (write to `$TMPDIR/.distill-body-6.md`, without the fence lines):

````markdown
Three open items: three production-dead `KnowledgeDir` methods kept alive only by each other's tests, a
writer/reader plan-key normalisation mismatch, and a `LOOM_PERMISSIONS_WORKTREE` constant with no non-test consumer.
Refresh source reads are bounded (`SourceRead` hashes through a fixed 16 KiB buffer and retains at most
`MAX_EXTRACTED_FILE_BYTES`); source windows read through a separate path that needs its own bound audit. The topic also
covers natural-language queries that corpus stopwording empties.

Full detail: [automatic-knowledge-source-graph-followups.md](concerns/automatic-knowledge-source-graph-followups.md).
````

### 7. concerns/automatic-knowledge-source-graph-followups.md#Open Items From the Automatic-Knowledge Plan

Stale claim: The first paragraph, "Bounded refresh source retention", sits under "Open Items" although it describes a closed item.
Tree today: `loom/src/context/source_graph/read.rs:15-40`, as in item 6. The body drops the lead paragraph and ends with it as a plain statement; the three real open items keep their text.
Action:

```sh
loom knowledge replace-section concerns/automatic-knowledge-source-graph-followups 'Open Items From the Automatic-Knowledge Plan' - < "$TMPDIR/.distill-body-7.md"
```
Lines: `concerns/automatic-knowledge-source-graph-followups.md` 69 -> 69 after this item.

New body (write to `$TMPDIR/.distill-body-7.md`, without the fence lines):

````markdown
**Three production-dead `KnowledgeDir` methods.** `append`, `read_index` and the
`KnowledgeFile`-keyed `replace_section` (`fs/knowledge/dir.rs`) have no non-test callers.
`read` is live (`commands/knowledge/bootstrap/mod.rs`), and so are `read_target` and
`replace_section_target` (`commands/knowledge/mod.rs`). All are `pub` on a `pub` type, so clippy
cannot flag them, and `tests_dir.rs` and `tests_dir_bugfixes.rs` exercise the dead three against
each other. Settle them in one change: delete the methods and their tests together, or wire them
to a real consumer. A dead-accessor list holds for one revision only; re-check it before acting.

**Plan-key normalisation on the writer side.** `delivery::plan_key_from` files a blank or absent
`plan_id` under `"default"`. `MergeLifecycle::plan_id` (`orchestrator/merge_lifecycle.rs`) reads
the raw config `plan_id` instead: an absent id skips overlay reconcile and discard, and a blank
one is used as is. Silent by construction; see `mistakes/writer-reader-address.md`.

**`LOOM_PERMISSIONS_WORKTREE` has no consumer.** The constant (`fs/permissions/constants.rs`)
lists worktree-shaped `Read(...)` rules and `Bash(loom *)`, but only its re-export in
`fs/permissions/mod.rs` and `constants_tests.rs` reference it; no settings writer uses it. Wire it
into the worktree settings or delete it with its tests.

Refresh source reads are bounded: `context/refresh/source_graph/layer.rs::read_bytes` uses `context/source_graph/read.rs::SourceRead` through `read_source` or `run_git_stream`. The reader hashes and counts every byte through a fixed 16 KiB buffer while retaining at most `MAX_EXTRACTED_FILE_BYTES`; oversized nodes keep full-file content identity and byte spans without whole-file allocation. Source windows have a separate read path and require a separate bound audit.
````

### 8. concerns.md#Tier-1 Knowledge Housekeeping Backlog

Stale claim: "`loom knowledge check --strict` exits 0 on this tree ... `INDEX.md` ... has about 100 bytes of headroom ... none [baseline] is needed while `--strict` stays green", plus the chunker/splicer fence disagreement sentence.
Tree today: `loom knowledge check --strict` exits 1 at `62be2a3d2` with six issues (`INDEX.md` 19,536 bytes against `MAX_INDEX_BYTES` = 16,384 at `loom/src/fs/knowledge/catalog/size.rs:21`, `mistakes.md` 274 lines, `mistakes/hooks-shell-portability.md` 412, `mistakes/testing-and-lint.md` 408, and two missing source references in `mistakes/sandbox-hook-preflight.md`); `doc/loom/knowledge/check-baseline.txt` is tracked. Items 28, 29, 30 and 34 clear five of the six; the index stays over budget until the plan's code change. The chunker and splicer now share `fs/knowledge/scanner.rs` (`Scanner::fenced` and `atx_heading`: `loom/src/fs/knowledge/chunker.rs:138-143`, `splice.rs:148,244-246`), so the fence sentence goes. `concerns.md` is at the 250-line tier-1 limit, so this section must not grow; the new body is shorter.
Action:

```sh
loom knowledge replace-section concerns 'Tier-1 Knowledge Housekeeping Backlog' - < "$TMPDIR/.distill-body-8.md"
```
Lines: `concerns.md` 250 -> 245 after this item.

New body (write to `$TMPDIR/.distill-body-8.md`, without the fence lines):

````markdown
`loom knowledge check --strict` enforces the tier-1 limits (250 lines per file, 40 per section), the tier-2 limits (400 per file, 80 per section), resolvable source references and a byte budget on `INDEX.md` (`MAX_INDEX_BYTES`, `loom/src/fs/knowledge/catalog/size.rs`). `INDEX.md` is over that budget and `doc/loom/knowledge/check-baseline.txt` records it as `oversized-index`, so the gate is `loom knowledge check --strict --baseline doc/loom/knowledge/check-baseline.txt`; every new topic adds an INDEX row, so shrinking the index is the open work.
What remains of the CLI's own rough edges: `replace-section`'s CRLF/trailing-blank-line quirks, `update`'s stdin-vs-inline trim mismatch, no blurb flag on `update`, and no in-place heading rename (delete-section plus update is the workaround).

→ [Knowledge CLI Gaps](concerns/knowledge-cli-gaps.md)
````

### 9. concerns/knowledge-cli-gaps.md#Tier-1 Knowledge Housekeeping Backlog

Stale claim: "Closed by the 2026-09-19 cleanup. `loom knowledge check --strict` exits 0 on this tree", "no baseline file is needed while `--strict` stays green", "After `./dev-install.sh`, run `loom knowledge check --strict`: exit 0 means no baseline file to generate" (a dated "closed" narrative, and a claim the tree contradicts).
Tree today: As in item 8: strict exits 1 at `62be2a3d2` and the baseline is tracked. `rg -n "e\.g\." loom/src/fs/knowledge/chunker/references.rs` shows the example markers (`EXAMPLE_MARKERS`, lines 62-77) that item 28 relies on; checked on a scratch copy: with `e.g.` in front of the list both `bunfig.toml` and `rust-toolchain.toml` become `unresolved example reference` notes. A clean clone at `62be2a3d2` reports ten `missing-source-ref` issues for paths that exist only as untracked files in the working checkout (checked with `git clone` plus `loom knowledge check`). The three bullets after the first keep their text.
Action:

```sh
loom knowledge replace-section concerns/knowledge-cli-gaps 'Tier-1 Knowledge Housekeeping Backlog' - < "$TMPDIR/.distill-body-9.md"
```
Lines: `concerns/knowledge-cli-gaps.md` 108 -> 99 after this item.

New body (write to `$TMPDIR/.distill-body-9.md`, without the fence lines):

````markdown
`loom knowledge check --strict` enforces the tier-1 limits (250 lines per file, 40 per section), the tier-2 limits (400 lines per file, 80 per section, `fs/knowledge/catalog/size.rs`), resolvable source references and the `INDEX.md` byte budget (`MAX_INDEX_BYTES`). `INDEX.md` is over that budget; the committed baseline `doc/loom/knowledge/check-baseline.txt` records it as `oversized-index`, and the gate is `loom knowledge check --strict --baseline doc/loom/knowledge/check-baseline.txt`.

What remains is discipline:

- **A tier-2 file or section that grows past its limit is split, not baselined.** `--write-baseline` exists for adopting a limit on a tree that cannot clear it yet. Write the baseline from a clean clone of the repository (with the knowledge directory copied over it), never from a working checkout: references to plan briefs that are untracked in a working checkout (`doc/plans/briefs/...`) resolve there and not in a clone, so a baseline written from the checkout drops entries that a clean checkout reports again.
- **Source references accept project-relative and Cargo-module-relative paths, bare basenames, and path suffixes.** `fs/knowledge/catalog/source_roots.rs::repository_source_path_exists` accepts a bare basename when any indexed project file has that name; a slash-containing suffix must identify exactly one file. An unresolved live project reference produces `MissingSourceRef`; example and external-reference classification are handled separately (an `e.g.` before a list of file names marks them as examples).
- **A topic's blurb is set with `loom knowledge annotate <target> --blurb "<text>"`** (at most 80 characters), which
  works from inside a stage. `update` still cannot set it, so a new topic takes two calls.
- **Moving a section between topics has no verb.** The split record used `update <dst>` then `delete-section <src>`. An
  extractor that finds the section end MUST skip fenced code: a `# comment` line inside a fence is not a heading, and one
  split truncated a section at such a line and left an unclosed fence that made the checker count 152 lines.
````

### 10. concerns/knowledge-cli-gaps.md#File Tools Are Blocked on Knowledge Files, and No Command Renames a Heading

Stale claim: "`INDEX.md` sits within a few bytes of its 16 384-byte `OversizedIndex` budget".
Tree today: `INDEX.md` is 19,536 bytes at `62be2a3d2` (`loom knowledge check --strict`), so it is over the budget, not near it. The new wording carries no byte figure, so it stays true whichever way the plan resolves the index budget. Only the second paragraph changes.
Action:

```sh
loom knowledge replace-section concerns/knowledge-cli-gaps 'File Tools Are Blocked on Knowledge Files, and No Command Renames a Heading' - < "$TMPDIR/.distill-body-10.md"
```
Lines: `concerns/knowledge-cli-gaps.md` 99 -> 99 after this item.

New body (write to `$TMPDIR/.distill-body-10.md`, without the fence lines):

````markdown
The worktree file guard refuses Edit and Write under `doc/loom/knowledge/` ("knowledge files are
recorded through `loom knowledge update`, not file tools"), in a knowledge-distill stage too. The
in-place channels are `replace-section` (rewrites a section body, keeps its heading) and
`delete-section` (removes a heading and its subsections); a heading rename is delete plus `update`. A surgical
fix inside a long section means regenerating the whole body: extract it with a FENCE-AWARE scanner, apply
exact-once substitutions, and pipe the result to `replace-section` from a file. `loom-control-complete.sh` strips
inert heredoc bodies (quoted delimiter, inert reader), but a long body still belongs in a file fed on
stdin; name that file `.distill-body-*` so the main-agent edit advisory ignores it.

`INDEX.md` has a byte budget (`MAX_INDEX_BYTES`, `fs/knowledge/catalog/size.rs`) that `loom knowledge check` reports as
`OversizedIndex`; the SessionStart knowledge-orient line reports its current size. Every new tier-2 topic adds a row, so each one has to be paid for with shorter
blurbs (`loom knowledge annotate <target> --blurb`) or by merging small topics.
````

### 11. concerns/knowledge-cli-gaps.md#Two Fenced-Code-Block Models Disagree in `fs/knowledge/`

Stale claim: The splicer and the chunker use different fenced-code models, so the span `context` reads differs from the span `replace-section` overwrites.
Tree today: `loom/src/fs/knowledge/chunker.rs:138-143` (`split_sections`) and `loom/src/fs/knowledge/splice.rs:148,244-246` (`scan_fences`) both call `Scanner::fenced` and `atx_heading` from `fs/knowledge/scanner.rs`; `rg -n "fence_marker" loom/src/fs/knowledge` finds nothing. No other file links to this section.
Action:

```sh
loom knowledge delete-section concerns/knowledge-cli-gaps 'Two Fenced-Code-Block Models Disagree in `fs/knowledge/`'
```
Lines: `concerns/knowledge-cli-gaps.md` 99 -> 87 after this item.

New body: none; the section is removed with its nested subsections.

### 12. concerns/code-quality-and-hook-debt.md#Efficiency-Plan Debt: Hook File Size, Unwired False Positives, Baseline Pressure, Criterion Rendering

Stale claim: The bullet "The stage signal renders an acceptance criterion as its bare command" (and the word "Criterion Rendering" in the heading).
Tree today: `loom/src/orchestrator/signals/format/sections/acceptance.rs:5-35` (`format_criterion`) appends `(expected exit code: N)` and the stdout, stderr and timeout conditions; the merge, recovery, knowledge and stage signals all call it (`signals/merge.rs:261`, `recovery_format.rs:138`, `knowledge.rs:145`, `format/sections.rs:474`). `loom-hooks/commit-filter.sh` is still 463 lines (`wc -l`), so the first bullet stays. The heading names the deleted topic, and `replace-section` cannot rename a heading, so the action is delete-section then `update` with the new heading (the section moves to the end of the file; no file links to its anchor).
Action: two commands, because `replace-section` keeps the heading text; the section lands at the end of the file under the new heading.

```sh
loom knowledge delete-section concerns/code-quality-and-hook-debt 'Efficiency-Plan Debt: Hook File Size, Unwired False Positives, Baseline Pressure, Criterion Rendering'
loom knowledge update concerns/code-quality-and-hook-debt - < "$TMPDIR/.distill-body-12.md"
```
Lines: `concerns/code-quality-and-hook-debt.md` 330 -> 327 after this item.

New body (write to `$TMPDIR/.distill-body-12.md`, heading line included, without the fence lines):

````markdown
## Efficiency-Plan Debt: Hook File Size, Unwired False Positives, Baseline Pressure

- **`loom-hooks/commit-filter.sh` is 463 lines**.
  The repository's 400-line file cap is not enforced for `loom-hooks/*.sh`: `loom/tests/maintainability.rs`'s scanner walks
  only the loom crate's `build`, `src` and `tests` Rust files, so no gate sees hook scripts. A refactor stage should
  either split the script or extend the scanner to hooks.
- **The stage-completion unwired-file detector uses the resolved source graph.**
  `verify/wiring_detection.rs:37-115` inspects resolved edges, marks missing or incomplete graph
  evidence uncertain, and excludes conventional test paths. `#[path]` support depends on graph
  extraction and resolution; any remaining false positive needs a current fixture with its graph
  evidence. It is not a grep of importable names in `mod.rs`.
- **Every new `Stage` field costs a baseline line.** `Stage::default` (`models/stage/defaults.rs`) is the single
  exhaustive literal and its `loom/maintainability-baseline.txt` entry (`function src/models/stage/defaults.rs default 76`)
  is exact; the next field needs a field-grouping refactor (nested structs) rather than another raise.
- **Two masked-exit shapes are still misjudged** (`plan/schema/validation/criterion_hazards/masked_exit.rs`): `cargo build && (cargo test ||
true)` is flagged although a failing build still fails it, and `cmd || true 2>/dev/null` is not flagged. Both are rare;
  they are recorded in [plan-lifecycle-and-fields](../architecture/plan-lifecycle-and-fields.md).
````

### 13. concerns/code-quality-and-hook-debt.md#Tests Shell Out to Perl and Python

Stale claim: "9 files under `loom/src` and `loom/tests` run `/usr/bin/perl` ... and 72 `.rs` files mention `python`/`python3`".
Tree today: `rg -l "/usr/bin/perl" loom/src loom/tests` finds 3 files. Bare `perl` runs at `verify/criteria/confine_host_tests.rs:370-371`, `confine_seatbelt_signal_tests.rs:125` and `escapees_tests.rs:96-97`, which makes 6 files, all under `loom/src` (the validation table suggests 3, which undercounts). `rg -l "python3?" --glob "*.rs" loom/src loom/tests` finds 80 files, not 72 (the table left that count unverified).
Action:

```sh
loom knowledge replace-section concerns/code-quality-and-hook-debt 'Tests Shell Out to Perl and Python' - < "$TMPDIR/.distill-body-13.md"
```
Lines: `concerns/code-quality-and-hook-debt.md` 327 -> 327 after this item.

New body (write to `$TMPDIR/.distill-body-13.md`, without the fence lines):

````markdown
Rust tests reach other languages for helper programs: 6 files under `loom/src` run `perl`, three by absolute path (`/usr/bin/perl` in `verify/criteria/confine_seatbelt_tests.rs`, `confine_seatbelt_loopback_tests.rs` and `confine_stage_cache_tests.rs`) and three by bare name (`confine_host_tests.rs`, `confine_seatbelt_signal_tests.rs` and `escapees_tests.rs`), and 80 `.rs` files mention `python`/`python3`, beside the Python scripts under `scripts/` and the bash hooks. Each language adds an interpreter the test host and the sandbox must provide (macOS `/usr/bin/python3` is an xcrun shim). A probe that needs a real process inside a boundary can re-run the test binary itself instead (`std::env::current_exe()` with a hidden mode), as `process/host_job_lane_tests.rs` and `pressure/record/publication_tests.rs` do; `verify/criteria/confine_seatbelt_network_tests.rs` follows that pattern. Open: migrate the remaining Perl and Python helpers to it.
````

### 14. concerns/runtime-and-session-safety.md#Dispute Bookkeeping Gaps Around Escalation

Stale claim: The first bullet, "A refused filing can leave an open request in a race" (publication before the stage-lock transition).
Tree today: `stage_service/dispute_kinds.rs:197-222` (`file_dispute_inner`) and `stage_service/dispute.rs:107` both call `file_under_stage_lock` (`stage_service/dispute_store.rs:80-90`), which runs the admission and transition checks (`dispute_kinds.rs:235-275`, `file_on_locked_stage`) before `Publication::write`, and on a failed stage write calls `receipts::withdraw`. Only the crash window is left; the bullet now says so. The second bullet ("Unverified: a survivor may block the fresh spawn") depends on `CR-21` in another validation group; it is kept as written and was not verified.
Action:

```sh
loom knowledge replace-section concerns/runtime-and-session-safety 'Dispute Bookkeeping Gaps Around Escalation' - < "$TMPDIR/.distill-body-14.md"
```
Lines: `concerns/runtime-and-session-safety.md` 147 -> 147 after this item.

New body (write to `$TMPDIR/.distill-body-14.md`, without the fence lines):

````markdown
`--approve` refuses while a live worker or merge resolver may write to the stage (`commands/stage/human_review.rs::refuse_live_writer`). A filing runs under the stage lock (`stage_service/dispute_store.rs::file_under_stage_lock`) and checks admission, budget and the transition on the locked stage before it publishes the request; `loom stage reset` closes open disputes (`commands/stage/state/loop_recovery/mod.rs`).

- **A crash between a filing's two writes leaves a request the stage does not name.** `file_under_stage_lock` publishes `disputes/<stage>/<n>/request.md`, then updates the stage; a failed stage write withdraws the request (`stage_service/receipts.rs::withdraw`), but a crash between the two writes cannot. The receipt, when there is one, makes the retried filing reuse the request, and the adjudication scan settles it unjudged once a later filing has named another. `daemon/server/dispute.rs` and `dispute_kinds.rs` are transport adapters.
- **Unverified: a survivor may block the fresh spawn.** A surviving session with no PID identity can read as dead to an approval probe. If it stays in `active_sessions` and `stage.session`, `insert_active_session` (`orchestrator/core/session_lifecycle.rs`) may refuse the fresh session; verify this against the current retirement and adoption paths before changing behavior.
````

### 15. concerns/sandbox-and-confinement-gaps.md#Plan Dry Run on macOS Lacks the xcrun Bypass, So Cargo Criteria Time Out

Stale claim: A plan dry run on macOS applies the developer-tools environment only when a stage cache is passed.
Tree today: `loom/src/verify/criteria/confine.rs:325-356` (`apply_confined_environment`) calls `DeveloperTools::detect()` for every confined command; its doc comment (:327-331) says "Every confined command gets them, a stage cache or not". No file links to this section.
Action:

```sh
loom knowledge delete-section concerns/sandbox-and-confinement-gaps 'Plan Dry Run on macOS Lacks the xcrun Bypass, So Cargo Criteria Time Out'
```
Lines: `concerns/sandbox-and-confinement-gaps.md` 400 -> 396 after this item.

New body: none; the section is removed with its nested subsections.

### 16. concerns/verification-v2-followups.md#Verification v2 Known Gaps

Stale claim: The "Journal splitting" bullet: a note whose text starts with `###` splits the journal.
Tree today: `loom/src/fs/memory/parser.rs:154-169` (`escape_body`) prefixes a backslash to any body line starting with `\`, `---`, `#`, `<!--` or `*`, and `parse_entry_line` strips it (`parser.rs:109-116`, a `strip_prefix` of one leading backslash), so a `###` line in a note stays inside its entry. The rest of the section keeps its text.
Action:

```sh
loom knowledge replace-section concerns/verification-v2-followups 'Verification v2 Known Gaps' - < "$TMPDIR/.distill-body-16.md"
```
Lines: `concerns/verification-v2-followups.md` 251 -> 248 after this item.

New body (write to `$TMPDIR/.distill-body-16.md`, without the fence lines):

````markdown
- **Contract red runs are host-owned.** The freeze RPC accepts head/tree identity and queues a Freeze host job; `host_verify/freeze.rs` prepares and runs contracts through the confined host runner. `stage_service/contracts.rs::freeze_with_reports` applies host reports only after checking captured file hashes. Session-supplied red reports are not the freeze admission interface.
- **Tracking-key collision.** `loom-contract-<id>` (Contract session of stage `<id>`) equals the Stage key of a stage named
  `contract-<id>`; `orphan_candidates.rs::orphan_candidate` matches pid-file stems by prefix, so after a daemon restart the scanner
  can adopt one stage's process into the other. Same class as the `merge-`, `knowledge-` and `adjudication-` prefixes;
  `validation.rs` `RESERVED_NAMES` reserves none of them.
- **Escalated contract stage recovery.** `loom stage retry` refuses `NeedsHumanReview`; `human-review --approve` resets
  the contract budget inside the locked stage update that queues the stage (`loom/src/commands/stage/human_review.rs::handle_approve`).
  Approval does not retain the exhausted contract budget.
- **Continuation writer.** After a ceiling handoff the replacement contract writer gets a fresh contract signal without
  its predecessor's handoff (`generate_contract_signal` takes no handoff file).
- **Target-branch resolution differs.** `commands/hook/review_harvest.rs::review_harvest` resolves from the stage worktree, completion
  and both `loom stage review` commands from the CWD repo root. They agree while the CWD is inside the repo; share
  `commands/stage/review_status.rs::stage_worktree_and_target`.
- **Impact selection misses macro-only use** (`context/extract/rust.rs`) and skips every test in a contract file.
- **Reachable by bare name** passes when any symbol of that name is reached from any `from` node; common names (`run`,
  `handle`) can false-pass. `Partial` coverage counts as checked in definition-site exclusion.
- **`loom plan verify` does not check where contracts run.** A stage with `contracts` whose `working_dir` has no manifest
  for the contract's adapter (`Cargo.toml` for cargo) verifies clean, although every contract command then exits
  non-zero: `cargo test <name> -- --exact` takes no manifest path. See
  [verification-v2-delivery.md](../mistakes/verification-v2-delivery.md). A preflight check could resolve each
  contract's adapter and require its manifest in `working_dir`.
- **Integrity blind spot.** A file marked skip-worktree or assume-unchanged hides its edits from the `git diff` the
  fingerprint and the count totals trust. `verify::integrity::current_events` is the
  evidence source for `loom stage dispute-integrity` (`commands/stage/dispute_kinds.rs:73`), so the blind spot reaches
  the judge's evidence too.
- **Duplicated helpers** to unify: `verify/review/store.rs::canonical_work_dir` and the private one in
  `verify/contracts/store.rs`; the journal walk in `signals/v2_section_review.rs` and `commands/memory/handlers/pending.rs::staged_entries`;
  `CwdGuard` copied into four test files; `verify/utils.rs` `.size_limit(1 << 20)` versus `wiring.rs` `PATTERN_SIZE_LIMIT`.
- **Slow hook tests** (`loom-hooks/tests/run-all.sh` is 162 s after the PATH-fixture fix): `prefer-modern-tools-missing-rg-fd.sh`
  78 s, `poll-guard-subagent-waits.sh` 63 s, and four others near 40 s.

- **Waiting check verdicts cover the union.** Followers waiting for different criterion subsets share the verdict for the combined waiting selection. Revisit if reviewers misread failures from criteria they did not request.
- **Followers of one selection share wait state.** The first follower to see the job end removes the state; a later follower can relay again. An unchanged passing tree hits the cache, while a failing criterion runs again. Revisit if field data shows repeated failing integration-verify runs.
- **A promoted waiting job's hold counts its wait.** `hung_latch::job_holds` (`orchestrator/monitor/hung_latch.rs`) times an unstarted job from `queued_at`, and a waiting job keeps the `queued_at` of its first request. Following sessions heartbeat on every `loom check`. Revisit if a waiting job's session is flagged hung while the check ahead of it ran.
- **The `--criterion` normaliser call is unpinned.** Deleting `normalized_criteria` from `commands/verify.rs::execute` passes every test, because relay mode exits the process. Revisit when the relay path gains an in-process test seam.
- **Integration-verify full runs are routed by doctrine only.** `subagent-verify-guard.sh` exits 0 for every subagent and the main agent of an integration-verify stage. `scripts/full-runs-field-check.py` measures bare runs there until the field check is done. Revisit if they stay common next to `loom check integration-verify` calls.
- **A package-scoped run passes the guard as scoped.** `_verify_matchers.sh` reads a package selection (`cargo test -p <crate>`, `--package`, and each runner's equivalent) as scoped; in a project with one package that is the whole suite. Revisit if field data shows package scope used as a full run.
````

### 17. concerns/token-accounting-and-proof-defects.md#Open Follow-Ups After the Token-Optimization Plan

Stale claim: The "No shared any-path bounded reader" bullet says `usage --compare` keeps a local bounded reader.
Tree today: `loom/src/commands/usage/comparison.rs:6,29-38` loads its file through `fs::safe_read::read_bounded` after canonicalising the parent directory; there is no local reader.
Action:

```sh
loom knowledge replace-section concerns/token-accounting-and-proof-defects 'Open Follow-Ups After the Token-Optimization Plan' - < "$TMPDIR/.distill-body-17.md"
```
Lines: `concerns/token-accounting-and-proof-defects.md` 333 -> 329 after this item.

New body (write to `$TMPDIR/.distill-body-17.md`, without the fence lines):

````markdown
Found during the plan's stages and integration-verify, verified against the tree at this page's
`verified` revision, and not fixed by the plan. Each names its owner.

- **Git runner environment.** `git/runner.rs` removes inherited repository-selection variables before applying explicit per-call overrides; ordinary calls use the specified repository root.
- **Bounded process output.** `process::run_bounded_output` caps retained bytes per stream, drains excess and reports overflow while retaining its deadline and process-group retirement behavior.
- **Two atomic-write implementations for quota state.** History rewrites reuse
  `quota/cache.rs`'s `atomic_write` instead of `fs::locking::atomic_write_locked`, and
  `ensure_history_dir` (`loom/src/quota/history_store.rs`) checks for symlinks before
  `locked_dir_update` takes the lock (TOCTOU; low, since the quota dir is 0700 and single-writer).
- **Legacy stage inference from a project slug.** `commands/usage/sections/edits.rs::build` infers a
  stage from `project_slug` via `stage_id()`, the pattern the provider ledger forbids for new
  attribution. `cache_ignore.rs::check_ignore` duplicates `process::run_bounded`'s spawn and wait
  handling except stdin.
- **`#[serial]` env mutation races non-serial siblings.** `verify/criteria` runner tests mutate the
  process-wide `LANG` through `EnvGuard` (`loom/src/verify/criteria/tests/runner_tests.rs::EnvGuard`);
  `apply_stage_environment` (`loom/src/process/environment.rs::apply_stage_environment`) re-reads the process environment
  per run, so a concurrent non-serial test
  that spawns a confined command could fingerprint the mutated value. Not observed failing; pass the
  allowlisted env explicitly in those fixtures.
- **Deliberate fail-open choices.** `codex-forward-guard.sh`'s one-forward limit allows a call when
  the forwarder transcript is missing or unreadable (the harness may not have written it yet when
  PreToolUse fires); worker-bound brief records are never reset by compaction. Both stay until
  runtime evidence justifies failing closed.
- **Operator cleanup.** Knowledge-bootstrap left stray fixture state and 11 fake forward records in
  the live `.loom/work` (recorded in [Verification Harness](../mistakes/verification-harness.md));
  agents never edit `.loom/work` directly.
````

### 18. concerns/token-accounting-and-proof-defects.md#Open Follow-Ups From the Efficiency-and-Acceptance Plan

Stale claim: The first bullet says `loom knowledge check --strict` "exits 0 on this tree, so no baseline file is needed".
Tree today: Strict exits 1 at `62be2a3d2` and `doc/loom/knowledge/check-baseline.txt` is tracked (see item 8). The same wrong claim sat in a third file the validation table did not list. The other bullets keep their text.
Action:

```sh
loom knowledge replace-section concerns/token-accounting-and-proof-defects 'Open Follow-Ups From the Efficiency-and-Acceptance Plan' - < "$TMPDIR/.distill-body-18.md"
```
Lines: `concerns/token-accounting-and-proof-defects.md` 329 -> 327 after this item.

New body (write to `$TMPDIR/.distill-body-18.md`, without the fence lines):

````markdown
- **Knowledge check baseline.** `doc/loom/knowledge/check-baseline.txt` records the structural issues `loom knowledge check --strict`
  tolerates; gate with `--baseline` and regenerate it from a clean clone (see [knowledge-cli-gaps](knowledge-cli-gaps.md)).
- **Confirm the retrieval precision floor.** Run `loom knowledge eval`: `precision_floor` is 0.40 against a measured
  precision@5 of 0.45 on 2026-09-19, judged over the hook-delivered pack for prompt cases
  ([context-retrieval](../architecture/context-retrieval.md)).
- **Read the `peaks` section a week later.** `loom usage --since 7d` now prints peak resident context by scope
  (`commands/usage/sections/peaks.rs`): main and subagent p50, p90 and max, the share of transcripts above 250k and
  400k tokens, and for subagents the share whose peak never exceeded twice their first request (boot-dominated). The
  plan set no orchestrator context budget and no separate worker-brief budget key on purpose ("precision first,
  measure after"); this section is the measurement that decides whether either is needed.
- **No bounded build/test output command was built.** The `post-tool-use.sh` large-output notice covers the behaviour
  generally, and the by-command breakdown needed to design a dedicated command does not exist. Measure before adding.
- **24 oversized skills keep their bodies.** They cost tokens only when loaded and 77% were never loaded in the audit
  window; only their triggers and descriptions were in scope. Revisit if `loom usage` shows load cost.
- **Declared skills resolve against the home catalog only.** `check_declared_skills` reads `~/.claude/skills` plus the
  catalog, so a project-local skills directory is not consulted for a stage's `skills:` list
  ([plan-lifecycle-and-fields](../architecture/plan-lifecycle-and-fields.md)).
````

### 19. concerns/token-accounting-and-proof-defects.md#Unimplemented Reviewer Suggestions From the Usage-Evidence Plan

Stale claim: Two claims: `classify_response_ids` "runs before the scope drops foreign rows", and `run_evidence.rs` and `comparison.rs` "both `File::open` an operator path".
Tree today: `loom/src/commands/usage/provider.rs:97-98` calls `codex_scope::apply` and then `codex_provider::classify_response_ids`, so scope runs first. `comparison.rs` reads through `read_bounded` (`:6,38`); only `run_evidence.rs:93` (`read_declaration`) still calls `File::open`, bounded by `take(MAX_DECLARATION_BYTES + 1)` at :95.
Action:

```sh
loom knowledge replace-section concerns/token-accounting-and-proof-defects 'Unimplemented Reviewer Suggestions From the Usage-Evidence Plan' - < "$TMPDIR/.distill-body-19.md"
```
Lines: `concerns/token-accounting-and-proof-defects.md` 327 -> 323 after this item.

New body (write to `$TMPDIR/.distill-body-19.md`, without the fence lines):

````markdown
Open items from the plan's reviews, all small. The implemented ones are gone from this list.

- **Receipt writer.** `post-tool-use.sh` prefilters on `*"codex-forward.sh task"*`, stricter
  than `is_forward_wrapper`: a forwarder that quotes the absolute wrapper path runs the forward
  and forms no receipt, so its Codex rows read foreign with no partial reason (`*codex-forward.sh*`
  would match). `is_forward_wrapper` accepts the absolute spelling only in its canonical form
  (`claude_hooks_dir` canonicalizes) while the shell guard also accepts the unresolved
  `$HOME` path, and `forward_argv` accepts any `[A-Za-z0-9._-]{1,64}` model and effort while the
  guard allows a fixed list. `plain_child` in `forward_receipt.rs` checks directories with
  `exists()` and `symlink_metadata`, then opens by path with `O_NOFOLLOW` on the last component
  only and does not check `nlink`; a dirfd-based open like `safe_read::open_regular_no_follow`
  would close the swap and hard-link window. `read_payload` is the fourth bounded-stdin reader
  in `commands/hook` (`worker_brief.rs`, `review_harvest.rs`, `relay.rs`); a shared helper in
  `hook/mod.rs` would remove the duplication.
- **Codex scope.** `unthreaded` counts receipts observed in `[since, until]` only, so a
  thread-less receipt observed within `MAX_FORWARD_SECS` after `until` leaves its in-window rows
  foreign with no reason (`[since, until + MAX_FORWARD_SECS]` would cover it). No test pins the
  inclusive bounds (a receipt at exactly `since + 600 s`, exactly `until`, exactly `since`), so a
  `>=` to `>` mutation passes.
- **Comparison and evidence.** `--run-evidence` does not conflict with `--windows` or `--json`,
  which it ignores. A provider the declaration assigns but that has no measured rows makes
  `accounting_provenance` unknown and excludes the pair from net-total
  (`net-total-excluded-pairs`); the README does not say so. `net()` maps a `u64` overflow of
  resident plus output to `None`, reported as `missing-dimension`. `TokenRule` derives
  `Serialize` that nothing uses. `run_evidence.rs` `File::open`s an operator path (bounded by `take`), so a FIFO blocks and a symlink is followed; `print_evidence` repeats
  `render_json`. The quota tolerance tests stop at 1, 200, 281 and 301 s, so the exact `300 s`
  and `180 s` boundaries are unpinned, and nothing pins the human-mode failure report that now
  prints a `reasons:` line.
- **Sections.** `sections/tools.rs::is_subcommand` lists `rg`, `fd` and `jq`, so each distinct
  `rg "pattern"` becomes its own key and the top 20 fills with per-pattern rows while the
  aggregate `rg` volume never shows. `lifecycle_orientation.rs` drops entries older than
  `--since` before scanning, so a session that began before the window measures orientation from
  the cutoff (the transcript's unclipped `first_user_entry` is the real start), and it counts
  synthetic-model requests that `agents.rs` and `polling.rs` skip. `polling.rs::tool_kind`
  classifies by command prefix, so `git status && loom stage complete s1` counts as observation,
  and a command repeated three times counts as observation even when it does work;
  `repeated_set` duplicates `repeated_commands`.
- **Duplicated helpers and test gaps.** `spawn_join.rs` reimplements `json_lines`,
  `stage_directories`, `plain_directory` and `MAX_LEDGER_BYTES` from `provider_data/ledger.rs`
  (a third `json_lines` lives in `status/data/execution_models.rs`), and
  `AMBIGUOUS_MODEL` is `pub(crate)` for a file-local use. No test pins `same_fact`'s comparison of `backend` and `backend_id` (dropping either passes). No test covers `receipt_matches` or the
  receipt side of `resolve_wait_codex_execution`, and the damaged-journal error does not name the
  receipts path. `discovery_tests.rs` pins `<slug>-other` but not `<slug>x--worktrees-y`. No test
  pins the stderr filtering of session-controlled names in `completeness.rs`, and `inline_safe`
  caps the whole enumeration diagnostic at 200 characters, which can elide a parse error tail.
- **Replay harness.** `asset_paths()` sorts by the rendered path string, matching `check_shape`. Signals still render with the real `HOME` while worker-brief runs with an isolated one, so developer-level configuration can shift byte counts between machines.
````

### 20. concerns/loom-ask-and-read-gate.md#Unimplemented Reviewer Suggestions: `loom ask`

Stale claim: "`parse_codex` fails closed only on `item.completed`; apply the allow-list to `item.started` too (`backend.rs:229`)".
Tree today: `loom/src/commands/ask/backend.rs:233-235`: `"item.started" | "item.updated" => refuse_tool_item(&row["item"])?` and `"item.completed"` calls it too (the cited `:229` has drifted). The rest of the bullet keeps its text; its other line references were not re-verified.
Action:

```sh
loom knowledge replace-section concerns/loom-ask-and-read-gate 'Unimplemented Reviewer Suggestions: `loom ask`' - < "$TMPDIR/.distill-body-20.md"
```
Lines: `concerns/loom-ask-and-read-gate.md` 108 -> 107 after this item.

New body (write to `$TMPDIR/.distill-body-20.md`, without the fence lines):

````markdown
- **Codex isolation:** the scratch directory honours `TMPDIR`, which can nest in a git checkout; Codex is
  assumed to walk to the nearest `.git` for its agents-instructions file, so a repo's copy could reach the worker
  (`run.rs:122`). The disable list names no web-search or image-view feature and the probe reads only a text
  file; extend it with a planted web-search request (`backend.rs:21`). Check that `--help` marks
  the Codex variant experimental in its first sentence (`backend.rs:36`).
- **Environment scrub is a deny-list** (`run.rs:34`): a new session variable passes silently.
  `process/environment.rs` shows the allow-list alternative, which withholds `SSH_AUTH_SOCK`. Revisit if the
  Codex probe fails or an agent is told to use `loom ask`.
- **Process lifetime:** unlink the payload after opening it so a SIGKILL leaves no plaintext copy
  (`run.rs:172`); a SIGKILL of loom orphans the worker with no deadline, `PR_SET_PDEATHSIG` in `pre_exec`
  would cover it on Linux, and a `setsid` descendant escapes group retirement (`run.rs:159`). The reader, cap
  and retire logic re-implements `process::run_bounded` and `kill_group` (`process/mod.rs`, `process/owned.rs`);
  an on-spawn hook in `run_bounded_with` would publish the pgid to the interrupt handler and allow reuse
  (`run.rs:189`). The stderr excerpt is the tail of the first 64 KiB, not the end of stderr; `drain_tail`
  keeps the end (`run.rs:249`).
- **Payload:** a per-run random nonce in the wrapper tag would stop a file forging an opening tag or a variant
  such as `</ file>` (`payload.rs:170`; the doc comment claiming nothing can forge a wrapper is stronger than
  the code). `read_to_string_bounded` does not check `nlink`, so a hard link with a harmless name to `.env`
  passes the credential check; `safe_read::single_link_regular_file` would refuse it, at the cost of refusing
  package-manager hard links the operator names (`payload.rs:52`). `is_credential` is `pub(crate)` but used
  only inside the module and its tests, and `payload_tests.rs` has no end-to-end `read_documents` test with
  `HOME` as the root (`ask_eval_contracts.rs` covers it) (`payload.rs:146`).
- **Evaluation:** `eval.rs:28` says case paths are relative to the cases file's directory, but `base.join`
  lets an absolute path through when it is inside the root; refuse absolute case paths or fix the comment. A
  1 MiB cases file can hold thousands of cases of up to 400 KB each with no case cap and no overall deadline.
  In `cases.json:78` the `cpp` case does not check the "why" half of the question, and an answer saying `c,
  not cpp` passes; the file is a frozen fixture.
- **Tests:** the `ask` parse tests in `run_tests.rs:79` belong in `backend_tests.rs` and a new test module for `mod.rs`.
````

### 21. architecture/plan-server.md#Open points

Stale claim: Two bullets: "Unbounded inputs" says `read_body` "has a per-read bound but no total deadline", and "Duplicated helpers" says `request.rs::read_body` and `web_server::http::read_body` "have separate body-reading implementations".
Tree today: `loom/src/plan_server/request.rs:233,241-248`: `BODY_TIME_LIMIT` is 30 s and `read_body` calls `http::read_body_within(stream, prefix, length, running, BODY_TIME_LIMIT)` (`web_server/http/body.rs:39`), so `POST /api/save` has a total deadline and planning reuses the shared reader. The remaining text of both bullets (the presence-map and broadcast limits, the `ws/conn.rs`, `token.rs`, `router.rs`, `gate.rs`, `bootstrap.rs` and `hub.rs` duplication list) was not re-verified here.
Action:

```sh
loom knowledge replace-section architecture/plan-server 'Open points' - < "$TMPDIR/.distill-body-21.md"
```
Lines: `architecture/plan-server.md` 293 -> 293 after this item.

New body (write to `$TMPDIR/.distill-body-21.md`, without the fence lines):

````markdown
Reviewer suggestions nobody implemented, each checked against the tree. File-mode follow-ups are under "File mode follow-ups" below.

- **Token and hardening gaps.** `browser::open` passes the tokenized URL to `xdg-open` as argv, readable by another local user through `/proc/<pid>/cmdline` while the opener runs; section 02 mandates opening the URL, so closing it needs a spec decision (for example opening a 0600 redirect file in the runtime directory). Refusals written by the shared layer (Host 403, head errors, 503 at the cap) and tungstenite's handshake errors carry no `Referrer-Policy`; the bodies are plain text without links. `lock::acquire` opens the lock by path rather than relative to the directory fd, and no test plants a symlink inside the runtime path. `Auth` and `Admitted` have redacted `Debug`; keep it when adding fields.
- **Unbounded inputs.** `Presence::public_id` records every distinct well-formed cookie for the life of the process, and `set_name`/`reading` broadcast to up to 48 connections with no per-connection rate limit; both need the access cookie.
- **Shared module carries dashboard wording and dashboard-only code.** The 503 bodies and logs in `web_server/unserved.rs`, `head.rs`, `ws.rs` and the accept-error log in `serve.rs` say "dashboard" and reach planning clients; the dashboard's tests pin those bodies, so neutral wording needs a name in `ServeConfig` and a test update. `token::cookie_name` hardcodes `loom_dashboard_<port>`, `auth::resolve_process_token` is dashboard-only, and `web_server/access_tests.rs` imports the dashboard's `ServeOptions`; moving `resolve_process_token` next to `AccessPolicy::resolve` in `commands/status/web/mod.rs` would remove the test dependency on `commands`.
- **Duplicated helpers.** `ws/conn.rs` repeats `SOCKET_TIMEOUT`, `send_pending` and the tungstenite config of `web_server/ws.rs`; random hex minting is hand-written in `web_server/token.rs`, `daemon/server/tokens.rs` and `csrf.rs` (a shared `random_hex(n) -> io::Result<String>` would also remove the `/dev/urandom` panic in `PublicId::mint`, `ids.rs:51`, which runs under the presence mutex); `router.rs` passes `ws::MAX_INBOUND_BYTES` while `mod.rs` builds `Limits::new` with it (read `conn.limits.max_inbound_bytes()`); `gate::check_role` re-derives the public id `router::call` already computed; `web_server/bootstrap.rs:46` drops the `write_redirect` result with `let _ =` while the planning router logs it; `ws/hub.rs::connections_of` is `pub` but test-only.
- **Hook failures freeze things silently.** `watch.rs` runs later plans' batch hooks and authoring subscribers on shared threads with no `catch_unwind`, and `RunningServer::stop` drops the join result. `PlanContext::project()` hands out a std `RwLock` read guard while the watcher takes the write lock: a second `ctx.project()` in one call stack can deadlock behind a waiting writer.
- **Authoring.** Events can arrive out of order (subscribers re-read `holder()`); a browser that takes authoring over HTTP without a WebSocket never goes away; `web/src/plan/state/atoms.ts::canAuthorAtom` lets the holder author while stopping although the server refuses every `TakeIfFree` then, and `authorBlockReasonAtom` is `null` while server state is unloaded.
- **Web shell gaps.** `web/src/plan/api/socket.ts` writes `lastMessageAt` and never reads it (a watchdog needs a server text heartbeat first); the checklist skeleton never resolves to an error when the socket cannot connect (the "Connection lost" banner belongs to the chat panel plan); `header/tabs.tsx` has no narrow-width strategy, so the header overflows at 390 px once plans register about 200 px of tabs; `header/popover.tsx` and `ui/dialog.tsx` use `data-open:`/`data-closed:` animation variants that never match Radix's `data-state`; `header/step-tracker.tsx` marks the current step with `bg-muted` on `bg-card` (low contrast); `header/share.tsx` copy state never returns to idle, so a repeat click announces nothing; `header/presence.tsx` renders a focusable list item with no action.
- **Tests.** `plan_server/tests/mod.rs::an_omitted_port_walks_past_a_busy_default` returns early when 7272 is taken and passes vacuously beside a running `loom plan`; `loom/tests/web_server_contracts.rs` has no loopback skip (failing loudly is defensible for contracts); the share-link test in `plan-shell.contracts.test.tsx` cannot detect a retained tokenized link (stub the third open's fetch as never-resolving and assert the URL is absent); the `wire/tests_schema.rs` helper overlaps the dashboard's and is not shared.
````

### 22. architecture/plan-server.md#Gate order

Stale claim: "Bodies are read with `Read::take` bounded by the gate-checked `Content-Length` (`request::read_body`), not `web_server::http::read_body`." (a second stale claim in the same file, found while checking row 14).
Tree today: `loom/src/plan_server/request.rs:233-248` (see item 21). Only the last sentence changes.
Action:

```sh
loom knowledge replace-section architecture/plan-server 'Gate order' - < "$TMPDIR/.distill-body-22.md"
```
Lines: `architecture/plan-server.md` 293 -> 293 after this item.

New body (write to `$TMPDIR/.distill-body-22.md`, without the fence lines):

````markdown
Every `/api/*` request and `/ws` upgrade passes these checks in order. `gate::admit` runs first: `Host` (403), exact `Origin` equal to `http://<Host>` on every POST and every WebSocket (403; the dashboard's loopback-alias leniency does not apply), the access cookie `loom_plan_<port>` (401), the browser identity. The load hold follows, because the role check can take authoring. `gate::check_route` runs last: the body's `Content-Type` and `Content-Length` (415/413, before any body byte is read), then the route's `Role`. The browser cookie is `loom_plan_browser_<port>`; both cookies are `Path=/; HttpOnly; SameSite=Strict`. An HTTP request without a valid browser cookie gets a minted `BrowserId` and a `Set-Cookie`; a WebSocket without one is refused 401 (the app calls `GET /api/server` first). A minted id reaches only `Role::Viewer` routes: on `Holder`/`HolderOrTake` routes it is refused 401, and `Presence::name_of` does not record it, so the presence map is bounded by browsers that send their cookie back. `GET /?token=<token>` with the right token sets the access cookie, the browser cookie when absent, and redirects to `/`; a wrong token or a foreign `Origin` is 403. Planning responses carry `Referrer-Policy: no-referrer`. Bodies are read through `request::read_body`, which calls `web_server::http::read_body_within` with the gate-checked `Content-Length` as the size and a 30 s total deadline (`BODY_TIME_LIMIT`); an incomplete body is cancelled when the server stops.
````

### 23. concerns/planning-session-sandbox-limits.md#Recurring flock-after-drop

Stale claim: "`ServerLock` is the third lock type found holding its `flock` past `drop`".
Tree today: `loom/src/plan_server/lock.rs:139-147` implements `Drop for ServerLock` with `libc::flock(..., LOCK_UN)` and a comment giving the forked-descriptor reason. The maintainability-test proposal stays.
Action:

```sh
loom knowledge replace-section concerns/planning-session-sandbox-limits 'Recurring flock-after-drop' - < "$TMPDIR/.distill-body-23.md"
```
Lines: `concerns/planning-session-sandbox-limits.md` 238 -> 236 after this item.

New body (write to `$TMPDIR/.distill-body-23.md`, without the fence lines):

````markdown
Lock types that own a `flock` release it in `Drop` with `flock(LOCK_UN)`, because a child forked on another thread holds a copy of the descriptor until its exec ([mistake](../mistakes/test-concurrency-and-fixtures.md)); `ServerLock` (`plan_server/lock.rs`) does. A proposal: a maintainability test that fails for any struct owning a locked `File` without a `Drop` that calls `flock(LOCK_UN)`.
````

### 24. concerns/planning-document-model-followups.md#Planning Trace Limits

Stale claim: The `version_route.rs` bullet says an I/O error from `symlink_component` "answers 400 \"crosses a symlink\" instead of 500".
Tree today: `loom/src/plan_server/trace/version_route.rs:29-38`: an `Err` from `symlink_component` answers `ApiResponse::error(500, "io", "the document path could not be checked")`; only `Ok(Some(_))` answers 400 `bad_path`. The `log.showSignature` half of the bullet was not re-verified.
Action:

```sh
loom knowledge replace-section concerns/planning-document-model-followups 'Planning Trace Limits' - < "$TMPDIR/.distill-body-24.md"
```
Lines: `concerns/planning-document-model-followups.md` 174 -> 174 after this item.

New body (write to `$TMPDIR/.distill-body-24.md`, without the fence lines):

````markdown
**Not checked in a stage sandbox** (the integration walk ran against the built binary in a scratch repository and could not reach these; run them outside a stage):

- Journeys J2 (needs sealed summaries in `doc/loom/summaries`), J3 and every server write (settle, findings status, save) fail on the read-only `/tmp` plan lock (`fs/locking.rs`); J4, J5 and J8 need tmux, which cannot create a socket. The walk covered a hand-written Settled line, hand-written `REVIEW-` files and ledger and stubbed frames instead.
- Still open: a real Review the feature session, a real summary rebuild over a brief bundle, a real plan completion importing into the ledger, an actionable finding reaching a later plan's revisit mark (unit-tested, not driven), and `trace_changed` refetching without a reload (evidence is `trace_changed_follows_a_heading_edit` and the web refetch tests, not the walk).
- `POST /api/summaries/auto` answers 403 when a chat start claims authoring, the browser posts the auto-rebuild setting on the holder transition, and the session fails to start and frees authoring before the post lands. Seen only when a session start fails within moments of the claim (tmux denied in a stage sandbox). A client settle timer narrows the window and cannot close it, and a server 200 would break the pinned assertion in `summaries/state.test.tsx` ("resends the current auto-rebuild preference after a reconnect hello").

**Server and model limits:**

- `settle_route.rs`'s stale check covers only the document's own file: a plan whose brief bundle or upstream section changed inside the watcher lag gets a line built from the old hashes and reads `revised` or `upstream_changed` after the refresh. A plan with validation errors answers 422, which the route table lists.
- `settle::clear` drops every column-0 `Settled:` line before hashing, so editing a prose line that begins `Settled:` never makes the document `revised` (the plan check still reports `settled-bad` or `settled-duplicate`); excluding only parsed marks from the hash would close it.
- `version_route.rs` runs `git log` with the user's configuration: with `log.showSignature=true` and signed commits, gpg lines precede the id, `is_commit_id` fails and the route answers `null` (pass `--no-show-signature`).
- `ReviewFeature` has a null wire `target` (`planning/checklist/offers.rs`) while the brief names `doc/plans/<f>/findings.md`; no consumer reads the target for this kind.
- `engine.rs` `with_source` rewrites only the plan's own `links` and `cites` keys on a move between homes; brief links, brief `cites` keys and `bundle` keys keep the old `briefs/<slug>/` path, so a moved plan with briefs reads stale and is rebuilt. An unreadable `.md` brief is exempt from must-cite while the rebuild prompts say "every Markdown brief file".
- `build/docs.rs` `finish` is exactly 50 lines, the function limit; the next edit there splits it. `planning/summary/brief.rs` ends a rule line with a trailing contrast ("in several sentences, not one") that the style rules forbid, and `brief/tests.rs` pins the text.
- `findings/tests.rs`: both symlink tests pass even without `refuse_symlinks` (`import.rs`), because `O_NOFOLLOW` in `replace_no_follow` and the no-follow plan read fail first; a linked directory that does hold the plan would pin the explicit check.
- The route tests lack a negative case (an untouched second feature receives no `trace_changed` frame) and the web settle tests lack the 422 invalid answer and `locked={false}` overriding the file-name rule; `visionRowText` (`trace/stub-text.ts`) matches the first numbered row of any table in the vision, so a numbered table above the sections table gives the wrong stub.
- `schema-trace.ts` `unresolvedSchema` uses `z.object` where the rest of the mirror uses `z.strictObject`; `skills/loom-plan-writer/SKILL.md` separates the stage-order block from the bullets that explain it with the `Implements` forms paragraph.

**Browser limits:**

- A finding's text enters a session as the user's own first message (the seed is spec-mandated), so an instruction-like finding is treated as a user instruction.
- The ledger hash is read after the trace, so a ledger edit between the two reads leaves the hash newer than the cards drawn and the 409 guard does not fire for that edit; `trace_changed` follows and reloads, and the window is small.
- Accessibility and cost nits on the trace pages: `findings-page.tsx` puts `aria-label` on a role-less `<p>` (About) and `coverage-page.tsx` sets `aria-label` beside an `h3` with the same text (use `role=group` or `aria-labelledby`); several "Affected documents" lists in one turn footer and every card's "About" group share an accessible name; Confirm dismissal shows no busy state while the POST is in flight; `coverage-page.tsx` calls `model.cards.indexOf` per card per render; `findings-cards.tsx` uses the literal 200 where `REASON_MAX` exists and draws the no-offer reason on every open card; `trace.css` should use `:focus:not(:focus-visible)` for its `outline: none` rule; the settle control draws its own inline reason for both Mark settled and Unsettle while working or blocked; `help/maps-help.tsx` `Definitions` nearly duplicates `help/legend.tsx` `LegendGroup`; `trace.css` line 3 passes 100 columns (oxfmt does not check CSS comments); `turn-footer-impact.test.tsx` checks absence before the trace has loaded and `settle-flow.test.tsx`'s state-label `findByText` resolves immediately; `STARTED_PLAN` in `findings-model.ts` duplicates `LOCKED_NAME` in `settle-control.tsx`, and reading `docOfPath(trace, entry.plan)?.progress` would remove it.
- `trace.contracts.test.tsx` "Mark settled" (`findByRole`, the default 1 s timeout) and `editor/live-preview.test.ts` failed once each in the 366-file vitest run of the completion check and pass alone: load-sensitive tests, not defects of the stages that ran them.

**Hook proposals for recurrences:** subagents ran `git mv` against a brief that forbids git writes ([again](../mistakes/planner-follow-ups-delivery.md#small-slips-that-recurred-in-this-plan)); a PreToolUse check refusing `git mv` and `git rm` in a subagent Bash call would stop it. The backgrounded `loom check` proposal is in [planning-guidance-followups](planning-guidance-followups.md).
````

### 25. concerns/planner-kickoff-fixes-followups.md#Reviewer suggestions not implemented

Stale claim: In the `drafts/delete.rs` bullet: "the locked-spelling check precedes the document unlink, so a concurrent `loom run` rename ... the executing plan's bundle is removed; re-check after the document step".
Tree today: `loom/src/plan_server/chat/drafts/delete.rs:105-112` (`remove_bundle`) calls `refuse_locked` before removing the bundle; its doc comment says a rename between the lookup and the unlink makes the locked spellings be looked up again first. The rest of that bullet (`removed` listing the document on `ENOENT`, `open_root` errors, `grant_of`, feature targets, `kickoff_cmd.rs`) was not re-verified.
Action:

```sh
loom knowledge replace-section concerns/planner-kickoff-fixes-followups 'Reviewer suggestions not implemented' - < "$TMPDIR/.distill-body-25.md"
```
Lines: `concerns/planner-kickoff-fixes-followups.md` 31 -> 31 after this item.

New body (write to `$TMPDIR/.distill-body-25.md`, without the fence lines):

````markdown
Server, `draft_cmd.rs` and the bridge:

- A repeated `now: false` set aside during the closing turn is already a no-op (`close_within` skips the interrupt), but with no chat running set aside answers `NO_CHAT`, and a Preparing session "the chat is closing"; both messages mislead (UI race only).
- `draft_cmd.rs` `set_aside` reads `draft.target` to decide confirmed or not; after a logged `on_confirm` draft-write failure it tears a confirmed kickoff down with no closing write, and discard and continue lose the target too. The scope's grant is authoritative.
- `discard` ignores the result of `drafts::remove` after the files went (the answer is ok and the draft stays listed), and holds `ChatState::starting()` across `remove_written` (up to 4096 entries plus `git cat-file` calls), so sends and starts wait for a large delete.
- `wrote_files` reads an I/O refusal from `written()` (a symlinked `doc/plans/briefs`) as "no files", so a discard-only removes the draft and leaves the document; the doc comment names only the invalid-target case.
- `end_plan.rs`: clamp an end's own bound to `final_write_bound` (`plan.bound.map_or(bound, |b| b.min(bound))`) so drain and recovery, which wait `final_write_bound + END_SLACK`, never return before a longer per-end bound expires. `Bridge::end_within` repeats the body of `end`; share one private helper taking `Option<Duration>`. The branch of `close_within` that returns `Ok(texts)` after `end_within` fails has no test (a stop would have to be injected between the two calls). A test, `a_repeated_set_aside_leaves_the_closing_turn_running`, does not discriminate the end-planned skip: the session already reads `Ending`, and `Session::interrupt` refuses with `NotRunning`.
- `drafts/delete.rs`: `removed` lists the document even when `unlinkat` returned `ENOENT`. `open_root` errors forward the absolute project root to the browser (every browser holds the access cookie). `grant_of` takes the size from the target's spelling, not `draft.size`: cross-check `grant.size == draft.size` (discard checks it; the resume path in `draft_continue.rs` does not). Feature targets have no locked spellings, so Delete does not check whether one of the feature's plans under `doc/plans/<name>/` is executing before removing `index.md`. `kickoff_cmd.rs`: the holder check guards only confirm, so a document created later by a non-kickoff writer under a confirmed draft's name can still be adopted by that draft's Continue or Delete (accepted; revisit if other writers of `doc/plans` appear in the chat flow).
- `fs/safe_fs/write/tree.rs`: no `st_dev` check, so a mount point inside the bundle is emptied across the filesystem boundary (`rm --one-file-system` equivalent). `delete_tests.rs` `a_bundle_refused_after_the_document_says_it_is_already_deleted` relies on `chmod 0o500` refusing the unlink; skip it when running as root (`nix` has no `user` feature here: use `libc::geteuid()` under an `unsafe` block with a SAFETY comment).
- `team.rs` `kickoff_size` caches only sizes it read; the scope read error path is not covered by a test.

Web:

- `draft-view.tsx`: the "Setting aside" disabled reason repeats the notice text (two messages for one state; the spec `section_08.md` requires it, so consider hiding Set it aside while ending). An agent done-fence end (`bridge/done_fence.rs`) is also "ending", so the notice says "being set aside…" for a finishing kickoff. After a "wrote files" refusal the page still offers only Discard; refetch the detail on that refusal. `useRestoreFocus` finds the opener through `button[data-look="quiet"]`, so a second quiet action would take the focus; a `data-*` attribute or a ref through `DisabledReason` would remove the dependence. `isFolder` infers a folder from a missing `.md` suffix; an explicit marker in `written` would stop the web restating a server naming rule.
- `kickoff.css`: `.kickoff-draft__confirm > p` outranks `.plan-fine`, so the committed note renders at `--type-body` and sits on the button row.
- No shared Rust and TS fixture covers `KickoffDraftDetail.written` and `committed`: `web/src/plan/api/fixtures/kickoff-drafts.json` is a list fixture and holds no detail.
- `header/feature-switcher.test.tsx` has a title saying the offer is a fallback; `kindOf` now always prefers the offer.
````

### 26. concerns/planner-ui-repair-followups.md#Chat, help and reading

Stale claim: "`chat/use-session-brief.ts` paints one frame with the previous label when a session changes".
Tree today: `web/src/plan/chat/use-session-brief.ts:29-58` keys the loaded state by session id (`useState<{ id: string; state: BriefState } | null>`) and returns `LOADING` until `loaded?.id === sessionId`.
Action:

```sh
loom knowledge replace-section concerns/planner-ui-repair-followups 'Chat, help and reading' - < "$TMPDIR/.distill-body-26.md"
```
Lines: `concerns/planner-ui-repair-followups.md` 214 -> 214 after this item.

New body (write to `$TMPDIR/.distill-body-26.md`, without the fence lines):

````markdown
- `chat/start-chat.tsx` should capture the screen's feature at click: the end wait can last 10 minutes and a project-wide session stays mounted across navigation.
- Offer refusals on `/` and `/new` (`chat/offer-flow.ts`) are plain text; a link to the home or `/files` would save the user a guess. `attachHere` and `AddToChat` do not clear `offerErrorAtom` as `discussDoc` does, so a stale refusal stays beside the new chip; the pending chips on a concluded foreign screen are not gated on `canAuthorAtom`.
- `composer-hooks.ts`: `c` sets `chatSheetOpenAtom` at desktop width, where nothing closes it, so narrowing the window later opens the sheet unasked.
- `help/pinned-scene.tsx` re-creates its observer and three timers on every search keystroke (drop the `children` dependency, skip `setMeasured` when unchanged).
- `chat/pressure/pressure-timeline.tsx`: keys repeat if a round ever holds two steps of one kind (server behaviour unconfirmed).
- `foldAll` and the header Shift+Esc path leave fold notes in place while `foldRun` and `useOpenAll` clear them (check against section 05); Open all is a no-op when `page.empty` shows `EmptyStates`; a summary whose source file is gone shows disabled Edit and Discuss with no reason; `tree-entry.tsx` wraps the state onto a second line at about four nesting levels.
- Header: the pressure strip shows a file name because the wire has no title; the `.ps-knob` shadow is a literal `oklch`.
````

### 27. concerns/planner-ui-repair-followups.md#Untrusted text and tests

Stale claim: The first bullet lists about 20 components as rendering raw server strings.
Tree today: Of the listed files, `concept/{concept-route,marks,run,plan-glyph,empty-states}.tsx`, `files/search-results.tsx`, `files/plan/{stage-row,stage-graph,stages-panel,validation-list}.tsx`, `editor/validation-panel.tsx`, `diff/diff-page.tsx`, `chat/{turn-footer-plans,chips}.tsx` and `summaries/rebuild-button.tsx` import `Untrusted` or `visibleControls` (`rg -c "Untrusted|visibleControls"`). Still raw: `web/src/plan/files/reading-column.tsx:106` and `web/src/plan/editor/edit-body.tsx:140`. `chat/connection-banner.tsx` renders fixed strings and a phase name only; `files/tree.tsx` has no server string outside `LoadState`. The review's "all 8 fixed" is wrong; the knowledge list was about 20 names, and two of them are still raw.
Action:

```sh
loom knowledge replace-section concerns/planner-ui-repair-followups 'Untrusted text and tests' - < "$TMPDIR/.distill-body-27.md"
```
Lines: `concerns/planner-ui-repair-followups.md` 214 -> 214 after this item.

New body (write to `$TMPDIR/.distill-body-27.md`, without the fence lines):

````markdown
- Raw server strings outside the Untrusted rule (`common.md`): `files/reading-column.tsx` (`{moved.title} is now at {moved.path}` in the moved-plan link) and `editor/edit-body.tsx` (`holder?.name` in the took-authoring note). Other planner components that show server text import `Untrusted` or `visibleControls`, and `ui/load-state.tsx` draws its `message` through `Untrusted`; `chat/connection-banner.tsx` renders no server text. A fresh audit of the remaining components has not been done.
- Missing format-character tests: `StepItems`, `queue.tsx` section and plan rows, `tree-entry.tsx` wraps, the `sentence-text` link and image branches and the BOM-only fallback, the chip and key-chip paths in `document-key`.
- Other test gaps: `dock.test` mocks `ChatRegistrations`, so nothing checks that `connect` mounts the socket and attention hooks; `dashboard-button.tsx` has no test that keeps the last URL when a refresh fails; `set_owner` during a launch (`bridge/start.rs`) and a recovery with a non-null feature (`tests_recovery.rs`) are untested.
- **Web lint.** `web/.oxlintrc.json` has no react plugin, so a conditional hook passes lint ([the mistake](../mistakes/planner-ui-repair.md#a-conditional-hook-passed-tsc-lint-and-tests)). Enabling `react/rules-of-hooks` raises about 80 warnings under `--deny-warnings`; clean those up, then enable the rule.
````

## Part B: content moves, link repoints and the history note (items 28 to 35)

### 28. mistakes/sandbox-hook-preflight.md#Extending the Check 5 Program Denylist Was Proposed as a Fix

Stale claim: The Why paragraph names `rust-toolchain.toml` and `bunfig.toml` as live paths, and `loom knowledge check --strict` fails with `source reference "bunfig.toml" does not exist` and the same for `rust-toolchain.toml`.
Tree today: Neither file exists in the repository (`fd -g bunfig.toml`, `fd -g rust-toolchain.toml` find nothing); the sentence lists them as examples of version-manager shim inputs. `loom/src/fs/knowledge/chunker/references.rs:62-77` (`EXAMPLE_MARKERS`) includes `e.g.`; checked on a scratch copy: with the marker in front of the list the checker reports both names as `unresolved example reference` notes and the two issues disappear. One word is added; the section stays 13 lines.
Action:

```sh
loom knowledge replace-section mistakes/sandbox-hook-preflight 'Extending the Check 5 Program Denylist Was Proposed as a Fix' - < "$TMPDIR/.distill-body-28.md"
```
Lines: `mistakes/sandbox-hook-preflight.md` 13 -> 13 after this item.

New body (write to `$TMPDIR/.distill-body-28.md`, without the fence lines):

````markdown
**What happened:** `loom run` refused a user-level `bunx -y ccstatusline@latest --hook` hook. The first response split package runners out of `SELF_DIRECTED` with a better message, scanned `statusLine`, then built a trust-once flow, and recommended `bun add -g` plus an absolute binary path as the safe alternative. The owner rejected it as insufficient and it was reverted.

**Why:** check 5 is a list of named programs to refuse, and the fix only extended that list. Hooks also run session-written code through subcommand runners (`pnpm dlx`, `bun x`, `dnx`, `jbang`, `nix run`), version-manager shims reading cwd files (e.g. `.tool-versions` `path:` versions, `rust-toolchain.toml` `path`, `bunfig.toml` preload), runtimes loading writable caches (deno, go build cache, bun auto-install), Linux hard links from bun/uv/pnpm caches into global installs, shebangs, and launchers check 5 never reads (MCP stdio servers, plugin hooks, managed settings, `apiKeyHelper`).

**Prevention:** treat an unsandboxed hook as running every file and cwd config its runtime loads; before extending check 5's tables, enumerate those load paths across ecosystems and both platforms. Never recommend a global install as safe without checking whether the manager hard-links it from a session-writable cache.

**Fix:** reverted; the structural option (run non-loom hooks inside the session sandbox) is open with the owner.
````

### 29. mistakes/testing-and-lint.md#two maintainability-ledger sections (31 lines)

Stale claim: `mistakes/testing-and-lint.md` is 408 lines, over the 400-line tier-2 limit (`loom knowledge check --strict`: `tier-2 file is 408 lines`).
Tree today: The sections at lines 100-130 are about the maintainability ledger and belong with `mistakes/maintainability-baseline.md` (103 lines). Neither section contains a link, and no file links to their anchors (`rg -n "growing-a-function-that-carries|ledger-growth-reached" doc/loom/knowledge` finds nothing), so no inbound link needs repointing.
Action: move (copy the sections to the destination with `update`, then `delete-section` them at the source). The helper script `$SEC` is written once under "Helper" at the end of this file. Headings, exact:

```text
Growing a Function That Carries a Maintainability Baseline Entry Breaks the Gate (2026-08-10)
Ledger Growth Reached Push Time Again — Gates Moved Earlier (2026-08-14)
```

Destination: `mistakes/maintainability-baseline`.
Expected line counts after the move: `mistakes/testing-and-lint.md` 408 -> 377 lines; `mistakes/maintainability-baseline.md` 103 -> 134 lines.

Commands:

```sh
F=doc/loom/knowledge/mistakes/testing-and-lint.md
{ for h in 'Growing a Function That Carries a Maintainability Baseline Entry Breaks the Gate (2026-08-10)' 'Ledger Growth Reached Push Time Again — Gates Moved Earlier (2026-08-14)'; do python3 -I "$SEC" "$F" "$h"; echo; done; } > "$TMPDIR/.distill-body-ledger.md"
loom knowledge update mistakes/maintainability-baseline - < "$TMPDIR/.distill-body-ledger.md"
loom knowledge delete-section mistakes/testing-and-lint 'Growing a Function That Carries a Maintainability Baseline Entry Breaks the Gate (2026-08-10)'
loom knowledge delete-section mistakes/testing-and-lint 'Ledger Growth Reached Push Time Again — Gates Moved Earlier (2026-08-14)'
```

### 30. mistakes/hooks-shell-portability.md#four macOS and Bash 3.2 sections (about 93 lines)

Stale claim: `mistakes/hooks-shell-portability.md` is 412 lines, over the 400-line tier-2 limit.
Tree today: Lines 194-286 hold four sections about macOS BSD tools and Bash 3.2 (the remaining sections are general hook portability). They move to a new topic with a blurb, and the tier-1 pointer in `mistakes.md` gains a link to it. Three other files link to three of the moved anchors; items 31 to 33 repoint them. `update` creates the topic with a title derived from the slug ("Hooks Macos And Bash32") and a generic blurb that `annotate` replaces.
Action: move (copy the sections to the destination with `update`, then `delete-section` them at the source). The helper script `$SEC` is written once under "Helper" at the end of this file. Headings, exact:

```text
BSD `wc` Padding and Bare `sha256sum` Skipped the Reviewer Harvest on macOS, and the Hook Suite Never Ran in CI (2026-10-06)
A Heredoc Inside `$( )` Parses Under Bash 5 and Fails Under Bash 3.2 (2026-10-13)
The First macOS Hook Run Found Bash 3.2 and BSD Faults the Linux Shims Hid (2026-10-13)
A Shim's `--version` Probe Recursed Through a Test's Wrapper and Killed the Linux Runner (2026-10-13)
```

Destination: `mistakes/hooks-macos-and-bash32`.
New topic blurb (58 characters): `macOS Bash 3.2 and BSD tool faults that Linux hook runs hid`. `update` seeds the title "Hooks Macos And Bash32"; leave it.
Expected line counts after the move: `mistakes/hooks-shell-portability.md` 412 -> 319 lines; `mistakes/hooks-macos-and-bash32.md` 0 -> 96 lines.

After the move, add the link to the tier-1 pointer. Write this body to `$TMPDIR/.distill-body-pointer.md` (one link added to the last line of the section) and run:

```sh
loom knowledge replace-section mistakes 'Hooks: Shell Command Matchers (2026-07-28)' - < "$TMPDIR/.distill-body-pointer.md"
```

````markdown
Token-based Bash matchers repeatedly shipped with bypasses because separators that are _glued_ to a neighbour never become tokens. Also: forgeable `glob | head -1` privilege lookups, env leakage into simulated process trees, three Bash parsing traps, and cross-platform portability traps (gawk extensions on BSD awk, non-portable `timeout`, redirect order, `cmd | head` under `pipefail` killing the producer with SIGPIPE). Lexical guards also blocked 435 mostly legitimate calls in one week and agents rewrote commands until they passed: a guard blocks only what it decides soundly, names the sanctioned route, rewrites the commands loom owns, ships each relaxation with its hostile test, resolves paths physically and never keys on `TMPDIR`.

→ [Shell Command Matchers](mistakes/shell-command-matchers.md), [Hooks: Shell Portability](mistakes/hooks-shell-portability.md), [Hooks: macOS and Bash 3.2](mistakes/hooks-macos-and-bash32.md), [Lexical Hooks Blocked Legitimate Work](mistakes/shell-command-matchers.md#lexical-hooks-blocked-legitimate-work-and-were-routed-around), [Hook-Guard Delivery Slips](mistakes/shell-command-matchers.md#hook-guard-delivery-slips-ledger-parser-template-trim-gate-coverage-spawner-list)
````

Commands:

```sh
F=doc/loom/knowledge/mistakes/hooks-shell-portability.md
{ for h in 'BSD `wc` Padding and Bare `sha256sum` Skipped the Reviewer Harvest on macOS, and the Hook Suite Never Ran in CI (2026-10-06)' 'A Heredoc Inside `$( )` Parses Under Bash 5 and Fails Under Bash 3.2 (2026-10-13)' 'The First macOS Hook Run Found Bash 3.2 and BSD Faults the Linux Shims Hid (2026-10-13)' 'A Shim'"'"'s `--version` Probe Recursed Through a Test'"'"'s Wrapper and Killed the Linux Runner (2026-10-13)'; do python3 -I "$SEC" "$F" "$h"; echo; done; } > "$TMPDIR/.distill-body-hooks.md"
loom knowledge update mistakes/hooks-macos-and-bash32 - < "$TMPDIR/.distill-body-hooks.md"
loom knowledge annotate mistakes/hooks-macos-and-bash32 --blurb "macOS Bash 3.2 and BSD tool faults that Linux hook runs hid"
loom knowledge delete-section mistakes/hooks-shell-portability 'BSD `wc` Padding and Bare `sha256sum` Skipped the Reviewer Harvest on macOS, and the Hook Suite Never Ran in CI (2026-10-06)'
loom knowledge delete-section mistakes/hooks-shell-portability 'A Heredoc Inside `$( )` Parses Under Bash 5 and Fails Under Bash 3.2 (2026-10-13)'
loom knowledge delete-section mistakes/hooks-shell-portability 'The First macOS Hook Run Found Bash 3.2 and BSD Faults the Linux Shims Hid (2026-10-13)'
loom knowledge delete-section mistakes/hooks-shell-portability 'A Shim'"'"'s `--version` Probe Recursed Through a Test'"'"'s Wrapper and Killed the Linux Runner (2026-10-13)'
```

### 31. concerns/platform-and-commit-gaps.md#macOS CI Jobs and Remaining BSD Gaps

Stale claim: A link points at `hooks-shell-portability.md#the-first-macos-hook-run-found-bash-32-and-bsd-faults-the-linux-shims-hid-2026-10-13`; the section moves to `hooks-macos-and-bash32.md` in item 30.
Tree today: After item 30 the anchor exists only in `mistakes/hooks-macos-and-bash32.md`. The link text and every other character of the section stay as they are; only the file part of one link changes. Do this item after item 30.
Action:

```sh
loom knowledge replace-section concerns/platform-and-commit-gaps 'macOS CI Jobs and Remaining BSD Gaps' - < "$TMPDIR/.distill-body-31.md"
```
Lines: `concerns/platform-and-commit-gaps.md` 249 -> 249 after this item.

New body (write to `$TMPDIR/.distill-body-31.md`, without the fence lines):

````markdown
Every gate whose result depends on the target OS runs on `macos-latest` as well as Linux. `.github/workflows/ci.yml`
runs `lint-macos` (clippy, rustdoc), `flake-check-macos`, and `hooks-macos` (hook syntax, the hook suite
and the partial-staging regression under `/bin/bash` 3.2, which a step asserts) on every push;
`.github/workflows/nightly.yml` runs `test-macos` as lib, integration and host-job-lane jobs, plus both smoke scripts,
because its build swaps on the push path ([CI macOS Test Build Time](ci-macos-test-build-time.md)).
fmt, audit, deny and the web job stay Linux-only.
Related portability lessons: [The First macOS Hook Run](../mistakes/hooks-macos-and-bash32.md#the-first-macos-hook-run-found-bash-32-and-bsd-faults-the-linux-shims-hid-2026-10-13),
[Rust Fixtures Assumed Linux](../mistakes/ci-toolchain-and-cargo.md#rust-fixtures-assumed-linux-paths-ptys-and-tool-output-2026-10-13)
and [macOS `proc_pidinfo` Hid Zombies](../mistakes/sessions-and-liveness.md#macos-proc_pidinfo-hid-zombies-so-an-unreaped-dead-session-read-as-alive-2026-10-13).
Open gaps:

- `run-all.sh` hides a passing test's output, so on the macOS runner a SKIP of the E3 Seatbelt case in
  `codex-forward-guard-stage-evidence.sh` cannot be ruled out. E3 relies on macOS refusing a nested
  profile that differs from the outer one; an identical nested profile is admitted, so the guard's
  allow-everything probe only detects confinement by a different outer profile.
- No hook step caps `ulimit -u`, so a runaway in a hook test kills the runner and its log, as a
  BSD-shim recursion once did to the Linux `Hook syntax` job.
- Inside Claude Code's macOS sandbox, `ps`, `sysctl` (the boot ID), `sandbox-exec`, PTY spawn, loopback
  bind and writes under `/var/folders` are denied, so the codex_evidence, worker_evidence, relay e2e,
  PTY, credential-guard and ancestry tests fail or skip there for that reason alone. A local run is no
  evidence either way; run them outside the sandbox (the `!` prefix) or read the CI job log with
  `gh api repos/cosmix/loom/actions/jobs/<job-id>/logs`.
- Unverified: `reject_symlink_components` (`subagent_lifecycle/claude_files.rs`) refuses an evidence
  path with any symlinked component. If a real transcript path runs through a symlinked `~/.claude`
  (dotfile managers create one), production refuses that evidence as unsafe.
- The smoke scripts run under the runner's Homebrew bash 5, never under bash 3.2.
- `loom-hooks/tests/run-all.sh` also runs in the Linux `hook-syntax` job twice, plain and with
  `LOOM_HOOK_TEST_BSD=1`, which puts `loom-hooks/tests/bsd-shims/` (padded `wc`, BSD `stat`, BSD `date`)
  first on PATH. A shim finds the real tool among the PATH entries after its own position (then
  `/usr/bin:/bin`), skipping copies that carry the `# loom-bsd-shim` marker line. A wrapper earlier on
  PATH that hands calls back to the shim is therefore never chosen. `LOOM_BSD_SHIM_DEPTH` stops a
  re-entered shim with exit 125 at depth 3. When the real tool already rejects `--version` (BSD, as on
  macOS), the shim execs it unchanged. `scripts/check-hook-syntax.sh` parses the
  extensionless shims too.
- `knowledge-orient-emits.sh` asserts `3 entries` but not the byte count, so dropping the padded-`wc`
  strip in `knowledge-orient.sh` survives under the shim (cosmetic).
- Hook stdin reads use `loom_read_input` from `loom-hooks/_bounded.sh::loom_read_input`; its portable
  `loom_run_bounded` fallback provides a deadline when GNU timeout is absent. Separate direct
  command-execution fallbacks remain in `user-prompt-context.sh:77-86`,
  `codex-apply-patch.sh:97-103` and `loom-relay.sh:196-204`; audit those independently.
- Bash 3.2 writes every here-string and here-document to a temp file. It tries `$TMPDIR`, `/tmp`,
  `/var/tmp`, `/usr/tmp`, then the current directory, and leaves an `sh-thd-*` file behind when
  killed mid-command. Only when none is writable does the redirect fail ("cannot create temp file for
  here document"). `set -e` does not stop the script then; the command just reads nothing.
  `codex-apply-patch.sh` (`done <<<"$PATCH"`) then sees no targets and blocks in `pre` mode, so it fails
  closed.
````

### 32. mistakes/subagent-briefing.md#Over-Scoped Fix Briefs Hit the Turn Cap Again, and Four Parallel Test Runs Exhausted Process Slots (2026-10-13)

Stale claim: A link points at `hooks-shell-portability.md#a-shims-version-probe-recursed-through-a-tests-wrapper-and-killed-the-linux-runner-2026-10-13`; the section moves to `hooks-macos-and-bash32.md` in item 30.
Tree today: After item 30 the anchor exists only in `mistakes/hooks-macos-and-bash32.md`. The link text and every other character of the section stay as they are; only the file part of one link changes. Do this item after item 30.
Action:

```sh
loom knowledge replace-section mistakes/subagent-briefing 'Over-Scoped Fix Briefs Hit the Turn Cap Again, and Four Parallel Test Runs Exhausted Process Slots (2026-10-13)' - < "$TMPDIR/.distill-body-32.md"
```
Lines: `mistakes/subagent-briefing.md` 394 -> 394 after this item.

New body (write to `$TMPDIR/.distill-body-32.md`, without the fence lines):

````markdown
**What happened:** fixing the macOS CI failures, the orchestrator spawned four debugging subagents at
once. Two stopped at `maxTurns` 150 without a report: one was briefed with eight failure clusters
(45 tests), the other with five items across a dozen files. That repeats
[the 2026-09-13 entry](#a-brief-with-5-6-tasks-across-a-dozen-files-overruns-the-agents-150-turn-limit-2026-09-13).
Their trees were complete, but only a diff read showed it. During the same run the shells of several
subagents got `fork: Resource temporarily unavailable` for minutes: EAGAIN from the per-user process
limit (`ulimit -u` 10666 there), not open files (that is EMFILE). All four were building and running
cargo tests and hook suites. The sandbox denies `ps`, so the cause was never observed. The probable
cause is a worker's new shim `--version` probe recursing through a test's PATH wrapper, which later
killed a Linux CI runner; see
[A Shim's `--version` Probe Recursed](hooks-macos-and-bash32.md#a-shims-version-probe-recursed-through-a-tests-wrapper-and-killed-the-linux-runner-2026-10-13).
Parallel test runs made the loop's effect worse rather than causing it. One brief also told a subagent
that a test "passes locally" when it had only SKIPPED.

**Why:** the briefs were sized by failure count, not by the distinct causes each needed to debug, and
debugging subagents each ran their own test suites in parallel.

**Prevention:** keep a debugging brief to about three causes. Run at most two test-running subagents
at once on one host. Ask every worker to keep a running list of files changed, so a cut-off run still
reports. Read SKIP lines before calling a test green. The cap now sits at `maxTurns: 1000` in
`agents/*.md`, so the turn cap no longer forces a split; context still does.

**Fix:** the stopped workers' diffs were reviewed and verified by the orchestrator instead of re-spawning.
````

### 33. mistakes/subagent-liveness-and-watch.md#SubagentStop Delivery Is Intermittent, and One Reviewer Can Record Two Rounds (2026-10-03)

Stale claim: A link points at `hooks-shell-portability.md#bsd-wc-padding-and-bare-sha256sum-skipped-the-reviewer-harvest-on-macos-and-the-hook-suite-never-ran-in-ci-2026-10-06`; the section moves to `hooks-macos-and-bash32.md` in item 30.
Tree today: After item 30 the anchor exists only in `mistakes/hooks-macos-and-bash32.md`. The link text and every other character of the section stay as they are; only the file part of one link changes. Do this item after item 30.
Action:

```sh
loom knowledge replace-section mistakes/subagent-liveness-and-watch 'SubagentStop Delivery Is Intermittent, and One Reviewer Can Record Two Rounds (2026-10-03)' - < "$TMPDIR/.distill-body-33.md"
```
Lines: `mistakes/subagent-liveness-and-watch.md` 387 -> 387 after this item.

New body (write to `$TMPDIR/.distill-body-33.md`, without the fence lines):

````markdown
**What happened:** in the target-guard plan, `loom subagents watch` exited 6 (hung) for an engineer that had already ended through a `SubagentHandback` call, and the first reviewer's round was never recorded (no `SubagentStop` lifecycle row) while the second reviewer's was. In integration-verify one reviewer spawn was recorded as two review rounds (identical 16 findings under `F-1-*` and `F-3-*`), leaving every finding open twice.

**Why:** a hand-back ending does not reliably deliver `SubagentStop` (replaying `subagent-stop.sh` with a well-formed payload against a scratch work dir reaches the harvest with GNU tools, but the hook logic was sound only there: on macOS a padded `wc` count made it skip silently, see [BSD `wc` Padding and Bare `sha256sum`](hooks-macos-and-bash32.md#bsd-wc-padding-and-bare-sha256sum-skipped-the-reviewer-harvest-on-macos-and-the-hook-suite-never-ran-in-ci-2026-10-06)); the double record is probably the hand-back record and the final-message record both harvesting.

**Prevention:** treat an exit 6 for a worker whose hand-back report arrived as returned, not failed. Re-review briefs paste the full open-id list from `loom stage review status`, and name both id sets under `resolved` when one reviewer is recorded twice. The commit-filter hook blocks any `git commit` string from a subagent, even in a scratch repository under `TMPDIR`, so a brief must not ask a subagent to probe commit semantics by shell.
````

### 34. mistakes.md#three full-form sections and "Sandbox Route Closure"

Stale claim: `mistakes.md` is 274 lines, over the 250-line tier-1 limit (`loom knowledge check --strict`: `tier-1 file is 274 lines`).
Tree today: A tier-1 file holds summaries and pointers; three sections are full what/why/prevention/fix entries (24 lines) and belong in `mistakes/plan-authoring.md` (323 lines, becomes 347). They are replaced by one section of three one-line bullets. "Sandbox Route Closure" (8 lines) holds only pointers to other topics; it moves to `mistakes/planning-sandbox-delivery.md` under a heading that says what it is, with the links made relative to the `mistakes/` directory. All the pointed-to topics stay listed in `INDEX.md`. The new tier-1 file is 248 lines.
Action: move (copy the sections to the destination with `update`, then `delete-section` them at the source). The helper script `$SEC` is written once under "Helper" at the end of this file. Headings, exact:

```text
Brief-path extractor fed markdown into the run glob
Treating this repository's conventions as loom's product behaviour
Plan Amendment Strips completed_commit From a Merged Stage
Sandbox Route Closure
```

Destinations: the first three go to `mistakes/plan-authoring`; `Sandbox Route Closure` goes to `mistakes/planning-sandbox-delivery` under the new heading `Sandbox Route and Autonomy Entries Recorded in Other Topics`.
Expected line counts after the move: `mistakes.md` 274 -> 248 lines; `mistakes/plan-authoring.md` 323 -> 347 lines; `mistakes/planning-sandbox-delivery.md` 90 -> 98 lines.

First write this body to `$TMPDIR/.distill-body-newsec.md` (the one tier-1 section that replaces the three full-form entries):

````markdown
## Plan Input, Convention and Amendment Slips

- [Brief-path extractor fed markdown into the run glob](mistakes/plan-authoring.md#brief-path-extractor-fed-markdown-into-the-run-glob): a plan that `loom plan verify --strict` accepted failed `loom run`'s input discovery. Rule: a check that accepts a plan runs the same discovery every later consumer runs.
- [Treating this repository's conventions as loom's product behaviour](mistakes/plan-authoring.md#treating-this-repositorys-conventions-as-looms-product-behaviour): this repository's `.gitignore` and plan habits are not loom's behaviour in a target project. Rule: find the code that does it before stating what loom does.
- [Plan Amendment Strips completed_commit From a Merged Stage](mistakes/plan-authoring.md#plan-amendment-strips-completed_commit-from-a-merged-stage): an amendment cleared `completed_commit` on a merged stage and left its dependent queued. Rule: never clear it on a stage with `merged` or `merge_assumed`.
````

Commands:

```sh
F=doc/loom/knowledge/mistakes.md
{ for h in 'Brief-path extractor fed markdown into the run glob' 'Treating this repository'"'"'s conventions as loom'"'"'s product behaviour' 'Plan Amendment Strips completed_commit From a Merged Stage'; do python3 -I "$SEC" "$F" "$h"; echo; done; } > "$TMPDIR/.distill-body-pa.md"
loom knowledge update mistakes/plan-authoring - < "$TMPDIR/.distill-body-pa.md"
loom knowledge delete-section mistakes 'Brief-path extractor fed markdown into the run glob'
loom knowledge delete-section mistakes 'Treating this repository'"'"'s conventions as loom'"'"'s product behaviour'
loom knowledge delete-section mistakes 'Plan Amendment Strips completed_commit From a Merged Stage'
loom knowledge update mistakes - < "$TMPDIR/.distill-body-newsec.md"
python3 -I "$SEC" --body doc/loom/knowledge/mistakes.md 'Sandbox Route Closure' > "$TMPDIR/.distill-body-src.md"
python3 -I - <<'PY'
import os
t = os.environ["TMPDIR"]
body = open(f"{t}/.distill-body-src.md").read().replace("](mistakes/", "](")
open(f"{t}/.distill-body-src2.md", "w").write("## Sandbox Route and Autonomy Entries Recorded in Other Topics\n\n" + body.strip() + "\n")
PY
loom knowledge update mistakes/planning-sandbox-delivery - < "$TMPDIR/.distill-body-src2.md"
loom knowledge delete-section mistakes 'Sandbox Route Closure'
```

### 35. mistakes/knowledge-base-drift.md (new entry)

Stale claim: There is no mistakes entry for the drift this stage corrects.
Tree today: `mistakes/knowledge-base-drift.md` is 276 lines and holds the related entries (`## Knowledge Base Drift` in `mistakes.md` points at it); history belongs only in the mistakes files, so the corrections above carry none.
Action (expect `Appended`):

```sh
loom knowledge update mistakes/knowledge-base-drift - < "$TMPDIR/.distill-body-35.md"
```
Lines: `mistakes/knowledge-base-drift.md` 276 -> 286 after this item.
History note for the mistakes topic `mistakes/knowledge-base-drift`; it records the drift, the corrections above carry none. Text, heading line included:

New body (write to `$TMPDIR/.distill-body-35.md`, without the fence lines):

````markdown
## Concern Entries Outlived the Fix They Described (2026-11-23)

**What happened:** a review found 14 knowledge sections that still described closed work: a layering list naming imports that had moved, a housekeeping section stating that `loom knowledge check --strict` exits 0 while it exited 1, an unbounded-read concern that `SourceRead` had closed, a dry-run gap fixed in `apply_confined_environment`, a journal-splitting gap closed by `escape_body`, and body-deadline and raw-string claims in the planner concerns. The correction pass found the same wrong claim in a second home more than once: the "exits 0" claim in three files, the body-reading claim in two sections of `architecture/plan-server.md`, the layering list in two files.

**Why:** the stages that fixed the code recorded no `stale-knowledge` memory, so no distill stage touched the concern, and `loom knowledge check` validates sizes and source paths, not claims. Counts, versions and "exits 0" statements go stale first.

**Prevention:** a stage that closes a concern records `loom memory note "stale-knowledge: ..."` in the same stage; a correction searches the knowledge base for the claim's other homes (`rg` on the symbol and on the number) before it finishes; prose carries no exact line count or crate version without a source reference to check it against.

**Fix:** the sections were corrected from the tree in the `knowledge-repair` stage, each verified at a file and line.
````

## Closing steps

1. `loom knowledge sync` (regenerates `INDEX.md` for the new topic and the changed blurbs; read its last lines for errors).
2. `loom knowledge check --strict`. Expected in the main checkout: exactly one issue line, `! INDEX.md is <bytes> bytes, over the 16384-byte budget - trim it`, and `1 issue(s)`. The index budget is the one failure this stage leaves; a code change elsewhere in the plan handles it. Any other `!` line is a defect of this work: fix it with the CLI and rerun. Plain `loom knowledge check` (without `--strict`) must exit 0.
3. Regenerate `doc/loom/knowledge/check-baseline.txt` from a clean clone, not from the working checkout. Several plan briefs cited by knowledge files (the design brief of the trace screens redesign and seven more) exist only as untracked files here, so `loom knowledge check --write-baseline` run in this checkout would drop their `missing-source-ref` lines, and a clean clone (CI) would then report them as new issues. Writing from a clone keeps them:

```sh
CLONE="$(mktemp -d)"
git clone -q --no-hardlinks "$PWD" "$CLONE"
cp -R doc/loom/knowledge/. "$CLONE/doc/loom/knowledge/"
( cd "$CLONE" && loom knowledge check --write-baseline doc/loom/knowledge/check-baseline.txt )
cp "$CLONE/doc/loom/knowledge/check-baseline.txt" doc/loom/knowledge/check-baseline.txt
```

   The `cp -R` overlay carries the corrected tree (including the regenerated `INDEX.md`) into the clone; leave `$CLONE` in place. Expected result: 8 `missing-source-ref` lines (planning-trace, handoff-followups, native-codex-qualification, planning-session-sandbox-limits, source-graph-review-backlog, token-accounting-and-proof-defects, planner-design-brainstorm, pre-commit-hardening) and `oversized-index INDEX.md`; no `oversized-file` line and no `mistakes/sandbox-hook-preflight.md` line. Verified on a scratch run of this list.
4. Confirm: `loom knowledge check --strict --baseline doc/loom/knowledge/check-baseline.txt` exits 0, and `wc -l doc/loom/knowledge/concerns.md doc/loom/knowledge/mistakes.md doc/loom/knowledge/mistakes/hooks-shell-portability.md doc/loom/knowledge/mistakes/testing-and-lint.md` shows at most 250, 250, 400 and 400 (expected 245, 248, 319 and 377).
5. `loom memory note` anything this list could not correct, naming the item number and the reason. Do not edit knowledge files with file tools to work around a CLI refusal; report it instead.

Not in this list: the `INDEX.md` byte budget (a code change elsewhere in the plan), the layering edges that later stages move in code (the plan's final knowledge stage rewrites item 5 once they land), and the constants and counts in `architecture/knowledge-hierarchy.md` (they follow the same code change).

## Helper

Read-only section extractor used by the move items. Write it to `$TMPDIR/knowledge_sec.py` with the Write tool and set `SEC="$TMPDIR/knowledge_sec.py"` in the shell that runs the move commands. It prints one section (heading line included; add `--body` to drop the heading line) from a file, skipping fenced code the way the CLI does, and writes the line span to stderr. It never writes to the knowledge directory.

```python
"""Fence-aware section extraction: sec.py <file> <heading> -> prints the full section (heading line included) or --body."""
import re, sys

def sections(lines):
    fence = None
    out = []
    for i, line in enumerate(lines):
        m = re.match(r'^ {0,3}(`{3,}|~{3,})', line)
        if m:
            tick = m.group(1)
            if fence is None:
                fence = tick
            elif tick[0] == fence[0] and len(tick) >= len(fence):
                fence = None
            continue
        if fence is None:
            h = re.match(r'^(#{1,6}) +(.*?) *$', line)
            if h:
                out.append((i, len(h.group(1)), h.group(2)))
    return out

def extract(path, heading, body=False):
    lines = open(path, encoding='utf-8').read().split('\n')
    secs = sections(lines)
    for idx, (i, lvl, text) in enumerate(secs):
        if text == heading:
            end = len(lines)
            for j, l2, _ in secs[idx + 1:]:
                if l2 <= lvl:
                    end = j
                    break
            chunk = lines[i + (1 if body else 0):end]
            while chunk and chunk[-1] == '':
                chunk.pop()
            while body and chunk and chunk[0] == '':
                chunk.pop(0)
            return '\n'.join(chunk), (i + 1, end)
    raise SystemExit(f'heading not found: {heading!r} in {path}')

if __name__ == '__main__':
    body = '--body' in sys.argv
    args = [a for a in sys.argv[1:] if a != '--body']
    text, span = extract(args[0], args[1], body)
    print(text)
    print(f'[lines {span[0]}-{span[1]}]', file=sys.stderr)
```
