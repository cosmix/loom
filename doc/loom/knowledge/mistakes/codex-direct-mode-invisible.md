# Codex Direct Mode Invisible

> Direct codex forwards unseen by reconcile and status

## Direct-Mode Codex Forwards Are Invisible to Lifecycle Reconcile and Status

**What happened:** In liquidmon on 2026-10-23, two `loom-codex-forwarder` units ran gpt-5.6-terra and succeeded, but `loom status` and the web dashboard never showed `gpt-5.6-terra (requested)` for the stage, which earlier runs did. The daemon log repeated `Codex lifecycle evidence is unknown ... reading companion directory ~/.codex/plugin-data/state/fix-instance-lock-<hash>/jobs` every 5 s.

**Why:** Commit 27a62e80 (2026-10-21) made `loom-hooks/codex-forward.sh` choose `mode=direct` (`codex exec --sandbox danger-full-access` through `_codex-direct.py`) when the outer sandbox refuses a nested bubblewrap, which is the Linux stage sandbox. Direct mode creates no companion job under the plugin-data state root. `codex_lifecycle::reconcile_codex_jobs` locates evidence only in companion job directories, so every direct forward is Unknown, `lifecycle.jsonl` is never written, and `status/data/execution_models.rs` `correlated_requested_model` drops the model because `has_correlated_lifecycle` is false. The forward receipts (`subagents/<stage>/forward-receipts.jsonl`, `backend_id` plus `running`/`succeeded`) held the evidence all along.

**Prevention:** A new execution mode for a forwarded worker must be traced through every reader of the old mode's evidence: `codex_lifecycle` reconcile, `loom subagents watch` codex binding and stall detection, token discovery, and status. A test that runs the forwarder in each mode and asserts the stage's `execution_models` closes it.

**Fix:** doc/plans/PLAN-contract-phase-autonomy.md, stage contract-collection, worker W4.
