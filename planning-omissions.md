# Planning omissions

## PLAN-portal-token-savings, pressure test of 2026-11-02

**Blocked execution**

- Contracts in `read-gate` and `ask-trial` asserted only behaviour today's code already has (rule 1 warns; an unknown subcommand never runs the fake). Freeze refuses a contract that passes at base, so these needed a control that is red at base.
- The `Ask` arm breaks `dispatch`, an exact 59-line ledger entry, and no standard stage ran `--test maintainability`, so it first failed at integration-verify.
- `docs/test-run.log` matches `*.log` in `.gitignore`, so the fixture would never have reached the commit.
- The briefs told the orchestration skill to grow past its pinned byte cap and claimed the new wording offset it.

**Would have shipped broken**

- The cost model charged a cache read on the request that wrote an item, charged replay on the last request's output, and used a flat 1.25 write weight where `accounting.rs` weights 1-hour writes at 2.0. `total_s2` was never defined.
- `chars` counted serialized attachments, including kinds the model never sees; `rendered` is the model-visible text. Windowed transcripts were reported as startup, and `requests_before` could exceed `n`.
- The worker-brief hook would tell `loom-mechanical-engineer` to invoke a `Skill` tool it lacks. Sidecar types would have read as ledger matches, and team agents' sidecars hold the teammate's name in `agentType`.
- Markdown outline rows would miss the hook's regex without the indent `render_outline` adds at its call site, and the acceptance lines passed on the footer alone. No contract crossed the hook-to-binary seam.
- The Codex lane had no isolation contract, and `--sandbox read-only` still allows shell reads. The child environment kept session identity. SIGINT would orphan a worker in its own process group.

**Wrong assumptions**

- `uncovered_file_warns_and_allows_even_with_deny_on` was expected to need rewriting and a dispute; it runs outside a live stage and passes unchanged.
- `sandbox_instructions` was claimed to appear on engineers; none of 180 engineer transcripts carries it. `spawn-guard.sh` was claimed to be in the size ledger, which covers only Rust files.
- `cache_ignore.rs::check_ignore` was named as the stdin spawning precedent; it only calls `git.run`.

**Under-specified**

- Plan files were not exempt from the read gate, though stages are told to read them. Standard stages ran no fmt, build, doc or hook suites. The distill stage could turn its own acceptance green with `--write-baseline`, and it overlapped feat-planner-13's distill. The plan named no sibling constraints and recorded no baselines.

## PLAN-portal-token-savings, Codex review of 2026-11-02

Most findings repeated the earlier round. These were new.

**Would have shipped broken**

- The `dispatch` fix added a comment line to hold the ledger at 59. The ratchet route, lowering the row through a `dispute-integrity`, was documented in `v2-contracts.md`.
- The Codex lane never received the mode prompt: it was wired only to Claude's `--system-prompt`. Nothing rejected a Codex reply that had used a tool, so a shell read would have passed as a one-call answer.
- `residual_s2` clamped at zero hid over-attribution, and no contract covered a result after the last request.
- Truncated transcripts left the startup medians with no count of how many were dropped.
- `_loom_ledger_append` trims to 300 rows, which contradicted "a repeat is allowed" once an agent passed 300 denied paths.
- Readers had no bound after a zero exit, so a descendant holding the pipes could hang `loom ask` and outlive the answer.

**Under-specified**

- `ask-trial` pinned `dispatch.rs` in `files:` and wiring, which a merged web-host-graft could invalidate.
- The After merge checks compared first-request size and read rows, which the `optim-token` metaplan says never count as a saving alone.
