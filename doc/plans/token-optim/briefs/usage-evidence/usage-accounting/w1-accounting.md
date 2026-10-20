# W1: accounting (discovery, instants, completeness, Codex scope)

Tier: sonnet (`loom-software-engineer`). Read `common.md` in this directory first. You own every file listed below and nothing else.

## Files you own

- `src/commands/usage/args.rs` (new)
- `src/commands/usage/mod.rs`
- `src/commands/usage/time_range.rs`, `src/commands/usage/time_range_tests.rs`
- `src/commands/usage/discovery.rs`, `src/commands/usage/discovery_tests.rs`
- `src/commands/usage/completeness.rs` (new), `src/commands/usage/completeness_tests.rs` (new)
- `src/commands/usage/codex_scope.rs` (new), `src/commands/usage/codex_scope_tests.rs` (new)
- `src/commands/usage/provider.rs`, `src/commands/usage/provider_types.rs`
- `src/commands/usage/provider_report.rs`, `src/commands/usage/provider_report_tests.rs`
- `src/commands/usage/forward_join.rs`
- `src/commands/usage/usage_tests.rs`, `src/commands/usage/quota_history_tests.rs` (only the `ProviderEventInput` literals)

Read-only: `src/fs/session_files/enumerate.rs`, `src/parser/frontmatter.rs`, `src/models/session/types.rs`, `src/provider_data/resolve.rs` (`project_slug`), `src/models/forward_receipt.rs`.

## Step 1: move the arguments out of `usage/mod.rs` first

`usage/mod.rs` is 392 lines; everything below would push it past 400. Move `ProviderSelection` with its `impl` (`mod.rs:55-72`) and `UsageArgs` with its `impl` (`:74-163`) verbatim into `args.rs`, with the `use` lines they need. In `mod.rs` add `mod args;` and `pub use args::{ProviderSelection, UsageArgs};`. Make `ProviderSelection::includes`, `UsageArgs::since`, `::provider`, `::windows` and `::has_explicit_selection` `pub(super)`. Every existing caller keeps its path (`crate::commands::usage::UsageArgs` in the CLI, `super::UsageArgs` in `comparison.rs`, `super::ProviderSelection` in `provider.rs`). Build before going on.

## Step 2: `--since` accepts an instant

In `time_range.rs`, `parse_since_at` (`:26-35`) tries `DateTime::parse_from_rfc3339(spec)` first and returns it in UTC; the date and duration forms follow unchanged. `parse_until` (`:37-42`) already accepts only RFC3339 and stays as it is. Update the `since` doc comment on `UsageArgs` (`args.rs`, the line reading "a duration (`7d`, `24h`, `30m`) or an ISO date") to name the instant form. Add tests to `time_range_tests.rs`: `2026-10-14T06:30:00Z` and `2026-10-14T09:30:00+03:00` both parse to the same instant; `2026-10-14` still means midnight UTC; `7d` still subtracts from `now`.

## Step 3: discovery reads removed worktree slugs

Stage sessions write under `<slug>--worktrees-<stage-id>` (`provider_data/resolve.rs:100-105`, `project_slug` maps `/` and `.` to `-`, so `<repo>/.worktrees/<id>` becomes `<slug>--worktrees-<id>`), and loom removes the worktree after merge. In `discovery.rs::selected_slugs` (`:101-120`):

- Add `fn worktree_slugs(root: &Path, slug: &str) -> Vec<String>`: every directory entry of `root` (the Claude projects root) whose name starts with `format!("{slug}--worktrees-")`, sorted. A read error yields an empty list.
- `--project <dir>`: return the project slug followed by `worktree_slugs(root, &slug)`.
- The default branch: the repository slug, the live `.worktrees/*` slugs it already collects, and `worktree_slugs(root, &repo_slug)`, deduplicated in that order.
- `--all` is unchanged. `filter_stage` (`:231-237`, suffix `-worktrees-{stage}`) and `filter_plan` keep working because `--worktrees-<id>` ends with `-worktrees-<id>`.

The separator `--worktrees-` matters: a sibling project `<slug>-other` must not be selected. Tests in `discovery_tests.rs`: a project slug, a removed worktree slug and a `<slug>-other` sibling under a fixture root; only the first two are selected, for both the `--project` and the default branch.

