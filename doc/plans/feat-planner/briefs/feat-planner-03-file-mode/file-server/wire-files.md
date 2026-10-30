# file-server / wire-files: spec for codex units F1, F2

Lane: codex (`codex-units.md`). F1 owns `loom/src/plan_server/wire/files.rs` (parity tests inline
in a `#[cfg(test)] mod tests`, mirroring plan 2's `wire/server.rs`) and the `files-*.json`
fixtures; F2 owns `web/src/plan/api/schema-files.ts` and `schema-files.test.ts`. The orchestrator
declares `pub mod files;` in `wire/mod.rs` and, after F1, adds the three `PlanMessage` variants to
`wire/message.rs` and the union in `web/src/plan/api/schema.ts`. Read
`doc/plans/briefs/feat-planner-03-file-mode/common.md` first, then the parity pattern of plan 2:
`loom/src/plan_server/wire/{message.rs, server.rs, tests_schema.rs}`,
`web/src/plan/api/{schema.ts, schema.test.ts}`, `web/src/plan/api/fixtures/`.

## Types (`wire/files.rs`; serde, `deny_unknown_fields`, snake_case)

```text
FileTree { product: Vec<TreeEntry>, plans: Vec<TreeEntry>, knowledge: Vec<TreeEntry>, other: Vec<TreeEntry>, archive: Vec<TreeEntry> }
All `path` fields below are repo-relative (`doc/...`), per plan 2's wire path rule; only the
HTTP query parameters of plan 3's routes are relative to `doc/`.

TreeEntry { path: Option<String> /* repo-relative; None for a gap or folder */, title: String, file_name: Option<String>,
            kind: EntryKind, state: Option<PlanStateView>, locked: bool, gap: bool, pressure_date: Option<String>,
            errors: u32, blurb: Option<String>, children: Vec<TreeEntry> }
EntryKind { Vision, Section, Note, Metaplan, MetaplanEntry, Plan, Brief, BriefFolder, Knowledge, KnowledgeFolder, Review, Other }
PlanStateView { Written, Running, Done, Retired }
DocView { path: String, title: String, text: String, hash: String, kind: EntryKind, editable: bool, lock: Option<LockView>,
          sections: Vec<SectionView>, plan: Option<PlanView>, too_large: bool, truncated: bool, size: u64 }
          /* too_large: over 1 MiB; then `text` holds only the first 1 MiB (cut on a char boundary) and truncated is true */
SectionView { level: u8, title: String, anchor: String, start_line: u32 /* 1-based */, end_line: u32 /* inclusive */ }
LockView { reason: LockKind, notice: String, plan: Option<String> }    LockKind { Running, Done, Archive, BriefOfLockedPlan, Knowledge, Summaries }
PlanView { slug: String, state: PlanStateView, implements: Vec<ImplementsLink>, pressure_date: Option<String>, last_change: Option<String>,
           validation: Option<ValidationView> /* None for a locked plan */, stages: Vec<StageRow>, yaml: Option<String>, yaml_error: Option<String>,
           metadata_line: Option<u32> }
ImplementsLink { number: u32, path: Option<String> /* None = no such section */ }
StageRow { id: String, name: String, stage_type: String, summary: Option<String>, dependencies: Vec<String>, description: String,
           acceptance: Vec<String>, files: Vec<String>, line: Option<u32> /* the `- id:` line, 1-based */ }
ValidationView { valid: bool, errors: Vec<ValidationItem>, warnings: ValidationWarnings, notes: Vec<String>, metadata: MetadataView }
ValidationItem { message: String, stage_id: Option<String>, line: Option<u32>, column: Option<u32> }
ValidationWarnings { structural: Vec<String>, knowledge: Vec<String>, sandbox: Vec<String>, baseline: Vec<String> }
MetadataView { Present, Missing }
SaveResult (tag "result"): Saved { hash: String, validation: Option<ValidationView> } | Conflict { current_text: String, current_hash: String }
                           | Invalid { validation: ValidationView }
DiffTexts { path: String, before: Option<String>, after: Option<String> }
SearchResults { hits: Vec<SearchHit>, truncated: bool }
SearchHit { path: String, title: String, heading_path: Vec<String>, anchor: String, snippet: String, terms: Vec<String> }
DocChanged { path: String, hash: Option<String> /* None = removed */ }
ValidationChanged { path: String, validation: ValidationView }
```

`PlanMessage` gains `TreeChanged(FileTree)`, `DocChanged(DocChanged)`, `Validation(ValidationChanged)`
(wire `type` values `tree_changed`, `doc_changed`, `validation`).

## Fixtures and tests

One fixture per top-level type (`files-tree.json` with every group and an archive orphan bundle
and a gap entry; `files-doc.json` a section; `files-doc-plan.json` a plan with two stages, one
validation error with a stage id and line, and a YAML block; `files-save-saved.json`,
`files-save-conflict.json`, `files-save-invalid.json`; `files-diff.json`; `files-search.json`;
`files-doc-changed.json`; `files-validation.json`; `files-tree-changed.json`). Rust tests inline
in `wire/files.rs` follow plan 2's `wire/server.rs` (`<name>_fixture_matches_serde_output`) and the
enum-values parity of `tests_schema.rs` against `schema-files.ts`'s exported const arrays. zod
schemas in `schema-files.ts` (`z.strictObject`), exporting `filesMessageSchemas` for the
orchestrator's union edit in plan 2's `web/src/plan/api/schema.ts` and `PLAN_MESSAGE_TYPES`. `schema-files.test.ts` parses every `files-*` fixture.

## Done when

`cargo test --manifest-path loom/Cargo.toml --lib plan_server::wire::` and
`cd web && node_modules/.bin/vitest run src/plan/api` pass.
