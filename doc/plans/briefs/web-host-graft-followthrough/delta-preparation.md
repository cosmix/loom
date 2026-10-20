# Worker brief: deterministic bounded distillation preparation

Implement an additive `loom memory prepare` command and make the terminal
knowledge-distill workflow consume it. The goal is to deliver unresolved,
source-bearing candidate groups without repeatedly printing every journal.
Architecture/edge-case owner: GPT-6 Sol. Implementation: GPT-5.6 Terra.
Pure fixture and CLI boilerplate units may go to GPT-6 Luna with disjoint files.

## Current seams

- `cli/types_memory.rs::MemoryCommands`: parse a new Prepare variant.
- `cli/dispatch.rs::dispatch_memory`: route it to `commands::memory::prepare`.
- `commands/memory/handlers/pending.rs::staged_entries`: sorted journal reader
  with current-worktree spool inclusion and ID deduplication.
- `pending_report`: current candidate policy and receipt settlement semantics.
- `resolve.rs::resolve`: existing disposition writer; preserve its outcomes.
- `fs/memory/types.rs::MemoryEntry`: stable ID, evidence, session and receipt.
- `orchestrator/signals/cache.rs::generate_knowledge_distill_stable_prefix`:
  currently repeats all-journal text/JSON reads; replace those consumers.

Read the companion audit before implementing. Follow repository graph-first
navigation. New files are absent from the published graph and can be read directly.

## Allowed files

Implementation ownership:

- `loom/src/commands/memory/mod.rs`
- `loom/src/commands/memory/handlers/mod.rs`
- `loom/src/commands/memory/handlers/pending.rs`
- New `loom/src/commands/memory/handlers/prepare.rs`
- New `loom/src/commands/memory/handlers/prepare/model.rs`
- New `loom/src/commands/memory/handlers/prepare/group.rs`
- New `loom/src/commands/memory/handlers/prepare/render.rs`
- New `loom/src/commands/memory/handlers/tests_prepare.rs`
- `loom/src/commands/memory/handlers/tests_resolve_pending.rs`
- `loom/src/orchestrator/signals/cache.rs`
- New `loom/src/orchestrator/signals/cache/knowledge_distill.rs`: extract the
  distill workflow and decompose it into functions <=50 lines
- New `loom/src/orchestrator/signals/cache/tests.rs`: move the parent's existing
  test module here so the touched cache file is <=400 lines; preserve coverage
- `loom/src/orchestrator/signals/cache/distill_ordering.rs`
- `loom/src/orchestrator/signals/tests_doctrine.rs`
- `commands/distill.md`

The shared CLI integration worker exclusively owns `cli/types_memory.rs` and
`cli/dispatch.rs`. Export this exact handler for its serial wiring:
`pub fn prepare(stage_id: Option<String>, budget_tokens: usize, json: bool) -> Result<()>`.
It adds `MemoryCommands::Prepare` and dispatches its parsed fields to this handler.
The parent plan should assign any README edits to one documentation owner, since
the web host feature also documents its CLI. No dependency changes are expected.
If a necessary consumer lies elsewhere, report it for ownership adjustment.

## Public contract

`loom memory prepare [--stage ID] [--budget-tokens N] [--json]` is read-only.
Default N is 2000 estimated tokens; reuse the existing CLI budget validator.
Normal output must fit the selected estimate including headings, metadata and
omission notice. JSON must carry the same groups, budget accounting and omission
facts; define and test cost against its actual serialized representation, rather
than counting only the event bodies. Label estimates explicitly.

Return an empty successful pack when no events require processing. Follow existing
read-only memory behavior when no active journal state exists; do not initialize
state, run sync, contact providers, or write an output file implicitly.

Each invocation observes the available journals/spool once and constructs:

- A versioned deterministic input digest over canonical observed entries and
  relevant receipts. No fresh timestamp or random nonce in canonical output.
- Candidate groups of unreceipted Note/Decision/Question events, carrying exact
  event IDs, source stage/session identity, original body/context and evidence.
- Stable group order: corrections (`stale-knowledge:` prefix) first, then stable
  group key, with event IDs sorted inside groups. Never rely on HashMap order.
- Counts for candidates, groups, equivalent receipts, deferred dispositions,
  Change entries without receipts, omitted groups and coverage of the page.
- A compact explanation that source/evidence strings are claims requiring current
  source verification, and this digest is an observation rather than a durable
  knowledge-operation receipt.