## Step 4: completeness (`completeness.rs`)

Pinned in `common.md` section 3 and 4. Two functions:

```rust
pub(crate) fn missed_claude_transcripts(
    work_dir: Option<&Path>,
    read: &[PathBuf],
    range: &TimeRange,
) -> usize
pub(crate) fn assess(coverage: &CoverageCounts, diagnostics: &ProviderDiagnostics) -> Completeness
```

- `missed_claude_transcripts`: `None` work dir returns 0. Enumerate `crate::fs::session_files::enumerate_sessions(work_dir)` (an error returns 0 with an stderr warning). For each record, read `record.path` with `std::fs::read_to_string` and parse it with `crate::parser::frontmatter::parse_from_markdown::<crate::models::session::Session>(&content, "Session")`; never call `load_session_exact`, which takes the session-file lock and would write a lock file from a read-only report. A record that fails to parse is skipped. Count a session when its `transcript_path` is `Some`, its activity overlaps the window (`last_active >= range.since` and, when `range.until` is set, `created_at <= until`), and the path is not among `read`. Compare paths literally, then by `canonicalize` when both exist.
- `assess` pushes, in this order: `missing-roots` (`diagnostics.missing_roots > 0`), `unreadable-files`, `missed-session-transcripts`, `unknown-usage-rows` (`coverage.unknown_usage_rows`), `ambiguous-rows`, `fallback-coverage-rows`, `codex-scope-unavailable`, `forward-thread-missing` (`missing_forward_threads`), `forward-receipt-unthreaded`. `state` is `Partial` iff any reason was pushed.

`provider_types.rs`: `ProviderDiagnostics` (`:188-218`) gains `missed_session_transcripts`, `foreign_codex_rows`, `missing_forward_threads`, `unthreaded_forward_receipts`, `codex_scope_unavailable` (`usize`), each added in `merge` (`:221-253`). `ProviderReport` (`:284-297`) gains `completeness`. `provider_report.rs::build` (`:23-65`) sets `completeness: completeness::assess(&coverage, &diagnostics)` before moving `diagnostics` into the report; `render` (`:225-256`) prints one row per provider, `"{provider} completeness"` with `measured` or `partial: <reasons joined by ", ">`.

## Step 5: Codex scope (`codex_scope.rs`, `forward_join.rs`, `provider.rs`)

Today Codex discovery receives only `codex_root` (`mod.rs:287-290`), every Codex row in the window enters normalization (`provider.rs:44-49`), and `forward_join.rs::join_ledger` (`:99-117`) only attaches labels, after the reports are built. Membership becomes a filter applied before the reports are built.

`forward_join.rs` (351 lines; keep it under 380): add

```rust
#[derive(Debug, Clone)]
pub(crate) struct ReceiptThread {
    pub(crate) thread_id: Option<String>,
    pub(crate) observed_at: DateTime<Utc>,
}
```

a `receipt_threads: Vec<ReceiptThread>` field filled in `ForwardJoin::load` from every loaded receipt (`receipt_thread_id(&receipt)`, `receipt.observed_at`), and two accessors: `pub(crate) fn receipt_threads(&self) -> &[ReceiptThread]` and `pub(crate) fn scope_available(&self) -> bool`, true when `load_diagnostics` has zero `forward_receipt_scope_unavailable`, `missing_forward_receipt_roots`, `truncated_forward_receipt_files` and `unreadable_files`.

`codex_scope.rs`:

```rust
pub(crate) enum CodexScope {
    Unscoped,
    Unavailable,
    Threads { threads: BTreeSet<String>, unthreaded: usize },
}
impl CodexScope {
    pub(crate) fn for_selection(all: bool, join: &ForwardJoin, range: &TimeRange) -> Self
}
#[derive(Debug, Default, PartialEq, Eq)]
pub(crate) struct ScopeOutcome {
    pub(crate) foreign_rows: usize,
    pub(crate) missing_threads: usize,
    pub(crate) unthreaded_receipts: usize,
    pub(crate) unavailable: bool,
}
pub(crate) fn apply(scope: &CodexScope, rows: &mut Vec<NormalizedEvent>) -> ScopeOutcome
```

