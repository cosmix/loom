# Execution contract

Created: 2026-09-13 (Europe/Athens). Read this and the assigned worker brief in full. The report cutoff remains 2026-09-12T20:12:13Z. Product work has not started by publication of this plan.

## Authority and lanes

Every stage orchestrator is Opus/high, an explicit user choice overriding bookend defaults too. Standard implementation/execution units use only `loom-codex-forwarder` with the assigned `gpt-5.6-sol` or `gpt-5.6-terra`, effort `xhigh`. These are worker choices, not a request to lower models in Loom's shipped defaults. If a named lane is unavailable, stop and report it; no silent substitution. Plugin `codex@openai-codex` 1.0.6 was enabled at planning time.

Orchestrators delegate all implementation. Use the existing exact wrapper argv and the forwarder's one foreground Bash tool call with explicit timeout 600000 ms. Do not improvise plugin invocations, background extra writer processes, or repeat forwarding when an acknowledgement arrives. The job-lifecycle stage updates this contract internally while retaining the single tool call and outer sandbox. Before that stage lands, inspect exact task evidence manually when uncertain; never release an unknown writer.

Workers write only their listed files. Read other paths only for the assigned seam; graph-first navigation and targeted source ranges apply. Do not read `CLAUDE.md`, run git, run verification, spawn subagents, or write shared runtime state. The orchestrator handles version control and the one canonical stage gate. Workers return exact changed files, assumptions, tests added, unresolved questions, and evidence paths. Record material source contradictions and decisions with existing Loom memory commands; direct knowledge writes belong only to bookends.

## Coordination and merge ownership

Do independent units concurrently inside a single stage worktree. Serial waves are explicit in each stage, not new stages. A worker finishing does not authorize another writer to take its files until actual execution has settled. A reviewer's read access never grants write ownership. On discovery of a shared file, pause the affected units, assign one owner, amend the brief inventory, and rerun the stage's structural verification; never allow a silent two-writer overlap.

One background watch covers the current healthy dependency set. Keep the normal two-second in-process notification interval. Do not loop model calls to list/status, message finished workers, or perform whole-corpus reads. A timeout, missing notification or unknown receipt gets one bounded exact-identity inspection and explicit escalation; it does not authorize cancellation, retry or ownership release. Batch independent known paths/searches into a bounded tool call without truncating required evidence. Dependent search-to-read calls remain sequential.

## Verification and performance floor

Standard gates run their owned module/integration tests plus all-target build/clippy and format checks. The full Rust suite runs at integration-verify; the later explicit plan-writer rule selecting this policy governs the skill's earlier blanket-suite wording. Hook-writing stages also run all hook cases and syntax checks. The orchestrator inspects test counts and failures, not exit code alone; a name filter selecting zero tests is failure of the proof. New targets named in the briefs are implementation deliverables, not claimed baseline tests.

Never edit `loom/maintainability-baseline.txt` to grandfather new violations. Split new and touched helpers to satisfy the repository's size limits. Keep warnings denied and existing tests selected. All gates are offline/locked with existing dependency caches; no installation or new network grants are planned. Standard workers must not run verification even if an example in a subordinate brief omits this reminder.

Preserve required quality, independent review, file ownership and semantic completion. Keep current model/effort/context limits and required facts. No numeric subscription-weight assumptions, unconditional handoff cap, lower-effort experiment or cache-TTL change is authorized. Measure total accepted work, both provider vectors and elapsed critical path. Fixing a false verification pass is a correctness prerequisite; that invalid shortcut cannot serve as the performance baseline.

## Activation and rollback

Do not run this plan alongside PLAN-loop-recovery, PLAN-model-router-hooks or PLAN-strengthen-verification (overlap: `loom/src/plan/schema/validation.rs`) on overlapping seams. Concurrent pre-commit work (merged during final review) and unrelated dirty files are outside this plan's implementation authority. Bootstrap checks current committed code against this plan; if a sibling has landed, reuse its actual tested public contract, amend paths/ownership/acceptance and revalidate before spawning workers. If absent, this plan's named owner builds the complete feature. No prose-only sibling capability is a dependency. The hook-directory rename landed in `a5520004`; the `loom-hooks/` edit-path probe passed at `7d6a14ca`.

The existing project sandbox policy remains enabled, confined, without unsandboxed escape, extra domains, local listeners or host sockets. Package caches and Codex transport/state grants are only those already emitted by Loom. Source writes are narrowed further by each stage's file inventory. Never relax a denial to make a test pass. If the exact stage-sandbox baseline fails, stop activation, diagnose the resource requirement, and amend this plan with a scoped fixture/repair; never waive the gate.

Keep full logs local; reports retain sanitized counters and bounded evidence pointers. No credentials, raw transcripts or prompts are copied into repository fixtures. Offline deterministic tests can validate contracts; subscription-consuming live canaries require an explicit scheduled experiment. A candidate with any observed quality or elapsed-time regression is rejected; unknown data is inconclusive. Rollback is the operator's ordinary scoped revert/deployment procedure, never automatic killing of an unknown worker.

The stage sandbox cannot create `/tmp/loom-token-optimization-checks`; the operator creates it on the host before each standard stage spawns, and again after any reboot. A stage's `setup` mkdir only no-ops when the directory already exists. If it is missing, the orchestrator stops and reports; it never repoints TMPDIR into the worktree, because an in-checkout TMPDIR fails about 290 existing tests. Fixtures create unique children; no destructive cleanup targets the scratch root itself.