Grouping is deliberately strict: same entry type, body, context and normalized
evidence set. Preserve original text for display; only normalize line endings and
comparison-safe metadata. Different rationale/evidence/correction text must remain
separate. Every grouped ID must remain accessible and independently resolvable.
No fuzzy semantic dedupe, embeddings or model judgment.

Do not silently drop groups because they do not fit. Ordinary pages show omitted
counts and instruct the curator to prepare again after resolving the current
page. If the first required correction/candidate itself cannot fit, emit a bounded
diagnostic and nonzero status instructing a larger explicit budget; do not advance
past it or provide an incomplete excerpt as sufficient evidence to resolve it.
Tests must include long evidence and large duplicate-ID groups, not only bodies.

## Receipt semantics and failure behavior

Reuse current receipt visibility. Deferred stays settled for this run and is
reported distinctly; do not introduce a retry cursor or automatic reopen.
Equivalent repeated receipts do not cause a duplicate candidate. Conflicting
outcomes/targets/reasons for one event are an explicit blocking diagnostic with
event/receipt IDs. Do not pick the latest receipt or mark anything processed.

The command is not a repair facility. Do not mutate journal history, knowledge,
source, archives or receipt targets. It does not promise concurrent atomicity;
new entries or resolutions between invocations change the digest and selection.
Preserve pending/resolve compatibility and don't strengthen their existing exit
contracts incidentally. An unsafe/conflicting prepared snapshot is actionable
failure rather than an empty “all done” result.

The sibling state-confinement plan owns `record.rs`, `work_dir.rs`, the relay,
inbox, daemon drain and completion paths. Keep them read-only. Reuse existing
`staged_entries` visibility instead of opening a new transport. A future relay
may acknowledge resolution before the daemon applies it; do not busy-loop on
an unchanged prepared digest. Follow that transport's status/acknowledgment
contract before asking for the next page. CLI integration must preserve sibling
dispatch changes and extract a shared helper module if needed for size limits.

## Live consumer and completion

Update the generated distill prefix and `/distill` workflow to:

1. Run prepare for a bounded candidate page.
2. Apply corrections first, verify code-grounded claims in the final source tree,
   and make semantic decisions as the existing single curator.
3. Perform existing `knowledge update`/`replace-section` writes where warranted.
4. Resolve **every original event ID** in a group with the existing outcome syntax.
5. Repeat prepare after those dispositions, then finish with `memory pending
   --strict` and the existing review/completion sequence.

Remove routine `memory show --all` and all-event JSON reads from this workflow;
retain explicit diagnostic reads only as an exceptional escalation. Preserve the
corrections-before-writes ordering tests, single-curator rule, final-tree evidence
check, tier routing, and existing sandbox write channel. No new host finalization
or canonical receipt claim belongs in this stage.

## Acceptance evidence

Required behavior tests, executed by the plan's verification owner:

- Unchanged fixture bytes produce byte-identical output/digest; journal iteration
  order cannot change groups or ordering.
- Settled entries disappear, deferred remains settled but visible in counts,
  Change entries retain today's non-candidate policy.
- Exact duplicates retain every ID; different context/evidence remains separate.
- Undrained current-worktree spool entries are included exactly once; sibling
  spools are not read. Exercise the existing stage/journal fixture path.
- Conflicting dispositions cannot return empty success; equivalent duplicate
  receipts do not fabricate additional work.
- Human and JSON serialized output honor budgets and report omissions. An
  oversized first correction fails explicitly without skipping its ID.
- A CLI integration cycle prepares, resolves one group with existing commands,
  prepares the remainder, and ends with pending strict success.
- Generated doctrine points at the real Prepare command, retains correction
  ordering and resolves each member ID. No routine all-journal read remains.
- Preparing does not change any fixture file or create state/cache artifacts.

Useful check filters after implementation are the actual test names in
`tests_prepare`, `tests_resolve_pending`, and `distill_ordering`; the integration
owner must run the final changed-tree build/tests/lint/format requirements.
The worker itself follows Loom stage verification restrictions and reports files,
assumptions and unresolved issues.

One Terra worker owns this entire territory; the Sol design reviewer is read-only.
No additional Luna write assignment overlaps these tests. Shared CLI integration
owns removal of resolved maintainability baseline rows after these extractions.
Do not run git, tests, builds, linters or formatters inside this worker.
