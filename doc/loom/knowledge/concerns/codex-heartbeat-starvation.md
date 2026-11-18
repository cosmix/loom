# Codex Heartbeat Starvation

>

## Long Codex Runs Starve the Loom Heartbeat

A foreground Codex forward can outlast the session heartbeat because PostToolUse runs after a blocking call. Sandboxed output, process names, job files and logs do not prove that a worker is the authorized launch, so the hold rests on what the sandbox cannot reach (`orchestrator/monitor/direct_worker.rs`, `codex_lifecycle.rs::forward_is_fresh`):

- `codex-forward-guard.sh` runs outside the sandbox, mints an invocation id, records it in a ledger the sandbox cannot write, writes it into the tool call it authorizes, and refuses every other tool command quoting a minted id. The authorized launch is the one direct launcher child of the PID-verified agent whose kernel-visible argv holds the id; two such children are refused.
- **Direct lane:** only CPU under the supervisor (`python3 _codex-direct.py`) counts, excluding the supervisor's own polling. No worker under it means no hold.
- **Companion lane:** the launch's own `status <job> --wait` names the job, and the job's log must be fresh, so a planted job and a touched log hold nothing without the authorized wait.
- A forward with no receipt yet tries both; one finished by its receipt holds nothing while a process lingers. The window is `RECEIPT_WINDOW_SECS` (1560 s) from `authorized_at`, with one idle budget.

Limits:

- Where the launcher runs the command with no outer process outside the sandbox (macOS `sandbox-exec`, an unsandboxed session), a stage that reads an id from the ledger can `exec` that launcher into a matching argv. That earns only the bounded CPU hold any foreground command earns. Revisit if the harness exposes a per-tool-call host identity.
- Inside a real authorized companion launch, the job file and log stay stage-writable, so a stage can stretch the hold to the end of that launch, inside the same window. Closing it needs a host-owned progress signal.
- A real bubblewrap-launched direct forward's argv has not been observed; the check fails closed. The macOS `ps` argv path has not been executed, and macOS tick totals exclude reaped children.

## `loom status` Badge Follows the Stage Budget

The status activity badge and the monitor share the same foreground evidence and bounded idle and wall-window decisions. Direct receipts alone do not authorize a progress hold. Transcript data supplies only the display label: a live process with fresh observed progress shows Busy while an idle process expires. Native macOS execution remains a CI check. Rules: [status-data-model.md](../architecture/status-data-model.md#heartbeat-and-activity).
