# Automatic Knowledge Source Graph Followups

> Topic notes for the concerns knowledge area.

## Open After PLAN-automatic-knowledge-and-source-graph (2026-08-18)

**Whole-file read ahead of the size cap.** `context/refresh/source_graph.rs:228` does
`fs::read` on every tracked file BEFORE `extract_file` applies the 512 KiB
`MAX_EXTRACTED_FILE_BYTES` cap, so the cap bounds parsing but not allocation, and the
daemon spikes to the size of the largest tracked blob on every merge reconcile.
Deliberately not fixed at the quality gate: `FileExtraction::file_level`
(`extract/mod.rs:103`) needs the BYTES to build the file node's span, so avoiding the
read means changing the oversized node's span semantics or threading a streamed line
count through the extractor API — a hot-path refactor. Peak is one file at a time and
`EXCLUDED_ROOTS` already skips `target/` and `node_modules/`, so the realistic worst
case is a transient spike, not corruption.

**Four production-dead `KnowledgeDir` methods.** Deleting `loom knowledge show`/`list`
orphaned part of the read/replace side: `read` (`dir.rs:120`), `append` (`dir.rs:127`),
`read_index` (`dir.rs:160`), and `replace_section` (`dir.rs:136`, the
`KnowledgeFile`-keyed variant) have no non-test callers, and all are `pub` on a `pub`
type so clippy cannot see them. They were kept because ~15 tests in `tests_dir.rs`
exercise them against each other (append → read, replace_section → read), so deleting
the methods deletes most of that file's coverage. **Settle them deliberately in one
follow-up: either delete methods and tests together, or wire them to a real consumer.**
General rule: when a stage deletes a read-side CLI verb, audit every accessor that verb
was the last caller of — and when a brief justifies keeping a module by naming a
caller, check whether that caller is itself reachable. A wrapper is not a consumer. The
converse also held here: `loom knowledge replace-section` was restored as a live CLI
verb (`cli/types_memory.rs:19`, `cli/dispatch.rs:84-88`, `commands/knowledge/mod.rs:115`),
which revived two of the original six dead methods — `read_target` (`dir.rs:176`, now
called at `commands/knowledge/mod.rs:126`) and `replace_section_target` (`dir.rs:212`,
now called at `commands/knowledge/mod.rs:130`). A dead-accessor list like this one is
only true against one revision; re-check it before trusting it.

**Plan-key normalisation on the writer side.** `delivery::plan_key` resolves both a blank
`plan_id` in `.work/config.toml` and a stage record with no plan to `"default"`;
`MergeLifecycle`'s writer side does not normalise identically. Silent by construction —
see `mistakes/writer-reader-address.md`.

**A permission deny now reaches child processes.** The knowledge tree is denied to the
agent AND to the `loom` binary the doctrine tells agents to use. See Part C of the
pending-knowledge document, and `concerns/sandbox-write-rules-inert.md` for the history.

**`fs/permissions/constants.rs`** still declares `LOOM_PERMISSIONS_WORKTREE` with
`Write(.work/**)` / `Bash(loom *)` rules that read like a blanket grant but have no real
consumers, and `Write(path)` rules are inert anyway. A documented fossil.