- `for_selection`: `all` gives `Unscoped`; `!join.scope_available()` gives `Unavailable`; otherwise the thread ids of receipts whose `observed_at` lies in the window (`range.includes`), plus the count of in-window receipts with no thread id.
- `apply` touches only rows with `provider == Provider::Codex` and `source_kind` `CodexDirect` or `CodexFallback`; execution-receipt rows (`SourceKind::ExecutionReceipt`) stay. `Unscoped`: nothing. `Unavailable`: remove every such row, count each as foreign, set `unavailable`. `Threads`: keep a row when it has no `codex_thread_conflict` and its `codex_thread_id` is in the set; remove the rest as foreign; `missing_threads` counts set members no kept row carries. Retries need no rule: each forward call writes its own receipt and names its own thread.

`provider.rs`: `ProviderEventInput` (`:11-21`) gains `codex_scope: &'a CodexScope` and `claude_missed_transcripts: usize`. In `normalize_provider_events` (`:28-75`, 48 lines today) extract the Codex block into `fn normalize_codex(input: &ProviderEventInput<'_>) -> (Vec<NormalizedEvent>, ProviderDiagnostics)` that normalizes, calls `codex_scope::apply`, and records the outcome in `foreign_codex_rows`, `missing_forward_threads`, `unthreaded_forward_receipts`, `codex_scope_unavailable` (1 when unavailable). Set `claude_diagnostics.missed_session_transcripts = input.claude_missed_transcripts`. Keep both functions under 50 lines.

## Step 6: wire it in `usage/mod.rs`

- `normalize_usage` (`:257-305`, 49 lines): extract the Claude discovery block into a helper so the function stays under 50 lines. Build `let scope = codex_scope::CodexScope::for_selection(args.all, &forward_join, &range);` and pass `codex_scope: &scope`. When the selection includes Claude and `!args.all`, compute `completeness::missed_claude_transcripts(work_dir, &paths, &range)` from the discovered files' paths and pass it; otherwise 0.
- Declare `mod args; mod codex_scope; mod completeness; mod spawn_join;`.
- `parse_all` (`:167-195`): `let requested = spawn_join::RequestedModels::load(work_dir);` beside `started_agent_types`; inside the `if let Some(metadata)` branch set `transcript.requested_model = requested.requested_model(agent_id, &file.session_id).map(str::to_owned);` where `agent_id` is `file.agent_id`. `spawn_join.rs` is W4's file and the field is W3's; both are pinned in `common.md`.
- Update the two `ProviderEventInput` literals in `usage_tests.rs` (`provider_input`, `:272-283`) and `quota_history_tests.rs` (`:26`) with `codex_scope: &CodexScope::Unscoped` and `claude_missed_transcripts: 0`. Change no assertion line.

## Tests you add

- `completeness_tests.rs`: a session record naming an unread transcript inside the window counts 1; outside the window 0; a read path 0; an unparseable record 0; `assess` orders the reasons as pinned and reads `measured` with none.
- `codex_scope_tests.rs`: three Codex rows with thread ids `t-a`, `t-b`, `t-x` and a `Threads` scope of `{t-a, t-missing}` keep only `t-a`, report 2 foreign and 1 missing; `Unavailable` removes all three; `Unscoped` keeps all; an execution-receipt row survives every scope.
- `provider_report_tests.rs`: a report with an `unknown_usage_rows` coverage count reads `partial` with `unknown-usage-rows`.

Run once at the end: `cargo test --lib commands::usage::` and report the result with every file you changed and its line count after `cargo fmt`.

## Traps

- The contract tests drive the binary with `--claude-root` and `--codex-root` fixtures; they compute the project slug from the canonical project path (`absolute` in `discovery.rs:131-142` canonicalizes an existing path), so keep that canonicalization.
- `usage_work_dir` (`mod.rs:202-210`) returns `None` for `--project` without `.loom/work`; Codex scope then reads `Unavailable` and Codex reads `partial`. That is intended: a project without loom state cannot claim Codex rows. `--all` stays unscoped.
- `ProviderLedger.rows` keep only in-scope Codex rows after this change; the rows list in `--json` output shrinks accordingly.
